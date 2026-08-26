//! Read-only Boxfile validation. This command deliberately stops at the parser
//! boundary: it does not construct specs, inspect a backend, or resolve secrets.

use clap::Args;
use serde::Serialize;

use crate::{boxfile, cli::output::OutputCtx, error::CboxError};

#[derive(Args, Debug)]
pub struct ValidateArgs {
    /// Boxfile to validate.
    #[arg(long, value_name = "PATH", default_value = "Boxfile.toml")]
    pub file: String,
}

#[derive(Serialize)]
struct ValidateOutput<'a> {
    ok: bool,
    file: &'a str,
    name: &'a str,
    warnings: &'a [String],
}

pub fn run(args: &ValidateArgs, ctx: &OutputCtx) -> Result<(), CboxError> {
    let (boxfile, warnings) = boxfile::parse_file(&args.file)?;

    if ctx.json {
        ctx.print_json(&ValidateOutput {
            ok: true,
            file: &args.file,
            name: &boxfile.name,
            warnings: &warnings,
        });
    } else {
        for warning in &warnings {
            ctx.warn(warning);
        }
        ctx.success(&format!("{} is valid", args.file));
    }

    Ok(())
}
