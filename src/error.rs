use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("could not read PDF: {0}")]
    Pdf(String),
    #[error("parse error in {section}: {detail}")]
    Parse {
        section: &'static str,
        detail: String,
    },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("choose your team before adding games")]
    NoTeam,
    #[error("update failed: {0}")]
    Update(String),
    #[error("{matchup}: {}", wrong_team_reason(team, *both))]
    WrongTeam { team: String, matchup: String, both: bool },
}

fn wrong_team_reason(team: &str, both: bool) -> String {
    if both {
        format!("both teams start with \"{team}\", so the app can't tell which is yours")
    } else {
        format!(
            "neither team starts with \"{team}\". If this is another team's game, only its Match report is needed; otherwise check your team name at the top of the Games page"
        )
    }
}

impl Error {
    pub fn parse(section: &'static str, detail: impl Into<String>) -> Self {
        Self::Parse {
            section,
            detail: detail.into(),
        }
    }
}
