use std::time::Duration;

use crate::error::exit;
use thiserror::Error;

/// How to run the child process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunMode {
    /// Collect stdout/stderr/exit — create/list/rm/inspect/doctor.
    Capture,
    /// Inherit TTY, no capture — enter/use.
    Interactive,
    /// Never spawn; return the would-be argv as stdout.
    DryRun,
    /// Stream stdout lines to a callback, cancellable via an `AtomicBool`.
    /// Used for `<backend> logs -f <id>` on a dedicated thread (GAP-2).
    Stream,
}

/// A single subprocess invocation descriptor.
#[derive(Debug, Clone)]
pub struct Invocation {
    pub program: String,
    pub args: Vec<String>,
    pub mode: RunMode,
    pub env: Vec<(String, String)>,
}

impl Invocation {
    pub fn new(program: impl Into<String>, args: Vec<String>, mode: RunMode) -> Self {
        Self {
            program: program.into(),
            args,
            mode,
            env: Vec::new(),
        }
    }

    #[allow(dead_code)]
    pub fn with_env(mut self, env: Vec<(String, String)>) -> Self {
        self.env = env;
        self
    }

    /// The full argv as a Vec (program + args), for display/provenance.
    pub fn argv(&self) -> Vec<String> {
        let mut v = vec![self.program.clone()];
        v.extend(self.args.iter().cloned());
        v
    }
}

/// Output from a captured child process.
#[derive(Debug, Clone)]
pub struct CmdOutput {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
    /// The exact argv (program + args), for -v and --json provenance.
    pub argv: Vec<String>,
}

/// Spawn-level errors (binary not found, IO error, etc.).
#[derive(Debug, Error)]
pub enum RunnerError {
    #[error("distrobox isn't installed or isn't on PATH. See:  cbox doctor")]
    BinaryNotFound { program: String },

    #[error("IO error spawning {program}: {source}")]
    Io {
        program: String,
        source: std::io::Error,
    },

    #[error("Interactive spawn failed for {program}: {source}")]
    InteractiveSpawnFailed {
        program: String,
        source: std::io::Error,
    },

    #[error("{program} timed out after {seconds}s")]
    Timeout { program: String, seconds: u64 },
}

impl RunnerError {
    pub fn exit_code(&self) -> i32 {
        match self {
            RunnerError::BinaryNotFound { .. } => exit::SOFTWARE,
            RunnerError::Io { .. } => exit::IOERR,
            RunnerError::InteractiveSpawnFailed { .. } => exit::IOERR,
            // Timeout maps to TEMPFAIL: backend not responding right now.
            RunnerError::Timeout { .. } => exit::TEMPFAIL,
        }
    }
}

/// The core process-wrapper seam. All spawns go through this.
/// No handler shells out directly — every spawn goes through this trait.
pub trait DistroboxRunner: Send + Sync {
    /// Capture mode: collect stdout/stderr/exit.
    /// DryRun mode: return the would-be argv as stdout without spawning.
    fn run(&self, inv: Invocation) -> Result<CmdOutput, RunnerError>;

    /// Interactive mode: inherit TTY, return exit code.
    fn run_interactive(&self, inv: Invocation) -> Result<i32, RunnerError>;

    /// Capture mode with a wall-clock deadline.
    ///
    /// Default: ignore the deadline and delegate to `run` (preserves behavior
    /// for runners that can't time out, e.g. the simplest mocks). `RealRunner`
    /// overrides this with a real poll-`try_wait` watchdog.
    ///
    /// ONLY called from the two silent poll effects (`SilentLoadList`,
    /// `StatsPoll`) — the rest of the CLI keeps using `run`/`run_interactive`
    /// unchanged (GAP-1 containment).
    fn run_with_timeout(
        &self,
        inv: Invocation,
        timeout: Duration,
    ) -> Result<CmdOutput, RunnerError> {
        let _ = timeout;
        self.run(inv)
    }

    /// Streaming mode: tails stdout line-by-line, calling `on_line` for each.
    ///
    /// The `stop` flag is checked between reads; when set to `true`, the child
    /// is killed and reaped and the function returns. On EOF (process exits
    /// normally), the child is reaped and the exit code returned.
    ///
    /// **Default impl:** returns `Err("streaming unsupported")` — every existing
    /// mock stays green without implementing this method (R6/G-COMPAT).
    ///
    /// `RealRunner` provides the real BufReader loop + kill+reap idiom.
    /// MUST be called on a **dedicated thread** (never the worker); see GAP-2.
    fn run_stream(
        &self,
        inv: Invocation,
        on_line: &mut dyn FnMut(String),
        stop: &std::sync::atomic::AtomicBool,
    ) -> Result<i32, RunnerError> {
        let _ = (inv, on_line, stop);
        Err(RunnerError::Io {
            program: "run_stream".to_string(),
            source: std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "streaming unsupported by this runner",
            ),
        })
    }
}
