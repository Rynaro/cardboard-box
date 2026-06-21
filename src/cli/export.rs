//! `cbox export <BOX> --app|--bin|--service|--list-apps|--list-bins [--delete] [--to]`
//! Surfaces a box's apps, binaries, or services onto the host via `distrobox-export`.

use super::output::OutputCtx;
use crate::boxfile::validate::is_valid_name;
use crate::core::{
    self,
    spec::{ExportSpec, ExportTarget},
};
use crate::dbox::runner::DistroboxRunner;
use crate::error::CboxError;
use clap::{ArgGroup, Args};

#[derive(Args, Debug)]
#[command(group = ArgGroup::new("target").required(true).multiple(false))]
pub struct ExportArgs {
    /// Box name.
    #[arg(value_name = "BOX")]
    pub box_name: String,

    /// Export a graphical app (creates a host .desktop launcher).
    #[arg(long, value_name = "APPNAME", group = "target")]
    pub app: Option<String>,

    /// Export a binary as a host wrapper script.
    #[arg(long, value_name = "GUEST_PATH", group = "target")]
    pub bin: Option<String>,

    /// Export a systemd service.
    #[arg(long, value_name = "NAME", group = "target")]
    pub service: Option<String>,

    /// List currently-exported apps.
    #[arg(long, group = "target")]
    pub list_apps: bool,

    /// List currently-exported binaries.
    #[arg(long, group = "target")]
    pub list_bins: bool,

    /// Host directory for an exported binary (maps to distrobox-export --export-path).
    /// Only valid with --bin.
    #[arg(long, value_name = "HOSTDIR")]
    pub to: Option<String>,

    /// Remove a previously-created export (combine with --app/--bin/--service).
    #[arg(long)]
    pub delete: bool,
}

pub fn run(
    args: &ExportArgs,
    global_json: bool,
    global_backend: Option<&str>,
    global_dry_run: bool,
    ctx: &OutputCtx,
    runner: &dyn DistroboxRunner,
) -> Result<(), CboxError> {
    // 1. Validate box name (exit 64 on bad name — before any runner call).
    if !is_valid_name(&args.box_name) {
        return Err(CboxError::usage(format!(
            "Invalid box name \"{}\". Names must match ^[a-zA-Z0-9][a-zA-Z0-9_.-]*$",
            args.box_name
        )));
    }

    // 2. Inter-flag guards clap's ArgGroup cannot express (exit 64).
    // --to is ONLY valid with --bin.
    if args.to.is_some() && args.bin.is_none() {
        return Err(CboxError::usage(
            "--to / --export-path is only valid with --bin",
        ));
    }
    // --delete is forbidden with list modes.
    if args.delete && (args.list_apps || args.list_bins) {
        return Err(CboxError::usage(
            "--delete cannot be combined with --list-apps or --list-bins",
        ));
    }

    // 3. Build ExportTarget.
    let target = if let Some(ref app) = args.app {
        ExportTarget::App { name: app.clone() }
    } else if let Some(ref bin) = args.bin {
        ExportTarget::Bin {
            path: bin.clone(),
            to: args.to.clone(),
        }
    } else if let Some(ref service) = args.service {
        ExportTarget::Service {
            name: service.clone(),
        }
    } else if args.list_apps {
        ExportTarget::ListApps
    } else {
        // list_bins is the only remaining possibility (ArgGroup ensures one is set).
        ExportTarget::ListBins
    };

    // 4. Resolve backend.
    let backend = core::resolve_backend(&args.box_name, global_backend, runner)?;

    // 5. Build spec + call core::export.
    let spec = ExportSpec {
        box_name: args.box_name.clone(),
        target,
        delete: args.delete,
        backend,
        dry_run: global_dry_run,
    };

    let outcome = core::export(&spec, runner)?;

    // 6. Render output.
    if global_json {
        ctx.print_json(&outcome);
        return Ok(());
    }

    render_export_human(&outcome, ctx);
    Ok(())
}

fn render_export_human(outcome: &crate::core::spec::ExportOutcome, ctx: &OutputCtx) {
    if outcome.action == "export-list" {
        // List mode: one entry per line; empty → informational message.
        if outcome.entries.is_empty() {
            if !ctx.quiet {
                let noun = if outcome.mode == "list-apps" {
                    "apps"
                } else {
                    "binaries"
                };
                println!("No {noun} exported from \"{}\".", outcome.box_name);
            }
        } else {
            for entry in &outcome.entries {
                println!("{entry}");
            }
        }
        return;
    }

    // Mutating modes (export / export-delete).
    let target = outcome.target.as_deref().unwrap_or("(unknown)");
    if outcome.deleted {
        ctx.success(&format!(
            "Removed exported {} \"{}\" for \"{}\".",
            outcome.mode, target, outcome.box_name
        ));
    } else {
        let verb_suffix = match outcome.mode.as_str() {
            "app" => "to your host menu",
            "bin" => "as a host wrapper",
            "service" => "as a host service",
            _ => "",
        };
        ctx.success(&format!(
            "Exported {} \"{}\" from \"{}\" {}.",
            outcome.mode, target, outcome.box_name, verb_suffix
        ));
    }

    // Echo distrobox-export's own stdout headline in non-quiet mode (provenance).
    if !outcome.detail.is_empty() && !ctx.quiet {
        ctx.hint(&outcome.detail);
    }
}
