//! Append a single line to `wiki/log.md`.
//!
//! Format: `- HH:MM <session title> → [[touched-pages]]`. Entries are grouped
//! under `## YYYY-MM-DD` date headings. New entry on an existing date appends
//! to that section; a new date creates a new heading at the end.
//!
//! **Append-only.** Existing lines are never modified — users can hand-write
//! their own notes anywhere; we treat all non-Alluvium content as theirs.

use anyhow::Result;
use chrono::{DateTime, Utc};
use std::path::Path;

const DEFAULT_HEADER: &str = "# Log\n\nAppend-only chronological ledger. Alluvium only appends; it never modifies existing lines. You can handwrite your own notes here — non-Alluvium content is yours.\n\nEntry format Alluvium uses: `- HH:MM <session title> → [[touched-pages]]`.\n\n";

/// Append one entry to `log_path`. File is created (with header) if absent.
pub fn append(
    log_path: &Path,
    now: &DateTime<Utc>,
    session_title: &str,
    touched_pages: &[String],
) -> Result<()> {
    let date_heading = format!("## {}", now.format("%Y-%m-%d"));
    let pages_links: Vec<String> = touched_pages.iter().map(|p| format!("[[{p}]]")).collect();
    let entry = format!(
        "- {time} {title} → {pages}",
        time = now.format("%H:%M"),
        title = session_title,
        pages = pages_links.join(" ")
    );

    let existing = std::fs::read_to_string(log_path).unwrap_or_else(|_| DEFAULT_HEADER.to_string());

    let new_content = insert_entry(&existing, &date_heading, &entry);
    super::writer::write_atomic(log_path, &new_content)
}

/// Insert `entry` under `date_heading`. If the heading is missing, append a
/// new date section at the end. Pure function for testability.
fn insert_entry(existing: &str, date_heading: &str, entry: &str) -> String {
    match find_heading_at_line_start(existing, date_heading) {
        Some(heading_idx) => {
            // Date section exists. Find its end (next "\n## " or EOF).
            let after_heading = heading_idx + date_heading.len();
            let section_end = existing[after_heading..]
                .find("\n## ")
                .map(|i| after_heading + i)
                .unwrap_or(existing.len());
            let before = existing[..section_end].trim_end_matches('\n');
            let after = &existing[section_end..];
            if after.is_empty() {
                format!("{before}\n{entry}\n")
            } else {
                format!("{before}\n{entry}\n{after}")
            }
        }
        None => {
            let trimmed = existing.trim_end_matches('\n');
            format!("{trimmed}\n\n{date_heading}\n\n{entry}\n")
        }
    }
}

/// Find a heading line (`## <date>`) that starts at column 0 of some line.
fn find_heading_at_line_start(content: &str, heading: &str) -> Option<usize> {
    if content.starts_with(heading) {
        return Some(0);
    }
    let needle = format!("\n{heading}");
    content.find(&needle).map(|i| i + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ts(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, min, 0).unwrap()
    }

    #[test]
    fn append_to_fresh_file_creates_header_section_entry() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("log.md");
        append(
            &path,
            &ts(2026, 5, 9, 14, 30),
            "Designing Alluvium",
            &[
                "entities/alluvium".into(),
                "concepts/claude-code-hooks".into(),
            ],
        )
        .unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("# Log"));
        assert!(content.contains("## 2026-05-09"));
        assert!(content.contains("- 14:30 Designing Alluvium"));
        assert!(content.contains("[[entities/alluvium]]"));
        assert!(content.contains("[[concepts/claude-code-hooks]]"));
    }

    #[test]
    fn append_same_date_groups_under_existing_heading() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("log.md");
        append(
            &path,
            &ts(2026, 5, 9, 9, 15),
            "Morning session",
            &["a".into()],
        )
        .unwrap();
        append(
            &path,
            &ts(2026, 5, 9, 16, 0),
            "Afternoon session",
            &["b".into()],
        )
        .unwrap();
        let content = std::fs::read_to_string(&path).unwrap();

        // Exactly one "## 2026-05-09" heading.
        assert_eq!(content.matches("## 2026-05-09").count(), 1);
        assert!(content.contains("09:15 Morning session"));
        assert!(content.contains("16:00 Afternoon session"));
    }

    #[test]
    fn append_different_date_creates_new_section() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("log.md");
        append(&path, &ts(2026, 5, 8, 11, 0), "Day one", &["a".into()]).unwrap();
        append(&path, &ts(2026, 5, 9, 11, 0), "Day two", &["b".into()]).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("## 2026-05-08"));
        assert!(content.contains("## 2026-05-09"));
    }

    #[test]
    fn append_preserves_user_handwritten_lines_in_existing_section() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("log.md");
        // Pre-seed file with a user-written note in the day's section.
        std::fs::write(
            &path,
            "# Log\n\n## 2026-05-09\n\n- (user note) tried something cool\n",
        )
        .unwrap();
        append(
            &path,
            &ts(2026, 5, 9, 17, 0),
            "Alluvium session",
            &["x".into()],
        )
        .unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(
            content.contains("(user note) tried something cool"),
            "user line must survive: {content}"
        );
        assert!(content.contains("17:00 Alluvium session"));
    }

    #[test]
    fn insert_entry_into_section_with_following_section_keeps_order() {
        let existing = "# Log\n\n## 2026-05-08\n\n- 10:00 first day\n\n## 2026-05-09\n\n- 09:00 second day a\n";
        let result = insert_entry(existing, "## 2026-05-09", "- 16:00 second day b");
        // Both 2026-05-09 entries should be in the second section, in chronological insert order.
        let pos_a = result.find("09:00 second day a").unwrap();
        let pos_b = result.find("16:00 second day b").unwrap();
        let pos_05_08 = result.find("## 2026-05-08").unwrap();
        assert!(pos_05_08 < pos_a);
        assert!(pos_a < pos_b);
    }

    #[test]
    fn empty_touched_pages_renders_arrow_with_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("log.md");
        append(&path, &ts(2026, 5, 9, 12, 0), "no pages session", &[]).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("12:00 no pages session → "));
    }
}
