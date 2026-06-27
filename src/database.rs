use std::path::PathBuf;

use chrono::Utc;
use rusqlite::{params, Connection};

use crate::error::RavenError;
use crate::types::*;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ScanRecord {
    pub id: i64,
    pub username: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub total_sites: i64,
    pub claimed: i64,
    pub available: i64,
    pub unknown: i64,
    pub illegal: i64,
    pub waf: i64,
}

pub struct ScanDb {
    conn: Connection,
}

impl ScanDb {
    pub fn open() -> Result<Self, RavenError> {
        let path = db_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&path)?;
        let db = ScanDb { conn };
        db.initialize()?;
        Ok(db)
    }

    fn initialize(&self) -> Result<(), RavenError> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS scans (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                username TEXT NOT NULL,
                started_at TEXT NOT NULL,
                completed_at TEXT,
                total_sites INTEGER NOT NULL DEFAULT 0,
                claimed INTEGER NOT NULL DEFAULT 0,
                available INTEGER NOT NULL DEFAULT 0,
                unknown INTEGER NOT NULL DEFAULT 0,
                illegal INTEGER NOT NULL DEFAULT 0,
                waf INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS scan_results (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                scan_id INTEGER NOT NULL,
                site_name TEXT NOT NULL,
                site_url TEXT NOT NULL,
                status TEXT NOT NULL,
                http_status INTEGER,
                response_time_ms INTEGER,
                probe_url TEXT,
                FOREIGN KEY (scan_id) REFERENCES scans(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_scan_results_scan_id ON scan_results(scan_id);
            CREATE INDEX IF NOT EXISTS idx_scans_username ON scans(username);",
        )?;
        Ok(())
    }

    pub fn save_scan(&self, username: &str, results: &SearchResults) -> Result<i64, RavenError> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO scans (username, started_at, completed_at, total_sites, claimed, available, unknown, illegal, waf)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                username,
                now,
                now,
                results.total_sites as i64,
                results.claimed_count as i64,
                results.available_count as i64,
                results.unknown_count as i64,
                results.illegal_count as i64,
                results.waf_count as i64,
            ],
        )?;
        let scan_id = self.conn.last_insert_rowid();

        let mut stmt = self.conn.prepare(
            "INSERT INTO scan_results (scan_id, site_name, site_url, status, http_status, response_time_ms, probe_url)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;

        for r in &results.results {
            stmt.execute(params![
                scan_id,
                r.site_name,
                r.site_url_user,
                r.status.to_string(),
                r.http_status,
                r.query_time_ms,
                r.probe_url,
            ])?;
        }

        Ok(scan_id)
    }

    pub fn get_last_scan(&self, username: &str) -> Result<Option<ScanRecord>, RavenError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, username, started_at, completed_at, total_sites, claimed, available, unknown, illegal, waf
             FROM scans WHERE username = ?1 ORDER BY id DESC LIMIT 1",
        )?;

        let mut rows = stmt.query_map(params![username], |row| {
            Ok(ScanRecord {
                id: row.get(0)?,
                username: row.get(1)?,
                started_at: row.get(2)?,
                completed_at: row.get(3)?,
                total_sites: row.get(4)?,
                claimed: row.get(5)?,
                available: row.get(6)?,
                unknown: row.get(7)?,
                illegal: row.get(8)?,
                waf: row.get(9)?,
            })
        })?;

        match rows.next() {
            Some(Ok(record)) => Ok(Some(record)),
            _ => Ok(None),
        }
    }

    pub fn get_last_scan_results(&self, scan_id: i64) -> Result<Vec<SimpleResult>, RavenError> {
        let mut stmt = self.conn.prepare(
            "SELECT site_name, site_url, status FROM scan_results WHERE scan_id = ?1",
        )?;

        let results = stmt.query_map(params![scan_id], |row| {
            let status_str: String = row.get(2)?;
            let status: QueryStatus = status_str
                .parse()
                .unwrap_or(QueryStatus::Unknown);
            Ok(SimpleResult {
                site_name: row.get(0)?,
                site_url: row.get(1)?,
                status,
            })
        })?;

        results.collect::<Result<Vec<_>, _>>().map_err(RavenError::from)
    }

    pub fn list_scans(&self, username: Option<&str>) -> Result<Vec<ScanRecord>, RavenError> {
        let mut stmt = if let Some(_) = username {
            self.conn.prepare(
                "SELECT id, username, started_at, completed_at, total_sites, claimed, available, unknown, illegal, waf
                 FROM scans WHERE username = ?1 ORDER BY id DESC LIMIT 50",
            )?
        } else {
            self.conn.prepare(
                "SELECT id, username, started_at, completed_at, total_sites, claimed, available, unknown, illegal, waf
                 FROM scans ORDER BY id DESC LIMIT 50",
            )?
        };

        let rows = if let Some(u) = username {
            stmt.query_map(params![u], map_scan_row)?
        } else {
            stmt.query_map([], map_scan_row)?
        };

        rows.collect::<Result<Vec<_>, _>>().map_err(RavenError::from)
    }
}

fn map_scan_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ScanRecord> {
    Ok(ScanRecord {
        id: row.get(0)?,
        username: row.get(1)?,
        started_at: row.get(2)?,
        completed_at: row.get(3)?,
        total_sites: row.get(4)?,
        claimed: row.get(5)?,
        available: row.get(6)?,
        unknown: row.get(7)?,
        illegal: row.get(8)?,
        waf: row.get(9)?,
    })
}

fn db_path() -> PathBuf {
    let proj_dirs =
        directories::ProjectDirs::from("", "", "raven").expect("Cannot determine config directory");
    let mut path = proj_dirs.data_dir().to_path_buf();
    path.push("scans.db");
    path
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SimpleResult {
    pub site_name: String,
    pub site_url: String,
    pub status: QueryStatus,
}

pub fn find_new_results<'a>(
    previous: &'a [SimpleResult],
    current: &'a SearchResults,
) -> Vec<&'a QueryResult> {
    let prev_names: std::collections::HashSet<&str> =
        previous.iter().map(|r| r.site_name.as_str()).collect();

    current
        .results
        .iter()
        .filter(|r| !prev_names.contains(r.site_name.as_str()))
        .collect()
}
