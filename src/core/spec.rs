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
}

/// Spec for `cbox enter`.
#[derive(Debug, Clone)]
pub struct EnterSpec {
    pub name: String,
    pub root: bool,
    pub clean_path: bool,
    pub cmd: Vec<String>,
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
