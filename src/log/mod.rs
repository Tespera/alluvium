//! Per-archive audit trail.
//!
//! Writes one line of JSONL to `<data>/alluvium/log/archive.jsonl` per
//! archive run (success or failure). [`status`] reads these to render the
//! recent-activity table for `alluvium status`.

pub mod status;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArchiveLogEntry {
    pub session_id: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub touched_pages: Vec<PathBuf>,
    /// `None` on success; `Some(msg)` on failure.
    pub error: Option<String>,
    /// Optional cost-tracking fields populated when distillation succeeded.
    #[serde(default)]
    pub input_tokens: Option<u64>,
    #[serde(default)]
    pub output_tokens: Option<u64>,
}

/// Default path for the archive log: `<data_dir>/log/archive.jsonl`.
pub fn default_log_path() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("dev", "alluvium", "alluvium")
        .context("no home directory available; cannot locate data dir")?;
    Ok(dirs.data_dir().join("log").join("archive.jsonl"))
}

/// Append one entry to the archive log file. Creates parent dirs if missing.
/// Append is line-atomic (single write of "<json>\n").
pub fn record(log_path: &Path, entry: &ArchiveLogEntry) -> Result<()> {
    if let Some(parent) = log_path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating log parent dir {}", parent.display()))?;
        }
    }
    let line = serde_json::to_string(entry).context("serializing archive log entry")?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .with_context(|| format!("opening archive log {}", log_path.display()))?;
    writeln!(file, "{line}")
        .with_context(|| format!("appending to archive log {}", log_path.display()))?;
    Ok(())
}

/// Read at most `n` most recent entries from the log (oldest first).
/// Malformed lines are skipped with a warning.
pub fn read_recent(log_path: &Path, n: usize) -> Result<Vec<ArchiveLogEntry>> {
    if !log_path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(log_path)
        .with_context(|| format!("reading archive log {}", log_path.display()))?;
    let lines: Vec<&str> = content.lines().collect();
    let take_from = lines.len().saturating_sub(n);
    let mut entries = Vec::with_capacity(n);
    for (idx, line) in lines.iter().enumerate().skip(take_from) {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<ArchiveLogEntry>(line) {
            Ok(e) => entries.push(e),
            Err(err) => {
                tracing::warn!(
                    line = idx + 1,
                    error = %err,
                    "skipping malformed archive log entry"
                );
            }
        }
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn make_entry(session: &str, error: Option<&str>) -> ArchiveLogEntry {
        ArchiveLogEntry {
            session_id: session.into(),
            started_at: Utc.with_ymd_and_hms(2026, 5, 9, 14, 30, 0).unwrap(),
            finished_at: Utc.with_ymd_and_hms(2026, 5, 9, 14, 30, 5).unwrap(),
            touched_pages: vec![PathBuf::from("wiki/concepts/x.md")],
            error: error.map(String::from),
            input_tokens: Some(1000),
            output_tokens: Some(200),
        }
    }

    #[test]
    fn record_to_fresh_log_creates_file_and_parent_dir() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("nested/log/archive.jsonl");
        record(&log_path, &make_entry("s1", None)).unwrap();
        assert!(log_path.exists());
    }

    #[test]
    fn record_appends_and_does_not_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("archive.jsonl");
        record(&log_path, &make_entry("s1", None)).unwrap();
        record(&log_path, &make_entry("s2", None)).unwrap();
        record(&log_path, &make_entry("s3", Some("oops"))).unwrap();
        let content = std::fs::read_to_string(&log_path).unwrap();
        assert_eq!(content.lines().count(), 3);
        assert!(content.contains("\"s1\""));
        assert!(content.contains("\"s3\""));
    }

    #[test]
    fn read_recent_returns_n_most_recent_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("archive.jsonl");
        for i in 0..10 {
            record(&log_path, &make_entry(&format!("s{i}"), None)).unwrap();
        }
        let recent = read_recent(&log_path, 3).unwrap();
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].session_id, "s7");
        assert_eq!(recent[1].session_id, "s8");
        assert_eq!(recent[2].session_id, "s9");
    }

    #[test]
    fn read_recent_with_n_larger_than_log_returns_all() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("archive.jsonl");
        record(&log_path, &make_entry("only", None)).unwrap();
        let recent = read_recent(&log_path, 100).unwrap();
        assert_eq!(recent.len(), 1);
    }

    #[test]
    fn read_recent_missing_file_returns_empty() {
        let recent = read_recent(Path::new("/nonexistent/archive.jsonl"), 5).unwrap();
        assert!(recent.is_empty());
    }

    #[test]
    fn read_recent_skips_malformed_lines() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("archive.jsonl");
        record(&log_path, &make_entry("good", None)).unwrap();
        // Manually append garbage.
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&log_path)
            .unwrap();
        writeln!(f, "this is not json").unwrap();
        record(&log_path, &make_entry("good2", None)).unwrap();
        let recent = read_recent(&log_path, 10).unwrap();
        assert_eq!(recent.len(), 2);
        assert!(recent.iter().any(|e| e.session_id == "good"));
        assert!(recent.iter().any(|e| e.session_id == "good2"));
    }

    #[test]
    fn round_trip_via_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("archive.jsonl");
        let original = make_entry("s1", Some("err msg"));
        record(&log_path, &original).unwrap();
        let read = read_recent(&log_path, 1).unwrap();
        assert_eq!(read[0], original);
    }

    #[test]
    fn default_log_path_is_under_data_dir_and_ends_with_archive_jsonl() {
        let path = default_log_path().unwrap();
        assert!(path.to_string_lossy().ends_with("archive.jsonl"));
    }
}
