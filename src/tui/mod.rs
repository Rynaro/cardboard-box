//! Phase 3 TUI stub. Empty in P1.
//! Will use `ratatui` + `crossterm` behind the `tui` feature flag.
//! The TUI will call `core::` functions — not re-implement flag mapping.

#[cfg(feature = "tui")]
#[allow(dead_code)]
pub fn run() -> anyhow::Result<()> {
    todo!("TUI is Phase 3")
}
