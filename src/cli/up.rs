//! `cbox up [NAME]` — create-if-absent then apply.

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
    let backend = Backend::detect(global_backend)?;

    let create_spec = if let Some(ref file_path) = args.file {
        spec_from_boxfile(file_path, &backend)?
    } else {
        let name = args
            .name
            .as_ref()
            .ok_or_else(|| CboxError::usage("NAME is required unless --file is provided."))?;
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
            backend: backend.clone(),
            uid: get_uid(),
            dry_run: global_dry_run,
        }
    };

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
    let outcome = core::up(&up_spec, &store, runner)?;

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
