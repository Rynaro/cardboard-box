//! `cbox up [NAME]` — create-if-absent then apply.

use super::discover_boxfile_in;
use super::output::OutputCtx;
use crate::boxfile::model::DockerModeField;
use crate::boxfile::{self, validate::is_valid_name};
use crate::core::state_store::GuestStateStore;
use crate::core::{
    self,
    spec::{CreateSpec, DockerMode, MountSpec, UpSpec},
};
use crate::dbox::backend::Backend;
use crate::dbox::runner::DistroboxRunner;
use crate::error::CboxError;
use clap::Args;

#[derive(Args, Debug)]
pub struct UpArgs {
    /// Box name.
    #[arg(value_name = "NAME")]
    pub name: Option<String>,

    /// Path to a Boxfile.toml (name/image/etc. all from the Boxfile).
    #[arg(long = "file", value_name = "PATH")]
    pub file: Option<String>,

    /// Container image.
    #[arg(
        short = 'i',
        long,
        default_value = "registry.fedoraproject.org/fedora-toolbox:latest"
    )]
    pub image: String,

    /// Additional packages.
    #[arg(short = 'p', long = "package", value_name = "PKG")]
    pub packages: Vec<String>,

    /// Host:guest[:mode] mounts.
    #[arg(short = 'm', long = "mount", value_name = "H:G[:mode]")]
    pub mounts: Vec<String>,

    /// Docker access mode.
    #[arg(long, default_value = "none", value_name = "none|host|nested")]
    pub docker: String,

    /// Custom home directory.
    #[arg(long)]
    pub home: Option<String>,

    /// Hostname inside the box.
    #[arg(long)]
    pub hostname: Option<String>,

    /// Enable systemd/init inside the box.
    #[arg(long)]
    pub init: bool,

    /// Pull the image even if present.
    #[arg(long)]
    pub pull: bool,

    // Apply flags
    /// Re-run all provision steps, ignoring stored hashes.
    #[arg(long)]
    pub force: bool,

    /// Re-run only provision step IDX (0-based, repeatable).
    #[arg(long = "redo", value_name = "IDX")]
    pub redo: Vec<usize>,

    /// Apply package/diff changes but skip [[provision]] steps.
    #[arg(long)]
    pub no_provision: bool,

    /// Permit recreate-class changes.
    #[arg(long)]
    pub recreate: bool,
}

pub fn run(
    args: &UpArgs,
    global_dry_run: bool,
    global_backend: Option<&str>,
    global_yes: bool,
    ctx: &OutputCtx,
    runner: &dyn DistroboxRunner,
) -> Result<(), CboxError> {
    // Name resolution happens before backend resolution because we need the name
    // to look up which backend already hosts the box (mirrors the `enter` pattern).
    // We build the CreateSpec first using Backend::detect as a placeholder, then
    // replace the backend with the resolve_backend result once we have the name.
    let detected_backend = Backend::detect(global_backend)?;

    let mut create_spec = if let Some(ref file_path) = args.file {
        // Priority 1: --file explicitly given.
        spec_from_boxfile(file_path, &detected_backend)?
    } else if let Some(ref name) = args.name {
        // Priority 2: positional NAME given — existing label/XDG behaviour.
        if !is_valid_name(name) {
            return Err(CboxError::usage(format!(
                "Invalid box name \"{name}\". Names must match ^[a-zA-Z0-9][a-zA-Z0-9_.-]*$"
            )));
        }
        let docker_mode = parse_docker_mode(&args.docker)?;
        let mounts = parse_mounts(&args.mounts)?;

        CreateSpec {
            name: name.clone(),
            image: args.image.clone(),
            packages: args.packages.clone(),
            docker_mode,
            mounts,
            home: args.home.clone(),
            hostname: args.hostname.clone(),
            init: args.init,
            pull: args.pull,
            root: false,
            boxfile_path: None,
            unshare: None,
            backend: detected_backend.clone(),
            uid: get_uid(),
            dry_run: global_dry_run,
        }
    } else if let Some(cwd_path) = std::env::current_dir()
        .ok()
        .as_deref()
        .and_then(discover_boxfile_in)
    {
        // Priority 3: Boxfile.toml found in the current working directory.
        if !ctx.quiet {
            ctx.hint(&format!("Using ./{cwd_path}"));
        }
        spec_from_boxfile(cwd_path, &detected_backend)?
    } else {
        return Err(CboxError::usage(
            "NAME is required unless --file is provided or a Boxfile.toml exists in the current directory.",
        ));
    };

    // Route to whichever engine actually hosts this box — mirrors the pattern
    // used by `enter`. When the box doesn't exist yet, resolve_backend falls back
    // to the preferred usable backend, which is the correct target for creation.
    let backend = core::resolve_backend(&create_spec.name, global_backend, runner)?;
    create_spec.backend = backend;

    let up_spec = UpSpec {
        create_spec,
        apply_force: args.force,
        apply_redo: args.redo.clone(),
        no_provision: args.no_provision,
        recreate: args.recreate,
        yes: global_yes,
        dry_run: global_dry_run,
    };

    let store = GuestStateStore;
    // Capture name + boxfile_path before moving into the outcome for hint purposes.
    let box_name = up_spec.create_spec.name.clone();
    let boxfile_path_for_hint = up_spec.create_spec.boxfile_path.clone().unwrap_or_default();

    let outcome = core::up(&up_spec, &store, runner).inspect_err(|e| {
        emit_provision_failure_hint(e, &box_name, &boxfile_path_for_hint, ctx);
    })?;

    if ctx.json {
        let v = serde_json::to_value(&outcome)
            .unwrap_or_else(|_| serde_json::json!({"ok": true, "action": "up"}));
        ctx.print_json(&v);
    } else {
        if outcome.created && !ctx.quiet {
            ctx.success(&format!("Created box \"{}\"", outcome.name));
        }
        render_apply_outcome(&outcome.apply, ctx);
        if !ctx.quiet {
            ctx.hint(&format!("Enter it with:  cbox enter {}", outcome.name));
        }
    }

    Ok(())
}

