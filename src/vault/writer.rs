//! Atomic file write: temp file in the same directory + rename.
//!
//! On POSIX, `rename(2)` is atomic when source and destination are on the
//! same filesystem. By creating the temp file inside the *target's parent
//! directory* (via `tempfile_in`), we guarantee that, then atomically
//! swap into place.
//!
//! Why atomic: vault writes can race with Obsidian's filesystem watcher,
//! with concurrent archives, and with the user editing the same file. A
//! partial write (interrupted mid-stream) would corrupt the page; an
//! atomic rename means readers see either the old version or the new
//! version, never a half-written one.
//!
//! Parent directories are created automatically (`create_dir_all`).

use anyhow::{Context, Result};
use std::io::Write;
use std::path::Path;

/// Write `content` to `path` atomically: stage to a tempfile in the same
/// directory, then rename. Parent directories are created if missing.
pub fn write_atomic(path: &Path, content: &str) -> Result<()> {
    let parent = parent_dir(path);

    if !parent.exists() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating parent directory {}", parent.display()))?;
    }

    let mut tmp = tempfile::Builder::new()
        .prefix(".alluvium-tmp-")
        .tempfile_in(parent)
        .with_context(|| format!("creating temp file in {}", parent.display()))?;

    tmp.write_all(content.as_bytes())
        .with_context(|| format!("writing to temp file for {}", path.display()))?;
    tmp.flush()
        .with_context(|| format!("flushing temp file for {}", path.display()))?;

    // persist() consumes the NamedTempFile and renames it. On error it
    // returns the tempfile so it can be cleaned up; we map straight to
    // anyhow which drops it (and tempfile auto-deletes on drop).
    tmp.persist(path)
        .map_err(|e| anyhow::anyhow!("renaming temp file to {}: {}", path.display(), e.error))?;

    Ok(())
}

/// Resolve a path's parent, falling back to `.` for bare filenames.
fn parent_dir(path: &Path) -> &Path {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        write_atomic(&path, "hello").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello");
    }

    #[test]
    fn overwrites_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        write_atomic(&path, "version 1").unwrap();
        write_atomic(&path, "version 2").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "version 2");
    }

    #[test]
    fn creates_missing_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wiki/concepts/nested/note.md");
        write_atomic(&path, "deep").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "deep");
    }

    #[test]
    fn empty_content_writes_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.md");
        write_atomic(&path, "").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
    }

    #[test]
    fn unicode_content_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("unicode.md");
        let content = "中文 + emoji 🎉 + zero-width\u{200B}joiner";
        write_atomic(&path, content).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
    }

    #[test]
    fn large_content_works() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("large.md");
        let content = "x".repeat(1_000_000);
        write_atomic(&path, &content).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap().len(), 1_000_000);
    }

    #[test]
    fn does_not_leave_tempfile_behind_on_success() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        write_atomic(&path, "hello").unwrap();
        let leftover_count = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(".alluvium-tmp-")
            })
            .count();
        assert_eq!(leftover_count, 0);
    }

    #[test]
    fn writes_concurrent_to_different_files_succeed() {
        // Sanity: two threads writing to different files in the same dir
        // shouldn't step on each other's tempfiles.
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path().to_path_buf();

        let h1 = {
            let p = dir_path.clone();
            std::thread::spawn(move || {
                for i in 0..50 {
                    write_atomic(&p.join("a.md"), &format!("a {i}")).unwrap();
                }
            })
        };
        let h2 = {
            let p = dir_path.clone();
            std::thread::spawn(move || {
                for i in 0..50 {
                    write_atomic(&p.join("b.md"), &format!("b {i}")).unwrap();
                }
            })
        };
        h1.join().unwrap();
        h2.join().unwrap();

        assert!(std::fs::read_to_string(dir_path.join("a.md"))
            .unwrap()
            .starts_with("a "));
        assert!(std::fs::read_to_string(dir_path.join("b.md"))
            .unwrap()
            .starts_with("b "));
    }

    #[test]
    fn last_writer_wins_on_concurrent_same_path() {
        // Writes to the SAME file from two threads — last-writer-wins is the
        // expected POSIX rename semantics. We don't promise ordering here;
        // we just verify the file ends up with one of the valid values
        // (no truncation, no corruption).
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("contended.md");

        let h1 = {
            let p = path.clone();
            std::thread::spawn(move || {
                for _ in 0..20 {
                    write_atomic(&p, "version_A_with_padding_to_make_it_long").unwrap();
                }
            })
        };
        let h2 = {
            let p = path.clone();
            std::thread::spawn(move || {
                for _ in 0..20 {
                    write_atomic(&p, "version_B").unwrap();
                }
            })
        };
        h1.join().unwrap();
        h2.join().unwrap();

        let final_content = std::fs::read_to_string(&path).unwrap();
        assert!(
            final_content == "version_A_with_padding_to_make_it_long"
                || final_content == "version_B",
            "final content must be exactly one full version, got: {final_content:?}"
        );
    }

    #[test]
    fn parent_dir_resolves_bare_filename_to_current() {
        let p = parent_dir(Path::new("note.md"));
        assert_eq!(p, Path::new("."));
    }

    #[test]
    fn parent_dir_resolves_absolute_path() {
        let p = parent_dir(Path::new("/Users/eric/work/note.md"));
        assert_eq!(p, Path::new("/Users/eric/work"));
    }
}
