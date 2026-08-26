use super::output::OutputCtx;
use super::{discover_boxfile_in, ensure_boxfile_name_matches};
use crate::boxfile::validate::is_valid_name;
use crate::boxfile::{self, model::DockerModeField};
use crate::core::{
    self,
    secret_inject::{resolve_secret_env, SecretScope},
    spec::{CreateSpec, DockerMode, MountSpec},
};
use crate::dbox::backend::Backend;
use crate::dbox::runner::DistroboxRunner;
use crate::error::CboxError;
use crate::secret::SecretStore;
use clap::Args;

#[derive(Args, Debug)]
pub struct CreateArgs {
    /// Box name.
    #[arg(value_name = "NAME")]
    pub name: Option<String>,

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

    /// Create as root box.
    #[arg(long)]
    pub root: bool,

    /// Fully isolate from the host: private $HOME + process/ipc namespaces, so
    /// host shell config and apps don't bleed into the box.
    #[arg(long)]
    pub isolated: bool,

    /// Path to a Boxfile.toml.
    #[arg(long = "file", value_name = "PATH")]
    pub file: Option<String>,
}

#[allow(dead_code)]
pub fn run(
    args: &CreateArgs,
    global_dry_run: bool,
    global_backend: Option<&str>,
    ctx: &OutputCtx,
    runner: &dyn DistroboxRunner,
) -> Result<(), CboxError> {
    run_with_store(args, global_dry_run, global_backend, ctx, runner, None)
}

