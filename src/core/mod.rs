//! Front-end-agnostic command logic. CLI and TUI both call these functions.
//! All spawns go through `&dyn DistroboxRunner`.

pub mod spec;

use crate::dbox::{
    argv::{
        build_create_argv, build_dbox_list_argv, build_enter_argv, build_inspect_argv,
        build_list_argv, build_rm_argv,
    },
    backend::Backend,
    runner::{DistroboxRunner, Invocation, RunMode},
};
use crate::error::CboxError;
use spec::{
    BackendInfo, BackendStatus, BoxRow, CreateSpec, DistroboxInfo, DoctorResult, DoctorSpec,
    EditSpec, EnterSpec, InspectResult, InspectSpec, MountResult, RmSpec,
};

// ─── create ──────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct CreateOutcome {
    pub name: String,
    pub image: String,
    pub docker_mode: String,
    pub backend: String,
    pub argv: Vec<String>,
    pub dry_run_output: Option<String>,
}

pub fn create(spec: &CreateSpec, runner: &dyn DistroboxRunner) -> Result<CreateOutcome, CboxError> {
    let args = build_create_argv(spec);
    let mode = if spec.dry_run {
        RunMode::DryRun
    } else {
        RunMode::Capture
    };
    let inv = Invocation::new("distrobox", args.clone(), mode);
    let out = runner.run(inv)?;

    if spec.dry_run {
        return Ok(CreateOutcome {
            name: spec.name.clone(),
            image: spec.image.clone(),
            docker_mode: spec.docker_mode.as_str().to_string(),
            backend: spec.backend.as_str().to_string(),
            argv: {
                let mut v = vec!["distrobox".to_string()];
                v.extend(args);
                v
            },
            dry_run_output: Some(out.stdout.clone()),
        });
    }

    if out.status != 0 {
        return Err(CboxError::backend_error(out.status, &out.stderr, &out.argv));
    }

    Ok(CreateOutcome {
        name: spec.name.clone(),
        image: spec.image.clone(),
        docker_mode: spec.docker_mode.as_str().to_string(),
        backend: spec.backend.as_str().to_string(),
        argv: out.argv,
        dry_run_output: None,
    })
}

// ─── list ────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct ListOutcome {
    pub boxes: Vec<BoxRow>,
    /// Raw distrobox list text (for human rendering — Phase 2 human path).
    #[allow(dead_code)]
    pub raw_human: Option<String>,
}

pub fn list_machine(
    backend: &Backend,
    runner: &dyn DistroboxRunner,
) -> Result<ListOutcome, CboxError> {
    let args = build_list_argv();
    let inv = Invocation::new(backend.as_str(), args, RunMode::Capture);
    let out = runner.run(inv)?;

    if out.status != 0 {
        return Err(CboxError::backend_error(out.status, &out.stderr, &out.argv));
    }

    let boxes = parse_backend_ps_json(&out.stdout, backend)?;
    Ok(ListOutcome {
        boxes,
        raw_human: None,
    })
}

#[allow(dead_code)]
pub fn list_human(runner: &dyn DistroboxRunner) -> Result<ListOutcome, CboxError> {
    let args = build_dbox_list_argv();
    let inv = Invocation::new("distrobox", args, RunMode::Capture);
    let out = runner.run(inv)?;

    if out.status != 0 {
        return Err(CboxError::backend_error(out.status, &out.stderr, &out.argv));
    }

    Ok(ListOutcome {
        boxes: Vec::new(),
        raw_human: Some(out.stdout),
    })
}

/// Parse `podman/docker ps -a --filter label=manager=distrobox --format json`.
/// Both podman and docker return a JSON array; field names differ slightly.
fn parse_backend_ps_json(json: &str, _backend: &Backend) -> Result<Vec<BoxRow>, CboxError> {
    let json = json.trim();
    if json.is_empty() || json == "null" || json == "[]" {
        return Ok(Vec::new());
    }

    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| CboxError::ioerr(format!("Failed to parse backend JSON: {e}")))?;

    let arr = match value {
        serde_json::Value::Array(a) => a,
        serde_json::Value::Object(_) => vec![value],
        _ => vec![],
    };

    let mut boxes = Vec::new();
    for item in arr {
        let name = extract_str(&item, &["Names", "Name", "name"])
            .unwrap_or_default()
            .trim_start_matches('/')
            .to_string();
        let status = extract_str(&item, &["State", "Status", "status"]).unwrap_or_default();
        let image = extract_str(&item, &["Image", "image"]).unwrap_or_default();
        let id = extract_str(&item, &["Id", "ID", "id"]).unwrap_or_default();

        // Labels
        let labels = item.get("Labels").or_else(|| item.get("labels"));
        let docker_mode = labels
            .and_then(|l| l.get("cbox.docker_mode"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let cbox_managed = labels
            .and_then(|l| l.get("cbox.managed"))
            .and_then(|v| v.as_str())
            .map(|v| v == "true")
            .unwrap_or(false);

        boxes.push(BoxRow {
            name,
            status,
            image,
            docker_mode,
            cbox_managed,
            id,
        });
    }

    Ok(boxes)
}

fn extract_str(val: &serde_json::Value, keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Some(v) = val.get(k) {
            // handle array (podman Names is an array)
            if let Some(arr) = v.as_array() {
                if let Some(first) = arr.first().and_then(|x| x.as_str()) {
                    return Some(first.to_string());
                }
            }
            if let Some(s) = v.as_str() {
                return Some(s.to_string());
            }
        }
    }
    None
}

