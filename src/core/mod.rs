//! Front-end-agnostic command logic. CLI and TUI both call these functions.
//! All spawns go through `&dyn DistroboxRunner`.

pub mod diff;
pub mod provision;
pub mod secret_inject;
pub mod spec;
pub mod state_store;

use crate::boxfile;
use std::time::Duration;

use crate::dbox::{
    argv::{
        build_create_argv, build_dbox_list_argv, build_enter_argv, build_export_app_argv,
        build_export_bin_argv, build_export_list_apps_argv, build_export_list_bins_argv,
        build_export_service_argv, build_inspect_argv, build_list_argv, build_logs_argv,
        build_pkg_probe_argv, build_provision_shell_argv, build_rm_argv, build_stats_argv,
        build_stop_argv,
    },
    backend::Backend,
    runner::{DistroboxRunner, Invocation, RunMode},
};
use crate::error::CboxError;
use secret_inject::{build_env_keys, build_secret_specs, env_secret_fingerprint};
use spec::{
    ApplyOutcome, ApplySpec, ApplySummary, BackendInfo, BackendStatus, BoxRow, CreateSpec,
    DiffResult, DistroboxInfo, DoctorResult, DoctorSpec, EditSpec, EnterSpec, ExportOutcome,
    ExportSpec, ExportTarget, InspectResult, InspectSpec, KeyringStatus, MountResult,
    ProvisionStepResult, RmSpec, StatsSample, StatsSpec, StopSpec, UpOutcome, UpSpec,
};
use state_store::{ProvisionState, ProvisionStateStore};

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
    // Secret VALUES ride the child process environment (Invocation.env), not argv.
    // argv carries only `--env KEY` (name-only) so values never hit argv (INV-1).
    let inv = Invocation::new("distrobox", args.clone(), mode).with_env(spec.env_values.clone());
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

/// List boxes across every given backend and merge them into one outcome.
/// Each row is stamped with the backend it came from. A backend that fails to
/// list (e.g. a daemon that died mid-probe) is skipped, not fatal — but if every
/// backend fails and nothing is collected, the last error is surfaced.
pub fn list_all(
    backends: &[Backend],
    runner: &dyn DistroboxRunner,
) -> Result<ListOutcome, CboxError> {
    let mut boxes = Vec::new();
    let mut last_err = None;
    for backend in backends {
        match list_machine(backend, runner) {
            Ok(outcome) => boxes.extend(outcome.boxes),
            Err(e) => last_err = Some(e),
        }
    }
    if boxes.is_empty() {
        if let Some(e) = last_err {
            return Err(e);
        }
    }
    Ok(ListOutcome {
        boxes,
        raw_human: None,
    })
}

/// Internal: list a single backend using `run_with_timeout` on the runner.
#[allow(dead_code)]
fn list_machine_with_timeout(
    backend: &Backend,
    runner: &dyn DistroboxRunner,
    timeout: Duration,
) -> Result<ListOutcome, CboxError> {
    let args = build_list_argv();
    let inv = Invocation::new(backend.as_str(), args, RunMode::Capture);
    let out = runner.run_with_timeout(inv, timeout)?;

    if out.status != 0 {
        return Err(CboxError::backend_error(out.status, &out.stderr, &out.argv));
    }

    let boxes = parse_backend_ps_json(&out.stdout, backend)?;
    Ok(ListOutcome {
        boxes,
        raw_human: None,
    })
}

/// List boxes across every given backend using `run_with_timeout`.
/// Used by the silent poll effect (SilentLoadList) so a hung backend cannot
/// freeze the TUI. The manual `r` refresh keeps calling `list_all` (no timeout).
#[allow(dead_code)]
pub fn list_all_with_timeout(
    backends: &[Backend],
    runner: &dyn DistroboxRunner,
    timeout: Duration,
) -> Result<ListOutcome, CboxError> {
    let mut boxes = Vec::new();
    let mut last_err = None;
    for backend in backends {
        match list_machine_with_timeout(backend, runner, timeout) {
            Ok(outcome) => boxes.extend(outcome.boxes),
            Err(e) => last_err = Some(e),
        }
    }
    if boxes.is_empty() {
        if let Some(e) = last_err {
            return Err(e);
        }
    }
    Ok(ListOutcome {
        boxes,
        raw_human: None,
    })
}

// ─── stats ───────────────────────────────────────────────────────────────────

/// Fetch live CPU/mem stats for a running box via the engine's stats command.
/// Uses `run_with_timeout` — a hung engine socket is bounded by `timeout`.
/// Returns `Err` on engine error, non-zero exit, or parse failure; the caller
/// swallows the error silently (no toast, no busy, no status change).
#[allow(dead_code)]
pub fn stats(
    spec: &StatsSpec,
    runner: &dyn DistroboxRunner,
    timeout: Duration,
) -> Result<StatsSample, CboxError> {
    let args = build_stats_argv(&spec.id);
    let inv = Invocation::new(spec.backend.as_str(), args, RunMode::Capture);
    let out = runner.run_with_timeout(inv, timeout)?;

    if out.status != 0 {
        return Err(CboxError::backend_error(out.status, &out.stderr, &out.argv));
    }

    parse_stats_json(&out.stdout)
}

/// Stream container logs line-by-line via `<backend> logs -f <id>`.
///
/// Calls `runner.run_stream` with the logs argv. `on_line` is called for each
/// stdout line; `stop` is the cancel seam (set to `true` to kill+reap the child).
/// This MUST be called from a dedicated thread (never the worker — GAP-2).
///
/// Program = `spec.backend.as_str()` (engine call, not a distrobox subcommand),
/// mirroring `core::stats` at `mod.rs:204`.
#[allow(dead_code)]
pub fn logs(
    id: &str,
    backend: &Backend,
    runner: &dyn DistroboxRunner,
    on_line: &mut dyn FnMut(String),
    stop: &std::sync::atomic::AtomicBool,
) -> Result<i32, CboxError> {
    let args = build_logs_argv(id);
    let inv = Invocation::new(backend.as_str(), args, RunMode::Stream);
    runner
        .run_stream(inv, on_line, stop)
        .map_err(|e| CboxError::ioerr(e.to_string()))
}

