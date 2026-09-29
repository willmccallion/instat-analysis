use hockey_stats::ingest::{build_games, parse_document};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let docs = std::env::args()
        .skip(1)
        .map(|p| Ok(parse_document(&std::fs::read(p)?)?))
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let (games, problems) = build_games(&docs);
    println!("problems: {problems:?}");
    for g in &games {
        println!(
            "{} vs {} {}-{} warnings: {:#?}",
            g.date, g.opponent.0, g.goals_for, g.goals_against, g.warnings
        );
        for p in &g.players {
            println!(
                "  {:?} {:>3} {:<22} {:?} shifts {} pm {:?}",
                p.position,
                p.jersey.map_or(String::new(), |j| j.to_string()),
                p.name,
                p.group,
                p.shifts.len(),
                p.skater
                    .as_ref()
                    .map(|s| (s.plus_minus, s.toi.0, s.passes, s.xg))
            );
        }
        for goal in &g.goals {
            println!(
                "  goal {:.1} {:?} {:?} {:?} {:?}",
                goal.time.0,
                goal.scored_by,
                goal.strength,
                goal.score_after,
                goal.on_ice
                    .iter()
                    .map(|i| i.0.split('|').nth(1).unwrap_or_default())
                    .collect::<Vec<_>>()
            );
        }
        println!("  units {} summary {:?}", g.units.len(), g.summary);
    }
    Ok(())
}
