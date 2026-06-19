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
        // Try to resolve the Boxfile to learn which keys are secrets (for masking).
        // If the Boxfile is not discoverable, mask nothing and note it on stderr.
        let secret_keys = resolve_boxfile_secret_keys(&args.name, &spec, runner);
        let raw = core::inspect_raw_with_secret_keys(&spec, runner, &secret_keys)?;
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

/// Attempt to discover the Boxfile for a box and return its [secrets] KEY names.
/// Returns empty Vec when the Boxfile is not discoverable (no label, file gone).
fn resolve_boxfile_secret_keys(
    name: &str,
    spec: &InspectSpec,
    runner: &dyn DistroboxRunner,
) -> Vec<String> {
    use crate::core::spec::EditSpec;
    let edit_spec = EditSpec {
        name: Some(name.to_string()),
        file: None,
        backend: spec.backend.clone(),
    };
    let path = match core::resolve_boxfile_path(name, &edit_spec, runner) {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "note: could not resolve Boxfile; secret values in Config.Env are not masked."
            );
            return vec![];
        }
    };
    if !std::path::Path::new(&path).exists() {
        eprintln!("note: could not resolve Boxfile; secret values in Config.Env are not masked.");
        return vec![];
    }
    match crate::boxfile::parse_file(&path) {
        Ok((bf, _)) => bf.secrets.keys().cloned().collect(),
        Err(_) => {
            eprintln!("note: could not parse Boxfile; secret values in Config.Env are not masked.");
            vec![]
        }
    }
}
