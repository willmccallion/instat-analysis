//! Hockey Stats: double-click to open the app in the browser, or run
//! `hockey-stats --report <pdfs…> -o report.html` to build a report without the UI.

use std::path::PathBuf;
use std::process::ExitCode;

use hockey_stats::analysis::{Request, analyse};
use hockey_stats::error::Error;
use hockey_stats::ingest::{build_games, parse_document};
use hockey_stats::server;
use hockey_stats::store::{self, Store};
use hockey_stats::web;

const USAGE: &str = "\
Hockey Stats

  hockey-stats                        open the app in your browser
  hockey-stats --report FILES… -o OUT write a standalone HTML report from PDFs

Options:
  --data-dir DIR   where games are stored (default: platform app-data folder)
  --port N         serve on a fixed port (default: any free port)
  --no-browser     do not open a browser window
  -h, --help       show this help";

enum Mode {
    App,
    Report { inputs: Vec<PathBuf>, output: PathBuf },
    Help,
}

struct Options {
    mode: Mode,
    data_dir: PathBuf,
    port: u16,
    open_browser: bool,
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        mode: Mode::App,
        data_dir: store::default_dir(),
        port: 0,
        open_browser: true,
    };
    let mut inputs = Vec::new();
    let mut output = None;
    let mut report = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => options.mode = Mode::Help,
            "--report" => report = true,
            "-o" | "--output" => output = Some(PathBuf::from(iter.next().ok_or("-o needs a file name")?)),
            "--data-dir" => options.data_dir = PathBuf::from(iter.next().ok_or("--data-dir needs a folder")?),
            "--port" => {
                options.port = iter
                    .next()
                    .ok_or("--port needs a number")?
                    .parse()
                    .map_err(|_| "--port needs a number between 0 and 65535")?;
            }
            "--no-browser" => options.open_browser = false,
            // macOS may pass a process serial number when launched from Finder.
            other if other.starts_with("-psn_") => {}
            other if other.starts_with('-') => return Err(format!("unknown option {other}")),
            other => inputs.push(PathBuf::from(other)),
        }
    }
    if report {
        let output = output.ok_or("--report needs -o OUTPUT.html")?;
        if inputs.is_empty() {
            return Err("--report needs at least one PDF".into());
        }
        options.mode = Mode::Report { inputs, output };
    } else if !inputs.is_empty() && !matches!(options.mode, Mode::Help) {
        return Err("PDF files are only accepted with --report; in the app, drop them on the page".into());
    }
    Ok(options)
}

fn write_report(inputs: &[PathBuf], output: &PathBuf) -> Result<(), Error> {
    let documents = inputs
        .iter()
        .map(|path| {
            let bytes = std::fs::read(path)?;
            parse_document(&bytes).map_err(|e| Error::Pdf(format!("{}: {e}", path.display())))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let (games, problems) = build_games(&documents);
    for problem in &problems {
        eprintln!("warning: {problem}");
    }
    for game in &games {
        for warning in &game.warnings {
            eprintln!("warning ({} vs {}): {warning}", game.date, game.opponent.0);
        }
    }
    let analysis = analyse(&games, &Request::default());
    std::fs::write(output, web::snapshot_page(&serde_json::to_string(&analysis)?))?;
    eprintln!("wrote {} ({} game(s))", output.display(), games.len());
    Ok(())
}

fn run_app(options: &Options) -> Result<(), Error> {
    std::fs::create_dir_all(&options.data_dir)?;
    if let Some(url) = server::running_instance(&options.data_dir) {
        eprintln!("Hockey Stats is already running: {url}");
        if options.open_browser {
            server::open_browser(&url)?;
        }
        return Ok(());
    }
    let store = Store::open(&options.data_dir)?;
    for problem in &store.problems {
        eprintln!("warning: {problem}");
    }
    let open = options.open_browser;
    server::serve(&options.data_dir, store, options.port, |url| {
        eprintln!("Hockey Stats is running at {url}");
        if open
            && let Err(e) = server::open_browser(url)
        {
            eprintln!("could not open a browser ({e}); open the address above manually");
        }
    })
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match parse_args(&args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let result = match &options.mode {
        Mode::Help => {
            println!("{USAGE}");
            Ok(())
        }
        Mode::Report { inputs, output } => write_report(inputs, output),
        Mode::App => run_app(&options),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
