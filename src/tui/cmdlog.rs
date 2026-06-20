//! Command-log ring buffer and the `LoggingRunner` decorator.
//!
//! `CmdLog` is always compiled (no `tui` feature gate) so tests can import it
//! directly without the full ratatui dep.
//!
//! `LoggingRunner` is gated behind `#[cfg(feature = "tui")]` because it imports
//! the `DistroboxRunner` trait, which lives in `src/dbox/runner.rs` and is always
//! compiled — but the decorator itself is only used by `app.rs` (tui feature).
#![allow(dead_code)]

use std::collections::VecDeque;

// ─── CmdLogEntry ─────────────────────────────────────────────────────────────

/// A single entry in the command log ring buffer.
#[derive(Debug, Clone)]
pub struct CmdLogEntry {
    /// Space-joined program+args, e.g. `"distrobox create --name web"`.
    /// Stdout/stderr are intentionally excluded (privacy + size; this is a record
    /// of WHAT ran, not what it printed).
    pub argv: String,
    /// Exit code; `None` for interactive spawns where the code is not available.
    pub status: Option<i32>,
    /// Monotonic sequence id for stable ordering (newest has highest seq).
    pub seq: u64,
}

// ─── CmdLog ──────────────────────────────────────────────────────────────────

/// Bounded ring buffer of command-log entries.
///
/// `push` drops the OLDEST entry (front of deque) when the cap is exceeded,
/// so the buffer always holds the most recent `cap` entries.
pub struct CmdLog {
    buf: VecDeque<CmdLogEntry>,
    cap: usize,
    next_seq: u64,
}

impl CmdLog {
    /// Create a new log with the given capacity (e.g. 200).
    pub fn new(cap: usize) -> Self {
        CmdLog {
            buf: VecDeque::new(),
            cap,
            next_seq: 0,
        }
    }

    /// Append a new entry. Drops the oldest entry when the cap is exceeded.
    pub fn push(&mut self, argv: String, status: Option<i32>) {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.buf.push_back(CmdLogEntry { argv, status, seq });
        // Drop oldest past cap.
        while self.buf.len() > self.cap {
            self.buf.pop_front();
        }
    }

    /// Iterate over all entries, oldest first (suitable for "newest at bottom" display).
    pub fn entries(&self) -> impl Iterator<Item = &CmdLogEntry> {
        self.buf.iter()
    }

    /// Number of entries currently in the log.
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// Whether the log is empty.
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

// ─── LoggingRunner decorator ──────────────────────────────────────────────────

#[cfg(feature = "tui")]
pub use logging_runner::LoggingRunner;

#[cfg(feature = "tui")]
mod logging_runner {
    use std::sync::{Arc, Mutex};

    use super::CmdLog;
    use crate::dbox::runner::{CmdOutput, DistroboxRunner, Invocation, RunMode, RunnerError};
    use crate::tui::history::{redact_argv, HistoryStore};

    /// A `DistroboxRunner` decorator that captures the real argv + exit code of
    /// every spawned command into a shared `CmdLog` ring buffer AND the persistent
    /// `HistoryStore` (cross-session, redacted).
    ///
    /// DryRun invocations are NOT logged (they never spawn a real process).
    ///
    /// Wire-in: `app::run` wraps the injected runner in `LoggingRunner` before
    /// passing it to `run_loop`/`spawn_worker`. The same `Arc<Mutex<CmdLog>>`
    /// handle is also stored on `Model` so the view can read it during render.
    pub struct LoggingRunner {
        inner: Arc<dyn DistroboxRunner>,
        log: Arc<Mutex<CmdLog>>,
        history: Arc<Mutex<HistoryStore>>,
    }

    impl LoggingRunner {
        pub fn new(
            inner: Arc<dyn DistroboxRunner>,
            log: Arc<Mutex<CmdLog>>,
            history: Arc<Mutex<HistoryStore>>,
        ) -> Self {
            LoggingRunner {
                inner,
                log,
                history,
            }
        }
    }

    impl DistroboxRunner for LoggingRunner {
        fn run(&self, inv: Invocation) -> Result<CmdOutput, RunnerError> {
            let argv = inv.argv().join(" ");
            let skip = inv.mode == RunMode::DryRun; // dry-runs never spawn; don't log
            let out = self.inner.run(inv);
            if !skip {
                let status = out.as_ref().ok().map(|o| o.status);
                if let Ok(mut log) = self.log.lock() {
                    log.push(argv.clone(), status);
                }
                // Persist to cross-session history (redacted, best-effort).
                if let Ok(h) = self.history.lock() {
                    h.append(redact_argv(&argv), status);
                }
            }
            out
        }

        fn run_interactive(&self, inv: Invocation) -> Result<i32, RunnerError> {
            let argv = inv.argv().join(" ");
            let res = self.inner.run_interactive(inv);
            let status = res.as_ref().ok().copied();
            if let Ok(mut log) = self.log.lock() {
                log.push(argv.clone(), status);
            }
            // Persist to cross-session history (redacted, best-effort).
            if let Ok(h) = self.history.lock() {
                h.append(redact_argv(&argv), status);
            }
            res
        }
    }
}
