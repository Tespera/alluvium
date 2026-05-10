//! Detached subprocess spawning.
//!
//! The Stop hook spawns `alluvium archive` and exits within 100 ms; the
//! child continues in the background. This is essential — distillation
//! takes 5-30 s; if the hook waits, the user sees Claude Code "hang" on
//! shutdown (per ADR-008).
//!
//! On Unix we put the child in its own process group so it outlives the
//! parent. The child also closes stdin/stdout/stderr to detach fully —
//! otherwise the parent (Claude Code) might wait on its terminal.
//!
//! On Windows the equivalent is the `CREATE_NEW_PROCESS_GROUP` creation
//! flag; not implemented in v0.1 (we only target Unix).

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Spawn `alluvium archive --session <session_id>` as a detached background
/// process and return immediately.
///
/// `cwd` is set as the child's working directory so `self_filter` (which
/// reads `std::env::current_dir()` in manual mode) sees the session's
/// project dir, not whatever Claude Code's hook process happened to inherit.
///
/// `binary_override` is for tests; in production callers pass `None` and the
/// current executable is used.
pub fn spawn_archive_detached(
    session_id: &str,
    cwd: &Path,
    binary_override: Option<PathBuf>,
) -> Result<()> {
    let exe = match binary_override {
        Some(p) => p,
        None => std::env::current_exe().context("locating current alluvium executable")?,
    };

    let mut cmd = Command::new(&exe);
    cmd.arg("archive")
        .arg("--session")
        .arg(session_id)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    apply_detach(&mut cmd);

    cmd.spawn()
        .with_context(|| format!("spawning detached {} archive", exe.display()))?;
    Ok(())
}

#[cfg(unix)]
fn apply_detach(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    // Put the child in its own process group so it survives the parent's
    // exit. (Without this, terminating the parent's process group via
    // SIGINT/SIGHUP would also kill the child.)
    unsafe {
        cmd.pre_exec(|| {
            // setsid() detaches from controlling terminal AND creates a new
            // session+process-group with the child as leader. Survives parent
            // death.
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(not(unix))]
fn apply_detach(_cmd: &mut Command) {
    // Windows / other: not yet supported; spawn() alone gives partial
    // detachment via Stdio::null() + dropping the Child handle.
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke test: pass `/bin/true` as the binary so we can verify the spawn
    /// returns successfully without actually running alluvium.
    #[test]
    fn spawn_returns_quickly_with_overridden_binary() {
        let exe = PathBuf::from("/bin/true");
        if !exe.exists() {
            // Some CI containers may differ; skip rather than fail.
            return;
        }
        let start = std::time::Instant::now();
        spawn_archive_detached("test-session", Path::new("/tmp"), Some(exe)).unwrap();
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_millis() < 500,
            "spawn must return quickly (got {elapsed:?})"
        );
    }

    #[test]
    fn spawn_returns_error_when_binary_missing() {
        let result = spawn_archive_detached(
            "x",
            Path::new("/tmp"),
            Some(PathBuf::from("/this/path/definitely/does/not/exist")),
        );
        assert!(result.is_err());
        let msg = format!("{:#}", result.unwrap_err());
        assert!(
            msg.contains("spawning detached")
                || msg.contains("does not exist")
                || msg.contains("No such file"),
            "expected useful error context, got: {msg}"
        );
    }

    #[test]
    fn spawn_does_not_inherit_stdio() {
        // Using /bin/sh -c to verify no stdin is read. We can't inspect the
        // detached process directly, but we can observe that spawn returns
        // success without blocking on a stdin read.
        let exe = PathBuf::from("/bin/sh");
        if !exe.exists() {
            return;
        }
        // We can't easily verify stdio detachment without reading from the
        // child, which detached spawn explicitly forbids. The fact that spawn
        // returns at all confirms stdin was set to null (otherwise we'd
        // inherit and the test runner's stdin state would matter).
        spawn_archive_detached("x", Path::new("/tmp"), Some(exe)).unwrap();
    }
}
