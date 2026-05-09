//! Merge PreCompact snapshots with the final session JSONL.
//!
//! When Claude Code triggers context compaction mid-session, the final
//! transcript JSONL retains only post-compaction summaries. The PreCompact
//! hook captures snapshots of the original detail before each compaction,
//! stored at `<cache>/sessions/<id>/snapshots/NNNN.jsonl`. This module
//! stitches them back together so the distiller sees the full
//! pre-compaction conversation.
//!
//! ## Strategy
//!
//! 1. Read all `.jsonl` files in `snapshots_dir`. Filenames are
//!    zero-padded sequence numbers (`0001.jsonl`, `0002.jsonl`, …), so
//!    alphabetical sort = chronological order.
//! 2. Read the final transcript JSONL.
//! 3. Combine, deduping events by `uuid`. Earliest snapshot wins —
//!    the older a source, the closer it is to the original
//!    pre-compaction detail. Final transcript only contributes events
//!    that no snapshot has (i.e. events created after the last compaction).
//! 4. Events without a `uuid` (mostly bookkeeping types) are kept
//!    without dedup; reconstruct filters them out anyway.
//! 5. Sort by timestamp; events without timestamp drop to the end.
//!
//! ## Failure modes
//!
//! - Missing `snapshots_dir` → treat as "no snapshots", return final as-is.
//! - Unreadable individual snapshot file → log + skip; remaining
//!   snapshots and final are still merged.
//! - Final transcript unreadable → propagate error (fatal — no archive
//!   without a transcript).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::transcript::jsonl::{self, Event};

/// Read `final_jsonl` and any `<snapshots_dir>/*.jsonl` snapshots, then merge
/// them into a single time-ordered Vec<Event>.
pub fn merge(final_jsonl: &Path, snapshots_dir: &Path) -> Result<Vec<Event>> {
    let final_events = jsonl::read(final_jsonl)
        .with_context(|| format!("reading final transcript {}", final_jsonl.display()))?;

    let snapshot_groups = read_snapshots(snapshots_dir)?;

    Ok(combine(final_events, snapshot_groups))
}

/// Enumerate `.jsonl` files in `dir` in alphabetical (chronological) order
/// and parse each. Unreadable files log a warning and are skipped.
fn read_snapshots(dir: &Path) -> Result<Vec<Vec<Event>>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("reading snapshots dir {}", dir.display()))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|e| e == "jsonl").unwrap_or(false))
        .collect();
    paths.sort();

    let mut groups = Vec::new();
    for path in paths {
        match jsonl::read(&path) {
            Ok(events) => groups.push(events),
            Err(err) => {
                tracing::warn!(
                    file = %path.display(),
                    error = %format!("{err:#}"),
                    "skipping unreadable snapshot",
                );
            }
        }
    }
    Ok(groups)
}

