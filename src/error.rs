use thiserror::Error;

#[derive(Error, Debug)]
pub enum RavenError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("TOML parsing error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("Regex error: {0}")]
    Regex(#[from] fancy_regex::Error),

    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),

    #[error("XLSX error: {0}")]
    Xlsx(#[from] rust_xlsxwriter::XlsxError),

    #[error("Manifest error: {0}")]
    Manifest(String),

    #[error("CLI error: {0}")]
    Cli(String),

    #[error("Report error: {0}")]
    Report(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("{0}")]
    Other(String),
}

impl From<String> for RavenError {
    fn from(s: String) -> Self {
        RavenError::Other(s)
    }
}

impl From<&str> for RavenError {
    fn from(s: &str) -> Self {
        RavenError::Other(s.to_string())
    }
}

impl From<rusqlite::Error> for RavenError {
    fn from(e: rusqlite::Error) -> Self {
        RavenError::Database(e.to_string())
    }
}