/// Variant that accepts an optional SecretStore — used in tests and from main.rs.
/// When `store` is None, no secret resolution happens (no [secrets] in the Boxfile
/// or the caller guarantees the spec's env_flags/env_values are already populated).
#[allow(clippy::too_many_arguments)]
pub fn run_with_store(
    args: &CreateArgs,
    global_dry_run: bool,
    global_backend: Option<&str>,
    ctx: &OutputCtx,
    runner: &dyn DistroboxRunner,
    store: Option<&dyn SecretStore>,
) -> Result<(), CboxError> {
    // Detect backend
    let backend = Backend::detect(global_backend)?;

    // Start building the spec — may be overridden by Boxfile
    let (mut spec, resolved_bf) = if let Some(ref file_path) = args.file {
        // Priority 1: --file explicitly given.
        let (bf, warnings) = crate::boxfile::parse_file(file_path)?;
        for w in &warnings {
            eprintln!("warn: {w}");
        }
        ensure_boxfile_name_matches(args.name.as_deref(), &bf.name, file_path)?;
        (spec_from_boxfile_model(&bf, file_path, &backend)?, Some(bf))
    } else if let Some(cwd_path) = std::env::current_dir()
        .ok()
        .as_deref()
        .and_then(discover_boxfile_in)
    {
        // Priority 2: matching Boxfile.toml found in the current working directory.
        if !ctx.quiet {
            ctx.hint(&format!("Using ./{cwd_path}"));
        }
        let (bf, warnings) = crate::boxfile::parse_file(cwd_path)?;
        for w in &warnings {
            eprintln!("warn: {w}");
        }
        ensure_boxfile_name_matches(args.name.as_deref(), &bf.name, cwd_path)?;
        (spec_from_boxfile_model(&bf, cwd_path, &backend)?, Some(bf))
    } else if let Some(ref name) = args.name {
        // No Boxfile: preserve imperative positional-name creation.
        if !is_valid_name(name) {
            return Err(CboxError::usage(format!(
                "Invalid box name \"{name}\". Names must match ^[a-zA-Z0-9][a-zA-Z0-9_.-]*$"
            )));
        }
        (CreateSpec::new(name.clone(), backend.clone()), None)
    } else {
        return Err(CboxError::usage(
            "NAME is required unless --file is provided or a Boxfile.toml exists in the current directory.",
        ));
    };

    // Resolve secrets (persist=true) from keyring — ALL-OR-NOTHING before any spawn (D3).
    if let (Some(store), Some(ref bf)) = (store, &resolved_bf) {
        if !bf.secrets.is_empty() {
            let env_pairs =
                resolve_secret_env(&spec.name, &bf.secrets, SecretScope::Persisted, store)?;
            for (k, v) in &env_pairs {
                spec.env_flags.push(k.clone());
                spec.env_values.push((k.clone(), v.clone()));
            }
        }
        // Populate plain_env from [env]
        for (k, v) in &bf.env {
            spec.plain_env.push((k.clone(), v.clone()));
        }
    }

    apply_cli_overrides(args, resolved_bf.is_some(), &mut spec)?;
    spec.dry_run = global_dry_run;
    spec.backend = backend;

    // Isolation: from the Boxfile `[box] isolated` OR the --isolated flag. Applied
    // after the --home override so an explicit home always wins (apply_isolation is
    // idempotent and only synthesizes a home when none was set).
    let isolated = resolved_bf
        .as_ref()
        .map(|b| b.box_config.isolated)
        .unwrap_or(false)
        || args.isolated;
    if isolated {
        let nm = spec.name.clone();
        core::apply_isolation(&mut spec, &nm);
        // distrobox won't create a custom --home whose parent dirs don't exist
        // (podman/crun fails to bind-mount a missing source). The synthesized XDG
        // path lives several levels deep, so create it before the box.
        if !spec.dry_run {
            if let Some(home) = spec.home.as_deref().filter(|h| !h.is_empty()) {
                std::fs::create_dir_all(home).map_err(|e| {
                    CboxError::ioerr(format!("Cannot create isolated home {home}: {e}"))
                })?;
            }
        }
    }

    let outcome = core::create(&spec, runner)?;

    if let Some(ref dry_output) = outcome.dry_run_output {
        // DryRun: print the would-be argv
        println!("{dry_output}");
        return Ok(());
    }

    if ctx.json {
        let v = serde_json::json!({
            "ok": true,
            "action": "create",
            "name": outcome.name,
            "image": outcome.image,
            "docker": outcome.docker_mode,
            "backend": outcome.backend,
            "argv": outcome.argv,
        });
        ctx.print_json(&v);
    } else {
        ctx.success(&format!(
            "Created box \"{}\" ({}, docker: {})",
            outcome.name,
            outcome.image.rsplit('/').next().unwrap_or(&outcome.image),
            outcome.docker_mode
        ));
        ctx.hint(&format!("Enter it with:  cbox enter {}", outcome.name));
    }

    Ok(())
}

fn apply_cli_overrides(
    args: &CreateArgs,
    has_boxfile: bool,
    spec: &mut CreateSpec,
) -> Result<(), CboxError> {
    // CLI flags override Boxfile. Clap's defaults are not explicit user intent,
    // so preserve Boxfile values when those defaults are present.
    if args.image != "registry.fedoraproject.org/fedora-toolbox:latest" || !has_boxfile {
        spec.image = args.image.clone();
    }
    if !args.packages.is_empty() {
        spec.packages = args.packages.clone();
    }
    if !args.mounts.is_empty() {
        spec.mounts = parse_mounts(&args.mounts)?;
    }
    if args.docker != "none" || !has_boxfile {
        spec.docker_mode = parse_docker_mode(&args.docker)?;
    }
    if let Some(ref h) = args.home {
        spec.home = Some(h.clone());
    }
    if let Some(ref h) = args.hostname {
        spec.hostname = Some(h.clone());
    }
    if args.init {
        spec.init = true;
    }
    if args.pull {
        spec.pull = true;
    }
    if args.root {
        spec.root = true;
    }
    Ok(())
}

#[allow(dead_code)]
fn spec_from_boxfile(path: &str, backend: &Backend) -> Result<CreateSpec, CboxError> {
    let (bf, warnings) = boxfile::parse_file(path)?;
    for w in &warnings {
        eprintln!("warn: {w}");
    }
    spec_from_boxfile_model(&bf, path, backend)
}

