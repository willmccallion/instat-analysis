use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let bytes = std::fs::read(&args[1])?;
    let only: u32 = args[2].parse()?;
    for page in hockey_stats::pdf::extract_pages(&bytes)? {
        if page.number != only {
            continue;
        }
        for r in &page.fills {
            println!(
                "fill {:8.2} {:8.2} {:8.2} {:8.2}",
                r.x0, r.top, r.x1, r.bottom
            );
        }
        for s in &page.shapes {
            let r = s.bounds;
            println!(
                "shape {:8.2} {:8.2} {:8.2} {:8.2} rgb({:.2},{:.2},{:.2}){}",
                r.x0, r.top, r.x1, r.bottom, s.fill.r, s.fill.g, s.fill.b,
                if s.stroked { " outlined" } else { "" }
            );
        }
        for r in &page.clips {
            println!(
                "clip {:8.2} {:8.2} {:8.2} {:8.2}",
                r.x0, r.top, r.x1, r.bottom
            );
        }
    }
    Ok(())
}
