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
}

impl Error {
    pub fn parse(section: &'static str, detail: impl Into<String>) -> Self {
        Self::Parse {
            section,
            detail: detail.into(),
        }
    }
}
