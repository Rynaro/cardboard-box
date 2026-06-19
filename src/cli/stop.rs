use super::output::OutputCtx;
use crate::core::{self, spec::StopSpec};
use crate::dbox::backend::Backend;
use crate::dbox::runner::DistroboxRunner;
use crate::error::CboxError;
use clap::Args;

#[derive(Args, Debug)]
pub struct StopArgs {
    /// Box name(s) to stop.
    #[arg(value_name = "NAME", required_unless_present = "all")]
    pub names: Vec<String>,

    /// Stop all boxes.
    #[arg(short = 'a', long)]
    pub all: bool,
}

pub fn run(
    args: &StopArgs,
    global_backend: Option<&str>,
    ctx: &OutputCtx,
    runner: &dyn DistroboxRunner,
) -> Result<(), CboxError> {
    // Route to the engine hosting the box. For --all (no specific name) fall
    // back to the preferred usable backend.
    let backend = match args.names.first() {
        Some(name) => core::resolve_backend(name, global_backend, runner)?,
        None => Backend::usable(global_backend)?[0].clone(),
    };

    let spec = StopSpec {
        names: args.names.clone(),
        all: args.all,
        backend,
    };

    let outcome = core::stop(&spec, runner)?;

    if ctx.json {
        let v = serde_json::json!({
            "ok": true,
            "action": "stop",
            "stopped": outcome.stopped,
        });
        ctx.print_json(&v);
    } else {
        for name in &outcome.stopped {
            ctx.success(&format!("Stopped box \"{name}\""));
        }
    }

    Ok(())
}
