//! Pre-prompt byte caps.
//!
//! Truncates per-field content (tool_use args, tool_result output, individual
//! messages) before they go into the prompt. Pattern borrowed from
//! cognee-integrations: cap each field at a configured byte size to keep the
//! transcript-into-prompt step under a predictable token budget regardless of
//! how chatty the underlying session got.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ByteCaps {
    pub tool_use: usize,
    pub tool_result: usize,
    pub user_message: usize,
    pub assistant_message: usize,
}

impl Default for ByteCaps {
    fn default() -> Self {
        Self {
            tool_use: 4096,
            tool_result: 8192,
            user_message: 8192,
            assistant_message: 16384,
        }
    }
}

const TRUNCATION_MARKER_PREFIX: &str = "\n[... truncated, original was ";
const TRUNCATION_MARKER_SUFFIX: &str = " bytes]";

/// Truncate `s` to at most `cap` bytes plus a `[... truncated, original was N bytes]`
/// marker if it had to be shortened. UTF-8 safe — the cut point is rounded
/// down to the nearest char boundary so multi-byte glyphs are never split.
///
/// Strings already at or under `cap` are returned unchanged (no marker).
pub fn truncate(s: &str, cap: usize) -> String {
    if s.len() <= cap {
        return s.to_owned();
    }
    let cut = floor_char_boundary(s, cap);
    let mut out = String::with_capacity(cut + 64);
    out.push_str(&s[..cut]);
    out.push_str(TRUNCATION_MARKER_PREFIX);
    out.push_str(&s.len().to_string());
    out.push_str(TRUNCATION_MARKER_SUFFIX);
    out
}

/// Returns the largest index `<= idx` that is a char boundary in `s`.
/// `str::is_char_boundary(0)` and `str::is_char_boundary(len)` are always
/// true, so this terminates.
fn floor_char_boundary(s: &str, idx: usize) -> usize {
    let mut i = idx.min(s.len());
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorter_than_cap_returned_unchanged() {
        assert_eq!(truncate("hello", 100), "hello");
    }

    #[test]
    fn equal_to_cap_returned_unchanged() {
        let s = "hello";
        assert_eq!(truncate(s, 5), "hello");
    }

    #[test]
    fn longer_than_cap_gets_truncated_with_marker() {
        let s = "abcdefghij"; // 10 bytes
        let out = truncate(s, 4);
        assert!(out.starts_with("abcd"), "should keep first 4 bytes: {out}");
        assert!(
            out.contains("original was 10 bytes"),
            "marker should mention original length: {out}"
        );
    }

    /// The bug this whole UTF-8 path exists to prevent: `&s[..cap]` panics if
    /// `cap` lands in the middle of a multi-byte char. We must round down.
    #[test]
    fn utf8_multibyte_boundary_does_not_panic() {
        // "héllo" = h(1) é(2) l(1) l(1) o(1) = 6 bytes. Cap at 2 lands in
        // the middle of é.
        let s = "héllo";
        assert_eq!(s.len(), 6);
        let out = truncate(s, 2);
        // Should round down to char boundary at byte 1 (after 'h').
        assert!(out.starts_with("h"), "should keep at least 'h': {out}");
        assert!(
            !out.contains('é') || out.find('é').unwrap() == 0,
            "no half-character: {out}"
        );
    }

    #[test]
    fn utf8_chinese_truncates_safely() {
        // Each Chinese char is 3 bytes in UTF-8.
        let s = "中文测试一下"; // 6 chars × 3 bytes = 18 bytes
        assert_eq!(s.len(), 18);
        // Cap at 7 lands in the middle of the 3rd char ("测", bytes 6..9).
        let out = truncate(s, 7);
        // First two chars are kept (6 bytes), then truncation marker.
        assert!(out.starts_with("中文"), "first two whole chars kept: {out}");
        assert!(out.contains("original was 18 bytes"));
    }

    #[test]
    fn cap_zero_keeps_nothing_but_emits_marker() {
        let out = truncate("anything", 0);
        assert!(out.starts_with("\n[... truncated"), "got: {out}");
        assert!(out.contains("original was 8 bytes"));
    }

    #[test]
    fn empty_string_returned_unchanged() {
        assert_eq!(truncate("", 100), "");
        assert_eq!(truncate("", 0), "");
    }

    #[test]
    fn default_caps_are_sane() {
        let caps = ByteCaps::default();
        assert!(caps.tool_use > 0);
        assert!(caps.tool_result > caps.tool_use); // results often bigger
        assert!(caps.assistant_message > caps.user_message); // assistant verbose
    }

    #[test]
    fn floor_char_boundary_terminates_on_ascii() {
        let s = "hello";
        for i in 0..=s.len() {
            assert_eq!(floor_char_boundary(s, i), i);
        }
    }

    #[test]
    fn floor_char_boundary_handles_idx_past_end() {
        let s = "hi";
        assert_eq!(floor_char_boundary(s, 999), 2);
    }
}