/// Merge final transcript events with snapshot groups (oldest-first ordering).
/// Pure data; testable without filesystem.
fn combine(final_events: Vec<Event>, snapshot_groups: Vec<Vec<Event>>) -> Vec<Event> {
    // For uuid-keyed events: first writer wins. Snapshots iterated
    // oldest-to-newest, so the OLDEST snapshot version of any uuid sticks
    // (closest to original pre-compaction detail). Final fills in any
    // uuids no snapshot saw (i.e. events after the last compaction).
    let mut by_uuid: HashMap<String, Event> = HashMap::new();
    let mut without_uuid: Vec<Event> = Vec::new();

    for snapshot in snapshot_groups {
        for event in snapshot {
            match event.uuid() {
                Some(uuid) => {
                    by_uuid.entry(uuid.to_string()).or_insert(event);
                }
                None => without_uuid.push(event),
            }
        }
    }

    for event in final_events {
        match event.uuid() {
            Some(uuid) => {
                by_uuid.entry(uuid.to_string()).or_insert(event);
            }
            None => without_uuid.push(event),
        }
    }

    let mut all: Vec<Event> = by_uuid.into_values().chain(without_uuid).collect();
    all.sort_by(|a, b| match (a.timestamp(), b.timestamp()) {
        (Some(at), Some(bt)) => at.cmp(bt),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    all
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn evt(json: &str) -> Event {
        serde_json::from_str(json).expect("test event JSON should parse")
    }

    fn write_jsonl(path: &Path, lines: &[&str]) {
        let mut f = std::fs::File::create(path).unwrap();
        for line in lines {
            writeln!(f, "{line}").unwrap();
        }
    }

    fn user_evt(uuid: &str, ts: Option<&str>, content: &str) -> String {
        let ts_field = ts
            .map(|t| format!(r#","timestamp":"{t}""#))
            .unwrap_or_default();
        format!(
            r#"{{"type":"user","sessionId":"S","uuid":"{uuid}"{ts_field},"message":{{"role":"user","content":"{content}"}}}}"#
        )
    }

    // ─────────────────── combine() unit tests ───────────────────

    #[test]
    fn combine_with_no_snapshots_returns_final_unchanged() {
        let final_evts = vec![evt(&user_evt("u1", Some("2026-04-23T16:00:00Z"), "hello"))];
        let merged = combine(final_evts, vec![]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].uuid(), Some("u1"));
    }

    #[test]
    fn combine_with_empty_final_returns_only_snapshots() {
        let snap = vec![evt(&user_evt(
            "u1",
            Some("2026-04-23T16:00:00Z"),
            "from snapshot",
        ))];
        let merged = combine(vec![], vec![snap]);
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn snapshot_wins_over_final_on_uuid_conflict() {
        // Same uuid in both. Snapshot has the original detail; final has
        // the post-compaction summary. Snapshot must win.
        let final_evts = vec![evt(&user_evt(
            "u1",
            Some("2026-04-23T16:00:00Z"),
            "summarized",
        ))];
        let snap = vec![evt(&user_evt(
            "u1",
            Some("2026-04-23T16:00:00Z"),
            "original detail",
        ))];
        let merged = combine(final_evts, vec![snap]);
        assert_eq!(merged.len(), 1);
        let Event::User(c) = &merged[0] else { panic!() };
        assert_eq!(c.message.content.as_str(), Some("original detail"));
    }

    #[test]
    fn earliest_snapshot_wins_when_uuid_in_multiple_snapshots() {
        // Older snapshot is closer to pre-compaction state.
        let snap_old = vec![evt(&user_evt(
            "u1",
            Some("2026-04-23T16:00:00Z"),
            "oldest version",
        ))];
        let snap_new = vec![evt(&user_evt(
            "u1",
            Some("2026-04-23T16:00:00Z"),
            "newer version (already partially compacted)",
        ))];
        let merged = combine(vec![], vec![snap_old, snap_new]);
        assert_eq!(merged.len(), 1);
        let Event::User(c) = &merged[0] else { panic!() };
        assert_eq!(c.message.content.as_str(), Some("oldest version"));
    }

    #[test]
    fn final_only_uuids_are_kept() {
        // Events that come after the last snapshot only exist in final.
        let snap = vec![evt(&user_evt("u1", Some("2026-04-23T16:00:00Z"), "snap"))];
        let final_evts = vec![
            evt(&user_evt(
                "u1",
                Some("2026-04-23T16:00:00Z"),
                "final-overlap",
            )),
            evt(&user_evt(
                "u2",
                Some("2026-04-23T17:00:00Z"),
                "post-compaction",
            )),
        ];
        let merged = combine(final_evts, vec![snap]);
        assert_eq!(merged.len(), 2);
        // u1 → snapshot wins; u2 → only in final, kept.
        let uuids: Vec<&str> = merged.iter().filter_map(|e| e.uuid()).collect();
        assert!(uuids.contains(&"u1"));
        assert!(uuids.contains(&"u2"));
    }

    #[test]
    fn events_without_uuid_are_kept_unduped() {
        // Bookkeeping events lack uuid; we keep all instances since we
        // can't safely dedup them.
        let bookkeeping = r#"{"type":"last-prompt","lastPrompt":"x","sessionId":"S"}"#;
        let final_evts = vec![evt(bookkeeping)];
        let snap = vec![evt(bookkeeping)];
        let merged = combine(final_evts, vec![snap]);
        assert_eq!(merged.len(), 2, "no dedup possible without uuid");
    }

    #[test]
    fn merged_events_sorted_by_timestamp() {
        // Out-of-order input.
        let final_evts = vec![
            evt(&user_evt("u3", Some("2026-04-23T18:00:00Z"), "third")),
            evt(&user_evt("u1", Some("2026-04-23T16:00:00Z"), "first")),
        ];
        let snap = vec![evt(&user_evt("u2", Some("2026-04-23T17:00:00Z"), "second"))];
        let merged = combine(final_evts, vec![snap]);
        assert_eq!(merged.len(), 3);
        let uuids: Vec<Option<&str>> = merged.iter().map(|e| e.uuid()).collect();
        assert_eq!(uuids, vec![Some("u1"), Some("u2"), Some("u3")]);
    }

    #[test]
    fn events_without_timestamp_sort_to_end() {
        let stamped = evt(&user_evt("u1", Some("2026-04-23T16:00:00Z"), "stamped"));
        let unstamped = evt(&user_evt("u2", None, "no timestamp"));
        let merged = combine(vec![stamped, unstamped], vec![]);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].uuid(), Some("u1"));
        assert_eq!(merged[1].uuid(), Some("u2"));
    }

    // ─────────────────── merge() integration tests (filesystem) ───────────────────

    #[test]
    fn merge_with_missing_snapshots_dir_succeeds() {
        let tmp = tempfile::tempdir().unwrap();
        let final_path = tmp.path().join("transcript.jsonl");
        write_jsonl(
            &final_path,
            &[&user_evt("u1", Some("2026-04-23T16:00:00Z"), "hi")],
        );

        let snapshots_dir = tmp.path().join("snapshots-that-dont-exist");
        let merged = merge(&final_path, &snapshots_dir).unwrap();
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn merge_with_empty_snapshots_dir_returns_final() {
        let tmp = tempfile::tempdir().unwrap();
        let final_path = tmp.path().join("transcript.jsonl");
        write_jsonl(
            &final_path,
            &[&user_evt("u1", Some("2026-04-23T16:00:00Z"), "hi")],
        );
        let snapshots_dir = tmp.path().join("snapshots");
        std::fs::create_dir(&snapshots_dir).unwrap();

        let merged = merge(&final_path, &snapshots_dir).unwrap();
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn merge_reads_snapshots_alphabetical_order() {
        let tmp = tempfile::tempdir().unwrap();
        let final_path = tmp.path().join("transcript.jsonl");
        write_jsonl(&final_path, &[]);

        let snapshots_dir = tmp.path().join("snapshots");
        std::fs::create_dir(&snapshots_dir).unwrap();

        // Same uuid in both snapshots, different content.
        write_jsonl(
            &snapshots_dir.join("0001.jsonl"),
            &[&user_evt(
                "u1",
                Some("2026-04-23T16:00:00Z"),
                "from-0001-oldest",
            )],
        );
        write_jsonl(
            &snapshots_dir.join("0002.jsonl"),
            &[&user_evt(
                "u1",
                Some("2026-04-23T16:00:00Z"),
                "from-0002-newer",
            )],
        );

        let merged = merge(&final_path, &snapshots_dir).unwrap();
        assert_eq!(merged.len(), 1);
        let Event::User(c) = &merged[0] else { panic!() };
        assert_eq!(c.message.content.as_str(), Some("from-0001-oldest"));
    }

    #[test]
    fn merge_skips_unreadable_snapshot_continues_with_others() {
        let tmp = tempfile::tempdir().unwrap();
        let final_path = tmp.path().join("transcript.jsonl");
        write_jsonl(
            &final_path,
            &[&user_evt("u3", Some("2026-04-23T18:00:00Z"), "final")],
        );

        let snapshots_dir = tmp.path().join("snapshots");
        std::fs::create_dir(&snapshots_dir).unwrap();

        // 0001 is OK.
        write_jsonl(
            &snapshots_dir.join("0001.jsonl"),
            &[&user_evt("u1", Some("2026-04-23T16:00:00Z"), "good")],
        );
        // 0002 contains nothing parseable as Event but is itself a valid file.
        // (It WILL be opened; jsonl::read will warn-and-skip every line.)
        write_jsonl(
            &snapshots_dir.join("0002.jsonl"),
            &[
                "this is garbage",
                "also not json",
                r#"{"type":"future-unknown","sessionId":"S"}"#,
            ],
        );
        // 0003 is OK.
        write_jsonl(
            &snapshots_dir.join("0003.jsonl"),
            &[&user_evt("u2", Some("2026-04-23T17:00:00Z"), "good too")],
        );

        let merged = merge(&final_path, &snapshots_dir).unwrap();
        assert_eq!(merged.len(), 3);
        let uuids: Vec<&str> = merged.iter().filter_map(|e| e.uuid()).collect();
        assert!(uuids.contains(&"u1"));
        assert!(uuids.contains(&"u2"));
        assert!(uuids.contains(&"u3"));
    }

    #[test]
    fn merge_propagates_error_when_final_transcript_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("does-not-exist.jsonl");
        let snapshots_dir = tmp.path().join("snapshots");
        let err = merge(&missing, &snapshots_dir).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("reading final transcript") || msg.contains("does-not-exist"));
    }

    #[test]
    fn merge_ignores_non_jsonl_files_in_snapshots_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let final_path = tmp.path().join("transcript.jsonl");
        write_jsonl(
            &final_path,
            &[&user_evt("u1", Some("2026-04-23T16:00:00Z"), "hi")],
        );

        let snapshots_dir = tmp.path().join("snapshots");
        std::fs::create_dir(&snapshots_dir).unwrap();
        write_jsonl(&snapshots_dir.join("README.md"), &["# notes"]);
        write_jsonl(&snapshots_dir.join(".DS_Store"), &["macos junk"]);

        let merged = merge(&final_path, &snapshots_dir).unwrap();
        assert_eq!(merged.len(), 1, "should only read .jsonl files");
    }
}
