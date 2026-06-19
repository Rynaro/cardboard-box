//! `cbox apply <NAME>` — converge an existing box to its Boxfile.

use super::discover_boxfile_in;
use super::output::OutputCtx;
use crate::boxfile::{self, validate::is_valid_name};
use crate::core::secret_inject::{resolve_secret_env, SecretScope};
use crate::core::state_store::GuestStateStore;
use crate::core::{self, spec::ApplySpec};
use crate::dbox::runner::DistroboxRunner;
use crate::error::CboxError;
use crate::secret::SecretStore;
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

/// Thin wrapper that calls `run_with_store` without a secret store.
#[allow(dead_code)]
pub fn run(
    args: &ApplyArgs,
    global_dry_run: bool,
    global_backend: Option<&str>,
    global_yes: bool,
    ctx: &OutputCtx,
    runner: &dyn DistroboxRunner,
) -> Result<(), CboxError> {
    run_with_store(
        args,
        global_dry_run,
        global_backend,
        global_yes,
        ctx,
        runner,
        None,
    )
}

/// Full implementation: resolves secrets ALL-OR-NOTHING (D3) before any spawn.
///
/// When `store` is `Some`, secrets in the Boxfile are resolved eagerly:
/// - `persist=false` → `spec.provision_env_keys` / `provision_env` (provision-time only)
/// - `persist=true` on `--recreate` → `spec.recreate_env_flags` / `recreate_env_values`
/// - `[env]` on `--recreate` → `spec.recreate_plain_env` (non-secret)
///
/// A missing or unavailable secret returns exit 75 BEFORE `core::apply` runs —
/// nothing is spawned (D3 safety guarantee).
#[allow(clippy::too_many_arguments)]
pub fn run_with_store(
    args: &ApplyArgs,
    global_dry_run: bool,
    global_backend: Option<&str>,
    global_yes: bool,
    ctx: &OutputCtx,
    runner: &dyn DistroboxRunner,
    store: Option<&dyn SecretStore>,
) -> Result<(), CboxError> {
    // Resolution precedence:
    //   1. --file PATH given → use it (name comes from Boxfile).
    //   2. NAME given → existing label/XDG path (--file overrides path only).
    //   3. ./Boxfile.toml exists in cwd → use it (name + path from Boxfile).
    //   4. → improved usage error.

    // For secret resolution we need the parsed Boxfile when available.
    let (name, boxfile_path, resolved_bf) = if let Some(ref file) = args.file {
        // Priority 1: --file given; resolve name from the Boxfile.
        let (bf, warnings) = boxfile::parse_file(file)?;
        for w in &warnings {
            eprintln!("warn: {w}");
        }
        if !is_valid_name(&bf.name) {
            return Err(CboxError::dataerr(format!(
                "Boxfile name \"{}\" is invalid.",
                bf.name
            )));
        }
        let bf_name = bf.name.clone();
        (bf_name, file.clone(), Some(bf))
    } else if let Some(ref name) = args.name {
        // Priority 2: positional NAME given — XDG resolution.
        if !is_valid_name(name) {
            return Err(CboxError::usage(format!(
                "Invalid box name \"{name}\". Names must match ^[a-zA-Z0-9][a-zA-Z0-9_.-]*$"
            )));
        }
        let config_home = std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
            format!("{home}/.config")
        });
        let path = format!("{config_home}/cbox/boxes/{name}/Boxfile.toml");
        // For the name-only path we still need to read the Boxfile for secret resolution.
        // If no store is provided or the file doesn't exist, resolved_bf stays None.
        let bf_opt = if store.is_some() {
            boxfile::parse_file(&path).ok().map(|(bf, _)| bf)
        } else {
            None
        };
        (name.clone(), path, bf_opt)
    } else if let Some(cwd_path) = std::env::current_dir()
        .ok()
        .as_deref()
        .and_then(discover_boxfile_in)
    {
        // Priority 3: Boxfile.toml found in the current working directory.
        if !ctx.quiet {
            ctx.hint(&format!("Using ./{cwd_path}"));
        }
        let (bf, warnings) = boxfile::parse_file(cwd_path)?;
        for w in &warnings {
            eprintln!("warn: {w}");
        }
        if !is_valid_name(&bf.name) {
            return Err(CboxError::dataerr(format!(
                "Boxfile name \"{}\" is invalid.",
                bf.name
            )));
        }
        let bf_name = bf.name.clone();
        (bf_name, cwd_path.to_string(), Some(bf))
    } else {
        return Err(CboxError::usage(
            "NAME is required unless --file is provided or a Boxfile.toml exists in the current directory.",
        ));
    };

    // ── Secret resolution — ALL-OR-NOTHING before any spawn (D3) ────────────
    let mut provision_env_keys: Vec<String> = Vec::new();
    let mut provision_env: Vec<(String, String)> = Vec::new();
    let mut recreate_env_flags: Vec<String> = Vec::new();
    let mut recreate_env_values: Vec<(String, String)> = Vec::new();
    let mut recreate_plain_env: Vec<(String, String)> = Vec::new();

    if let (Some(secret_store), Some(ref bf)) = (store, &resolved_bf) {
        if !bf.secrets.is_empty() {
            // persist=false → provision-time injection only
            let provision_only =
                resolve_secret_env(&name, &bf.secrets, SecretScope::ProvisionOnly, secret_store)?;
            provision_env_keys = provision_only.iter().map(|(k, _)| k.clone()).collect();
            provision_env = provision_only;

            // persist=true is only needed on the --recreate path (the create call needs them)
            if args.recreate {
                let persisted =
                    resolve_secret_env(&name, &bf.secrets, SecretScope::Persisted, secret_store)?;
                recreate_env_flags = persisted.iter().map(|(k, _)| k.clone()).collect();
                recreate_env_values = persisted;
            }
        }
        // Populate plain_env for recreate path from [env] table
        if args.recreate {
            for (k, v) in &bf.env {
                recreate_plain_env.push((k.clone(), v.clone()));
            }
        }
    }

    // Route to whichever engine actually hosts this box — mirrors the pattern
    // used by `enter` so a box on a non-default backend is found without the
    // user having to pass --backend explicitly.
    let backend = core::resolve_backend(&name, global_backend, runner)?;

    let spec = ApplySpec {
        force: args.force,
        redo: args.redo.clone(),
        no_provision: args.no_provision,
        recreate: args.recreate,
        yes: global_yes,
        dry_run: global_dry_run,
        provision_env_keys,
        provision_env,
        recreate_env_flags,
        recreate_env_values,
        recreate_plain_env,
        ..ApplySpec::new(name, boxfile_path, backend)
    };

    let state_store = GuestStateStore;
    let outcome = core::apply(&spec, &state_store, runner).inspect_err(|e| {
        emit_provision_failure_hint(e, &spec.name, &spec.boxfile_path, ctx);
    })?;

    if ctx.json {
        let v = serde_json::to_value(&outcome)
            .unwrap_or_else(|_| serde_json::json!({"ok": true, "action": "apply"}));
        ctx.print_json(&v);
    } else {
        render_apply_outcome(&outcome, ctx);
    }

    Ok(())
}

/// Emit a Vagrant-style debug/resume hint to stderr when a provision step fails.
/// Only emits in human mode (not --json, not --quiet).
/// The error is inspected by exit code: 125 (BACKEND_NONZERO) is the provision-failure code.
fn emit_provision_failure_hint(
    err: &crate::error::CboxError,
    name: &str,
    boxfile_path: &str,
    ctx: &OutputCtx,
) {
    if ctx.json || ctx.quiet {
        return;
    }
    if err.exit_code() != crate::error::exit::BACKEND_NONZERO {
        return;
    }
    eprintln!();
    eprintln!("hint: The box is still up.");
    eprintln!("hint: Enter it to debug:  cbox enter {name}");
    eprintln!("hint: After fixing, resume with:  cbox apply --file {boxfile_path}");
    eprintln!("hint: Completed steps are skipped; the failed step will re-run.");
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
