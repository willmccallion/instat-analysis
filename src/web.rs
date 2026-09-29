//! The browser UI, embedded in the binary so the app is a single file.

const TEMPLATE: &str = include_str!("../web/index.html");
const STYLES: &str = include_str!("../web/app.css");
const CHARTS: &str = include_str!("../web/charts.js");
const APP: &str = include_str!("../web/app.js");

fn page(snapshot: &str) -> String {
    // Data goes in last so text inside it can never be mistaken for a placeholder.
    TEMPLATE
        .replacen("/*STYLES*/", STYLES, 1)
        .replacen("/*CHARTS*/", CHARTS, 1)
        .replacen("/*APP*/", APP, 1)
        .replacen("/*SNAPSHOT*/", snapshot, 1)
}

/// The live app, talking to the local server.
#[must_use]
pub fn app_page() -> String {
    page("")
}

/// A self-contained report with the analysis baked in; works offline with no server.
#[must_use]
pub fn snapshot_page(analysis_json: &str) -> String {
    let safe = analysis_json
        .replace("</", "<\\/")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");
    page(&format!("window.HOCKEY_SNAPSHOT = {safe};"))
}

#[cfg(test)]
mod tests {
    use super::snapshot_page;

    #[test]
    fn snapshot_cannot_close_its_script_tag() {
        let page = snapshot_page(r#"{"name":"</script><script>alert(1)</script>"}"#);
        assert!(!page.contains("</script><script>alert(1)"));
        assert!(page.contains("window.HOCKEY_SNAPSHOT"));
    }
}
