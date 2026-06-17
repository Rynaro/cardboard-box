use thiserror::Error;

/// Exit code taxonomy per §3.8 (sysexits-aligned).
#[allow(dead_code)]
pub mod exit {
    pub const OK: i32 = 0;
    pub const USAGE: i32 = 64;
    pub const DATAERR: i32 = 65;
    pub const UNAVAILABLE: i32 = 69;
    pub const SOFTWARE: i32 = 70;
    pub const IOERR: i32 = 74;
    pub const TEMPFAIL: i32 = 75;
    pub const BACKEND_NONZERO: i32 = 125;
}

/// Typed error enum for cbox — core/library layers use this.
/// `main.rs` maps each variant's `.exit_code()` and renders the message.
#[derive(Debug, Error)]
pub enum CboxError {
    /// Bad CLI usage, invalid NAME, --json on interactive command.
    #[error("{message}")]
    Usage { message: String },

    /// Boxfile schema / validation failure.
    #[error("{message}")]
    DataErr { message: String },

    /// Named box does not exist.
    #[error("No box named \"{name}\". Create it with:  cbox create {name}")]
    BoxNotFound { name: String },

    /// distrobox binary missing or unusable.
    #[error("{message}")]
    Software { message: String },

    /// Spawn / IO failure.
    #[error("IO error: {message}")]
    IoErr { message: String },

    /// Backend (podman/docker) unreachable.
    #[error("{message}")]
    TempFail { message: String },

    /// Wrapped distrobox/backend exited non-zero.
    #[error("Command failed (exit {code}): {headline}\n  argv: {argv}\n  stderr: {stderr_tail}")]
    Backend {
        code: i32,
        headline: String,
        stderr_tail: String,
        argv: String,
    },

    /// Runner error promoted to CboxError.
    #[error(transparent)]
    Runner(#[from] crate::dbox::runner::RunnerError),
}

impl CboxError {
    pub fn exit_code(&self) -> i32 {
        match self {
            CboxError::Usage { .. } => exit::USAGE,
            CboxError::DataErr { .. } => exit::DATAERR,
            CboxError::BoxNotFound { .. } => exit::UNAVAILABLE,
            CboxError::Software { .. } => exit::SOFTWARE,
            CboxError::IoErr { .. } => exit::IOERR,
            CboxError::TempFail { .. } => exit::TEMPFAIL,
            CboxError::Backend { .. } => exit::BACKEND_NONZERO,
            CboxError::Runner(e) => e.exit_code(),
        }
    }

    pub fn usage(msg: impl Into<String>) -> Self {
        CboxError::Usage {
            message: msg.into(),
        }
    }

    pub fn dataerr(msg: impl Into<String>) -> Self {
        CboxError::DataErr {
            message: msg.into(),
        }
    }

    pub fn software(msg: impl Into<String>) -> Self {
        CboxError::Software {
            message: msg.into(),
        }
    }

    pub fn tempfail(msg: impl Into<String>) -> Self {
        CboxError::TempFail {
            message: msg.into(),
        }
    }

    pub fn ioerr(msg: impl Into<String>) -> Self {
        CboxError::IoErr {
            message: msg.into(),
        }
    }

    pub fn box_not_found(name: impl Into<String>) -> Self {
        CboxError::BoxNotFound { name: name.into() }
    }

    /// Build a Backend error for a failed provision step.
    ///
    /// Unlike `backend_error`, this bypasses distrobox-signal pattern matching and
    /// surfaces the step's own output directly.
    ///
    /// * `idx`       — 0-based step index
    /// * `step_type` — "shell" or "copy"
    /// * `code`      — non-zero exit code from the subprocess
    /// * `output`    — captured stderr (or stdout if stderr is empty), already
    ///   tail-truncated to a sane size
    /// * `argv`      — the full argv that was executed
    pub fn provision_step_error(
        idx: usize,
        step_type: &str,
        code: i32,
        output: &str,
        argv: &[String],
    ) -> Self {
        let argv_str = argv.join(" ");
        let headline = format!("Provision step [{idx}] ({step_type}) failed (exit {code})");
        CboxError::Backend {
            code,
            headline,
            stderr_tail: output.to_string(),
            argv: argv_str,
        }
    }

    /// Build a Backend error with cozy pattern-matching on distrobox stderr signals (§4.3).
    pub fn backend_error(code: i32, stderr: &str, argv: &[String]) -> Self {
        let argv_str = argv.join(" ");
        let last5: Vec<&str> = stderr
            .lines()
            .rev()
            .take(5)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let stderr_tail = last5.join("\n");

        let headline = if stderr.contains("already exists") {
            let name = argv
                .iter()
                .skip_while(|a| a.as_str() != "--name")
                .nth(1)
                .cloned()
                .unwrap_or_default();
            format!(
                "A box named \"{name}\" already exists. Use a different name or remove it:  cbox rm {name}"
            )
        } else if stderr.contains("image")
            && (stderr.contains("not found") || stderr.contains("pull"))
        {
            let image = argv
                .iter()
                .skip_while(|a| a.as_str() != "--image")
                .nth(1)
                .cloned()
                .unwrap_or_default();
            format!(
                "Couldn't pull image \"{image}\". Check the name, or try:  cbox create … --pull"
            )
        } else if stderr.contains("command not found") && stderr.contains("distrobox") {
            "distrobox isn't installed or isn't on PATH. See:  cbox doctor".to_string()
        } else if stderr.contains("refused") || stderr.contains("cannot connect") {
            "Can't reach podman/docker. Is the service running?  cbox doctor".to_string()
        } else {
            format!("distrobox exited with code {code}")
        };

        CboxError::Backend {
            code,
            headline,
            stderr_tail,
            argv: argv_str,
        }
    }
}
