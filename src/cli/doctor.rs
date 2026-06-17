use super::output::{render_doctor, OutputCtx};
use crate::core::{self, spec::DoctorSpec};
use crate::dbox::runner::DistroboxRunner;
use crate::error::CboxError;
use clap::Args;

#[derive(Args, Debug)]
pub struct DoctorArgs {
    // No per-command flags; global --backend is used.
}

pub fn run(
    _args: &DoctorArgs,
    global_backend: Option<&str>,
    ctx: &OutputCtx,
    runner: &dyn DistroboxRunner,
) -> Result<(), CboxError> {
    let spec = DoctorSpec {
        backend_override: global_backend.map(|s| s.to_string()),
    };

    let result = core::doctor(&spec, runner)?;

    if ctx.json {
        let v = serde_json::json!({
            "ok": result.ok,
            "distrobox": result.distrobox,
            "backend": result.backend,
            "warnings": result.warnings,
        });
        ctx.print_json(&v);
    } else {
        render_doctor(&result, ctx);
    }

    Ok(())
}
