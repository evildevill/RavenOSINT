use std::fs;
use std::path::PathBuf;

use tracing::info;

use crate::error::RavenError;
use crate::reporter::Reporter;
use crate::types::*;

pub struct TxtReporter {
    file_path: PathBuf,
    lines: Vec<String>,
    claimed_count: usize,
}

impl TxtReporter {
    pub fn new<P: Into<PathBuf>>(file_path: P) -> Self {
        TxtReporter {
            file_path: file_path.into(),
            lines: Vec::new(),
            claimed_count: 0,
        }
    }
}

impl Reporter for TxtReporter {
    fn write_search_start(&mut self, _username: &str) -> Result<(), RavenError> {
        self.lines.clear();
        self.claimed_count = 0;
        Ok(())
    }

    fn write_result(&mut self, result: &QueryResult) -> Result<(), RavenError> {
        if result.status == QueryStatus::Claimed {
            self.claimed_count += 1;
            if !result.site_url_user.is_empty() {
                self.lines.push(result.site_url_user.clone());
            }
        }
        Ok(())
    }

    fn write_search_complete(&mut self, _results: &SearchResults) -> Result<(), RavenError> {
        let mut output = self.lines.join("\n");
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&format!(
            "Total Websites Username Detected On: {}\n",
            self.claimed_count
        ));
        fs::write(&self.file_path, &output)?;
        info!(
            "TXT report written to {} ({} URLs)",
            self.file_path.display(),
            self.claimed_count
        );
        Ok(())
    }

    fn finish(&mut self) -> Result<(), RavenError> {
        Ok(())
    }
}
