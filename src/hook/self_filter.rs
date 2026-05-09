//! Self-reference filter.
//!
//! If the user is using Claude Code to develop Alluvium itself, that session
//! must NOT be archived (would create recursive notes about archive logic).
//! More generally: any path the user listed in `skip_paths` should be skipped.
//!
//! Decision: SessionStart compares cwd against the user-configured `skip_paths`
//! (which `alluvium init` seeds with the Alluvium dev tree if it detects one).
//! Result lands in `resolved.json` as `skip_reason`. Subsequent hooks no-op
//! when the reason is non-empty.
//!
//! The comparison is path-component-aware (`/a/b` does NOT match `/a/bcd`),
//! so users don't have to worry about trailing-slash normalization.

use std::path::{Path, PathBuf};

/// Returns `Some(reason)` if `cwd` is at or under any path in `skip_paths`;
/// `None` otherwise.
///
/// Both `cwd` and the entries in `skip_paths` are expected to be absolute and
/// canonical — config-load and hook-payload-parse should canonicalize before
/// reaching this function. (We don't canonicalize here to keep the function
/// pure and IO-free; that makes it cheap to call on every hook event.)
pub fn should_skip(cwd: &Path, skip_paths: &[PathBuf]) -> Option<String> {
    for skip in skip_paths {
        if cwd.starts_with(skip) {
            return Some(format!("matched skip_path: {}", skip.display()));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn empty_skip_list_never_matches() {
        assert_eq!(should_skip(&p("/Users/eric/work"), &[]), None);
    }

    #[test]
    fn exact_match_returns_reason() {
        let skips = vec![p("/Users/eric/dev/alluvium")];
        let reason = should_skip(&p("/Users/eric/dev/alluvium"), &skips);
        assert!(reason.is_some());
        assert!(reason.unwrap().contains("/Users/eric/dev/alluvium"));
    }

    #[test]
    fn subdirectory_matches() {
        let skips = vec![p("/Users/eric/dev/alluvium")];
        assert!(should_skip(&p("/Users/eric/dev/alluvium/src"), &skips).is_some());
        assert!(should_skip(&p("/Users/eric/dev/alluvium/docs/HOOKS.md"), &skips).is_some());
    }

    #[test]
    fn unrelated_path_does_not_match() {
        let skips = vec![p("/Users/eric/dev/alluvium")];
        assert_eq!(
            should_skip(&p("/Users/eric/dev/other-project"), &skips),
            None
        );
        assert_eq!(should_skip(&p("/tmp"), &skips), None);
    }

    /// Critical: `/a/b` must NOT match `/a/bcd`. Without component-aware
    /// matching this would be a security/correctness bug — `/Users/eric/work`
    /// would shadow `/Users/eric/work-alt`.
    #[test]
    fn shared_prefix_but_different_component_does_not_match() {
        let skips = vec![p("/Users/eric/work")];
        assert_eq!(should_skip(&p("/Users/eric/work-alt"), &skips), None);
        assert_eq!(should_skip(&p("/Users/eric/workshop"), &skips), None);
    }

    #[test]
    fn first_matching_skip_path_wins() {
        let skips = vec![
            p("/Users/eric/dev/alluvium"),
            p("/Users/eric/private"),
            p("/Users/eric"), // overlapping; should never be the reported match if more specific paths come first
        ];
        let reason = should_skip(&p("/Users/eric/dev/alluvium/src"), &skips).unwrap();
        assert!(
            reason.contains("alluvium"),
            "first match should win, got: {reason}"
        );
    }

    #[test]
    fn multiple_skip_paths_any_match() {
        let skips = vec![p("/a"), p("/b"), p("/c")];
        assert!(should_skip(&p("/b/sub"), &skips).is_some());
        assert!(should_skip(&p("/c"), &skips).is_some());
        assert_eq!(should_skip(&p("/d"), &skips), None);
    }

    #[test]
    fn root_skip_path_matches_everything() {
        // Edge case: if a user accidentally puts "/" in skip_paths, every
        // session is skipped. We accept this — user-supplied list, user
        // problem — but verify the behavior is consistent.
        let skips = vec![p("/")];
        assert!(should_skip(&p("/anything"), &skips).is_some());
        assert!(should_skip(&p("/"), &skips).is_some());
    }
}
