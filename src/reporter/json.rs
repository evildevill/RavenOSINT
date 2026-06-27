use std::fs;
use std::path::PathBuf;

use serde::Serialize;
use tracing::info;

use crate::error::RavenError;
use crate::reporter::Reporter;
use crate::types::*;

#[derive(Serialize)]
struct JsonReport {
    results: Vec<SearchResults>,
}

pub struct JsonReporter {
    file_path: PathBuf,
    all_results: Vec<SearchResults>,
    print_all: bool,
}

impl JsonReporter {
    pub fn new<P: Into<PathBuf>>(file_path: P, print_all: bool) -> Self {
        JsonReporter {
            file_path: file_path.into(),
            all_results: Vec::new(),
            print_all,
        }
    }
}

impl Reporter for JsonReporter {
    fn write_search_start(&mut self, _username: &str) -> Result<(), RavenError> {
        Ok(())
    }

    fn write_result(&mut self, _result: &QueryResult) -> Result<(), RavenError> {
        Ok(())
    }

    fn write_search_complete(&mut self, results: &SearchResults) -> Result<(), RavenError> {
        let mut filtered = results.clone();
        if !self.print_all {
            filtered
                .results
                .retain(|r| r.status == QueryStatus::Claimed);
            filtered.total_sites = filtered.results.len();
            filtered.claimed_count = filtered
                .results
                .iter()
                .filter(|r| r.status == QueryStatus::Claimed)
                .count();
        }
        self.all_results.push(filtered);
        Ok(())
    }

    fn finish(&mut self) -> Result<(), RavenError> {
        let report = JsonReport {
            results: self.all_results.clone(),
        };
        let json = serde_json::to_string_pretty(&report)
            .map_err(|e| RavenError::Report(format!("Failed to serialize JSON: {e}")))?;
        fs::write(&self.file_path, &json)?;
        info!("JSON report written to {}", self.file_path.display());
        Ok(())
    }
}
