use hockey_stats::analysis::{Request, analyse};
use hockey_stats::ingest::{build_games, parse_document};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let docs = std::env::args()
        .skip(1)
        .map(|p| Ok(parse_document(&std::fs::read(p)?)?))
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let (games, problems) = build_games(&docs);
    eprintln!("problems: {problems:?}");
    let started = std::time::Instant::now();
    let analysis = analyse(&games, &Request::default());
    eprintln!("analysis took {:?}", started.elapsed());
    println!("{}", serde_json::to_string_pretty(&analysis)?);
    Ok(())
}
