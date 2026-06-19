//! Effects — declarative descriptions of side-work the worker executes.
//! `execute_effect` is the worker body; it is pure over (effect, store, runner) and
//! provable with MockRunner (no real distrobox needed).

use std::sync::Arc;

use crate::core;
use crate::core::spec::{
    ApplySpec, CreateSpec, DoctorSpec, EnterSpec, InspectSpec, RmSpec, StopSpec, UpSpec,
};
use crate::core::state_store::{GuestStateStore, ProvisionStateStore};
use crate::dbox::runner::DistroboxRunner;
use crate::secret::{SecretError, SecretStore};
use crate::tui::message::{CreateOutcome, Message, RmOutcome, StopOutcome};

/// No-op secret store for TUI context (keyring not wired into TUI).
struct TuiNoOpStore;
impl SecretStore for TuiNoOpStore {
    fn set(&self, _: &str, _: &str, _: &str) -> Result<(), SecretError> {
        Err(SecretError::Unavailable(
            "keyring not available in TUI".into(),
        ))
    }
    fn get(&self, _: &str, _: &str) -> Result<Option<String>, SecretError> {
        Err(SecretError::Unavailable(
            "keyring not available in TUI".into(),
        ))
    }
    fn delete(&self, _: &str, _: &str) -> Result<(), SecretError> {
        Err(SecretError::Unavailable(
            "keyring not available in TUI".into(),
        ))
    }
    fn list(&self, _: &str) -> Result<Vec<String>, SecretError> {
        Err(SecretError::Unavailable(
            "keyring not available in TUI".into(),
        ))
    }
}

/// Declarative side-work descriptions.
#[derive(Debug)]
#[allow(dead_code)]
pub enum Effect {
    /// Refresh the box list.
    LoadList,
    /// Inspect a single box.
    LoadDetail(InspectSpec),
    /// Create a new box.
    Create(CreateSpec),
    /// Remove one or more boxes.
    Rm(RmSpec),
    /// Stop one or more running boxes.
    Stop(StopSpec),
    /// Apply a Boxfile to an existing box.
    Apply(ApplySpec),
    /// Create-if-absent then apply.
    Up(UpSpec),
    /// Run the doctor check.
    Doctor(DoctorSpec),
    /// Suspend the TUI and run `distrobox enter` interactively.
    /// Handled by the main thread (needs the real TTY).
    SuspendAndEnter(EnterSpec),
    /// Suspend the TUI and open `$EDITOR` for the Boxfile at the given path.
    /// Handled by the main thread (needs the real TTY).
    SuspendAndEdit(String),
    /// Signal the event loop to exit cleanly.
    Quit,
}

/// Execute a data effect synchronously on the worker thread.
///
/// Terminal effects (`SuspendAndEnter`, `SuspendAndEdit`, `Quit`) are **not** handled here —
/// they are routed by the event loop shell on the main thread.
///
/// Returns the `Message` to post back into the reducer.
pub fn execute_effect(
    eff: Effect,
    store: &dyn ProvisionStateStore,
    runner: &Arc<dyn DistroboxRunner>,
    backends: &[crate::dbox::backend::Backend],
) -> Option<Message> {
    match eff {
        Effect::LoadList => {
            // Merge boxes from every usable backend so the list never hides a box
            // that lives on the other engine.
            let result = core::list_all(backends, runner.as_ref()).map(|o| o.boxes);
            Some(Message::ListLoaded(result))
        }

        Effect::LoadDetail(spec) => {
            let result = core::inspect(&spec, runner.as_ref());
            Some(Message::DetailLoaded(result))
        }

        Effect::Create(spec) => {
            let result =
                core::create(&spec, runner.as_ref()).map(|o| CreateOutcome { name: o.name });
            Some(Message::CreateDone(result))
        }

        Effect::Rm(spec) => {
            let result = core::rm(&spec, runner.as_ref()).map(|o| RmOutcome { removed: o.removed });
            Some(Message::RmDone(result))
        }

        Effect::Stop(spec) => {
            let result =
                core::stop(&spec, runner.as_ref()).map(|o| StopOutcome { stopped: o.stopped });
            Some(Message::StopDone(result))
        }

        Effect::Apply(spec) => {
            let result = core::apply(&spec, store, runner.as_ref());
            Some(Message::ApplyDone(result))
        }

        Effect::Up(spec) => {
            let result = core::up(&spec, store, runner.as_ref());
            Some(Message::UpDone(result))
        }

        Effect::Doctor(spec) => {
            let result = core::doctor(&spec, runner.as_ref(), &TuiNoOpStore);
            Some(Message::DoctorDone(result))
        }

        // These are handled by the main thread, not the worker.
        Effect::SuspendAndEnter(_) | Effect::SuspendAndEdit(_) | Effect::Quit => None,
    }
}

/// Build a `GuestStateStore` for use in the worker thread.
pub fn make_store() -> GuestStateStore {
    GuestStateStore
}

/// Assert at compile time that `Arc<dyn DistroboxRunner>` is Send + Sync
/// (so it can be moved into the worker thread). This is a static check; if the
/// trait ever loses `Send + Sync`, this will fail to compile.
#[allow(dead_code)]
fn _assert_runner_send_sync(_: Arc<dyn DistroboxRunner + Send + Sync>) {}
