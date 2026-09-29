use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let bytes = std::fs::read(&args[1])?;
    let only: Option<u32> = args.get(2).map(|s| s.parse()).transpose()?;
    for page in hockey_stats::pdf::extract_pages(&bytes)? {
        if only.is_some_and(|n| n != page.number) {
            continue;
        }
        println!("=== page {}", page.number);
        for w in &page.words {
            println!("{:8.2} {:8.2} {:8.2} {:8.2} {}", w.x0, w.top, w.x1, w.bottom, w.text);
        }
    }
    Ok(())
}
