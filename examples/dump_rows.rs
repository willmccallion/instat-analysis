use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let bytes = std::fs::read(&args[1])?;
    let only: u32 = args[2].parse()?;
    for page in hockey_stats::pdf::extract_pages(&bytes)? {
        if page.number != only {
            continue;
        }
        let mut words = page.words.clone();
        words.sort_by(|a, b| a.center_y().total_cmp(&b.center_y()).then(a.x0.total_cmp(&b.x0)));
        let mut rows: Vec<Vec<hockey_stats::pdf::Word>> = Vec::new();
        for w in words {
            match rows.last_mut() {
                Some(row) if (row[0].center_y() - w.center_y()).abs() < 1.5 => row.push(w),
                _ => rows.push(vec![w]),
            }
        }
        for mut row in rows {
            row.sort_by(|a, b| a.x0.total_cmp(&b.x0));
            let line: Vec<String> = row.iter().map(|w| format!("{}@{:.0}", w.text, w.x0)).collect();
            println!("{:6.1} | {}", row[0].center_y(), line.join(" "));
        }
    }
    Ok(())
}
