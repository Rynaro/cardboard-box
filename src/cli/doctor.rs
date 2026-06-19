use super::output::{render_doctor, OutputCtx};
use crate::core::{self, spec::DoctorSpec};
use crate::dbox::runner::DistroboxRunner;
use crate::error::CboxError;
use crate::secret::SecretStore;
use clap::Args;

#[derive(Args, Debug)]
pub struct DoctorArgs {
    // No per-command flags; global --backend is used.
}

#[allow(dead_code)]
pub fn run(
    _args: &DoctorArgs,
    global_backend: Option<&str>,
    ctx: &OutputCtx,
    runner: &dyn DistroboxRunner,
) -> Result<(), CboxError> {
    run_with_store(_args, global_backend, ctx, runner, &default_no_op_store())
}

/// Variant that accepts a SecretStore for the keyring probe.
pub fn run_with_store(
    _args: &DoctorArgs,
    global_backend: Option<&str>,
    ctx: &OutputCtx,
    runner: &dyn DistroboxRunner,
    store: &dyn SecretStore,
) -> Result<(), CboxError> {
    let spec = DoctorSpec {
        backend_override: global_backend.map(|s| s.to_string()),
    };

    let result = core::doctor(&spec, runner, store)?;

    if ctx.json {
        let v = serde_json::json!({
            "ok": result.ok,
            "distrobox": result.distrobox,
            "backend": result.backend,
            "warnings": result.warnings,
            "keyring": result.keyring,
        });
        ctx.print_json(&v);
    } else {
        render_doctor(&result, ctx);
    }

    Ok(())
}

/// A no-op store that always returns Unavailable — used when no store is available.
/// This gives the existing `run` path a store to call doctor with.
#[allow(dead_code)]
struct NoOpStore;

impl SecretStore for NoOpStore {
    fn set(
        &self,
        _box_name: &str,
        _key: &str,
        _value: &str,
    ) -> Result<(), crate::secret::SecretError> {
        Err(crate::secret::SecretError::Unavailable(
            "no store configured".to_string(),
        ))
    }
    fn get(
        &self,
        _box_name: &str,
        _key: &str,
    ) -> Result<Option<String>, crate::secret::SecretError> {
        Err(crate::secret::SecretError::Unavailable(
            "no store configured".to_string(),
        ))
    }
    fn delete(&self, _box_name: &str, _key: &str) -> Result<(), crate::secret::SecretError> {
        Err(crate::secret::SecretError::Unavailable(
            "no store configured".to_string(),
        ))
    }
    fn list(&self, _box_name: &str) -> Result<Vec<String>, crate::secret::SecretError> {
        Err(crate::secret::SecretError::Unavailable(
            "no store configured".to_string(),
        ))
    }
}

#[allow(dead_code)]
fn default_no_op_store() -> NoOpStore {
    NoOpStore
}