// ─── rm ──────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct RmOutcome {
    pub removed: Vec<String>,
    pub skipped: Vec<String>,
}

pub fn rm(spec: &RmSpec, runner: &dyn DistroboxRunner) -> Result<RmOutcome, CboxError> {
    let args = build_rm_argv(spec);
    let inv = Invocation::new("distrobox", args, RunMode::Capture);
    let out = runner.run(inv)?;

    if out.status != 0 {
        return Err(CboxError::backend_error(out.status, &out.stderr, &out.argv));
    }

    Ok(RmOutcome {
        removed: spec.names.clone(),
        skipped: Vec::new(),
    })
}

// ─── enter ───────────────────────────────────────────────────────────────────

pub fn enter(spec: &EnterSpec, runner: &dyn DistroboxRunner) -> Result<i32, CboxError> {
    let args = build_enter_argv(spec);
    let inv = Invocation::new("distrobox", args, RunMode::Interactive);
    let code = runner.run_interactive(inv)?;
    Ok(code)
}

// ─── inspect ─────────────────────────────────────────────────────────────────

pub fn inspect(
    spec: &InspectSpec,
    runner: &dyn DistroboxRunner,
) -> Result<InspectResult, CboxError> {
    let args = build_inspect_argv(&spec.name);
    let inv = Invocation::new(spec.backend.as_str(), args, RunMode::Capture);
    let out = runner.run(inv)?;

    if out.status != 0 {
        // Check for not-found
        if out.stderr.contains("no such")
            || out.stderr.contains("No such")
            || out.stdout.trim() == "[]"
            || out.stdout.trim().is_empty()
        {
            return Err(CboxError::box_not_found(&spec.name));
        }
        return Err(CboxError::backend_error(out.status, &out.stderr, &out.argv));
    }

    project_inspect_json(&out.stdout, &spec.name, &spec.backend)
}

