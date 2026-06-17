use super::output::OutputCtx;
use crate::boxfile::validate::is_valid_name;
use crate::core::{self, spec::EnterSpec};
use crate::dbox::runner::DistroboxRunner;
use crate::error::CboxError;
use clap::Args;

#[derive(Args, Debug)]
pub struct EnterArgs {
    /// Box name.
    #[arg(value_name = "NAME")]
    pub name: String,

    /// Enter as root.
    #[arg(long)]
    pub root: bool,

    /// Start with a clean PATH.
    #[arg(long)]
    pub clean_path: bool,

    /// Command to run inside the box (after --).
    #[arg(last = true, value_name = "CMD")]
    pub cmd: Vec<String>,
}

pub fn run(
    args: &EnterArgs,
    global_json: bool,
    _ctx: &OutputCtx,
    runner: &dyn DistroboxRunner,
) -> Result<(), CboxError> {
    // --json is rejected for enter (interactive)
    if global_json {
        return Err(CboxError::usage(
            "enter is interactive; --json not supported",
        ));
    }

    if !is_valid_name(&args.name) {
        return Err(CboxError::usage(format!(
            "Invalid box name \"{}\". Names must match ^[a-zA-Z0-9][a-zA-Z0-9_.-]*$",
            args.name
        )));
    }

    let spec = EnterSpec {
        name: args.name.clone(),
        root: args.root,
        clean_path: args.clean_path,
        cmd: args.cmd.clone(),
    };

    let exit_code = core::enter(&spec, runner)?;
    if exit_code != 0 {
        std::process::exit(exit_code);
    }

    Ok(())
}
