//! Typed spec structs — the contracts between cli/ and core/.
//! These are front-end-agnostic; both CLI and TUI populate them.

use crate::dbox::backend::Backend;

/// The docker access mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DockerMode {
    None,
    Host,
    Nested,
}

impl DockerMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            DockerMode::None => "none",
            DockerMode::Host => "host",
            DockerMode::Nested => "nested",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "none" => Some(DockerMode::None),
            "host" => Some(DockerMode::Host),
            "nested" => Some(DockerMode::Nested),
            _ => None,
        }
    }
}

/// A mount entry.
#[derive(Debug, Clone)]
pub struct MountSpec {
    pub host: String,
    pub guest: String,
    pub mode: String, // "ro" | "rw"
}

/// Spec for `cbox create`.
#[derive(Debug, Clone)]
pub struct CreateSpec {
    pub name: String,
    pub image: String,
    pub packages: Vec<String>,
    pub docker_mode: DockerMode,
    pub mounts: Vec<MountSpec>,
    pub home: Option<String>,
    pub hostname: Option<String>,
    pub init: bool,
    pub pull: bool,
    pub root: bool,
    pub boxfile_path: Option<String>,
    /// sandbox.unshare, as a normalized string ("all" or "netns ipc …").
    pub unshare: Option<String>,
    /// Backend for docker-mode socket resolution.
    pub backend: Backend,
    /// Host UID for socket path.
    pub uid: u32,
    /// Dry-run mode.
    pub dry_run: bool,

    // ─── secret / env injection (added for v5.0 native secrets) ─────────────
    /// KEY names for persist=true secrets → `--env KEY` (name-only) in argv.
    /// Values are NEVER placed in argv — they ride `env_values` on Invocation.env.
    pub env_flags: Vec<String>,
    /// (KEY, VALUE) pairs for persist=true secrets → attached to Invocation.env.
    /// The VALUE is never in argv.
    pub env_values: Vec<(String, String)>,
    /// (KEY, VALUE) pairs from `[env]` table → `--env KEY=VALUE` inline in argv
    /// (non-secret, so value-in-argv is acceptable).
    pub plain_env: Vec<(String, String)>,
}

impl CreateSpec {
    pub fn new(name: impl Into<String>, backend: Backend) -> Self {
        Self {
            name: name.into(),
            image: "registry.fedoraproject.org/fedora-toolbox:latest".to_string(),
            packages: Vec::new(),
            docker_mode: DockerMode::None,
            mounts: Vec::new(),
            home: None,
            hostname: None,
            init: false,
            pull: false,
            root: false,
            boxfile_path: None,
            unshare: None,
            backend,
            uid: get_uid(),
            dry_run: false,
            env_flags: Vec::new(),
            env_values: Vec::new(),
            plain_env: Vec::new(),
        }
    }
}

/// Spec for `cbox rm`.
#[derive(Debug, Clone)]
pub struct RmSpec {
    pub names: Vec<String>,
    pub force: bool,
    pub rm_home: bool,
    pub all: bool,
    /// Tracks whether -y was passed; not used by core but part of the contract.
    #[allow(dead_code)]
    pub yes: bool,
    /// Engine the target box(es) live on — pins `distrobox` to the right backend.
    pub backend: Backend,
}

/// Spec for `cbox stop`.
#[derive(Debug, Clone)]
pub struct StopSpec {
    pub names: Vec<String>,
    pub all: bool,
    /// Engine the target box(es) live on — pins `distrobox` to the right backend.
    pub backend: Backend,
}

/// Spec for `cbox enter`.
#[derive(Debug, Clone)]
pub struct EnterSpec {
    pub name: String,
    pub root: bool,
    pub clean_path: bool,
    pub cmd: Vec<String>,
    /// Engine the box lives on — pins `distrobox` to the right backend.
    pub backend: Backend,
}

/// Spec for `cbox inspect`.
#[derive(Debug, Clone)]
pub struct InspectSpec {
    pub name: String,
    /// When true, emit raw backend JSON (handled in cli/inspect.rs before calling core).
    #[allow(dead_code)]
    pub raw: bool,
    pub backend: Backend,
}

