//! Archive-process file lock.
//!
//! Two Claude Code sessions ending nearly simultaneously each spawn a
//! detached `alluvium archive` worker. Without coordination they could
//! race on the same vault topic page (read → modify → write) and clobber
//! each other's edits.
//!
//! [`acquire`] takes an exclusive flock on a sentinel lock file. Workers
//! serialize through it: the second process blocks until the first drops
//! its lock guard. The lock is released automatically when the
//! [`ArchiveLock`] handle is dropped (so panics during archive cleanly
//! release).
//!
//! Lock file is created if missing; never truncated (it carries no payload,
//! only acts as a coordination point).
//!
//! Uses BSD-style `flock(2)` via the `fs2` crate. This is process-level —
//! two different OS processes contend correctly. Within a single process
//! (e.g. tests opening the same file from multiple threads), each `File`
//! handle is independent so the contention behavior matches what we see
//! in production.

use anyhow::{Context, Result};
use fs2::FileExt;
use std::fs::File;
use std::path::Path;

/// RAII guard for the archive file lock. Drop releases the lock.
pub struct ArchiveLock {
    /// The locked file. Held to keep the OS lock alive; never read.
    _file: File,
}

/// Acquire an exclusive lock on `lock_path`. Blocks until the lock is
/// available. The parent directory is created if missing.
pub fn acquire(lock_path: &Path) -> Result<ArchiveLock> {
    let file = open_or_create(lock_path)?;
    file.lock_exclusive()
        .with_context(|| format!("acquiring exclusive lock on {}", lock_path.display()))?;
    Ok(ArchiveLock { _file: file })
}

/// Try to acquire the lock without blocking. Returns `None` if another
/// process / handle currently holds it.
pub fn try_acquire(lock_path: &Path) -> Result<Option<ArchiveLock>> {
    let file = open_or_create(lock_path)?;
    match file.try_lock_exclusive() {
        Ok(()) => Ok(Some(ArchiveLock { _file: file })),
        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
        // fs2 may surface WouldBlock as a different kind on some platforms;
        // be conservative and treat any try-lock error as "already held"
        // rather than failing the archive.
        Err(_) => Ok(None),
    }
}

fn open_or_create(lock_path: &Path) -> Result<File> {
    if let Some(parent) = lock_path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating lock-file parent {}", parent.display()))?;
        }
    }
    File::options()
        .create(true)
        .write(true)
        .truncate(false)
        .open(lock_path)
        .with_context(|| format!("opening lock file {}", lock_path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_creates_lock_file_if_missing() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join("alluvium.lock");
        assert!(!lock_path.exists());
        let _guard = acquire(&lock_path).unwrap();
        assert!(lock_path.exists());
    }

    #[test]
    fn acquire_creates_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join("nested/cache/lock");
        let _guard = acquire(&lock_path).unwrap();
        assert!(lock_path.exists());
    }

    #[test]
    fn lock_released_on_drop_allows_subsequent_acquire() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join("lock");

        {
            let _g = acquire(&lock_path).unwrap();
        }
        // Drop happened. Should be able to lock again.
        let _g2 = acquire(&lock_path).unwrap();
    }

    #[test]
    fn try_acquire_returns_none_when_already_held() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join("lock");
        let _first = acquire(&lock_path).unwrap();

        let second = try_acquire(&lock_path).unwrap();
        assert!(
            second.is_none(),
            "a second try_acquire should not succeed while the first is held"
        );
    }

    #[test]
    fn try_acquire_succeeds_after_holder_drops() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join("lock");

        {
            let _first = acquire(&lock_path).unwrap();
        }
        let second = try_acquire(&lock_path).unwrap();
        assert!(second.is_some());
    }

    #[test]
    fn lock_does_not_truncate_existing_lock_file_contents() {
        // Defensive: we use the lock file purely as a coordination point,
        // but if the user (or someone else) put bytes in it, we shouldn't
        // wipe them. Lock-file semantics, not data semantics.
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join("lock");
        std::fs::write(&lock_path, b"some sentinel bytes").unwrap();
        let _guard = acquire(&lock_path).unwrap();
        let after = std::fs::read(&lock_path).unwrap();
        assert_eq!(after, b"some sentinel bytes");
    }

    #[test]
    fn cross_thread_lock_serializes_critical_section() {
        // Two threads each spend a critical section under the lock. Verify
        // that they're serialized: while one is "inside" (sleeping briefly),
        // the other's try_acquire returns None.
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().to_path_buf().join("contended-lock");
        let started = Arc::new(AtomicBool::new(false));

        let lock_path_t = lock_path.clone();
        let started_t = started.clone();
        let holder = std::thread::spawn(move || {
            let _g = acquire(&lock_path_t).unwrap();
            started_t.store(true, Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(150));
            // _g drops here, releasing the lock.
        });

        // Wait for the holder to actually take the lock.
        while !started.load(Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        // While holder is in its 150ms sleep, try_acquire must return None.
        let attempt = try_acquire(&lock_path).unwrap();
        assert!(
            attempt.is_none(),
            "lock must be unavailable while holder thread is in critical section"
        );
        drop(attempt);

        holder.join().unwrap();

        // After the holder exits, the lock is free.
        let after = try_acquire(&lock_path).unwrap();
        assert!(
            after.is_some(),
            "lock should become available once the holder thread releases"
        );
    }
}
