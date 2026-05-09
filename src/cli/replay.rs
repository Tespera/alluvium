//! `alluvium replay` — re-archive an old session or batch.
//!
//! Three modes:
//! - `alluvium replay <session-id>` — re-archive a specific session
//! - `alluvium replay --since 7d|24h|2026-04-01` — re-archive sessions
//!   newer than the cutoff
//! - `alluvium replay --all` — re-archive every transcript
//!
//! Internally calls into `cli::archive::run(Some(id))` for each target.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use std::path::PathBuf;

pub async fn run(session: Option<&str>, since: Option<&str>, all: bool) -> Result<()> {
    let targets = collect_targets(session, since, all)?;
    if targets.is_empty() {
        println!("replay: no transcripts matched the filter");
        return Ok(());
    }
    println!("replay: re-archiving {} session(s)", targets.len());
    for (idx, session_id) in targets.iter().enumerate() {
        println!(
            "replay [{n}/{total}]: archiving session {sid}",
            n = idx + 1,
            total = targets.len(),
            sid = session_id
        );
        if let Err(err) = super::archive::run(Some(session_id)).await {
            tracing::warn!(
                session = %session_id,
                error = %format!("{err:#}"),
                "replay: archive failed; continuing with next"
            );
        }
    }
    println!("replay: done.");
    Ok(())
}

fn collect_targets(session: Option<&str>, since: Option<&str>, all: bool) -> Result<Vec<String>> {
    if let Some(id) = session {
        return Ok(vec![id.to_string()]);
    }
    let projects = home_dir()
        .context("no home directory")?
        .join(".claude")
        .join("projects");
    if !projects.exists() {
        anyhow::bail!(
            "Claude Code transcripts directory does not exist: {}",
            projects.display()
        );
    }
    let cutoff: Option<DateTime<Utc>> = if all {
        None
    } else if let Some(s) = since {
        Some(parse_since(s)?)
    } else {
        anyhow::bail!("replay needs one of: <session-id>, --since <duration>, --all");
    };

    let mut targets = Vec::new();
    for project_dir in std::fs::read_dir(&projects)?.flatten() {
        if !project_dir.path().is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(project_dir.path())?.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "jsonl") {
                let modified = std::fs::metadata(&path)
                    .ok()
                    .and_then(|m| m.modified().ok());
                let keep = match (cutoff, modified) {
                    (Some(c), Some(m)) => DateTime::<Utc>::from(m) >= c,
                    (None, _) => true,        // --all
                    (Some(_), None) => false, // can't tell, exclude
                };
                if keep {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        targets.push(stem.to_string());
                    }
                }
            }
        }
    }
    targets.sort();
    Ok(targets)
}

/// Parse a "since" expression into a UTC instant. Accepts:
/// - relative: `7d`, `24h`, `30m`, `2w` (weeks)
/// - absolute: `YYYY-MM-DD`
fn parse_since(s: &str) -> Result<DateTime<Utc>> {
    let now = Utc::now();
    if let Some((num_str, unit)) = split_relative(s) {
        let n: i64 = num_str
            .parse()
            .with_context(|| format!("not a number: {num_str:?}"))?;
        let dur = match unit {
            "m" => Duration::minutes(n),
            "h" => Duration::hours(n),
            "d" => Duration::days(n),
            "w" => Duration::weeks(n),
            other => anyhow::bail!("unknown duration unit {other:?}; use m/h/d/w"),
        };
        return Ok(now - dur);
    }
    // Try YYYY-MM-DD.
    if let Ok(date) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let dt = date
            .and_hms_opt(0, 0, 0)
            .context("invalid time components")?
            .and_utc();
        return Ok(dt);
    }
    anyhow::bail!("--since: expected `Nm`/`Nh`/`Nd`/`Nw` or YYYY-MM-DD; got {s:?}")
}

/// Split `"<digits><unit-letters>"` into `(digits, unit)`. Returns `None`
/// for inputs that aren't a relative duration (e.g. `"2026-04-01"`).
fn split_relative(s: &str) -> Option<(&str, &str)> {
    // Position just past the last digit. If the whole string is digits,
    // there's no unit; if it has no digits, we're not a relative duration.
    let split_pos = s.bytes().rposition(|b| b.is_ascii_digit()).map(|p| p + 1)?;
    if split_pos == s.len() {
        return None; // all digits, no unit suffix
    }
    let prefix = &s[..split_pos];
    let suffix = &s[split_pos..];
    if !prefix.is_empty() && suffix.chars().all(|c| c.is_ascii_alphabetic()) {
        Some((prefix, suffix))
    } else {
        None
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_since_relative_units() {
        // parse_since calls Utc::now() internally; comparing against an
        // outer `now` would race. Verify each unit yields a result close
        // to the expected delta from "right around now".
        let now = Utc::now();

        let d7 = parse_since("7d").unwrap();
        let delta = now.signed_duration_since(d7).num_seconds();
        let expected = chrono::Duration::days(7).num_seconds();
        assert!(
            (delta - expected).abs() < 5,
            "7d expected ~{expected}s, got {delta}s"
        );

        let h24 = parse_since("24h").unwrap();
        let dh = now.signed_duration_since(h24).num_hours();
        assert!((dh - 24).abs() <= 1, "24h expected ~24 hours, got {dh}");

        let m30 = parse_since("30m").unwrap();
        let dm = now.signed_duration_since(m30).num_minutes();
        assert!((dm - 30).abs() <= 1);

        let w2 = parse_since("2w").unwrap();
        let dw = now.signed_duration_since(w2).num_days();
        assert!((dw - 14).abs() <= 1);
    }

    #[test]
    fn parse_since_absolute_date() {
        let dt = parse_since("2026-04-01").unwrap();
        assert_eq!(dt.format("%Y-%m-%d").to_string(), "2026-04-01");
    }

    #[test]
    fn parse_since_unknown_unit_errors() {
        assert!(parse_since("5x").is_err());
    }

    #[test]
    fn parse_since_garbage_errors() {
        assert!(parse_since("not-a-thing").is_err());
    }
}