/// Project raw backend inspect JSON into cbox's stable schema.
pub fn project_inspect_json(
    json: &str,
    name: &str,
    backend: &Backend,
) -> Result<InspectResult, CboxError> {
    let json = json.trim();
    if json.is_empty() || json == "[]" || json == "null" {
        return Err(CboxError::box_not_found(name));
    }

    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| CboxError::ioerr(format!("Failed to parse inspect JSON: {e}")))?;

    // Both podman and docker return an array; take the first element.
    let item = match &value {
        serde_json::Value::Array(arr) if !arr.is_empty() => &arr[0],
        serde_json::Value::Array(_) => return Err(CboxError::box_not_found(name)),
        obj @ serde_json::Value::Object(_) => obj,
        _ => return Err(CboxError::box_not_found(name)),
    };

    let id = extract_str(item, &["Id", "ID", "id"]).unwrap_or_default();
    let status = item
        .pointer("/State/Status")
        .or_else(|| item.get("State"))
        .or_else(|| item.get("status"))
        .and_then(|v| {
            if let Some(s) = v.as_str() {
                Some(s.to_string())
            } else {
                v.get("Status")
                    .and_then(|x| x.as_str())
                    .map(|s| s.to_string())
            }
        })
        .unwrap_or_default();

    let image = extract_str(item, &["Image", "image", "Config.Image"])
        .or_else(|| {
            item.pointer("/Config/Image")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_default();

    let created = extract_str(item, &["Created", "created"]).unwrap_or_default();

    let labels = item
        .get("Config")
        .and_then(|c| c.get("Labels"))
        .or_else(|| item.get("Labels"))
        .or_else(|| item.get("labels"));

    let docker_mode = labels
        .and_then(|l| l.get("cbox.docker_mode"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let boxfile_path = labels
        .and_then(|l| l.get("cbox.boxfile_path"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let packages: Vec<String> = labels
        .and_then(|l| l.get("cbox.packages"))
        .and_then(|v| v.as_str())
        .map(|s| s.split_whitespace().map(|p| p.to_string()).collect())
        .unwrap_or_default();

    // Parse mounts
    let mounts = parse_mounts(item);

    Ok(InspectResult {
        name: name.to_string(),
        status,
        image,
        created,
        docker_mode,
        mounts,
        packages,
        backend: backend.as_str().to_string(),
        id,
        boxfile_path,
    })
}

fn parse_mounts(item: &serde_json::Value) -> Vec<MountResult> {
    let arr = item
        .get("Mounts")
        .or_else(|| item.get("mounts"))
        .and_then(|v| v.as_array());

    match arr {
        None => Vec::new(),
        Some(mounts) => mounts
            .iter()
            .filter_map(|m| {
                let host = extract_str(m, &["Source", "source", "host"])?;
                let guest = extract_str(m, &["Destination", "destination", "guest"])?;
                let mode = m
                    .get("Mode")
                    .or_else(|| m.get("mode"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("rw")
                    .to_string();
                Some(MountResult { host, guest, mode })
            })
            .collect(),
    }
}

// ─── inspect raw ─────────────────────────────────────────────────────────────

pub fn inspect_raw(spec: &InspectSpec, runner: &dyn DistroboxRunner) -> Result<String, CboxError> {
    let args = build_inspect_argv(&spec.name);
    let inv = Invocation::new(spec.backend.as_str(), args, RunMode::Capture);
    let out = runner.run(inv)?;

    if out.status != 0 {
        return Err(CboxError::backend_error(out.status, &out.stderr, &out.argv));
    }
    Ok(out.stdout)
}

// ─── doctor ──────────────────────────────────────────────────────────────────

/// Minimum supported distrobox version.
pub const DISTROBOX_FLOOR: (u32, u32, u32) = (1, 6, 0);

pub fn doctor(spec: &DoctorSpec, runner: &dyn DistroboxRunner) -> Result<DoctorResult, CboxError> {
    let mut warnings = Vec::new();

    // 1. Check distrobox
    let dbox_info = check_distrobox(runner, &mut warnings);

    // 2. Detect backend
    let (backend_info, selected_backend) = check_backends(runner, spec.backend_override.as_deref());

    let ok = dbox_info.present
        && backend_info.selected.is_some()
        && (backend_info.podman.reachable || backend_info.docker.reachable);

    if !dbox_info.present {
        return Err(CboxError::software(
            "distrobox is not installed or not on PATH. \
             Install it from https://github.com/89luca89/distrobox or your distro packages.",
        ));
    }

    if selected_backend.is_none() {
        return Err(CboxError::tempfail(
            "No usable container backend found. \
             Install podman or docker and ensure the service is running.",
        ));
    }

    Ok(DoctorResult {
        ok,
        distrobox: dbox_info,
        backend: backend_info,
        warnings,
    })
}

fn check_distrobox(runner: &dyn DistroboxRunner, warnings: &mut Vec<String>) -> DistroboxInfo {
    let inv = Invocation::new("distrobox", vec!["version".to_string()], RunMode::Capture);
    let out = match runner.run(inv) {
        Ok(o) => o,
        Err(_) => {
            return DistroboxInfo {
                present: false,
                version: None,
                supported: false,
            }
        }
    };

    if out.status != 0 {
        return DistroboxInfo {
            present: false,
            version: None,
            supported: false,
        };
    }

    let version = parse_distrobox_version(&out.stdout);
    let supported = version
        .as_ref()
        .map(|v| is_version_supported(v))
        .unwrap_or(false);

    if !supported {
        warnings.push(format!(
            "distrobox version {:?} is below the supported floor (1.6). Some flags may not work.",
            version
        ));
    }

    DistroboxInfo {
        present: true,
        version,
        supported,
    }
}

fn parse_distrobox_version(output: &str) -> Option<String> {
    // "distrobox: 1.8.2.4" or "distrobox version 1.8.2.4"
    for line in output.lines() {
        let line = line.trim();
        if line.starts_with("distrobox") {
            // grab the last token that looks like a version
            if let Some(ver) = line.split_whitespace().last() {
                if ver
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false)
                {
                    return Some(ver.trim_matches(':').to_string());
                }
            }
        }
    }
    None
}

fn is_version_supported(version: &str) -> bool {
    let parts: Vec<u32> = version.split('.').filter_map(|p| p.parse().ok()).collect();
    let (fmaj, fmin, _fpatch) = DISTROBOX_FLOOR;
    match (parts.first(), parts.get(1)) {
        (Some(&maj), Some(&min)) => (maj, min) >= (fmaj, fmin),
        (Some(&maj), None) => maj >= fmaj,
        _ => false,
    }
}

fn check_backends(
    runner: &dyn DistroboxRunner,
    override_arg: Option<&str>,
) -> (BackendInfo, Option<String>) {
    let podman = probe_backend_status("podman", runner);
    let docker = probe_backend_status("docker", runner);

    let selected = if let Some(b) = override_arg {
        Some(b.to_string())
    } else if podman.reachable {
        Some("podman".to_string())
    } else if docker.reachable {
        Some("docker".to_string())
    } else {
        None
    };

    (
        BackendInfo {
            selected: selected.clone(),
            podman,
            docker,
        },
        selected,
    )
}

fn probe_backend_status(name: &str, runner: &dyn DistroboxRunner) -> BackendStatus {
    // Check if present (version)
    let version_inv = Invocation::new(name, vec!["--version".to_string()], RunMode::Capture);
    let present = runner
        .run(version_inv)
        .map(|o| o.status == 0)
        .unwrap_or(false);

    if !present {
        return BackendStatus {
            present: false,
            reachable: false,
            version: None,
        };
    }

    // Check if reachable (info)
    let info_inv = Invocation::new(name, vec!["info".to_string()], RunMode::Capture);
    let reachable = runner.run(info_inv).map(|o| o.status == 0).unwrap_or(false);

    BackendStatus {
        present: true,
        reachable,
        version: None,
    }
}

// ─── edit ────────────────────────────────────────────────────────────────────

/// Outcome of an edit operation (Phase 2 apply will use this).
#[derive(Debug)]
#[allow(dead_code)]
pub struct EditOutcome {
    pub boxfile_path: String,
    pub validation_warnings: Vec<String>,
}

/// Resolve and return the Boxfile path for a named box (inspects labels, falls back to XDG).
pub fn resolve_boxfile_path(
    name: &str,
    spec: &EditSpec,
    runner: &dyn DistroboxRunner,
) -> Result<String, CboxError> {
    // We do a best-effort inspect; if it fails we fall back to XDG path
    let label_path = runner
        .run(Invocation::new(
            spec.backend.as_str(),
            build_inspect_argv(name),
            RunMode::Capture,
        ))
        .ok()
        .filter(|o| o.status == 0)
        .and_then(|o| {
            let v: serde_json::Value = serde_json::from_str(&o.stdout).ok()?;
            let item = match &v {
                serde_json::Value::Array(arr) => arr.first()?,
                obj => obj,
            };
            let labels = item
                .get("Config")
                .and_then(|c| c.get("Labels"))
                .or_else(|| item.get("Labels"));
            labels
                .and_then(|l| l.get("cbox.boxfile_path"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        });

    if let Some(path) = label_path {
        if std::path::Path::new(&path).exists() {
            return Ok(path);
        }
    }

    // XDG fallback
    let config_home = std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
        format!("{home}/.config")
    });
    Ok(format!("{config_home}/cbox/boxes/{name}/Boxfile.toml"))
}

/// Scaffold a Boxfile from the inspected container state.
pub fn scaffold_boxfile(name: &str, spec: &EditSpec, runner: &dyn DistroboxRunner) -> String {
    // Best-effort inspect
    let inspect_spec = InspectSpec {
        name: name.to_string(),
        raw: false,
        backend: spec.backend.clone(),
    };
    let (image, docker_mode, mounts_str) = if let Ok(result) = inspect(&inspect_spec, runner) {
        let mounts_str = result
            .mounts
            .iter()
            .map(|m| {
                format!(
                    "\n[[mounts]]\nhost  = \"{}\"\nguest = \"{}\"\nmode  = \"{}\"",
                    m.host, m.guest, m.mode
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        (result.image, result.docker_mode, mounts_str)
    } else {
        (
            "registry.fedoraproject.org/fedora-toolbox:latest".to_string(),
            "none".to_string(),
            String::new(),
        )
    };

    format!(
        "# Boxfile.toml — generated by `cbox edit` from live container state\n\
         name  = \"{name}\"\n\
         image = \"{image}\"\n\
         docker = \"{docker_mode}\"\n\
         packages = []\n{mounts_str}\n"
    )
}
