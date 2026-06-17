//! `cbox apply <NAME>` — converge an existing box to its Boxfile.

use super::output::OutputCtx;
use crate::boxfile::validate::is_valid_name;
use crate::core::state_store::GuestStateStore;
use crate::core::{self, spec::ApplySpec};
use crate::dbox::backend::Backend;
use crate::dbox::runner::DistroboxRunner;
use crate::error::CboxError;
use clap::Args;

#[derive(Args, Debug)]
pub struct ApplyArgs {
    /// Box name to converge.
    #[arg(value_name = "NAME")]
    pub name: Option<String>,

    /// Path to a Boxfile.toml (overrides label/XDG resolution).
    #[arg(long = "file", value_name = "PATH")]
    pub file: Option<String>,

    /// Re-run all provision steps, ignoring stored hashes.
    #[arg(long)]
    pub force: bool,

    /// Re-run only provision step IDX (0-based, repeatable).
    #[arg(long = "redo", value_name = "IDX")]
    pub redo: Vec<usize>,

    /// Apply package/diff changes but skip [[provision]] steps.
    #[arg(long)]
    pub no_provision: bool,

    /// Permit recreate-class changes (destroys+recreates the container).
    #[arg(long)]
    pub recreate: bool,
}

pub fn run(
    args: &ApplyArgs,
    global_dry_run: bool,
    global_backend: Option<&str>,
    global_yes: bool,
    ctx: &OutputCtx,
    runner: &dyn DistroboxRunner,
) -> Result<(), CboxError> {
    let backend = Backend::detect(global_backend)?;

    // Resolve the box name
    let name = args
        .name
        .as_ref()
        .ok_or_else(|| CboxError::usage("NAME is required for `cbox apply`."))?;

    if !is_valid_name(name) {
        return Err(CboxError::usage(format!(
            "Invalid box name \"{name}\". Names must match ^[a-zA-Z0-9][a-zA-Z0-9_.-]*$"
        )));
    }

    // Resolve Boxfile path: --file → XDG fallback
    let boxfile_path = if let Some(ref file) = args.file {
        file.clone()
    } else {
        let config_home = std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
            format!("{home}/.config")
        });
        format!("{config_home}/cbox/boxes/{name}/Boxfile.toml")
    };

    let spec = ApplySpec {
        name: name.clone(),
        boxfile_path,
        force: args.force,
        redo: args.redo.clone(),
        no_provision: args.no_provision,
        recreate: args.recreate,
        yes: global_yes,
        dry_run: global_dry_run,
        backend,
    };

    let store = GuestStateStore;
    let outcome = core::apply(&spec, &store, runner)?;

    if ctx.json {
        let v = serde_json::to_value(&outcome)
            .unwrap_or_else(|_| serde_json::json!({"ok": true, "action": "apply"}));
        ctx.print_json(&v);
    } else {
        render_apply_outcome(&outcome, ctx);
    }

    Ok(())
}

fn render_apply_outcome(outcome: &crate::core::spec::ApplyOutcome, ctx: &OutputCtx) {
    if !ctx.quiet {
        println!("Applying Boxfile for \"{}\" ...", outcome.name);
    }
    for step in &outcome.steps {
        if ctx.quiet {
            break;
        }
        let marker = match step.status.as_str() {
            "skipped" => "skipped",
            "ran" => "ran    ",
            "copied" => "copied ",
            "failed" => "FAILED ",
            _ => "?      ",
        };
        println!(
            "  provision  [{}] {}  {} {}",
            step.idx, step.step_type, marker, step.hash
        );
    }

    let s = &outcome.summary;
    let detail = format!("{} ran, {} skipped, {} copied", s.ran, s.skipped, s.copied);
    if s.failed > 0 {
        if !ctx.quiet {
            eprintln!(
                "error: Provisioning stopped. {} step(s) failed. ({detail})",
                s.failed
            );
        }
    } else {
        ctx.success(&format!(
            "Box \"{}\" is up to date ({})",
            outcome.name, detail
        ));
    }
}