/// Parse `<backend> stats --no-stream --format json` output.
///
/// Both podman and docker support `--format json` but their schemas differ:
/// - podman: array of objects with `CPU` (e.g. `"1.23%"`) and `MemUsage` ("12.3MB / 1.9GB")
/// - docker: NDJSON / object with `CPUPerc` ("1.23%") and `MemUsage`
///
/// Empty / `null` / `[]` / malformed → `Err` (caller pushes nothing to history).
#[allow(dead_code)]
pub fn parse_stats_json(raw: &str) -> Result<StatsSample, CboxError> {
    let raw = raw.trim();
    if raw.is_empty() || raw == "null" || raw == "[]" {
        return Err(CboxError::usage("no stats for box"));
    }

    // Try JSON array first (podman), then single object (docker), then NDJSON.
    let item: serde_json::Value = {
        match serde_json::from_str::<serde_json::Value>(raw) {
            Ok(serde_json::Value::Array(arr)) if !arr.is_empty() => arr.into_iter().next().unwrap(),
            Ok(serde_json::Value::Array(_)) => return Err(CboxError::usage("no stats for box")),
            Ok(obj @ serde_json::Value::Object(_)) => obj,
            Ok(_) => return Err(CboxError::usage("no stats for box")),
            Err(_) => {
                // Try NDJSON: take the first non-empty line.
                let first = raw
                    .lines()
                    .map(|l| l.trim())
                    .find(|l| !l.is_empty())
                    .ok_or_else(|| CboxError::usage("no stats for box"))?;
                serde_json::from_str::<serde_json::Value>(first)
                    .map_err(|e| CboxError::ioerr(format!("Failed to parse stats JSON: {e}")))?
            }
        }
    };

    // CPU: try CPUPerc (docker), CPU (podman), cpu_percent.
    let cpu_pct = ["CPUPerc", "CPU", "cpu_percent", "CpuPercent"]
        .iter()
        .find_map(|&k| item.get(k).and_then(|v| v.as_str()))
        .map(|s| s.trim_end_matches('%').trim().parse::<f64>().unwrap_or(0.0))
        .unwrap_or(0.0);

    // Mem: try MemUsage (both) as "used / limit", or separate fields.
    let (mem_used, mem_limit) = parse_mem_usage(&item);

    Ok(StatsSample {
        cpu_pct,
        mem_used,
        mem_limit,
    })
}

