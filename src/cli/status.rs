//! `alluvium status` — show recent archive activity.
//!
//! Reads `<data>/log/archive.jsonl`, takes the last N entries, and renders
//! them via [`crate::log::status::render`].

use anyhow::Result;

use crate::cli::paths;
use crate::log::{read_recent, status::render};

const DEFAULT_N: usize = 10;

pub async fn run() -> Result<()> {
    let log_path = paths::archive_log_path()?;
    let entries = read_recent(&log_path, DEFAULT_N)?;
    let now = chrono::Utc::now();
    print!("{}", render(&entries, now));
    Ok(())
}
