use clap::{Parser, Subcommand};
use std::path::Path;

pub mod apply;
pub mod create;
pub mod doctor;
pub mod edit;
pub mod enter;
pub mod inspect;
pub mod list;
pub mod output;
pub mod rm;
pub mod tui_cmd;
pub mod up;

/// Cwd-Boxfile auto-discovery filename.
pub const CWD_BOXFILE: &str = "Boxfile.toml";

/// Returns `Some("Boxfile.toml")` when a `Boxfile.toml` exists in `dir`
/// (pass `std::env::current_dir()` for real usage, or a test directory).
/// Returns `None` otherwise.
///
/// The returned string is the exact relative path passed to the parse layer.
pub fn discover_boxfile_in(dir: &Path) -> Option<&'static str> {
    if dir.join(CWD_BOXFILE).exists() {
        Some(CWD_BOXFILE)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn discover_boxfile_present_returns_filename() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("Boxfile.toml"),
            "name = \"x\"\nimage = \"y\"\n",
        )
        .unwrap();
        assert_eq!(discover_boxfile_in(dir.path()), Some("Boxfile.toml"));
    }

    #[test]
    fn discover_boxfile_absent_returns_none() {
        let dir = TempDir::new().unwrap();
        assert_eq!(discover_boxfile_in(dir.path()), None);
    }

    #[test]
    fn discover_boxfile_other_files_not_matched() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("boxfile.toml"),
            "name = \"x\"\nimage = \"y\"\n",
        )
        .unwrap();
        // Lower-case "boxfile.toml" should NOT match — we look for "Boxfile.toml".
        assert_eq!(discover_boxfile_in(dir.path()), None);
    }
}

/// cbox — a cozy distrobox manager
#[derive(Parser, Debug)]
#[command(
    name = "cbox",
    about = "A cozy distrobox manager",
    version,
    propagate_version = true
)]
pub struct Cli {
    /// Emit machine JSON output (where supported).
    #[arg(long, global = true)]
    pub json: bool,

    /// Suppress cozy chrome; errors still print to stderr.
    #[arg(short = 'q', long, global = true)]
    pub quiet: bool,

    /// Increase verbosity (-v shows argv, -vv streams child output).
    #[arg(short = 'v', long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Disable ANSI colors (auto-disabled when stdout is not a TTY or NO_COLOR is set).
    #[arg(long, global = true)]
    pub no_color: bool,

    /// Assume yes to confirmations.
    #[arg(short = 'y', long, global = true)]
    pub yes: bool,

    /// Print the would-be argv without executing.
    #[arg(long, global = true)]
    pub dry_run: bool,

    /// Override backend detection.
    #[arg(long, value_name = "podman|docker", global = true)]
    pub backend: Option<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Create a new distrobox.
    Create(create::CreateArgs),

    /// List distroboxes.
    List(list::ListArgs),

    /// Remove one or more distroboxes.
    #[command(alias = "destroy")]
    Rm(rm::RmArgs),

    /// Enter a distrobox (interactive).
    #[command(alias = "use")]
    Enter(enter::EnterArgs),

    /// Inspect a distrobox.
    #[command(alias = "show")]
    Inspect(inspect::InspectArgs),

    /// Edit the Boxfile for a distrobox.
    Edit(edit::EditArgs),

    /// Check your environment (distrobox + backend).
    Doctor(doctor::DoctorArgs),

    /// Converge an existing box to its Boxfile.
    Apply(apply::ApplyArgs),

    /// Create-if-absent then apply (the "just works" entry point).
    Up(up::UpArgs),

    /// Launch the cozy terminal cockpit.
    Tui(tui_cmd::TuiArgs),
}