/// Projected inspect result (§13 inspect_json_schema).
#[derive(Debug, Clone, serde::Serialize)]
pub struct InspectResult {
    pub name: String,
    pub status: String,
    pub image: String,
    pub created: String,
    pub docker_mode: String,
    pub mounts: Vec<MountResult>,
    pub packages: Vec<String>,
    pub backend: String,
    pub id: String,
    pub boxfile_path: Option<String>,
    /// The `cbox.image` label written at create time (the Boxfile tag).
    /// `None` on older boxes that pre-date the label.
    pub cbox_image: Option<String>,
    /// The live `$HOME` inside the box, recovered from `Config.Env HOME=…`.
    /// `None` when the inspect JSON does not carry an Env array (unusual).
    pub home: Option<String>,
    /// The live container hostname, from `Config.Hostname`.
    /// `None` when absent from inspect JSON (unusual).
    pub hostname: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MountResult {
    pub host: String,
    pub guest: String,
    pub mode: String,
}

/// A row in the list output.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BoxRow {
    pub name: String,
    pub status: String,
    pub image: String,
    pub docker_mode: String,
    pub cbox_managed: bool,
    pub id: String,
    /// The container engine this box lives on ("podman" | "docker").
    pub backend: String,
}

/// Spec for `cbox edit`.
#[derive(Debug, Clone)]
pub struct EditSpec {
    pub name: Option<String>,
    pub file: Option<String>,
    pub backend: Backend,
}

/// Spec for `cbox doctor`.
#[derive(Debug, Clone)]
pub struct DoctorSpec {
    pub backend_override: Option<String>,
}

/// Doctor result.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DoctorResult {
    pub ok: bool,
    pub distrobox: DistroboxInfo,
    pub backend: BackendInfo,
    pub warnings: Vec<String>,
    /// Keyring availability (non-fatal informational line).
    pub keyring: KeyringStatus,
}

/// Secret Service / keyring availability probe result.
#[derive(Debug, Clone, serde::Serialize)]
pub struct KeyringStatus {
    pub available: bool,
    pub detail: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DistroboxInfo {
    pub present: bool,
    pub version: Option<String>,
    pub supported: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BackendInfo {
    pub selected: Option<String>,
    pub podman: BackendStatus,
    pub docker: BackendStatus,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BackendStatus {
    pub present: bool,
    pub reachable: bool,
    pub version: Option<String>,
}

// ─── Phase 2 spec types ──────────────────────────────────────────────────────

/// Spec for `cbox apply`.
#[derive(Debug, Clone)]
pub struct ApplySpec {
    pub name: String,
    pub boxfile_path: String,
    pub force: bool,
    pub redo: Vec<usize>,
    pub no_provision: bool,
    pub recreate: bool,
    #[allow(dead_code)]
    pub yes: bool,
    pub dry_run: bool,
    pub backend: Backend,

    // ─── persist=false secret injection (v5.0) ──────────────────────────────
    /// KEY names for persist=false secrets → `--env KEY` (name-only) in argv
    /// for provision shell steps.  Empty when no ProvisionOnly secrets exist.
    pub provision_env_keys: Vec<String>,
    /// (KEY, VALUE) pairs for persist=false secrets → Invocation.env for shell
    /// steps.  Never in argv (INV-1).
    pub provision_env: Vec<(String, String)>,

    // ─── persist=true secret injection for recreate path (v5.0) ─────────────
    /// KEY names for persist=true secrets → `--env KEY` in the recreate
    /// create call.  Only populated on the `--recreate` path.
    pub recreate_env_flags: Vec<String>,
    /// (KEY, VALUE) pairs for persist=true secrets on the recreate path.
    pub recreate_env_values: Vec<(String, String)>,
    /// (KEY, VALUE) pairs from `[env]` table for the recreate create call.
    pub recreate_plain_env: Vec<(String, String)>,
}

impl ApplySpec {
    /// Convenience constructor with all secret fields zeroed (the common case
    /// when `apply` is called without a secret store).
    pub fn new(name: impl Into<String>, boxfile_path: impl Into<String>, backend: Backend) -> Self {
        Self {
            name: name.into(),
            boxfile_path: boxfile_path.into(),
            force: false,
            redo: Vec::new(),
            no_provision: false,
            recreate: false,
            yes: false,
            dry_run: false,
            backend,
            provision_env_keys: Vec::new(),
            provision_env: Vec::new(),
            recreate_env_flags: Vec::new(),
            recreate_env_values: Vec::new(),
            recreate_plain_env: Vec::new(),
        }
    }
}

/// Spec for `cbox up`.
#[derive(Debug, Clone)]
pub struct UpSpec {
    pub create_spec: CreateSpec,
    pub apply_force: bool,
    pub apply_redo: Vec<usize>,
    pub no_provision: bool,
    pub recreate: bool,
    pub yes: bool,
    pub dry_run: bool,

    // ─── persist=false secret injection (v5.0) ──────────────────────────────
    /// KEY names for persist=false secrets → provision shell step `--env KEY`.
    pub provision_env_keys: Vec<String>,
    /// (KEY, VALUE) pairs for persist=false secrets → Invocation.env for
    /// provision shell steps.  Never in argv (INV-1).
    pub provision_env: Vec<(String, String)>,
}

/// Outcome of `cbox apply`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ApplyOutcome {
    pub ok: bool,
    pub action: String,
    pub name: String,
    pub diff: DiffResult,
    pub recreate_required: bool,
    pub steps: Vec<ProvisionStepResult>,
    pub summary: ApplySummary,
}

/// Outcome of `cbox up`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct UpOutcome {
    pub ok: bool,
    pub action: String,
    pub created: bool,
    pub name: String,
    pub apply: ApplyOutcome,
}

/// Summary counts for apply output.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ApplySummary {
    pub ran: usize,
    pub skipped: usize,
    pub copied: usize,
    pub failed: usize,
}

