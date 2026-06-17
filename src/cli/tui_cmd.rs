//! `cbox tui` subcommand argument struct.
//! The TUI code itself lives in `src/tui/`; this module only declares the clap args
//! so `cbox tui --help` works regardless of whether the `tui` feature is enabled.

use clap::Args;

/// Launch the cozy terminal cockpit.
#[derive(Args, Debug)]
pub struct TuiArgs {
    // No additional args for v3.0.
}
