//! Provision state store — DECISION-1 seam.
//! The default impl (GuestStateStore) reads/writes the guest-side JSON file via
//! `distrobox enter -- sh -c`. A host-side fallback is drop-in via the trait.

use crate::dbox::{
    argv::{build_state_read_argv, build_state_write_argv},
    runner::{DistroboxRunner, Invocation, RunMode},
};
use crate::error::CboxError;
use serde::{Deserialize, Serialize};

// ─── State model ─────────────────────────────────────────────────────────────

/// The content of `provision.json` inside the guest box.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProvisionState {
    pub cbox_state_version: u32,
    /// SHA-256 of the whole normalized Boxfile (for fast "any change?" check).
    pub boxfile_sha: String,
    /// Effective applied package set (cbox.packages label is stale after incremental adds).
    #[serde(default)]
    pub packages_applied: Vec<String>,
    pub steps: Vec<AppliedStep>,
}

impl ProvisionState {
    pub fn new() -> Self {
        ProvisionState {
            cbox_state_version: 1,
            boxfile_sha: String::new(),
            packages_applied: Vec::new(),
            steps: Vec::new(),
        }
    }

    /// Return the stored hash for a given step index, if any.
    pub fn step_hash(&self, idx: usize) -> Option<&str> {
        self.steps
            .iter()
            .find(|s| s.idx == idx && s.result == "ok")
            .map(|s| s.hash.as_str())
    }

    /// Upsert a step entry (by index).
    pub fn set_step(&mut self, step: AppliedStep) {
        if let Some(existing) = self.steps.iter_mut().find(|s| s.idx == step.idx) {
            *existing = step;
        } else {
            self.steps.push(step);
        }
    }
}

/// A single applied step record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppliedStep {
    pub idx: usize,
    #[serde(rename = "type")]
    pub step_type: String,
    pub hash: String,
    /// Unix epoch seconds.
    pub applied_at: u64,
    /// "ok" | "failed"
    pub result: String,
}

// ─── Trait ───────────────────────────────────────────────────────────────────

/// Seam for provision state persistence (DECISION-1).
/// Default: GuestStateStore (reads/writes via enter -- sh -c).
/// Fallback: host-side sidecar (swap this impl only).
pub trait ProvisionStateStore: Send + Sync {
    fn read(&self, name: &str, runner: &dyn DistroboxRunner) -> Result<ProvisionState, CboxError>;

    fn write(
        &self,
        name: &str,
        state: &ProvisionState,
        runner: &dyn DistroboxRunner,
    ) -> Result<(), CboxError>;
}

// ─── GuestStateStore (default) ───────────────────────────────────────────────

/// Reads/writes the guest-side `~/.local/state/cbox/provision.json`
/// via `distrobox enter --name <N> -- sh -c …`.
pub struct GuestStateStore;

impl ProvisionStateStore for GuestStateStore {
    fn read(&self, name: &str, runner: &dyn DistroboxRunner) -> Result<ProvisionState, CboxError> {
        let args = build_state_read_argv(name);
        let inv = Invocation::new("distrobox", args, RunMode::Capture);
        let out = runner.run(inv)?;

        if out.status != 0 {
            return Err(CboxError::ioerr(format!(
                "Failed to read provision state for \"{name}\": {}",
                out.stderr
            )));
        }

        let text = out.stdout.trim();
        if text.is_empty() {
            return Ok(ProvisionState::new());
        }

        serde_json::from_str(text).map_err(|e| {
            CboxError::ioerr(format!(
                "Provision state for \"{name}\" is corrupt ({e}). \
                 Re-run with --force to reset it."
            ))
        })
    }

    fn write(
        &self,
        name: &str,
        state: &ProvisionState,
        runner: &dyn DistroboxRunner,
    ) -> Result<(), CboxError> {
        let json = serde_json::to_string(state)
            .map_err(|e| CboxError::ioerr(format!("Failed to serialize provision state: {e}")))?;

        let args = build_state_write_argv(name, &json);
        let inv = Invocation::new("distrobox", args, RunMode::Capture);
        let out = runner.run(inv)?;

        if out.status != 0 {
            return Err(CboxError::ioerr(format!(
                "Failed to write provision state for \"{name}\": {}",
                out.stderr
            )));
        }

        Ok(())
    }
}

// ─── Epoch timestamp helper ──────────────────────────────────────────────────

pub fn epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
