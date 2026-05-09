//! YAML frontmatter parse + render.
//!
//! Markdown files in the vault have an optional YAML frontmatter block
//! delimited by `---` lines:
//!
//! ```markdown
//! ---
//! title: My note
//! tags: [a, b]
//! ---
//!
//! Body content here.
//! ```
//!
//! [`parse`] separates a markdown string into its frontmatter (as a
//! `serde_yaml::Value`) and body. [`render`] does the reverse: given a YAML
//! value and a body, produce a markdown string with the `---`-fenced block.
//!
//! ## Robustness
//!
//! - **No frontmatter** (file doesn't start with `---`): whole input is body,
//!   frontmatter is `Value::Null`.
//! - **Unterminated frontmatter** (opens with `---` but never closes): treat
//!   as having no frontmatter — preserve the user's content rather than fail
//!   loudly. (Distinct ADR-009 markers protect Alluvium-managed sections;
//!   user-invalid YAML at the top of a file should not eat the body.)
//! - **Malformed YAML** (well-fenced but invalid): propagate the parse error
//!   so the caller can decide whether to skip the file or surface to the user.
//! - **CRLF line endings**: tolerated.
//!
//! Round-trip property: `render(parse(s)?) == s` for any well-formed input.

use anyhow::{Context, Result};

/// Parse a markdown string into `(frontmatter, body)`.
pub fn parse(markdown: &str) -> Result<(serde_yaml::Value, String)> {
    // Frontmatter must start at byte 0.
    let after_open = match strip_open_fence(markdown) {
        Some(rest) => rest,
        None => return Ok((serde_yaml::Value::Null, markdown.to_string())),
    };

    let Some((yaml_str, body_after_close)) = find_close_fence_split(after_open) else {
        // Opening fence with no closing fence — preserve the original input
        // as the body. This is a forgiving fallback; the user is more upset
        // by losing content than by silently no-op'ing on a malformed header.
        return Ok((serde_yaml::Value::Null, markdown.to_string()));
    };

    // Strip the leading newline that separates the closing fence from the body.
    let body = body_after_close
        .strip_prefix("\r\n")
        .or_else(|| body_after_close.strip_prefix('\n'))
        .unwrap_or(body_after_close)
        .to_string();

    let value = if yaml_str.trim().is_empty() {
        // `---\n---` with no fields between is legal but yields Null.
        serde_yaml::Value::Null
    } else {
        serde_yaml::from_str(yaml_str).with_context(|| {
            let preview: String = yaml_str.chars().take(200).collect();
            format!("failed to parse frontmatter YAML; first 200 chars: {preview}")
        })?
    };

    Ok((value, body))
}

/// Render `(frontmatter, body)` back into a fenced markdown string.
///
/// If `frontmatter` is `Null`, the body is returned without a fence block.
/// Otherwise the output is `---\n<yaml>---\n\n<body>` (or just
/// `---\n<yaml>---\n` if `body` is empty).
pub fn render(frontmatter: &serde_yaml::Value, body: &str) -> Result<String> {
    if matches!(frontmatter, serde_yaml::Value::Null) {
        return Ok(body.to_string());
    }

    let yaml_str =
        serde_yaml::to_string(frontmatter).context("failed to serialize frontmatter to YAML")?;

    let mut out = String::with_capacity(yaml_str.len() + body.len() + 16);
    out.push_str("---\n");
    out.push_str(&yaml_str);
    if !yaml_str.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("---\n");
    if !body.is_empty() {
        out.push('\n');
        out.push_str(body);
    }
    Ok(out)
}

/// Strip an opening `---` fence (with following newline). Returns the rest of
/// the input on success.
fn strip_open_fence(s: &str) -> Option<&str> {
    s.strip_prefix("---\n")
        .or_else(|| s.strip_prefix("---\r\n"))
}

