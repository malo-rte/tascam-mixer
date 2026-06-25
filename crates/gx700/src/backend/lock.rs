//! Advisory mutual exclusion for a MIDI interface.
//!
//! A MIDI device answers an RQ1 request with a reply stream on the *same* port.
//! If two processes read that port at once, the operating system hands each
//! incoming chunk to whichever reader happens to be waiting, so the reply gets
//! split between them and neither receives a complete message — both then time
//! out. It looks exactly like a stalled interface but is pure contention.
//!
//! [`PortLock`] enforces the only safe rule — *one accessor per interface at a
//! time* — with a `flock`-style advisory lock on a per-port lockfile in the
//! runtime directory. The lock lives on the open file descriptor, so the kernel
//! releases it automatically when the process exits, even on a crash. It is
//! *advisory*: it only excludes other rackctl processes that take the same lock,
//! not unrelated tools such as `amidi`.
//!
//! This is manufacturer-independent and is a prime candidate to lift into a
//! shared `rackctl-core` (or the future arbitration daemon) alongside the
//! rawmidi transport.

use std::env;
use std::fs::{File, OpenOptions, TryLockError, create_dir_all};
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// An exclusive advisory lock on one MIDI interface, held for as long as the
/// value lives. Dropping it (or the process exiting) releases the lock.
#[derive(Debug)]
pub(crate) struct PortLock {
    /// The locked file. Held only so the lock outlives this struct's creation;
    /// closing the descriptor on drop releases the `flock`.
    _file: File,
}

impl PortLock {
    /// Acquire the exclusive lock for `port` (a `hw:CARD,DEV` address) without
    /// blocking.
    ///
    /// # Errors
    /// - [`Error::PortBusy`] if another rackctl process already holds it.
    /// - [`Error::Transport`] if the lockfile cannot be created or locked.
    pub(crate) fn acquire(port: &str) -> Result<Self> {
        Self::acquire_at(&lock_path(port), port)
    }

    /// Acquire the lock using an explicit lockfile path. Split out from
    /// [`Self::acquire`] so tests can point it at an isolated temp file without
    /// touching the process environment.
    fn acquire_at(path: &Path, port: &str) -> Result<Self> {
        if let Some(parent) = path.parent() {
            create_dir_all(parent).map_err(|e| {
                Error::Transport(format!("creating lock dir {}: {e}", parent.display()))
            })?;
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)
            .map_err(|e| Error::Transport(format!("opening lockfile {}: {e}", path.display())))?;
        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(TryLockError::WouldBlock) => Err(Error::PortBusy(port.to_owned())),
            Err(TryLockError::Error(e)) => {
                Err(Error::Transport(format!("locking {}: {e}", path.display())))
            }
        }
    }
}

/// The lockfile path for `port`: `<runtime dir>/rackctl/midi-<sanitized>.lock`.
///
/// Prefers `$XDG_RUNTIME_DIR` (a per-user tmpfs, cleared on logout) and falls
/// back to the system temp dir when it is unset.
fn lock_path(port: &str) -> PathBuf {
    let base = env::var_os("XDG_RUNTIME_DIR").map_or_else(env::temp_dir, PathBuf::from);
    base.join("rackctl")
        .join(format!("midi-{}.lock", sanitize(port)))
}

/// Reduce a `hw:CARD,DEV` address to a filename-safe stem (`hw:1,0` -> `hw-1-0`).
fn sanitize(port: &str) -> String {
    port.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn sanitize_makes_a_safe_stem() {
        assert_eq!(sanitize("hw:1,0"), "hw-1-0");
        assert_eq!(sanitize("hw:CARD,DEV"), "hw-CARD-DEV");
    }

    #[test]
    fn lock_path_lives_under_a_rackctl_dir() {
        let p = lock_path("hw:1,0");
        assert!(p.ends_with("rackctl/midi-hw-1-0.lock"), "{p:?}");
    }

    #[test]
    fn second_acquire_reports_busy_then_releases() {
        // flock is per open-file-description, so two handles to the same path
        // contend even within one process: exercise the whole cycle here.
        let path = env::temp_dir().join(format!("rackctl-locktest-{}.lock", std::process::id()));
        let held = PortLock::acquire_at(&path, "hw:test,0").expect("first acquire succeeds");
        match PortLock::acquire_at(&path, "hw:test,0") {
            Err(Error::PortBusy(p)) => assert_eq!(p, "hw:test,0"),
            other => panic!("expected PortBusy, got {other:?}"),
        }
        drop(held);
        PortLock::acquire_at(&path, "hw:test,0").expect("re-acquire after drop succeeds");
    }
}
