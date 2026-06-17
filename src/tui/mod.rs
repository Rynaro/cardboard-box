//! Phase 3 TUI — ratatui + crossterm, behind the `tui` feature flag.
//! Architecture: TEA (Model–Message–update–view) + Effect indirection.
//! The TUI calls `core::` directly; it never imports `cli::`.

// All TUI internals live behind the `tui` feature so the lean (default-off) build
// compiles cleanly with no dead code. Only `TuiConfig` + the stub `run` below are
// always present (the CLI entry point references them unconditionally).
#[cfg(feature = "tui")]
pub mod app;
#[cfg(feature = "tui")]
pub mod effect;
#[cfg(feature = "tui")]
pub mod message;
#[cfg(feature = "tui")]
pub mod model;
#[cfg(feature = "tui")]
pub mod update;
#[cfg(feature = "tui")]
pub mod view;

use std::sync::Arc;

use crate::dbox::runner::DistroboxRunner;
use crate::error::CboxError;

/// Configuration passed to `tui::run` from the CLI entry point.
pub struct TuiConfig {
    /// Backend override string (e.g. "podman" or "docker").
    pub backend_override: Option<String>,
}

/// Launch the TUI.
///
/// `runner` is injected (never constructed inside `tui/`) so tests can pass a `MockRunner`.
/// Returns when the user quits.
#[cfg(feature = "tui")]
pub fn run(cfg: TuiConfig, runner: Arc<dyn DistroboxRunner>) -> Result<(), CboxError> {
    use crate::dbox::backend::Backend;
    let backend = Backend::detect_or_default(cfg.backend_override.as_deref())?;
    app::run(runner, backend)
}

/// Stub when built without the `tui` feature — returns a cozy error (exit 70).
#[cfg(not(feature = "tui"))]
pub fn run(cfg: TuiConfig, _runner: Arc<dyn DistroboxRunner>) -> Result<(), CboxError> {
    // Consume the config so its fields aren't flagged dead in the lean build.
    let _ = cfg.backend_override;
    Err(CboxError::software(
        "This build of cbox has no TUI. \
         Rebuild with --features tui, or use the subcommands: cbox list, cbox create, …",
    ))
}
