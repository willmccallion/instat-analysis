use hockey_stats::parse::{match_report, players_report};
use hockey_stats::pdf::extract_pages;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let m = extract_pages(&std::fs::read("tests/fixtures/game1_match.pdf")?)?;
    let report = match_report::parse(&m)?;
    println!("{:?}", report.title);
    println!("abbr {:?}", report.team_stats.abbreviations);
    for e in &report.team_stats.entries[0] {
        println!("  [{}] {} = {}", e.group, e.label, e.value.text);
    }
    for (i, t) in [&report.ours].iter().enumerate() {
        println!("TEAM {i}: main rows {}, ch {}, to {}, en {}, shots {}, chz {}", t.tables.main.len(), t.tables.challenges.len(), t.tables.turnovers.len(), t.tables.entries.len(), t.tables.shots.len(), t.tables.challenges_by_zone.len());
        if let Some(r) = t.tables.main.first() { println!("  {:?}", r); }
        if let Some(r) = t.tables.entries.first() { println!("  {:?}", r); }
        for u in &t.units { println!("  unit {:?} {:?} {:?} {:?}", u.kind, u.members.iter().map(|m| format!("{:?} {}", m.number, m.surname)).collect::<Vec<_>>(), u.toi, u.stats); }
        if let Some(tl) = &t.timeline {
            println!("  timeline rows {} goals {:?} bands {:?}", tl.rows.len(), tl.goals, tl.bands);
            for r in &tl.rows { println!("   {:?} {} {:?} shifts {}", r.label.number, r.label.surname, r.group, r.shifts.len()); }
        }
        for (name, mx) in [("passes", &t.passes)] {
            if let Some(mx) = mx { println!("  {name}: cols {} rows {} first {:?}", mx.columns.len(), mx.rows.len(), mx.rows.first().map(|r| (&r.0.surname, &r.1))); }
        }
    }
    let p = extract_pages(&std::fs::read("tests/fixtures/game1_players.pdf")?)?;
    let pr = players_report::parse(&p)?;
    println!("{:?}", pr.title);
    for pl in &pr.players {
        println!("P {:?} {:?} {} stats {} hist {} first {:?}", pl.kind, pl.jersey, pl.full_name, pl.stats.len(), pl.history.len(), pl.history.first());
    }
    if let Some(pl) = pr.players.get(1) { for s in &pl.stats { println!("   {:?}", s); } }
    Ok(())
}
