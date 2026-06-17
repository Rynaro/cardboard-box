use super::output::{render_list_table, OutputCtx};
use crate::core;
use crate::dbox::backend::Backend;
use crate::dbox::runner::DistroboxRunner;
use crate::error::CboxError;
use clap::Args;

#[derive(Args, Debug)]
pub struct ListArgs {
    /// Include non-cbox distroboxes.
    #[arg(short = 'a', long)]
    pub all: bool,
}

pub fn run(
    args: &ListArgs,
    global_backend: Option<&str>,
    ctx: &OutputCtx,
    runner: &dyn DistroboxRunner,
) -> Result<(), CboxError> {
    let backend = Backend::detect(global_backend)?;

    // Both human and JSON paths use the machine read path for structured data.
    let outcome = core::list_machine(&backend, runner)?;
    let mut boxes = outcome.boxes;

    if !args.all {
        boxes.retain(|b| b.cbox_managed);
    }

    if ctx.json {
        let v = serde_json::json!({
            "ok": true,
            "boxes": boxes,
        });
        ctx.print_json(&v);
    } else {
        render_list_table(&boxes, ctx);
    }

    Ok(())
}
