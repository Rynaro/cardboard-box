//! Serde structs for Boxfile.toml (§6). Unknown top-level keys → warning, not error.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The Boxfile.toml declarative manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Boxfile {
    /// REQUIRED. Box name. ^[a-zA-Z0-9][a-zA-Z0-9_.-]*$
    pub name: String,

    /// Container image. Default: registry.fedoraproject.org/fedora-toolbox:latest
    #[serde(default = "default_image")]
    pub image: String,

    /// Additional packages to install. Default: []
    #[serde(default)]
    pub packages: Vec<String>,

    /// Docker access mode. Default: none
    #[serde(default)]
    pub docker: DockerModeField,

    /// Host↔guest mounts.
    #[serde(default)]
    pub mounts: Vec<MountEntry>,

    /// Sandbox hardening options.
    #[serde(default)]
    pub sandbox: SandboxConfig,

    /// Box runtime knobs.
    #[serde(rename = "box", default)]
    pub box_config: BoxConfig,

    /// Provisioning steps (P1: parsed+validated only; P2: executed).
    #[serde(default)]
    pub provision: Vec<ProvisionStep>,

    /// Secret KEY references. Values live ONLY in the OS keyring (D0).
    /// Each entry is `{ persist = <bool>, from = "keyring" }`.
    #[serde(default)]
    pub secrets: BTreeMap<String, SecretEntry>,

    /// Plaintext, non-secret env. Values are fine in argv (not secret).
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

/// A [secrets] entry — KEY-only reference; value lives in the keyring (D0).
/// `deny_unknown_fields` is ON (per sub-table convention) to catch typos hard.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SecretEntry {
    /// When true (default), the secret is baked into Config.Env at create time.
    /// When false, it is injected only into provision exec-sessions, never into Config.Env.
    #[serde(default = "default_persist")]
    pub persist: bool,
    /// Backend source. Only "keyring" is valid in v1; "prompt" and others → exit 65.
    #[serde(default = "default_from")]
    pub from: String,
}

fn default_persist() -> bool {
    true
}

fn default_from() -> String {
    "keyring".to_string()
}

fn default_image() -> String {
    "registry.fedoraproject.org/fedora-toolbox:latest".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum DockerModeField {
    #[default]
    None,
    Host,
    Nested,
}

impl DockerModeField {
    pub fn as_str(&self) -> &'static str {
        match self {
            DockerModeField::None => "none",
            DockerModeField::Host => "host",
            DockerModeField::Nested => "nested",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountEntry {
    pub host: String,
    pub guest: String,
    #[serde(default = "default_mode")]
    pub mode: MountMode,
}

fn default_mode() -> MountMode {
    MountMode::Rw
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MountMode {
    Ro,
    Rw,
}

impl MountMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            MountMode::Ro => "ro",
            MountMode::Rw => "rw",
        }
    }
}

/// Sandbox hardening sub-table.
/// deny_unknown_fields is ON for sub-tables (§6.2).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SandboxConfig {
    /// "all" or a list of: netns, ipc, process, devsys, groups
    #[serde(default)]
    pub unshare: UnshareSpec,

    /// --init (systemd; implies unshare-process)
    #[serde(default)]
    pub init: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(untagged)]
pub enum UnshareSpec {
    #[default]
    Empty,
    All(String),       // "all"
    List(Vec<String>), // ["netns","ipc",…]
}

impl UnshareSpec {
    pub fn is_empty(&self) -> bool {
        match self {
            UnshareSpec::Empty => true,
            UnshareSpec::All(s) => s.is_empty(),
            UnshareSpec::List(v) => v.is_empty(),
        }
    }

    /// Convert to a normalized string for use in argv building.
    /// Returns None if empty, Some("all") or Some("netns ipc …").
    pub fn to_arg_string(&self) -> Option<String> {
        match self {
            UnshareSpec::Empty => None,
            UnshareSpec::All(s) if s == "all" => Some("all".to_string()),
            UnshareSpec::All(s) if s.is_empty() => None,
            UnshareSpec::All(s) => Some(s.clone()),
            UnshareSpec::List(v) if v.is_empty() => None,
            UnshareSpec::List(v) => Some(v.join(" ")),
        }
    }

    #[allow(dead_code)]
    pub fn items(&self) -> Vec<&str> {
        match self {
            UnshareSpec::Empty => vec![],
            UnshareSpec::All(s) => vec![s.as_str()],
            UnshareSpec::List(v) => v.iter().map(|s| s.as_str()).collect(),
        }
    }
}

/// Box runtime knobs sub-table.
/// deny_unknown_fields is ON for sub-tables (§6.2).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct BoxConfig {
    #[serde(default)]
    pub home: String,
    #[serde(default)]
    pub hostname: String,
    #[serde(default)]
    pub pull: bool,
}

/// A provisioning step (P1: parse+validate only).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisionStep {
    #[serde(rename = "type")]
    pub step_type: ProvisionType,
    pub run: Option<String>,
    pub src: Option<String>,
    pub dst: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProvisionType {
    Shell,
    Copy,
}