/// Find the closing `---` fence (a line whose only content is `---`) and
/// return `(yaml_str, after_close_fence)`.
fn find_close_fence_split(s: &str) -> Option<(&str, &str)> {
    let mut byte_offset = 0;
    for line in s.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed == "---" {
            let yaml_end = byte_offset;
            let body_start = byte_offset + line.len();
            return Some((&s[..yaml_end], &s[body_start..]));
        }
        byte_offset += line.len();
    }
    // Handle the no-trailing-newline case: last line is exactly "---".
    if s.trim_end_matches(['\n', '\r']).ends_with("\n---")
        || s.trim_end_matches(['\n', '\r']) == "---"
    {
        let trimmed = s.trim_end_matches(['\n', '\r']);
        if let Some(idx) = trimmed.rfind("\n---") {
            return Some((&s[..idx], ""));
        }
        if trimmed == "---" {
            return Some(("", ""));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml::Value;

    fn yaml(s: &str) -> Value {
        serde_yaml::from_str(s).unwrap()
    }

    // ─────────────── parse ───────────────

    #[test]
    fn parse_with_frontmatter_extracts_yaml_and_body() {
        let input = "---\ntitle: hello\ntags:\n  - a\n  - b\n---\n\nBody text.";
        let (fm, body) = parse(input).unwrap();
        assert_eq!(fm["title"], Value::String("hello".into()));
        assert_eq!(fm["tags"][0], Value::String("a".into()));
        assert_eq!(body, "Body text.");
    }

    #[test]
    fn parse_without_frontmatter_returns_null_and_full_body() {
        let input = "Just some markdown.\n\nMore text.";
        let (fm, body) = parse(input).unwrap();
        assert_eq!(fm, Value::Null);
        assert_eq!(body, input);
    }

    #[test]
    fn parse_empty_string_yields_empty() {
        let (fm, body) = parse("").unwrap();
        assert_eq!(fm, Value::Null);
        assert_eq!(body, "");
    }

    #[test]
    fn parse_empty_frontmatter_block_yields_null() {
        let input = "---\n---\n\nBody.";
        let (fm, body) = parse(input).unwrap();
        assert_eq!(fm, Value::Null);
        assert_eq!(body, "Body.");
    }

    #[test]
    fn parse_only_frontmatter_no_body() {
        let input = "---\ntitle: hello\n---\n";
        let (fm, body) = parse(input).unwrap();
        assert_eq!(fm["title"], Value::String("hello".into()));
        assert_eq!(body, "");
    }

    #[test]
    fn parse_unterminated_frontmatter_falls_back_to_no_frontmatter() {
        // Opens with --- but never closes. Don't lose the body.
        let input = "---\nkey: value\n\n# Just a markdown that happens to start with ---\n";
        let (fm, body) = parse(input).unwrap();
        assert_eq!(fm, Value::Null);
        assert_eq!(body, input);
    }

    #[test]
    fn parse_invalid_yaml_returns_error() {
        let input = "---\nkey: : : invalid yaml\n---\nBody.";
        let err = parse(input).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("failed to parse frontmatter YAML"),
            "got: {msg}"
        );
    }

    #[test]
    fn parse_body_containing_triple_dash_not_confused_as_fence() {
        // The frontmatter closes properly. `---` later in the body should be
        // treated as content, not a fence.
        let input = "---\ntitle: x\n---\n\nFirst paragraph.\n\n---\n\nA horizontal rule above.";
        let (fm, body) = parse(input).unwrap();
        assert_eq!(fm["title"], Value::String("x".into()));
        assert!(body.contains("---"));
    }

    #[test]
    fn parse_handles_crlf_line_endings() {
        let input = "---\r\ntitle: hello\r\n---\r\n\r\nBody.";
        let (fm, body) = parse(input).unwrap();
        assert_eq!(fm["title"], Value::String("hello".into()));
        assert_eq!(body, "Body.");
    }

    #[test]
    fn parse_handles_unicode_in_yaml_and_body() {
        let input = "---\ntitle: 你好\n---\n\n中文 body 内容";
        let (fm, body) = parse(input).unwrap();
        assert_eq!(fm["title"], Value::String("你好".into()));
        assert_eq!(body, "中文 body 内容");
    }

    #[test]
    fn parse_array_and_nested_values() {
        let input =
            "---\ntags:\n  - rust\n  - obsidian\nrelations:\n  uses:\n    - foo\n---\n\nBody.";
        let (fm, _) = parse(input).unwrap();
        assert_eq!(fm["tags"][0], Value::String("rust".into()));
        assert_eq!(fm["relations"]["uses"][0], Value::String("foo".into()));
    }

    // ─────────────── render ───────────────

    #[test]
    fn render_null_frontmatter_returns_only_body() {
        let out = render(&Value::Null, "Just body.").unwrap();
        assert_eq!(out, "Just body.");
    }

    #[test]
    fn render_simple_frontmatter() {
        let fm = yaml("title: hello");
        let out = render(&fm, "Body.").unwrap();
        assert!(out.starts_with("---\n"));
        assert!(out.contains("title: hello"));
        assert!(out.ends_with("\nBody."));
    }

    #[test]
    fn render_with_empty_body_omits_trailing_newline_and_body() {
        let fm = yaml("title: hello");
        let out = render(&fm, "").unwrap();
        assert!(out.starts_with("---\ntitle: hello\n---\n"));
        assert!(!out.ends_with("\n\n"));
    }

    // ─────────────── round-trip ───────────────

    #[test]
    fn round_trip_simple_preserves_structure() {
        let input = "---\ntitle: hello\ntags:\n- a\n- b\n---\n\nBody.";
        let (fm, body) = parse(input).unwrap();
        let rendered = render(&fm, &body).unwrap();
        let (fm2, body2) = parse(&rendered).unwrap();
        assert_eq!(fm, fm2);
        assert_eq!(body, body2);
    }

    #[test]
    fn round_trip_with_unicode_preserves() {
        let input = "---\ntitle: 你好\n---\n\n中文 body";
        let (fm, body) = parse(input).unwrap();
        let rendered = render(&fm, &body).unwrap();
        let (fm2, body2) = parse(&rendered).unwrap();
        assert_eq!(fm, fm2);
        assert_eq!(body, body2);
    }

    #[test]
    fn round_trip_no_frontmatter_returns_body_unchanged() {
        let input = "No frontmatter at all.\n\nMore lines.";
        let (fm, body) = parse(input).unwrap();
        let rendered = render(&fm, &body).unwrap();
        assert_eq!(rendered, input);
    }

    #[test]
    fn round_trip_array_relations_preserves_typed_relations() {
        // Real-world Alluvium frontmatter shape.
        let input = "---\ntitle: claude-code-hooks\ntype: concept\ntags:\n- claude-code\n- automation\nrelations:\n  uses: []\n  used-by:\n  - alluvium\n  related: []\n  supersedes: []\n---\n\nBody.";
        let (fm, body) = parse(input).unwrap();
        assert_eq!(
            fm["relations"]["used-by"][0],
            Value::String("alluvium".into())
        );
        let rendered = render(&fm, &body).unwrap();
        let (fm2, _) = parse(&rendered).unwrap();
        assert_eq!(fm, fm2);
    }
}