/// Emit a Vagrant-style debug/resume hint to stderr when a provision step fails.
/// Only emits in human mode (not --json, not --quiet).
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
    if !boxfile_path.is_empty() {
        eprintln!("hint: After fixing, resume with:  cbox apply --file {boxfile_path}");
    } else {
        eprintln!("hint: After fixing, resume with:  cbox up --file <Boxfile.toml>");
    }
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
    if s.failed == 0 {
        ctx.success(&format!("Box \"{}\" is ready ({})", outcome.name, detail));
    }
}

fn spec_from_boxfile(path: &str, backend: &Backend) -> Result<CreateSpec, CboxError> {
    let (bf, warnings) = boxfile::parse_file(path)?;
    for w in &warnings {
        eprintln!("warn: {w}");
    }

    if !is_valid_name(&bf.name) {
        return Err(CboxError::dataerr(format!(
            "Boxfile name \"{}\" is invalid.",
            bf.name
        )));
    }

    let docker_mode = match bf.docker {
        DockerModeField::None => DockerMode::None,
        DockerModeField::Host => DockerMode::Host,
        DockerModeField::Nested => DockerMode::Nested,
    };

    let mounts: Vec<MountSpec> = bf
        .mounts
        .iter()
        .map(|m| MountSpec {
            host: m.host.clone(),
            guest: m.guest.clone(),
            mode: m.mode.as_str().to_string(),
        })
        .collect();

    Ok(CreateSpec {
        name: bf.name,
        image: bf.image,
        packages: bf.packages,
        docker_mode,
        mounts,
        home: if bf.box_config.home.is_empty() {
            None
        } else {
            Some(bf.box_config.home)
        },
        hostname: if bf.box_config.hostname.is_empty() {
            None
        } else {
            Some(bf.box_config.hostname)
        },
        init: bf.sandbox.init,
        pull: bf.box_config.pull,
        root: false,
        boxfile_path: Some(path.to_string()),
        unshare: bf.sandbox.unshare.to_arg_string(),
        backend: backend.clone(),
        uid: get_uid(),
        dry_run: false,
    })
}

fn parse_mounts(mounts: &[String]) -> Result<Vec<MountSpec>, CboxError> {
    mounts
        .iter()
        .map(|m| {
            let parts: Vec<&str> = m.splitn(3, ':').collect();
            match parts.as_slice() {
                [host, guest] => Ok(MountSpec {
                    host: host.to_string(),
                    guest: guest.to_string(),
                    mode: "rw".to_string(),
                }),
                [host, guest, mode] => {
                    if *mode != "ro" && *mode != "rw" {
                        return Err(CboxError::usage(format!(
                            "Invalid mount mode \"{mode}\". Use ro or rw."
                        )));
                    }
                    Ok(MountSpec {
                        host: host.to_string(),
                        guest: guest.to_string(),
                        mode: mode.to_string(),
                    })
                }
                _ => Err(CboxError::usage(format!(
                    "Invalid mount \"{m}\". Format: host:guest[:mode]"
                ))),
            }
        })
        .collect()
}

fn parse_docker_mode(s: &str) -> Result<DockerMode, CboxError> {
    DockerMode::parse(s).ok_or_else(|| {
        CboxError::usage(format!(
            "Invalid --docker \"{s}\". Use none, host, or nested."
        ))
    })
}

fn get_uid() -> u32 {
    #[cfg(unix)]
    unsafe {
        extern "C" {
            fn getuid() -> u32;
        }
        getuid()
    }
    #[cfg(not(unix))]
    {
        1000
    }
}
