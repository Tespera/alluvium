//! PreCompact hook handler.
//!
//! Snapshots the current `transcript_path` (Claude Code's session JSONL)
//! into `<cache>/sessions/<id>/snapshots/NNNN.jsonl` so original detail
//! is preserved across context compactions. The archive worker (Stop hook)
//! will merge these snapshots back with the final transcript.
//!
//! Must return within 100 ms (per docs/HOOKS.md).

use anyhow::{Context, Result};

use crate::cli::paths;
use crate::hook::payload;

pub async fn run() -> Result<()> {
    let p = payload::read_from_stdin()?;

    let snapshots_dir = paths::session_snapshots_dir(&p.session_id)?;
    if !snapshots_dir.exists() {
        std::fs::create_dir_all(&snapshots_dir)
            .with_context(|| format!("creating snapshots dir {}", snapshots_dir.display()))?;
    }

    // Pick next numbered slot: count existing .jsonl files.
    let n = next_snapshot_number(&snapshots_dir)?;
    let dst = snapshots_dir.join(format!("{n:04}.jsonl"));

    // Copy transcript_path → dst. Use a plain copy (we need to read the
    // file as Claude Code is writing it; std::fs::copy is fine for a
    // point-in-time snapshot).
    if !p.transcript_path.exists() {
        // Nothing to snapshot yet (transcript not flushed). Not an error.
        return Ok(());
    }
    std::fs::copy(&p.transcript_path, &dst).with_context(|| {
        format!(
            "copying {} → {}",
            p.transcript_path.display(),
            dst.display()
        )
    })?;
    Ok(())
}

fn next_snapshot_number(dir: &std::path::Path) -> Result<u32> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(1),
    };
    let mut max_n: u32 = 0;
    for e in entries.flatten() {
        let name = e.file_name();
        let s = name.to_string_lossy();
        if let Some(stem) = s.strip_suffix(".jsonl") {
            if let Ok(n) = stem.parse::<u32>() {
                if n > max_n {
                    max_n = n;
                }
            }
        }
    }
    Ok(max_n + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_snapshot_number_starts_at_1_for_empty_or_missing_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(next_snapshot_number(dir.path()).unwrap(), 1);
    }

    #[test]
    fn next_snapshot_number_one_more_than_max_existing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("0001.jsonl"), "").unwrap();
        std::fs::write(dir.path().join("0003.jsonl"), "").unwrap();
        std::fs::write(dir.path().join("README.md"), "").unwrap();
        assert_eq!(next_snapshot_number(dir.path()).unwrap(), 4);
    }
}
