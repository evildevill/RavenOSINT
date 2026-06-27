mod banner;
mod cli;
mod client;
mod config;
mod database;
mod detector;
mod engine;
mod error;
mod filter;
mod manifest;
mod rate_limiter;
mod reporter;
mod tor_controller;
mod types;
mod update_check;
mod web;

use std::collections::HashSet;
use std::io::BufRead;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use clap::CommandFactory;
use clap_complete::{generate, Shell};
use colored::Colorize;
use tokio::signal;
use tracing::{debug, info};
use tracing_subscriber::EnvFilter;

use cli::Cli;
use config::Config;
use database::ScanDb;
use error::RavenError;
use filter::{filter_sites, load_exclusions};
use manifest::Manifest;
use rate_limiter::RateLimiter;
use reporter::*;

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), RavenError> {
    let config = Config::load();
    let cli = Cli::new_with_config(config);

    if std::env::var("RUST_LOG").is_err() {
        if cli.verbose {
            std::env::set_var("RUST_LOG", "debug");
        } else {
            std::env::set_var("RUST_LOG", "info");
        }
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .compact()
        .init();

    debug!("CLI args: {cli:#?}");

    if !cli.update_manifest && cli.completions.is_none() && cli.history.is_none() {
        banner::print_banner(cli.no_color);
    }

    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_signal = shutdown.clone();
    tokio::spawn(async move {
        signal::ctrl_c().await.ok();
        eprintln!("\nReceived Ctrl+C, shutting down gracefully...");
        shutdown_signal.store(true, Ordering::Relaxed);
    });

    if let Some(shell) = &cli.completions {
        generate_completions(shell)?;
        return Ok(());
    }

    if cli.serve {
        let addr = format!("{}:{}", cli.host, cli.port);
        info!("Starting web UI server on http://{addr}");
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        axum::serve(listener, web::router()).await?;
        return Ok(());
    }

    if let Some(filter) = cli.history.as_ref() {
        let db = ScanDb::open()?;
        let username = if filter.is_empty() { None } else { Some(filter.as_str()) };
        let scans = db.list_scans(username)?;
        if scans.is_empty() {
            println!("No scan history found.");
            return Ok(());
        }
        for scan in &scans {
            let completed = scan.completed_at.as_deref().unwrap_or("-");
            println!(
                "  #{:<4} {:<20} {}  total={:<4} claimed={:<4} avail={:<4} unknown={:<4} illegal={:<4} waf={:<4}",
                scan.id,
                scan.username,
                completed,
                scan.total_sites,
                scan.claimed,
                scan.available,
                scan.unknown,
                scan.illegal,
                scan.waf,
            );
        }
        return Ok(());
    }

    let proxy_url = resolve_proxy(&cli);
    let http_client = client::create_http_client(cli.timeout, proxy_url.as_deref())?;

    update_check::check_for_update(&http_client, cli.no_color).await;

    if cli.update_manifest {
        Manifest::update_manifest(&http_client, types::DEFAULT_MANIFEST_REMOTE_URL).await?;
        return Ok(());
    }

    let manifest = if let Some(ref json_source) = cli.json {
        info!("Loading custom manifest from '{json_source}'");
        Manifest::load_custom(&http_client, json_source).await?
    } else {
        Manifest::load_default(&http_client, cli.local).await?
    };
    info!("Loaded {} sites from manifest", manifest.len());

    let exclusions: HashSet<String> = if cli.ignore_exclusions {
        debug!("Exclusions disabled by --ignore-exclusions");
        HashSet::new()
    } else {
        load_exclusions(&http_client).await
    };

    let sites = filter_sites(
        manifest.sites,
        cli.nsfw,
        &cli.site_list,
        &exclusions,
        &cli.tag,
    );

    if sites.is_empty() {
        return Err(RavenError::Cli(
            "No sites to search after filtering".to_string(),
        ));
    }

    info!(
        "Searching {} sites{}",
        sites.len(),
        if cli.nsfw { " (incl. NSFW)" } else { "" }
    );

    let concurrency = cli.effective_concurrency();
    let retry_count = cli.retry;
    let rate_limiter = cli.rate_limit.map(RateLimiter::new);

    debug!("Using concurrency of {concurrency}, retry={retry_count}");

    let usernames = resolve_usernames(&cli)?;
    let total_start = Instant::now();
    let mut all_results: Vec<types::SearchResults> = Vec::new();

    if let Some(ref cron_expr) = cli.schedule {
        let schedule = parse_cron(cron_expr)?;
        info!("Cron schedule: {cron_expr} — will re-scan and report new findings");
        let db = ScanDb::open()?;
        loop {
            if shutdown.load(Ordering::Relaxed) {
                break;
            }
            info!("Starting scheduled scan...");
            for username in &usernames {
                if shutdown.load(Ordering::Relaxed) {
                    break;
                }
                let prev = db.get_last_scan(username)?;
                let prev_results = match prev {
                    Some(ref record) => db.get_last_scan_results(record.id).ok().unwrap_or_default(),
                    None => vec![],
                };

                let results = engine::search_username(
                    username,
                    &sites,
                    &http_client,
                    concurrency,
                    retry_count,
                    rate_limiter.clone(),
                    cli.dump_response,
                    cli.unique_tor,
                    shutdown.clone(),
                    cli.print_all,
                    cli.verbose,
                    cli.browse,
                )
                .await?;

                write_reports(&cli, username, &results)?;
                let scan_id = db.save_scan(username, &results)?;
                info!("Scan #{scan_id} saved for '{username}'");

                let new_sites = database::find_new_results(&prev_results, &results);
                if new_sites.is_empty() {
                    println!("  [{}] No new findings.", username.green());
                } else {
                    println!("  [{}] {} new site(s) found!", username.green(), new_sites.len());
                    for r in &new_sites {
                        let status_color = match r.status {
                            types::QueryStatus::Claimed => "✓".green(),
                            types::QueryStatus::Available => "◻".yellow(),
                            types::QueryStatus::Unknown => "?".dimmed(),
                            types::QueryStatus::Illegal => "✗".red(),
                            types::QueryStatus::Waf => "⚠".red(),
                        };
                        println!("    {status_color} {} — {}", r.site_name.white().bold(), r.site_url_user.dimmed());
                    }
                }
                all_results.push(results);
            }
            print_performance_summary(&all_results, total_start.elapsed().as_millis() as u64);

            let next = schedule.upcoming(chrono::Utc).next();
            match next {
                Some(t) => {
                    let delay = t
                        .signed_duration_since(chrono::Utc::now())
                        .to_std()
                        .unwrap_or(std::time::Duration::from_secs(60));
                    let mins = delay.as_secs() / 60;
                    info!("Next scan at {t} ({mins} min from now)");
                    tokio::select! {
                        _ = tokio::time::sleep(delay) => {},
                        _ = tokio::signal::ctrl_c() => {
                            eprintln!("\nReceived Ctrl+C, exiting scheduler.");
                            break;
                        },
                    }
                }
                None => {
                    info!("No future schedule time, exiting.");
                    break;
                }
            }
        }
        return Ok(());
    }

    for username in &usernames {
        if shutdown.load(Ordering::Relaxed) {
            info!("Shutdown requested, skipping remaining usernames");
            break;
        }

        let results = engine::search_username(
            username,
            &sites,
            &http_client,
            concurrency,
            retry_count,
            rate_limiter.clone(),
            cli.dump_response,
            cli.unique_tor,
            shutdown.clone(),
            cli.print_all,
            cli.verbose,
            cli.browse,
        )
        .await?;

        write_reports(&cli, username, &results)?;

        // Save to scan history database
        if let Ok(db) = ScanDb::open() {
            if let Ok(scan_id) = db.save_scan(username, &results) {
                debug!("Scan #{scan_id} saved to history for '{username}'");
            }
        }

        all_results.push(results);
    }

    let total_elapsed = total_start.elapsed().as_millis() as u64;

    let total_claimed: usize = all_results.iter().map(|r| r.claimed_count).sum();
    let total_searched: usize = all_results.iter().map(|r| r.total_sites).sum();

    print_performance_summary(&all_results, total_elapsed);

    info!(
        "Done. Searched {} users across {} sites. Total claimed: {total_claimed}",
        usernames.len(),
        total_searched
    );

    Ok(())
}

fn print_performance_summary(results: &[types::SearchResults], total_time_ms: u64) {
    if results.is_empty() {
        return;
    }

    let all_results: Vec<&types::QueryResult> = results.iter()
        .flat_map(|r| r.results.iter())
        .filter(|r| r.query_time_ms.is_some())
        .collect();

    if all_results.is_empty() {
        return;
    }

    let total_resp: u64 = all_results.iter().filter_map(|r| r.query_time_ms).sum();
    let avg = total_resp as f64 / all_results.len() as f64;

    let slowest = all_results.iter()
        .filter_map(|r| r.query_time_ms.map(|t| (t, r.site_name.clone())))
        .max_by_key(|(t, _)| *t);

    let fastest = all_results.iter()
        .filter_map(|r| r.query_time_ms.map(|t| (t, r.site_name.clone())))
        .min_by_key(|(t, _)| *t);

    println!("  {}", "─".repeat(60).dimmed());
    println!(
        "  {} {:>9}  {} {:>7.0}ms",
        "Total time:".dimmed(),
        if total_time_ms >= 1000 {
            format!("{:.1}s", total_time_ms as f64 / 1000.0)
        } else {
            format!("{total_time_ms}ms")
        }.white().bold(),
        "Avg response:".dimmed(),
        avg,
    );
    if let Some((t, ref name)) = fastest {
        println!(
            "  {} {:>7}ms ({})",
            "Fastest:".dimmed(),
            t.to_string().green(),
            name.green(),
        );
    }
    if let Some((t, ref name)) = slowest {
        println!(
            "  {} {:>7}ms ({})",
            "Slowest:".dimmed(),
            t.to_string().red(),
            name.red(),
        );
    }
    println!("  {}", "─".repeat(60).dimmed());
    println!();
}

fn resolve_usernames(cli: &Cli) -> Result<Vec<String>, RavenError> {
    let mut usernames = cli.usernames.clone();

    if let Some(ref path) = cli.usernames_file {
        let file = std::fs::File::open(path)
            .map_err(|e| RavenError::Cli(format!("Failed to open usernames file '{path}': {e}")))?;
        let reader = std::io::BufReader::new(file);
        for line in reader.lines() {
            let line = line.map_err(|e| {
                RavenError::Cli(format!("Failed to read usernames file: {e}"))
            })?;
            let trimmed = line.trim().to_string();
            if !trimmed.is_empty() {
                usernames.push(trimmed);
            }
        }
        if usernames.is_empty() {
            return Err(RavenError::Cli(
                "No usernames found in file or command line".to_string(),
            ));
        }
        info!(
            "Loaded {} usernames from file + {} from CLI",
            cli.usernames_file.as_ref().map_or(0, |_| {
                usernames.len() - cli.usernames.len()
            }),
            cli.usernames.len()
        );
    }

    if usernames.is_empty() {
        return Err(RavenError::Cli(
            "No usernames provided. Use --help for usage.".to_string(),
        ));
    }

    Ok(usernames)
}

fn resolve_proxy(cli: &Cli) -> Option<String> {
    if cli.unique_tor || cli.tor {
        let tor_proxy = Some("socks5://127.0.0.1:9050".to_string());
        if cli.proxy.is_some() && cli.proxy.as_deref() != Some("socks5://127.0.0.1:9050") {
            info!("--tor overrides proxy setting to socks5://127.0.0.1:9050");
        }
        return tor_proxy;
    }
    cli.proxy.clone()
}

fn generate_completions(shell_str: &str) -> Result<(), RavenError> {
    let shell = match shell_str.to_lowercase().as_str() {
        "bash" => Shell::Bash,
        "zsh" => Shell::Zsh,
        "fish" => Shell::Fish,
        "powershell" => Shell::PowerShell,
        "elvish" => Shell::Elvish,
        other => {
            return Err(RavenError::Cli(format!(
                "Unknown shell '{other}'. Supported: bash, zsh, fish, powershell, elvish"
            )));
        }
    };

    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    generate(shell, &mut cmd, name, &mut std::io::stdout());
    Ok(())
}

fn parse_cron(expr: &str) -> Result<cron::Schedule, RavenError> {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    let full_expr = if parts.len() == 5 {
        format!("0 {expr}")
    } else if parts.len() == 6 {
        expr.to_string()
    } else {
        return Err(RavenError::Cli(
            "Cron expression must have 5 or 6 fields (e.g. \"0 */6 * * *\")".to_string(),
        ));
    };
    full_expr
        .parse::<cron::Schedule>()
        .map_err(|e| RavenError::Cli(format!("Invalid cron expression '{expr}': {e}")))
}

fn write_reports(cli: &Cli, username: &str, results: &types::SearchResults) -> Result<(), RavenError> {
    let mut reporters = Reporters::new();
    let result_path = resolve_output_path(cli, username);
    let has_export = cli.csv.is_some() || cli.xlsx.is_some() || cli.txt.is_some() || cli.json_report.is_some();

    if let Some(path_override) = &cli.csv {
        let path = if path_override.is_empty() {
            with_extension(&result_path, "csv")
        } else {
            PathBuf::from(path_override)
        };
        reporters.add(CsvReporter::new(path, cli.print_all));
    }

    if let Some(path_override) = &cli.xlsx {
        let path = if path_override.is_empty() {
            with_extension(&result_path, "xlsx")
        } else {
            PathBuf::from(path_override)
        };
        reporters.add(XlsxReporter::new(path, cli.print_all));
    }

    if let Some(path_override) = &cli.txt {
        let path = if path_override.is_empty() {
            with_extension(&result_path, "txt")
        } else {
            PathBuf::from(path_override)
        };
        reporters.add(TxtReporter::new(path));
    }

    if let Some(path_override) = &cli.json_report {
        let path = if path_override.is_empty() {
            with_extension(&result_path, "json")
        } else {
            PathBuf::from(path_override)
        };
        reporters.add(JsonReporter::new(path, cli.print_all));
    }

    if !has_export && (cli.output.is_some() || cli.folderoutput.is_some()) {
        reporters.add(TxtReporter::new(result_path));
    }

    reporters.write_search_start(username)?;

    for result in &results.results {
        reporters.write_result(result)?;
    }

    reporters.write_search_complete(results)?;
    reporters.finish()?;

    Ok(())
}

fn resolve_output_path(cli: &Cli, username: &str) -> PathBuf {
    if let Some(ref output) = cli.output {
        PathBuf::from(output)
    } else if let Some(ref folder) = cli.folderoutput {
        std::fs::create_dir_all(folder).ok();
        PathBuf::from(folder).join(username)
    } else {
        PathBuf::from(username)
    }
}

fn with_extension(path: &PathBuf, ext: &str) -> PathBuf {
    let mut p = path.clone();
    match p.extension() {
        Some(_) => {
            let stem = p.file_stem().unwrap_or_default().to_string_lossy().to_string();
            p.set_file_name(format!("{stem}.{ext}"));
        }
        None => {
            p.set_extension(ext);
        }
    }
    p
}