fn spec_from_boxfile_model(
    bf: &crate::boxfile::model::Boxfile,
    path: &str,
    backend: &Backend,
) -> Result<CreateSpec, CboxError> {
    // Validate name
    if !is_valid_name(&bf.name) {
        return Err(CboxError::dataerr(format!(
            "Boxfile name \"{}\" is invalid.",
            bf.name
        )));
    }

    let uid = {
        #[cfg(unix)]
        unsafe {
            extern "C" {
                fn getuid() -> u32;
            }
            getuid()
        }
        #[cfg(not(unix))]
        {
            1000u32
        }
    };

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
        name: bf.name.clone(),
        image: bf.image.clone(),
        packages: bf.packages.clone(),
        docker_mode,
        mounts,
        home: if bf.box_config.home.is_empty() {
            None
        } else {
            Some(bf.box_config.home.clone())
        },
        hostname: if bf.box_config.hostname.is_empty() {
            None
        } else {
            Some(bf.box_config.hostname.clone())
        },
        init: bf.sandbox.init,
        pull: bf.box_config.pull,
        root: false,
        boxfile_path: Some(path.to_string()),
        unshare: bf.sandbox.unshare.to_arg_string(),
        backend: backend.clone(),
        uid,
        dry_run: false,
        env_flags: Vec::new(),
        env_values: Vec::new(),
        plain_env: Vec::new(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dbox::mock::{MockResponse, MockRunner};

    #[test]
    fn boxfile_defaults_survive_create_cli_defaults_for_docker() {
        let bf = boxfile::parse_and_validate(
            r#"
name = "web-dev"
image = "ubuntu:24.04"
docker = "nested"

[box]
home = "/tmp/web-home"

[sandbox]
unshare = "all"
"#,
        )
        .unwrap()
        .0;
        let mut spec = spec_from_boxfile_model(&bf, "Boxfile.toml", &Backend::Docker).unwrap();
        let args = CreateArgs {
            name: Some("web-dev".into()),
            image: "registry.fedoraproject.org/fedora-toolbox:latest".into(),
            packages: vec![],
            mounts: vec![],
            docker: "none".into(),
            home: None,
            hostname: None,
            init: false,
            pull: false,
            root: false,
            isolated: false,
            file: None,
        };

        apply_cli_overrides(&args, true, &mut spec).unwrap();

        assert_eq!(spec.image, "ubuntu:24.04");
        assert_eq!(spec.docker_mode, DockerMode::Nested);
        assert_eq!(spec.home.as_deref(), Some("/tmp/web-home"));
        assert_eq!(spec.unshare.as_deref(), Some("all"));
        assert_eq!(spec.boxfile_path.as_deref(), Some("Boxfile.toml"));
        assert_eq!(spec.backend, Backend::Docker);

        spec.dry_run = true;
        let runner = MockRunner::new().with_default(MockResponse::ok("dry run"));
        core::create(&spec, &runner).unwrap();
        let call = runner.calls().pop().unwrap();
        assert!(call
            .env
            .iter()
            .any(|(key, value)| key == "DBX_CONTAINER_MANAGER" && value == "docker"));
        assert!(call
            .args
            .windows(2)
            .any(|w| w[0] == "--image" && w[1] == "ubuntu:24.04"));
        assert!(call
            .args
            .windows(2)
            .any(|w| w[0] == "--home" && w[1] == "/tmp/web-home"));
        assert!(call.args.iter().any(|arg| arg == "--unshare-all"));
        assert!(call
            .args
            .iter()
            .any(|arg| arg.contains("cbox.boxfile_path=Boxfile.toml")));
        assert!(call
            .args
            .iter()
            .any(|arg| arg.contains("cbox.docker_mode=nested")));
    }
}
