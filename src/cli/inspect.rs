use super::output::{render_inspect_panel, OutputCtx};
use crate::core::{self, spec::InspectSpec};
use crate::dbox::runner::DistroboxRunner;
use crate::error::CboxError;
use clap::Args;

#[derive(Args, Debug)]
pub struct InspectArgs {
    /// Box name.
    #[arg(value_name = "NAME")]
    pub name: String,

    /// Emit raw backend JSON without projection.
    #[arg(long)]
    pub raw: bool,
}

pub fn run(
    args: &InspectArgs,
    global_backend: Option<&str>,
    ctx: &OutputCtx,
    runner: &dyn DistroboxRunner,
) -> Result<(), CboxError> {
    // Route to whichever engine actually hosts this box.
    let backend = core::resolve_backend(&args.name, global_backend, runner)?;

    let spec = InspectSpec {
        name: args.name.clone(),
        raw: args.raw,
        backend,
    };

    if args.raw {
        let raw = core::inspect_raw(&spec, runner)?;
        println!("{raw}");
        return Ok(());
    }

    let result = core::inspect(&spec, runner)?;

    if ctx.json {
        let v = serde_json::json!({
            "ok": true,
            "box": result,
        });
        ctx.print_json(&v);
    } else {
        render_inspect_panel(&result, ctx);
    }

    Ok(())
}
