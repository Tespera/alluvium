//! Render `alluvium status` output from the archive audit log.
//!
//! Pure function: takes a slice of [`super::ArchiveLogEntry`] (oldest-first),
//! produces a human-readable plain-text table. The `cli/status` command
//! reads the log file, formats relative time (e.g. "5 minutes ago"), and
//! calls [`render`].

use chrono::{DateTime, Utc};

use super::ArchiveLogEntry;

/// Render the recent-archive listing for `alluvium status`.
///
/// `now` is passed in for testable relative-time formatting.
pub fn render(entries: &[ArchiveLogEntry], now: DateTime<Utc>) -> String {
    if entries.is_empty() {
        return "No archive runs recorded yet.\n\nIf you've been using Claude Code with Alluvium installed, the first archive will appear after your next session ends.\n".into();
    }

    let mut out = String::new();
    out.push_str("Recent archive runs (most recent first):\n\n");
    for entry in entries.iter().rev() {
        let when = format_relative(now, entry.finished_at);
        let status_icon = if entry.error.is_some() { "✗" } else { "✓" };
        let session_short = short_session(&entry.session_id);

        out.push_str(&format!(
            "{status_icon} {when:>14}  session={session_short}\n"
        ));

        // touched pages, indented
        if !entry.touched_pages.is_empty() && entry.error.is_none() {
            for page in &entry.touched_pages {
                out.push_str(&format!("                 → {}\n", page.display()));
            }
        }

        // tokens (cost line)
        if let (Some(inp), Some(outp)) = (entry.input_tokens, entry.output_tokens) {
            out.push_str(&format!("                 tokens: {inp} in / {outp} out\n"));
        }

        // error
        if let Some(err) = &entry.error {
            let truncated: String = err.chars().take(120).collect();
            out.push_str(&format!("                 error: {truncated}\n"));
        }

        out.push('\n');
    }
    out
}

fn short_session(id: &str) -> String {
    // Show first 8 chars of the UUID; sufficient to identify, won't blow line width.
    id.chars().take(8).collect()
}

fn format_relative(now: DateTime<Utc>, when: DateTime<Utc>) -> String {
    let delta = now.signed_duration_since(when);
    let secs = delta.num_seconds();

    if secs < 0 {
        return "future".into();
    }

    if secs < 60 {
        return "just now".into();
    }
    let mins = secs / 60;
    if mins < 60 {
        return format!("{mins} min ago");
    }
    let hours = mins / 60;
    if hours < 24 {
        return format!("{hours} hr ago");
    }
    let days = hours / 24;
    if days < 30 {
        return format!("{days} day{} ago", plural(days));
    }
    let months = days / 30;
    format!("{months} mo ago")
}

fn plural(n: i64) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::path::PathBuf;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 5, 9, 15, 0, 0).unwrap()
    }

    fn entry(session: &str, finished_offset_secs: i64, error: Option<&str>) -> ArchiveLogEntry {
        let finished = now() - chrono::Duration::seconds(finished_offset_secs);
        ArchiveLogEntry {
            session_id: session.into(),
            started_at: finished - chrono::Duration::seconds(5),
            finished_at: finished,
            touched_pages: vec![
                PathBuf::from("wiki/entities/alluvium.md"),
                PathBuf::from("wiki/concepts/hooks.md"),
            ],
            error: error.map(String::from),
            input_tokens: Some(1234),
            output_tokens: Some(567),
        }
    }

    #[test]
    fn empty_log_renders_friendly_message() {
        let out = render(&[], now());
        assert!(out.contains("No archive runs recorded yet"));
    }

    #[test]
    fn renders_recent_first() {
        let entries = vec![
            entry("aaaaaaaa-old", 3600, None), // 1 hour ago
            entry("zzzzzzzz-new", 60, None),   // 1 min ago
        ];
        let out = render(&entries, now());
        let pos_new = out.find("zzzzzzzz").unwrap();
        let pos_old = out.find("aaaaaaaa").unwrap();
        assert!(pos_new < pos_old, "newest should be first; got: {out}");
    }

    #[test]
    fn includes_touched_pages_on_success() {
        let out = render(&[entry("abc", 60, None)], now());
        assert!(out.contains("wiki/entities/alluvium.md"));
        assert!(out.contains("wiki/concepts/hooks.md"));
    }

    #[test]
    fn includes_token_counts() {
        let out = render(&[entry("abc", 60, None)], now());
        assert!(out.contains("1234 in"));
        assert!(out.contains("567 out"));
    }

    #[test]
    fn shows_error_indicator_and_message() {
        let out = render(&[entry("abc", 60, Some("API 529 overloaded"))], now());
        assert!(out.contains("✗"));
        assert!(out.contains("API 529 overloaded"));
    }

    #[test]
    fn long_error_message_is_truncated() {
        let long_err = "x".repeat(500);
        let out = render(&[entry("abc", 60, Some(&long_err))], now());
        // The error line shouldn't blow up the whole status output.
        assert!(out.len() < 1000);
    }

    #[test]
    fn relative_time_format_progression() {
        // Direct tests of format_relative.
        let fmt = |secs: i64| format_relative(now(), now() - chrono::Duration::seconds(secs));
        assert_eq!(fmt(10), "just now");
        assert_eq!(fmt(120), "2 min ago");
        assert_eq!(fmt(7200), "2 hr ago");
        assert_eq!(fmt(86400), "1 day ago");
        assert_eq!(fmt(86400 * 5), "5 days ago");
        assert_eq!(fmt(86400 * 90), "3 mo ago");
    }

    #[test]
    fn future_timestamps_handled() {
        let future = entry("abc", -60, None); // finished 1 min in the future
        let out = render(&[future], now());
        assert!(out.contains("future"));
    }

    #[test]
    fn session_id_displayed_short() {
        let out = render(&[entry("abcdef0123456789-very-long-uuid", 60, None)], now());
        assert!(out.contains("abcdef01"));
        // Long-form UUID should not appear in full.
        assert!(!out.contains("abcdef0123456789-very-long-uuid"));
    }
}
