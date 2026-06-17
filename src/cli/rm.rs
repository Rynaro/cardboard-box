use super::output::OutputCtx;
use crate::core::{self, spec::RmSpec};
use crate::dbox::runner::DistroboxRunner;
use crate::error::CboxError;
use clap::Args;

#[derive(Args, Debug)]
pub struct RmArgs {
    /// Box name(s) to remove.
    #[arg(value_name = "NAME", required_unless_present = "all")]
    pub names: Vec<String>,

    /// Force removal (stop running box).
    #[arg(short = 'f', long)]
    pub force: bool,

    /// Remove the custom home directory too.
    #[arg(long)]
    pub rm_home: bool,

    /// Remove all boxes.
    #[arg(long)]
    pub all: bool,
}

pub fn run(
    args: &RmArgs,
    global_yes: bool,
    ctx: &OutputCtx,
    runner: &dyn DistroboxRunner,
) -> Result<(), CboxError> {
    // all requires -y
    if args.all && !global_yes {
        return Err(CboxError::usage(
            "--all requires -y/--yes to prevent accidental removal of all boxes.",
        ));
    }

    // Confirm unless -y
    if !global_yes && !args.all {
        let names_str = args.names.join(", ");
        eprint!("Remove box \"{names_str}\"? This deletes the container. [y/N] ");
        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .map_err(|e| CboxError::ioerr(format!("Failed to read input: {e}")))?;
        if input.trim().to_lowercase() != "y" {
            if ctx.json {
                let v = serde_json::json!({
                    "ok": true,
                    "action": "rm",
                    "removed": [],
                    "skipped": args.names,
                    "reason": "aborted by user",
                });
                ctx.print_json(&v);
            } else {
                println!("Aborted.");
            }
            return Ok(());
        }
    }

    let spec = RmSpec {
        names: args.names.clone(),
        force: args.force,
        rm_home: args.rm_home,
        all: args.all,
        yes: global_yes,
    };

    let outcome = core::rm(&spec, runner)?;

    if ctx.json {
        let v = serde_json::json!({
            "ok": true,
            "action": "rm",
            "removed": outcome.removed,
            "skipped": outcome.skipped,
        });
        ctx.print_json(&v);
    } else {
        for name in &outcome.removed {
            ctx.success(&format!("Removed box \"{name}\""));
        }
    }

    Ok(())
}