/// The result of a single provision step.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProvisionStepResult {
    pub idx: usize,
    #[serde(rename = "type")]
    pub step_type: String,
    pub status: String, // "ran" | "skipped" | "copied" | "failed"
    pub hash: String,
    pub duration_ms: u64,
    pub exit_code: Option<i32>,
    /// Captured stderr from the step subprocess (non-empty only on failure).
    /// Excluded from the stable JSON schema to avoid breaking existing consumers.
    #[serde(skip)]
    pub captured_stderr: String,
    /// Captured stdout from the step subprocess (non-empty only on failure,
    /// used as fallback when stderr is empty).
    /// Excluded from the stable JSON schema to avoid breaking existing consumers.
    #[serde(skip)]
    pub captured_stdout: String,
    /// The argv that was executed (the distrobox enter … -- sh -c "<run>" vector).
    /// Excluded from the stable JSON schema.
    #[serde(skip)]
    pub argv: Vec<String>,
}

/// Diff result between Boxfile and live.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DiffResult {
    pub class: String, // "Incremental" | "Recreate"
    pub fields: Vec<DiffField>,
}

/// A single changed field in the diff.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DiffField {
    pub field: String,
    pub old: String,
    pub new: String,
    pub class: String, // "Incremental" | "Recreate"
}

/// Package additions identified by the diff.
#[derive(Debug, Clone)]
pub struct PackageDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

// ─── Bundle 2: stats spec types ─────────────────────────────────────────────

/// Spec for a per-box stats poll (engine call: `<backend> stats <id> --no-stream --format json`).
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct StatsSpec {
    /// The container ID (not name) — matches `BoxRow.id`.
    pub id: String,
    /// The engine the box lives on.
    pub backend: Backend,
}

/// A single stats sample parsed from the engine JSON.
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct StatsSample {
    /// CPU usage as a percentage (e.g. 12.5 for 12.5%).
    pub cpu_pct: f64,
    /// Memory currently in use, in bytes.
    pub mem_used: u64,
    /// Memory limit for the container, in bytes.
    pub mem_limit: u64,
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
