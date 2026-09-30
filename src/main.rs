//! Hockey Stats: double-click to open the app in the browser, or run
//! `hockey-stats --report <pdfs…> -o report.html` to build a report without the UI.
// On Windows, a double-clicked app must not open a console window.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::path::PathBuf;
use std::process::ExitCode;

use hockey_stats::analysis::{Request, analyse};
use hockey_stats::error::Error;
use hockey_stats::ingest::{build_games, parse_document};
use hockey_stats::model::TeamPrefix;
use hockey_stats::server::{self, RunningInstance};
use hockey_stats::store::{self, Store};
use hockey_stats::update;
use hockey_stats::web;

const USAGE: &str = "\
Hockey Stats

  hockey-stats                                   open the app in your browser
  hockey-stats --report FILES… --team NAME -o OUT write a standalone HTML report from PDFs

Options:
  --team NAME      start of your team's name in the reports, e.g. SSAC (--report only)
  --data-dir DIR   where games are stored (default: platform app-data folder)
  --port N         serve on a fixed port (default: any free port)
  --no-browser     do not open a browser window
  --foreground     keep the server attached to this process (default except on macOS)
  --background     run the server as a detached background process (default on macOS)
  -h, --help       show this help";

enum Mode {
    App,
    Report { inputs: Vec<PathBuf>, output: PathBuf, team: TeamPrefix },
    Help,
}

struct Options {
    mode: Mode,
    data_dir: PathBuf,
    port: u16,
    open_browser: bool,
    foreground: bool,
}

fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        mode: Mode::App,
        data_dir: store::default_dir(),
        port: 0,
        open_browser: true,
        foreground: !cfg!(any(target_os = "macos", windows)),
    };
    let mut inputs = Vec::new();
    let mut output = None;
    let mut team = None;
    let mut report = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => options.mode = Mode::Help,
            "--report" => report = true,
            "-o" | "--output" => output = Some(PathBuf::from(iter.next().ok_or("-o needs a file name")?)),
            "--team" => {
                team = Some(TeamPrefix::parse(iter.next().ok_or("--team needs a name")?).ok_or("--team needs a name")?);
            }
            "--data-dir" => options.data_dir = PathBuf::from(iter.next().ok_or("--data-dir needs a folder")?),
            "--port" => {
                options.port = iter
                    .next()
                    .ok_or("--port needs a number")?
                    .parse()
                    .map_err(|_| "--port needs a number between 0 and 65535")?;
            }
            "--no-browser" => options.open_browser = false,
            "--foreground" => options.foreground = true,
            "--background" => options.foreground = false,
            // macOS may pass a process serial number when launched from Finder.
            other if other.starts_with("-psn_") => {}
            other if other.starts_with('-') => return Err(format!("unknown option {other}")),
            other => inputs.push(PathBuf::from(other)),
        }
    }
    if report {
        let output = output.ok_or("--report needs -o OUTPUT.html")?;
        let team = team.ok_or("--report needs --team NAME")?;
        if inputs.is_empty() {
            return Err("--report needs at least one PDF".into());
        }
        options.mode = Mode::Report { inputs, output, team };
    } else if !inputs.is_empty() && !matches!(options.mode, Mode::Help) {
        return Err("PDF files are only accepted with --report; in the app, drop them on the page".into());
    }
    Ok(options)
}

fn write_report(inputs: &[PathBuf], output: &PathBuf, team: &TeamPrefix) -> Result<(), Error> {
    let documents = inputs
        .iter()
        .map(|path| {
            let bytes = std::fs::read(path)?;
            parse_document(&bytes, team).map_err(|e| Error::Pdf(format!("{}: {e}", path.display())))
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

/// Relaunches this executable as a detached background server and returns at once.
///
/// macOS sends a launched app an "open" Apple Event and reports error -1712 if nothing
/// answers it; this binary has no event loop, so the launched process must exit promptly
/// while the server carries on in a child process. On Windows the app has no console, so
/// the child's log file is the only place its messages can go.
fn spawn_background_server(options: &Options) -> Result<(), Error> {
    let log_path = options.data_dir.join("hockey-stats.log");
    let log = std::fs::File::create(&log_path)?;
    let mut command = std::process::Command::new(std::env::current_exe()?);
    command
        .arg("--foreground")
        .arg("--data-dir")
        .arg(&options.data_dir)
        .arg("--port")
        .arg(options.port.to_string())
        .stdin(std::process::Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    if !options.open_browser {
        command.arg("--no-browser");
    }
    command.spawn()?;
    eprintln!("Hockey Stats is starting in the background (log: {})", log_path.display());
    Ok(())
}

fn run_app(options: &Options) -> Result<(), Error> {
    std::fs::create_dir_all(&options.data_dir)?;
    update::remove_replaced_copy();
    let build = server::build_id()?;
    match server::running_instance(&options.data_dir, &build) {
        Some(RunningInstance::Current(url)) => {
            eprintln!("Hockey Stats is already running: {url}");
            if options.open_browser {
                server::open_browser(&url)?;
            }
            return Ok(());
        }
        Some(RunningInstance::Outdated(stale)) => {
            eprintln!("stopping an older copy of Hockey Stats");
            server::stop(&stale)?;
        }
        None => {}
    }
    if !options.foreground {
        return spawn_background_server(options);
    }
    let store = Store::open(&options.data_dir)?;
    for problem in &store.problems {
        eprintln!("warning: {problem}");
    }
    let open = options.open_browser;
    server::serve(&options.data_dir, store, options.port, build, |url| {
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
        Mode::Report { inputs, output, team } => write_report(inputs, output, team),
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
