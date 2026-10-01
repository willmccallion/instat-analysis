use hockey_stats::analysis::{Request, analyse};
use hockey_stats::model::TeamPrefix;
use hockey_stats::store::Store;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let team = args.next().and_then(|t| TeamPrefix::parse(&t)).ok_or("usage: TEAM FILE...")?;
    let library = std::env::temp_dir().join(format!("hockey-stats-analyse-{}", std::process::id()));
    let mut store = Store::open(&library)?;
    store.set_team(team)?;
    for path in args.map(std::path::PathBuf::from) {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_owned();
        eprintln!("{name}: {:?}", store.add_file(&std::fs::read(&path)?, &name)?);
    }
    let started = std::time::Instant::now();
    let analysis = analyse(&store.games(), &store.league_games(), &Request::default());
    eprintln!("analysis took {:?}", started.elapsed());
    println!("{}", serde_json::to_string_pretty(&analysis)?);
    std::fs::remove_dir_all(library)?;
    Ok(())
}