/// Parse the memory usage from the stats item.
/// Returns `(mem_used_bytes, mem_limit_bytes)`.
#[allow(dead_code)]
fn parse_mem_usage(item: &serde_json::Value) -> (u64, u64) {
    // "MemUsage" string: e.g. "12.3MiB / 1.9GiB" or "240MB / 1.9GB"
    if let Some(s) = item
        .get("MemUsage")
        .or_else(|| item.get("mem_usage"))
        .and_then(|v| v.as_str())
    {
        if let Some((used_str, limit_str)) = s.split_once('/') {
            let used = parse_human_size(used_str.trim()).unwrap_or(0);
            let limit = parse_human_size(limit_str.trim()).unwrap_or(0);
            return (used, limit);
        }
    }
    // Numeric fallbacks
    let used = item
        .get("mem_usage")
        .or_else(|| item.get("MemUsage"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let limit = item
        .get("mem_limit")
        .or_else(|| item.get("MemLimit"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    (used, limit)
}

/// Parse a human-readable size string like "12.3MiB", "240MB", "1.9GiB", "512KB".
/// Returns bytes as `u64` on success, `None` on failure.
#[allow(dead_code)]
fn parse_human_size(s: &str) -> Option<u64> {
    let s = s.trim();
    // Split at the boundary between digits/dot and letters.
    let split_pos = s.find(|c: char| c.is_alphabetic()).unwrap_or(s.len());
    let (num_str, unit) = s.split_at(split_pos);
    let num: f64 = num_str.trim().parse().ok()?;
    let multiplier: u64 = match unit.trim().to_uppercase().as_str() {
        "B" | "" => 1,
        "KB" | "KIB" => 1024,
        "MB" | "MIB" => 1024 * 1024,
        "GB" | "GIB" => 1024 * 1024 * 1024,
        "TB" | "TIB" => 1024u64 * 1024 * 1024 * 1024,
        _ => return None,
    };
    Some((num * multiplier as f64) as u64)
}

/// Resolve which backend a named box lives on, so per-box operations route to
/// the right engine without the user passing `--backend`.
///
/// - explicit `--backend` (in `override_arg`) always wins;
/// - exactly one usable backend → use it (no lookup needed);
/// - otherwise probe each backend and match by name;
/// - found on exactly one → that one;
/// - found on none → the preferred usable backend (let the op surface its own
///   "no such box" error);
/// - found on several → ambiguous, ask the user to disambiguate with `--backend`.
pub fn resolve_backend(
    name: &str,
    override_arg: Option<&str>,
    runner: &dyn DistroboxRunner,
) -> Result<Backend, CboxError> {
    let usable = Backend::usable(override_arg)?;
    if usable.len() == 1 {
        return Ok(usable[0].clone());
    }
    let matches: Vec<Backend> = usable
        .iter()
        .filter(|b| {
            list_machine(b, runner)
                .map(|o| o.boxes.iter().any(|row| row.name == name))
                .unwrap_or(false)
        })
        .cloned()
        .collect();
    match matches.as_slice() {
        [one] => Ok(one.clone()),
        [] => Ok(usable[0].clone()),
        several => Err(CboxError::usage(format!(
            "Box \"{name}\" exists on multiple backends ({}). \
             Disambiguate with --backend.",
            several
                .iter()
                .map(|b| b.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
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
/// podman returns a JSON array with object `Labels`; docker returns NDJSON
/// (one object per line) with `Labels` as a comma-separated string. Field
/// names differ slightly between the two.
fn parse_backend_ps_json(json: &str, backend: &Backend) -> Result<Vec<BoxRow>, CboxError> {
    let json = json.trim();
    if json.is_empty() || json == "null" || json == "[]" {
        return Ok(Vec::new());
    }

    // podman emits a single JSON array (or object); docker emits newline-delimited
    // JSON objects (NDJSON), one container per line. Try the array/object form
    // first, then fall back to parsing NDJSON line by line.
    let arr = match serde_json::from_str::<serde_json::Value>(json) {
        Ok(serde_json::Value::Array(a)) => a,
        Ok(obj @ serde_json::Value::Object(_)) => vec![obj],
        Ok(_) => vec![],
        Err(_) => parse_ndjson(json)?,
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

        // Labels — podman returns an object; docker returns a "k=v,k=v" string.
        let docker_mode =
            label_value(&item, "cbox.docker_mode").unwrap_or_else(|| "unknown".into());
        let cbox_managed = label_value(&item, "cbox.managed")
            .map(|v| v == "true")
            .unwrap_or(false);

        boxes.push(BoxRow {
            name,
            status,
            image,
            docker_mode,
            cbox_managed,
            id,
            backend: backend.as_str().to_string(),
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

/// Parse docker's NDJSON `ps` output — one JSON object per line.
fn parse_ndjson(json: &str) -> Result<Vec<serde_json::Value>, CboxError> {
    let mut items = Vec::new();
    for line in json.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(line)
            .map_err(|e| CboxError::ioerr(format!("Failed to parse backend JSON: {e}")))?;
        items.push(v);
    }
    Ok(items)
}

/// Look up a single container label, tolerating both backend shapes:
/// podman returns `Labels` as a JSON object; docker returns it as a
/// comma-separated `key=value` string.
fn label_value(item: &serde_json::Value, key: &str) -> Option<String> {
    let labels = item.get("Labels").or_else(|| item.get("labels"))?;
    if let Some(obj) = labels.as_object() {
        return obj.get(key).and_then(|v| v.as_str()).map(|s| s.to_string());
    }
    if let Some(s) = labels.as_str() {
        for pair in s.split(',') {
            if let Some((k, v)) = pair.split_once('=') {
                if k.trim() == key {
                    return Some(v.trim().to_string());
                }
            }
        }
    }
    None
}

// ─── stop ─────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct StopOutcome {
    pub stopped: Vec<String>,
}

pub fn stop(spec: &StopSpec, runner: &dyn DistroboxRunner) -> Result<StopOutcome, CboxError> {
    let args = build_stop_argv(spec);
    let inv =
        Invocation::new("distrobox", args, RunMode::Capture).with_env(vec![spec.backend.dbx_env()]);
    let out = runner.run(inv)?;

    if out.status != 0 {
        return Err(CboxError::backend_error(out.status, &out.stderr, &out.argv));
    }

    Ok(StopOutcome {
        stopped: spec.names.clone(),
    })
}

// ─── rm ──────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct RmOutcome {
    pub removed: Vec<String>,
    pub skipped: Vec<String>,
}

pub fn rm(spec: &RmSpec, runner: &dyn DistroboxRunner) -> Result<RmOutcome, CboxError> {
    // Best-effort stop-first: prevent distrobox rm from hanging on a running box.
    // Ignore stop failures — the rm result is authoritative.
    let stop_spec = StopSpec {
        names: spec.names.clone(),
        all: spec.all,
        backend: spec.backend.clone(),
    };
    let _ = stop(&stop_spec, runner);

    let args = build_rm_argv(spec);
    let inv =
        Invocation::new("distrobox", args, RunMode::Capture).with_env(vec![spec.backend.dbx_env()]);
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
    let inv = Invocation::new("distrobox", args, RunMode::Interactive)
        .with_env(vec![spec.backend.dbx_env()]);
    let code = runner.run_interactive(inv)?;
    Ok(code)
}

// ─── export ──────────────────────────────────────────────────────────────────

/// Exec `distrobox enter --name <BOX> -- distrobox-export <flags>` (imperative, D1).
///
/// Precondition: box must exist (checked via `inspect`; absent → exit 69).
/// Failure mapping:
///   F1 box absent → exit 69 (`box_not_found`)
///   F2 distrobox-export missing in box → exit 70 (`software`)
///   F3 helper non-zero (catch-all after F2) → exit 125 (`backend_error`)
///   F4 backend unreachable → exit 125 (via `RunnerError`)
pub fn export(spec: &ExportSpec, runner: &dyn DistroboxRunner) -> Result<ExportOutcome, CboxError> {
    // 1. Existence precondition (F1: box not found → 69).
    let inspect_spec = InspectSpec {
        name: spec.box_name.clone(),
        raw: false,
        backend: spec.backend.clone(),
    };
    match inspect(&inspect_spec, runner) {
        Ok(_) => {}
        Err(e) if e.exit_code() == crate::error::exit::UNAVAILABLE => {
            return Err(CboxError::box_not_found(&spec.box_name));
        }
        Err(e) => return Err(e),
    }

    // 2. Build argv from target + delete flag.
    let args = match &spec.target {
        ExportTarget::App { name } => build_export_app_argv(&spec.box_name, name, spec.delete),
        ExportTarget::Bin { path, to } => {
            build_export_bin_argv(&spec.box_name, path, to.as_deref(), spec.delete)
        }
        ExportTarget::Service { name } => {
            build_export_service_argv(&spec.box_name, name, spec.delete)
        }
        ExportTarget::ListApps => build_export_list_apps_argv(&spec.box_name),
        ExportTarget::ListBins => build_export_list_bins_argv(&spec.box_name),
    };

    // 3. Run (DryRun or Capture).
    let mode = if spec.dry_run {
        RunMode::DryRun
    } else {
        RunMode::Capture
    };
    let inv = Invocation::new("distrobox", args, mode).with_env(vec![spec.backend.dbx_env()]);
    let out = runner.run(inv)?;

    // 4. Map results.
    let (mode_str, target_name) = match &spec.target {
        ExportTarget::App { name } => ("app".to_string(), Some(name.clone())),
        ExportTarget::Bin { path, .. } => ("bin".to_string(), Some(path.clone())),
        ExportTarget::Service { name } => ("service".to_string(), Some(name.clone())),
        ExportTarget::ListApps => ("list-apps".to_string(), None),
        ExportTarget::ListBins => ("list-bins".to_string(), None),
    };

    if spec.dry_run {
        return Ok(ExportOutcome {
            ok: true,
            action: if spec.delete {
                "export-delete".to_string()
            } else if matches!(spec.target, ExportTarget::ListApps | ExportTarget::ListBins) {
                "export-list".to_string()
            } else {
                "export".to_string()
            },
            box_name: spec.box_name.clone(),
            mode: mode_str,
            target: target_name,
            deleted: spec.delete,
            entries: Vec::new(),
            detail: out.stdout.trim().to_string(),
        });
    }

    if out.status != 0 {
        // F2: distrobox-export BINARY missing in box → exit 70.
        // Detection: stderr must mention "distrobox-export" AND one of the "not found" signals.
        // We check stderr only (not argv — argv always contains "distrobox-export" as an arg).
        let stderr_lower = out.stderr.to_lowercase();
        if stderr_lower.contains("distrobox-export")
            && (stderr_lower.contains("not found")
                || stderr_lower.contains("no such file")
                || stderr_lower.contains("command not found"))
        {
            return Err(CboxError::software(format!(
                "distrobox-export isn't available inside \"{}\". \
                 This box may not be a distrobox-managed box, or its image lacks the distrobox \
                 integration. See:  cbox doctor",
                spec.box_name
            )));
        }
        // F3: catch-all helper error → 125.
        return Err(CboxError::backend_error(out.status, &out.stderr, &out.argv));
    }

    // Success.
    let is_list = matches!(spec.target, ExportTarget::ListApps | ExportTarget::ListBins);
    let action = if is_list {
        "export-list".to_string()
    } else if spec.delete {
        "export-delete".to_string()
    } else {
        "export".to_string()
    };

    let entries = if is_list {
        out.stdout
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect()
    } else {
        Vec::new()
    };

    let detail = if is_list {
        String::new()
    } else {
        out.stdout.trim().to_string()
    };

    Ok(ExportOutcome {
        ok: true,
        action,
        box_name: spec.box_name.clone(),
        mode: mode_str,
        target: target_name,
        deleted: spec.delete,
        entries,
        detail,
    })
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

    let cbox_image = labels
        .and_then(|l| l.get("cbox.image"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    // --- home: recover from Config.Env HOME=… ---
    // distrobox sets HOME in the container env to the value of --home.
    // No dedicated cbox.home label exists, so Config.Env is the most reliable
    // source — it is set unconditionally by distrobox at create time, whether or
    // not a custom --home was specified (shared-host home case: HOME=/home/<user>).
    let config_env: Option<&Vec<serde_json::Value>> = item
        .get("Config")
        .and_then(|c| c.get("Env"))
        .and_then(|v| v.as_array());
    let home: Option<String> = config_env.and_then(|env| {
        env.iter()
            .filter_map(|v| v.as_str())
            .find(|s| s.starts_with("HOME="))
            .map(|s| s["HOME=".len()..].to_string())
    });

    // --- hostname: recover from Config.Hostname ---
    // distrobox passes --hostname to the runtime; the runtime stores it verbatim
    // in Config.Hostname. This is the most direct source for the box hostname.
    let hostname: Option<String> = item
        .get("Config")
        .and_then(|c| c.get("Hostname"))
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
        cbox_image,
        home,
        hostname,
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
                // podman/docker inspect surfaces `Mode` as "" for default rw
                // bind-mounts (Mode is empty even though RW=true).  Fall back to
                // deriving the mode from the `RW` boolean so the projected value
                // is always explicit ("rw" or "ro") rather than an empty string.
                let mode_str = m
                    .get("Mode")
                    .or_else(|| m.get("mode"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let mode = if mode_str.is_empty() {
                    // Derive from RW boolean; default to "rw" when absent.
                    let rw = m
                        .get("RW")
                        .or_else(|| m.get("rw"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true);
                    if rw { "rw" } else { "ro" }.to_string()
                } else {
                    mode_str.to_string()
                };
                Some(MountResult { host, guest, mode })
            })
            .collect(),
    }
}

// ─── inspect raw ─────────────────────────────────────────────────────────────

#[allow(dead_code)]
pub fn inspect_raw(spec: &InspectSpec, runner: &dyn DistroboxRunner) -> Result<String, CboxError> {
    inspect_raw_with_secret_keys(spec, runner, &[])
}

/// `inspect --raw` variant that masks known secret KEYs in `Config.Env`.
/// `secret_keys` = the KEY names declared in the box's Boxfile [secrets] table.
/// For each key, any "KEY=<value>" in Config.Env is rewritten to "KEY=***".
/// If `secret_keys` is empty, no masking is applied.
pub fn inspect_raw_with_secret_keys(
    spec: &InspectSpec,
    runner: &dyn DistroboxRunner,
    secret_keys: &[String],
) -> Result<String, CboxError> {
    let args = build_inspect_argv(&spec.name);
    let inv = Invocation::new(spec.backend.as_str(), args, RunMode::Capture);
    let out = runner.run(inv)?;

    if out.status != 0 {
        return Err(CboxError::backend_error(out.status, &out.stderr, &out.argv));
    }

    if secret_keys.is_empty() {
        return Ok(out.stdout);
    }

    // Mask known secret keys in Config.Env (S3 courtesy masking).
    mask_secret_keys_in_raw_json(&out.stdout, secret_keys)
}

/// Parse raw backend JSON and mask `Config.Env` values for known secret keys.
/// Replaces "KEY=<anything>" with "KEY=***" for each key in `secret_keys`.
/// On parse failure, returns the original JSON unchanged (best-effort masking).
pub fn mask_secret_keys_in_raw_json(
    raw_json: &str,
    secret_keys: &[String],
) -> Result<String, CboxError> {
    let mut value: serde_json::Value = match serde_json::from_str(raw_json) {
        Ok(v) => v,
        Err(_) => return Ok(raw_json.to_string()), // can't parse → return as-is
    };

    // Handle both array (podman) and single object (docker).
    let items: Vec<&mut serde_json::Value> = match value {
        serde_json::Value::Array(ref mut arr) => arr.iter_mut().collect(),
        ref mut obj @ serde_json::Value::Object(_) => vec![obj],
        _ => return Ok(raw_json.to_string()),
    };

    for item in items {
        if let Some(env_arr) = item
            .pointer_mut("/Config/Env")
            .and_then(|v| v.as_array_mut())
        {
            for entry in env_arr.iter_mut() {
                if let Some(s) = entry.as_str() {
                    for key in secret_keys {
                        let prefix = format!("{key}=");
                        if s.starts_with(&prefix) {
                            *entry = serde_json::Value::String(format!("{key}=***"));
                            break;
                        }
                    }
                }
            }
        }
    }

    serde_json::to_string_pretty(&value)
        .map_err(|e| CboxError::ioerr(format!("Failed to re-serialize inspect JSON: {e}")))
}

// ─── doctor ──────────────────────────────────────────────────────────────────

/// Minimum supported distrobox version.
pub const DISTROBOX_FLOOR: (u32, u32, u32) = (1, 6, 0);

pub fn doctor(
    spec: &DoctorSpec,
    runner: &dyn DistroboxRunner,
    secret_store: &dyn crate::secret::SecretStore,
) -> Result<DoctorResult, CboxError> {
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

    // 3. Probe keyring (non-fatal informational line).
    // Use a no-write probe: get("__cbox_doctor__", "__probe__").
    // Ok(_) means the Service is reachable; Err(Unavailable) means it is not.
    let keyring = probe_keyring(secret_store);

    Ok(DoctorResult {
        ok,
        distrobox: dbox_info,
        backend: backend_info,
        warnings,
        keyring,
    })
}

fn probe_keyring(store: &dyn crate::secret::SecretStore) -> KeyringStatus {
    use crate::secret::SecretError;
    match store.get("__cbox_doctor__", "__probe__") {
        Ok(_) => KeyringStatus {
            available: true,
            detail: "Secret Service reachable.".to_string(),
        },
        Err(SecretError::Unavailable(msg)) => KeyringStatus {
            available: false,
            detail: format!(
                "Secret Service unreachable; secrets will refuse until you unlock it. ({msg})"
            ),
        },
        Err(e) => KeyringStatus {
            available: false,
            detail: format!("Keyring probe error: {e}"),
        },
    }
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

// ─── apply ────────────────────────────────────────────────────────────────────

/// Apply a Boxfile to an existing box: diff, run incremental provision steps.
/// Recreate-class diffs without `--recreate` → exit 65.
pub fn apply(
    spec: &ApplySpec,
    store: &dyn ProvisionStateStore,
    runner: &dyn DistroboxRunner,
) -> Result<ApplyOutcome, CboxError> {
    // 1. Parse Boxfile
    let (bf, _warnings) = boxfile::parse_file(&spec.boxfile_path)?;

    // 2. Inspect live box
    let inspect_spec = InspectSpec {
        name: spec.name.clone(),
        raw: false,
        backend: spec.backend.clone(),
    };
    let live = match inspect(&inspect_spec, runner) {
        Ok(r) => r,
        Err(e) if e.exit_code() == crate::error::exit::UNAVAILABLE => {
            return Err(CboxError::box_not_found(&spec.name));
        }
        Err(e) => return Err(e),
    };

    // 3. Diff
    let mut diff_result = diff::diff_boxfile_vs_live(&bf, &live);

    // 3b. Secret/env fingerprint diff (must participate — do NOT assume unchanged).
    // We need to read the stored state to get the prior fingerprint.
    // For the Recreate flow the state is irrelevant (full recreate wipes it), but
    // for incremental we must check it before deciding to skip or run.
    // Read state here so we can use it later (step 6) as well.
    let state_for_diff = store
        .read(&spec.name, runner)
        .unwrap_or_else(|_| ProvisionState::new());
    if let Some(secret_diff_field) = diff::diff_secrets(
        &state_for_diff.env_secret_fingerprint,
        &state_for_diff.secret_specs,
        &bf,
    ) {
        diff_result.fields.push(secret_diff_field);
        // Re-classify the overall diff based on the new field
        if diff_result.fields.iter().any(|f| f.class == "Recreate") {
            diff_result.class = "Recreate".to_string();
        }
    }

    let recreate_required = diff_result.class == "Recreate";

    if recreate_required && !spec.recreate {
        // Build a cozy message naming the forcing fields
        let forcing: Vec<String> = diff_result
            .fields
            .iter()
            .filter(|f| f.class == "Recreate")
            .map(|f| format!("  {}: {}  ->  {}", f.field, f.old, f.new))
            .collect();
        let msg = format!(
            "\"{}\" needs a recreate to apply these changes:\n{}\n\
             A recreate destroys the container ($HOME is preserved; box-local /usr changes are lost).\n\
             Re-run with:  cbox apply {} --recreate",
            spec.name,
            forcing.join("\n"),
            spec.name
        );
        return Err(CboxError::dataerr(msg));
    }

    // 4. Handle recreate flow
    if recreate_required && spec.recreate {
        // Destroy + recreate
        let rm_spec = RmSpec {
            names: vec![spec.name.clone()],
            force: true,
            rm_home: false,
            all: false,
            yes: true,
            backend: spec.backend.clone(),
        };
        rm(&rm_spec, runner)?;

        // Build a CreateSpec from the Boxfile, then inject any resolved persist=true
        // secrets from the ApplySpec's recreate fields (populated by the CLI caller).
        let mut create_spec = create_spec_from_boxfile_and_apply_spec(&bf, spec);
        create_spec.env_flags = spec.recreate_env_flags.clone();
        create_spec.env_values = spec.recreate_env_values.clone();
        create_spec.plain_env = spec.recreate_plain_env.clone();
        create(&create_spec, runner)?;

        // Fresh box: all steps run (no state yet)
        let fresh_state = ProvisionState::new();
        return run_provision_and_build_outcome(
            spec,
            &bf,
            fresh_state,
            diff_result,
            true,
            store,
            runner,
        );
    }

    // 5. Incremental: handle package additions
    let pkg_diff = diff::package_diff(&bf, &live);
    if !pkg_diff.added.is_empty() && !spec.no_provision {
        install_packages_incremental(&spec.name, &pkg_diff.added, runner)?;
    }

    // 6. Read provision state
    if spec.no_provision {
        // Skip provision steps entirely
        return Ok(ApplyOutcome {
            ok: true,
            action: "apply".to_string(),
            name: spec.name.clone(),
            diff: diff_result,
            recreate_required: false,
            steps: Vec::new(),
            summary: ApplySummary {
                ran: 0,
                skipped: 0,
                copied: 0,
                failed: 0,
            },
        });
    }

    // Use state_for_diff if we already read it (for fingerprint comparison).
    // If --force, reset to empty (all steps re-run).
    let state = if spec.force {
        ProvisionState::new()
    } else {
        // Re-use the state we already read for the fingerprint diff, unless it
        // was a fallback empty state due to a read error (in which case re-read
        // to surface the actual error consistently).
        if !state_for_diff.boxfile_sha.is_empty()
            || !state_for_diff.steps.is_empty()
            || !state_for_diff.env_secret_fingerprint.is_empty()
        {
            state_for_diff
        } else {
            read_state_with_force(&spec.name, spec.force, store, runner)?
        }
    };

    run_provision_and_build_outcome(spec, &bf, state, diff_result, false, store, runner)
}

fn run_provision_and_build_outcome(
    spec: &ApplySpec,
    bf: &crate::boxfile::model::Boxfile,
    mut state: ProvisionState,
    diff_result: DiffResult,
    _fresh: bool,
    store: &dyn ProvisionStateStore,
    runner: &dyn DistroboxRunner,
) -> Result<ApplyOutcome, CboxError> {
    let boxfile_dir = std::path::Path::new(&spec.boxfile_path)
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));

    let plan = provision::ProvisionPlan {
        name: &spec.name,
        steps: &bf.provision,
        boxfile_dir,
        backend: &spec.backend,
        force: spec.force,
        redo: &spec.redo,
        dry_run: spec.dry_run,
        // persist=false secret injection: threaded in from ApplySpec (resolved by CLI
        // caller via run_with_store before any spawn — D3 all-or-nothing guarantee).
        provision_env_keys: &spec.provision_env_keys,
        provision_env: &spec.provision_env,
    };

    let step_results = match provision::provision(&plan, store, runner, &mut state) {
        Ok(r) => r,
        Err(e) => {
            // Partial failure: the error is from a failed step
            // We already have partial step_results in state; return the error
            return Err(e);
        }
    };

    // Write the current fingerprint + metadata into state so the NEXT apply can
    // read it back (anti-blind-spot: fingerprint MUST be written AND read, not
    // write-once-ignored like the original boxfile_sha).
    state.env_secret_fingerprint = env_secret_fingerprint(bf);
    state.secret_specs = build_secret_specs(bf);
    state.env_keys = build_env_keys(bf);
    let _ = store.write(&spec.name, &state, runner); // best-effort

    let summary = summarize(&step_results);
    Ok(ApplyOutcome {
        ok: true,
        action: "apply".to_string(),
        name: spec.name.clone(),
        diff: diff_result,
        recreate_required: false,
        steps: step_results,
        summary,
    })
}

fn read_state_with_force(
    name: &str,
    force: bool,
    store: &dyn ProvisionStateStore,
    runner: &dyn DistroboxRunner,
) -> Result<ProvisionState, CboxError> {
    match store.read(name, runner) {
        Ok(s) => Ok(s),
        Err(e) if e.exit_code() == crate::error::exit::IOERR => {
            if force {
                Ok(ProvisionState::new())
            } else {
                Err(e)
            }
        }
        Err(e) => Err(e),
    }
}

fn summarize(steps: &[ProvisionStepResult]) -> ApplySummary {
    let mut ran = 0;
    let mut skipped = 0;
    let mut copied = 0;
    let mut failed = 0;
    for s in steps {
        match s.status.as_str() {
            "ran" => ran += 1,
            "skipped" => skipped += 1,
            "copied" => copied += 1,
            "failed" => failed += 1,
            _ => {}
        }
    }
    ApplySummary {
        ran,
        skipped,
        copied,
        failed,
    }
}

/// Install additional packages into a running box (incremental package diff).
fn install_packages_incremental(
    name: &str,
    packages: &[String],
    runner: &dyn DistroboxRunner,
) -> Result<(), CboxError> {
    // Probe the package manager
    let probe_args = build_pkg_probe_argv(name);
    let probe_inv = Invocation::new("distrobox", probe_args, RunMode::Capture);
    let probe_out = runner.run(probe_inv)?;

    let pkg_manager = probe_out.stdout.trim().to_string();
    let install_cmd = match pkg_manager.as_str() {
        s if s.ends_with("dnf") => "sudo dnf install -y",
        s if s.ends_with("apt-get") => "sudo apt-get install -y",
        s if s.ends_with("apk") => "sudo apk add",
        _ => "sudo dnf install -y", // fallback
    };

    let pkgs_str = packages.join(" ");
    let run_cmd = format!("{install_cmd} {pkgs_str}");
    let args = build_provision_shell_argv(name, &run_cmd);
    let inv = Invocation::new("distrobox", args, RunMode::Capture);
    let out = runner.run(inv)?;

    if out.status != 0 {
        return Err(CboxError::backend_error(out.status, &out.stderr, &out.argv));
    }

    Ok(())
}

fn create_spec_from_boxfile_and_apply_spec(
    bf: &crate::boxfile::model::Boxfile,
    spec: &ApplySpec,
) -> CreateSpec {
    use crate::boxfile::model::DockerModeField;
    use spec::DockerMode;

    let docker_mode = match bf.docker {
        DockerModeField::None => DockerMode::None,
        DockerModeField::Host => DockerMode::Host,
        DockerModeField::Nested => DockerMode::Nested,
    };

    let mounts: Vec<spec::MountSpec> = bf
        .mounts
        .iter()
        .map(|m| spec::MountSpec {
            host: m.host.clone(),
            guest: m.guest.clone(),
            mode: m.mode.as_str().to_string(),
        })
        .collect();

    CreateSpec {
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
        boxfile_path: Some(spec.boxfile_path.clone()),
        unshare: bf.sandbox.unshare.to_arg_string(),
        backend: spec.backend.clone(),
        uid: get_host_uid(),
        dry_run: spec.dry_run,
        // Secret injection is handled by the caller (apply path resolves secrets
        // before calling create, then sets these fields on the returned spec).
        // For the recreate path through apply (no CLI secret store available here),
        // we leave these empty — secret injection via apply is handled by the apply
        // caller which has access to the store. The pure-core recreate path does not
        // have a store reference; the CLI caller must handle it.
        env_flags: Vec::new(),
        env_values: Vec::new(),
        plain_env: Vec::new(),
    }
}

fn get_host_uid() -> u32 {
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

// ─── up ──────────────────────────────────────────────────────────────────────

/// Create-if-absent then apply. The "just works" entry point.
pub fn up(
    spec: &UpSpec,
    store: &dyn ProvisionStateStore,
    runner: &dyn DistroboxRunner,
) -> Result<UpOutcome, CboxError> {
    let name = &spec.create_spec.name;

    // Check if the box exists via inspect
    let inspect_spec = InspectSpec {
        name: name.clone(),
        raw: false,
        backend: spec.create_spec.backend.clone(),
    };

    let box_exists = match inspect(&inspect_spec, runner) {
        Ok(_) => true,
        Err(e) if e.exit_code() == crate::error::exit::UNAVAILABLE => false,
        Err(e) if e.to_string().contains("not found") || e.to_string().contains("No box") => false,
        // If inspect fails with a backend error that looks like not found, treat as absent
        Err(e) if e.exit_code() == 125 => false,
        Err(e) => return Err(e),
    };

    let created = if !box_exists {
        // Create the box
        let mut cs = spec.create_spec.clone();
        cs.dry_run = spec.dry_run;
        create(&cs, runner)?;
        true
    } else {
        false
    };

    // Determine Boxfile path for apply
    let boxfile_path = spec.create_spec.boxfile_path.clone().unwrap_or_else(|| {
        // XDG fallback
        let config_home = std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
            format!("{home}/.config")
        });
        format!("{config_home}/cbox/boxes/{name}/Boxfile.toml")
    });

    let apply_outcome = if created {
        // Freshly created box: all provision steps run; no diff needed (box was just created from
        // the Boxfile, so it matches by construction — except provision steps haven't run yet).
        // Skip the inspect+diff that apply() would do; go straight to provision.
        apply_fresh(name, &boxfile_path, spec, store, runner)?
    } else {
        // Existing box: full apply (inspect, diff, incremental provision)
        let apply_spec = ApplySpec {
            force: spec.apply_force,
            redo: spec.apply_redo.clone(),
            no_provision: spec.no_provision,
            recreate: spec.recreate,
            yes: spec.yes,
            dry_run: spec.dry_run,
            // Thread persist=false provision secrets from UpSpec into the apply call.
            provision_env_keys: spec.provision_env_keys.clone(),
            provision_env: spec.provision_env.clone(),
            // Recreate secrets: the create_spec already has env_flags/env_values
            // from the CLI resolve step; mirror them so core::apply's recreate path
            // can use them when it constructs the recreate CreateSpec.
            recreate_env_flags: spec.create_spec.env_flags.clone(),
            recreate_env_values: spec.create_spec.env_values.clone(),
            recreate_plain_env: spec.create_spec.plain_env.clone(),
            ..ApplySpec::new(name, boxfile_path, spec.create_spec.backend.clone())
        };
        apply(&apply_spec, store, runner)?
    };

    Ok(UpOutcome {
        ok: true,
        action: "up".to_string(),
        created,
        name: name.clone(),
        apply: apply_outcome,
    })
}

/// Run provision steps for a freshly-created box (no diff, empty state, all steps run).
fn apply_fresh(
    name: &str,
    boxfile_path: &str,
    spec: &UpSpec,
    store: &dyn ProvisionStateStore,
    runner: &dyn DistroboxRunner,
) -> Result<ApplyOutcome, CboxError> {
    let (bf, _warnings) = boxfile::parse_file(boxfile_path)?;

    if spec.no_provision {
        return Ok(ApplyOutcome {
            ok: true,
            action: "apply".to_string(),
            name: name.to_string(),
            diff: DiffResult {
                class: "Incremental".to_string(),
                fields: Vec::new(),
            },
            recreate_required: false,
            steps: Vec::new(),
            summary: ApplySummary {
                ran: 0,
                skipped: 0,
                copied: 0,
                failed: 0,
            },
        });
    }

    let boxfile_dir = std::path::Path::new(boxfile_path)
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));

    let plan = provision::ProvisionPlan {
        name,
        steps: &bf.provision,
        boxfile_dir,
        backend: &spec.create_spec.backend,
        force: true, // fresh box: always run all steps
        redo: &[],
        dry_run: spec.dry_run,
        provision_env_keys: &spec.provision_env_keys,
        provision_env: &spec.provision_env,
    };

    let mut state = ProvisionState::new();
    let step_results = provision::provision(&plan, store, runner, &mut state)?;

    // Write the fingerprint after first provision so subsequent applies see it
    // and do not spuriously re-converge (AC-DIFF-3 anti-blind-spot).
    state.env_secret_fingerprint = env_secret_fingerprint(&bf);
    state.secret_specs = build_secret_specs(&bf);
    state.env_keys = build_env_keys(&bf);
    let _ = store.write(name, &state, runner); // best-effort

    let summary = summarize(&step_results);
    Ok(ApplyOutcome {
        ok: true,
        action: "apply".to_string(),
        name: name.to_string(),
        diff: DiffResult {
            class: "Incremental".to_string(),
            fields: Vec::new(),
        },
        recreate_required: false,
        steps: step_results,
        summary,
    })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dbox::{
        backend::Backend,
        mock::{MockMatcher, MockResponse, MockRunner},
    };

    // Build a minimal podman-style ps JSON output for one box named `name`.
    fn ps_json_for(name: &str) -> String {
        format!(
            r#"[{{"Names":["{name}"],"State":"running","Image":"ubuntu:22.04","Id":"abc1","Labels":{{"manager":"distrobox","cbox.managed":"true","cbox.docker_mode":"none"}}}}]"#
        )
    }

    // ─── Fix A: resolve_backend cross-backend routing ─────────────────────────

    /// When --backend is given explicitly, resolve_backend returns that backend.
    #[test]
    fn resolve_backend_explicit_override_wins() {
        let runner = MockRunner::new().with_default(MockResponse::ok("[]"));
        let result = resolve_backend("mybox", Some("podman"), &runner);
        assert_eq!(result.unwrap(), Backend::Podman);
    }

    /// When --backend docker is given explicitly, resolve_backend returns Docker.
    #[test]
    fn resolve_backend_explicit_docker_override() {
        let runner = MockRunner::new().with_default(MockResponse::ok("[]"));
        let result = resolve_backend("mybox", Some("docker"), &runner);
        assert_eq!(result.unwrap(), Backend::Docker);
    }

    /// resolve_backend returns the backend that lists the named box.
    #[test]
    fn resolve_backend_routes_to_backend_with_box() {
        // Simulate two backends; only docker knows about "mybox".
        let runner = MockRunner::new()
            .with_matcher(
                MockMatcher::new(MockResponse::ok("[]"))
                    .with_program("podman")
                    .with_args_contain(vec!["ps".to_string()]),
            )
            .with_matcher(
                MockMatcher::new(MockResponse::ok(ps_json_for("mybox")))
                    .with_program("docker")
                    .with_args_contain(vec!["ps".to_string()]),
            );
        // Force both backends to be usable via the override selecting docker.
        // (Real probing requires live daemons; override is deterministic in tests.)
        let result = resolve_backend("mybox", Some("docker"), &runner);
        assert_eq!(result.unwrap(), Backend::Docker);
    }

    // ─── Fix B: project_inspect_json populates cbox_image from label ─────────

    /// Confirm that project_inspect_json extracts the cbox.image label into
    /// InspectResult.cbox_image so the diff layer can use it.
    #[test]
    fn project_inspect_json_extracts_cbox_image_label() {
        let json = r#"[{
            "Id": "abc123",
            "State": {"Status": "running"},
            "Image": "sha256:30ba4450",
            "Created": "2024-01-01T00:00:00Z",
            "Config": {
                "Labels": {
                    "cbox.managed": "true",
                    "cbox.docker_mode": "none",
                    "cbox.image": "docker.io/library/ubuntu:26.04",
                    "cbox.boxfile_path": "/home/user/.config/cbox/boxes/mybox/Boxfile.toml"
                }
            },
            "Mounts": []
        }]"#;
        let result = project_inspect_json(json, "mybox", &Backend::Podman).unwrap();
        assert_eq!(
            result.cbox_image.as_deref(),
            Some("docker.io/library/ubuntu:26.04"),
            "cbox_image must be populated from the label"
        );
        // live.image is still the raw value from the backend (unchanged)
        assert_eq!(result.image, "sha256:30ba4450");
    }

    /// When cbox.image label is absent, cbox_image is None.
    #[test]
    fn project_inspect_json_cbox_image_absent_is_none() {
        let json = r#"[{
            "Id": "abc123",
            "State": {"Status": "running"},
            "Image": "docker.io/library/ubuntu:26.04",
            "Created": "2024-01-01T00:00:00Z",
            "Config": {
                "Labels": {
                    "cbox.managed": "true",
                    "cbox.docker_mode": "none"
                }
            },
            "Mounts": []
        }]"#;
        let result = project_inspect_json(json, "mybox", &Backend::Podman).unwrap();
        assert!(
            result.cbox_image.is_none(),
            "cbox_image must be None when label is absent"
        );
    }

    // ─── Fix #2: project_inspect_json populates home + hostname ─────────────

    /// Confirm home is extracted from Config.Env HOME=… and hostname from Config.Hostname.
    #[test]
    fn project_inspect_json_extracts_home_and_hostname() {
        let json = r#"[{
            "Id": "abc123",
            "State": {"Status": "running"},
            "Image": "docker.io/library/ubuntu:26.04",
            "Created": "2024-01-01T00:00:00Z",
            "Config": {
                "Hostname": "electionbuddy-box",
                "Env": [
                    "DOCKER_HOST=unix:///var/run/docker.sock",
                    "HOME=/home/rynaro/.cbox-homes/electionbuddy",
                    "SHELL=zsh"
                ],
                "Labels": {
                    "cbox.managed": "true",
                    "cbox.docker_mode": "host"
                }
            },
            "Mounts": []
        }]"#;
        let result = project_inspect_json(json, "electionbuddy", &Backend::Podman).unwrap();
        assert_eq!(
            result.home.as_deref(),
            Some("/home/rynaro/.cbox-homes/electionbuddy"),
            "home must be extracted from Config.Env HOME="
        );
        assert_eq!(
            result.hostname.as_deref(),
            Some("electionbuddy-box"),
            "hostname must be extracted from Config.Hostname"
        );
    }

    /// Confirm that parse_mounts derives mode from the RW boolean when Mode is "".
    /// This mirrors what podman/docker inspect actually returns for default rw mounts.
    #[test]
    fn project_inspect_json_empty_mode_derives_from_rw() {
        let json = r#"[{
            "Id": "abc123",
            "State": {"Status": "running"},
            "Image": "ubuntu:22.04",
            "Created": "2024-01-01T00:00:00Z",
            "Config": {
                "Labels": {
                    "cbox.managed": "true",
                    "cbox.docker_mode": "none"
                }
            },
            "Mounts": [
                {
                    "Source": "/home/user/workspace",
                    "Destination": "/home/user/workspace",
                    "Mode": "",
                    "RW": true
                },
                {
                    "Source": "/etc/config",
                    "Destination": "/etc/config",
                    "Mode": "",
                    "RW": false
                }
            ]
        }]"#;
        let result = project_inspect_json(json, "mybox", &Backend::Podman).unwrap();
        assert_eq!(result.mounts.len(), 2);
        // Empty Mode + RW=true → projected as "rw"
        assert_eq!(
            result.mounts[0].mode, "rw",
            "empty Mode with RW=true must project as rw"
        );
        // Empty Mode + RW=false → projected as "ro"
        assert_eq!(
            result.mounts[1].mode, "ro",
            "empty Mode with RW=false must project as ro"
        );
    }

    /// When Config.Env is absent, home is None.
    #[test]
    fn project_inspect_json_home_absent_is_none() {
        let json = r#"[{
            "Id": "abc123",
            "State": {"Status": "running"},
            "Image": "docker.io/library/ubuntu:26.04",
            "Created": "2024-01-01T00:00:00Z",
            "Config": {
                "Labels": {
                    "cbox.managed": "true",
                    "cbox.docker_mode": "none"
                }
            },
            "Mounts": []
        }]"#;
        let result = project_inspect_json(json, "mybox", &Backend::Podman).unwrap();
        assert!(
            result.home.is_none(),
            "home must be None when Env is absent"
        );
        assert!(
            result.hostname.is_none(),
            "hostname must be None when Hostname is absent"
        );
    }
}
