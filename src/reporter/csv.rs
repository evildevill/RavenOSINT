use std::fs::File;
use std::path::PathBuf;

use tracing::info;

use crate::error::RavenError;
use crate::reporter::Reporter;
use crate::types::*;

pub struct CsvReporter {
    file_path: PathBuf,
    writer: Option<csv::Writer<File>>,
    print_all: bool,
}

impl CsvReporter {
    pub fn new<P: Into<PathBuf>>(file_path: P, print_all: bool) -> Self {
        CsvReporter {
            file_path: file_path.into(),
            writer: None,
            print_all,
        }
    }
}

impl Reporter for CsvReporter {
    fn write_search_start(&mut self, _username: &str) -> Result<(), RavenError> {
        let file = File::create(&self.file_path)?;
        let mut writer = csv::Writer::from_writer(file);
        writer.write_record([
            "username",
            "site_name",
            "url_user",
            "status",
            "http_status",
            "response_time_ms",
        ])?;
        self.writer = Some(writer);
        Ok(())
    }

    fn write_result(&mut self, result: &QueryResult) -> Result<(), RavenError> {
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| RavenError::Report("CSV writer not initialized".to_string()))?;

        if !self.print_all && result.status != QueryStatus::Claimed {
            return Ok(());
        }

        writer.write_record([
            &result.username,
            &result.site_name,
            &result.site_url_user,
            &result.status.to_string(),
            &result
                .http_status
                .map(|s| s.to_string())
                .unwrap_or_default(),
            &result
                .query_time_ms
                .map(|t| t.to_string())
                .unwrap_or_default(),
        ])?;

        Ok(())
    }

    fn write_search_complete(&mut self, _results: &SearchResults) -> Result<(), RavenError> {
        if let Some(writer) = self.writer.as_mut() {
            writer.flush()?;
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<(), RavenError> {
        info!("CSV report written to {}", self.file_path.display());
        Ok(())
    }
}
