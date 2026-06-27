use colored::Colorize;
use tracing::info;

use crate::error::RavenError;
use crate::reporter::Reporter;
use crate::types::*;

pub struct TerminalReporter {
    verbose: bool,
    print_all: bool,
    browse: bool,
    claimed_count: usize,
    available_count: usize,
    unknown_count: usize,
    illegal_count: usize,
    waf_count: usize,
    total_ms: u64,
}

impl TerminalReporter {
    pub fn new(verbose: bool, print_all: bool, browse: bool) -> Self {
        TerminalReporter {
            verbose,
            print_all,
            browse,
            claimed_count: 0,
            available_count: 0,
            unknown_count: 0,
            illegal_count: 0,
            waf_count: 0,
            total_ms: 0,
        }
    }

    fn status_icon(status: &QueryStatus) -> String {
        match status {
            QueryStatus::Claimed => "+".green().to_string(),
            QueryStatus::Available => "-".yellow().to_string(),
            QueryStatus::Unknown => "?".red().to_string(),
            QueryStatus::Illegal => "!".white().to_string(),
            QueryStatus::Waf => "!".red().to_string(),
        }
    }

    fn format_time(ms: u64) -> String {
        if ms >= 1000 {
            format!("{:.1}s", ms as f64 / 1000.0)
        } else {
            format!("{ms}ms")
        }
    }
}

impl Reporter for TerminalReporter {
    fn write_search_start(&mut self, username: &str) -> Result<(), RavenError> {
        self.claimed_count = 0;
        self.available_count = 0;
        self.unknown_count = 0;
        self.illegal_count = 0;
        self.waf_count = 0;
        self.total_ms = 0;

        let line = "─".repeat(60);
        println!("  {} {}", "◆".cyan().bold(), username.white().bold());
        println!("  {}", line);
        Ok(())
    }

    fn write_result(&mut self, result: &QueryResult) -> Result<(), RavenError> {
        let show = match result.status {
            QueryStatus::Claimed => true,
            _ => self.print_all,
        };

        if !show {
            return Ok(());
        }

        if let Some(ms) = result.query_time_ms {
            self.total_ms += ms;
        }

        let time_str = result
            .query_time_ms
            .map(|ms| Self::format_time(ms))
            .unwrap_or_default();

        let http_str = result
            .http_status
            .map(|s| s.to_string())
            .unwrap_or_else(|| "--".to_string());

        match result.status {
            QueryStatus::Claimed => {
                self.claimed_count += 1;
                let icon = Self::status_icon(&result.status);
                println!(
                    " {}  {}  {}",
                    icon,
                    result.site_name.green().bold(),
                    result.site_url_user.cyan().underline(),
                );

                if self.verbose {
                    let label = format!("HTTP {}  {}", http_str, time_str);
                    let indent = "     └─";
                    println!("{} {}", indent, label);
                }

                if self.browse && !result.site_url_user.is_empty() {
                    if let Err(e) = webbrowser::open(&result.site_url_user) {
                        info!("Failed to open browser: {e}");
                    }
                }
            }
            QueryStatus::Available => {
                self.available_count += 1;
                let icon = Self::status_icon(&result.status);
                println!(
                    " {}  {}    {}",
                    icon,
                    result.site_name.yellow(),
                    "Not Found".yellow(),
                );
            }
            QueryStatus::Unknown => {
                self.unknown_count += 1;
                let icon = Self::status_icon(&result.status);
                let context = result.context.as_deref().unwrap_or("Unknown error");
                let label = format!("{}  {}", context, time_str);
                println!(
                    " {}  {}  {}",
                    icon,
                    result.site_name.red(),
                    label.red(),
                );
            }
            QueryStatus::Illegal => {
                self.illegal_count += 1;
                let icon = Self::status_icon(&result.status);
                println!(
                    " {}  {}    {}",
                    icon,
                    result.site_name.white(),
                    "Invalid username format".white(),
                );
            }
            QueryStatus::Waf => {
                self.waf_count += 1;
                let icon = Self::status_icon(&result.status);
                println!(
                    " {}  {}    {}",
                    icon,
                    result.site_name.red(),
                    "WAF Blocked".red(),
                );
            }
        }

        Ok(())
    }

    fn write_search_complete(&mut self, results: &SearchResults) -> Result<(), RavenError> {
        let total = self.claimed_count
            + self.available_count
            + self.unknown_count
            + self.illegal_count
            + self.waf_count;

        let line = "─".repeat(60);
        println!();
        println!("  {}", line);

        if total > 0 {
            let claimed_pct = if total > 0 {
                self.claimed_count as f64 / total as f64 * 100.0
            } else {
                0.0
            };
            let bar_len: usize = 30;
            let filled = (claimed_pct / 100.0 * bar_len as f64).round() as usize;
            let bar = format!("{}{}", "▓".repeat(filled).green(), "░".repeat(bar_len.saturating_sub(filled)));
            println!("  {}  {} {:>3.0}%", "Found".green().bold(), bar, claimed_pct);
        }

        if self.verbose || self.print_all {
            let mut parts: Vec<String> = Vec::new();
            if self.claimed_count > 0 {
                parts.push(format!("{} {}", self.claimed_count.to_string().green().bold(), "found".green()));
            }
            if self.available_count > 0 {
                parts.push(format!("{} {}", self.available_count.to_string().yellow().bold(), "available".yellow()));
            }
            if self.unknown_count > 0 {
                parts.push(format!("{} {}", self.unknown_count.to_string().red().bold(), "unknown".red()));
            }
            if self.illegal_count > 0 {
                parts.push(format!("{} {}", self.illegal_count.to_string().white().bold(), "invalid".white()));
            }
            if self.waf_count > 0 {
                parts.push(format!("{} {}", self.waf_count.to_string().red().bold(), "WAF".red()));
            }
            if !parts.is_empty() {
                let label = format!("  Breakdown: {}", parts.join(", "));
                println!("{}", label);
            }
        }

        let total_label = format!("{} sites checked", results.total_sites);
        println!("  {}  {}", "Total:", total_label.white().bold());
        println!();

        Ok(())
    }

    fn finish(&mut self) -> Result<(), RavenError> {
        Ok(())
    }
}
