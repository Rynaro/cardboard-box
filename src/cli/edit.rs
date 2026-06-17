use super::output::OutputCtx;
use crate::boxfile;
use crate::core::{self, spec::EditSpec};
use crate::dbox::backend::Backend;
use crate::dbox::runner::DistroboxRunner;
use crate::error::CboxError;
use clap::Args;
use std::io::Write;

#[derive(Args, Debug)]
pub struct EditArgs {
    /// Box name (edit its associated Boxfile).
    #[arg(value_name = "NAME")]
    pub name: Option<String>,

    /// Edit a Boxfile directly.
    #[arg(long = "file", value_name = "PATH")]
    pub file: Option<String>,
}

pub fn run(
    args: &EditArgs,
    global_json: bool,
    global_backend: Option<&str>,
    _ctx: &OutputCtx,
    runner: &dyn DistroboxRunner,
) -> Result<(), CboxError> {
    // --json not supported for edit (interactive)
    if global_json {
        return Err(CboxError::usage(
            "edit is interactive; --json not supported",
        ));
    }

    let backend = Backend::detect(global_backend)?;

    let spec = EditSpec {
        name: args.name.clone(),
        file: args.file.clone(),
        backend,
    };

    // Determine the boxfile path
    let boxfile_path = if let Some(ref file) = spec.file {
        file.clone()
    } else if let Some(ref name) = spec.name {
        core::resolve_boxfile_path(name, &spec, runner)?
    } else {
        return Err(CboxError::usage("Provide a NAME or --file PATH."));
    };

    // Ensure the Boxfile exists (scaffold if absent)
    if !std::path::Path::new(&boxfile_path).exists() {
        let name = spec.name.as_deref().unwrap_or("box");
        let content = core::scaffold_boxfile(name, &spec, runner);

        // Create parent directories
        if let Some(parent) = std::path::Path::new(&boxfile_path).parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                CboxError::ioerr(format!("Cannot create directory {}: {e}", parent.display()))
            })?;
        }
        std::fs::write(&boxfile_path, &content)
            .map_err(|e| CboxError::ioerr(format!("Cannot write {boxfile_path}: {e}")))?;
    }

    // Save original content for abort/keep-anyway
    let original = std::fs::read_to_string(&boxfile_path)
        .map_err(|e| CboxError::ioerr(format!("Cannot read {boxfile_path}: {e}")))?;

    // Resolve editor
    let editor = resolve_editor()?;

    // Open in editor (inherit TTY)
    loop {
        let status = std::process::Command::new(&editor)
            .arg(&boxfile_path)
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()
            .map_err(|e| CboxError::ioerr(format!("Failed to spawn editor \"{editor}\": {e}")))?;

        if !status.success() {
            eprintln!("Editor exited with non-zero status.");
        }

        // Re-validate
        let current = std::fs::read_to_string(&boxfile_path)
            .map_err(|e| CboxError::ioerr(format!("Cannot read {boxfile_path}: {e}")))?;

        match boxfile::parse_and_validate(&current) {
            Ok((_, warnings)) => {
                for w in &warnings {
                    eprintln!("warn: {w}");
                }
                let name_display = spec.name.as_deref().unwrap_or(&boxfile_path);
                println!("Saved Boxfile for \"{name_display}\".");
                println!("  Apply changes with:  cbox apply {name_display}   (Phase 2)");
                return Ok(());
            }
            Err(e) => {
                eprintln!("Boxfile validation error:\n  {e}");
                eprint!("[r]e-edit / [k]eep-anyway / [a]bort? ");
                std::io::stderr().flush().ok();
                let mut choice = String::new();
                std::io::stdin()
                    .read_line(&mut choice)
                    .map_err(|e| CboxError::ioerr(format!("Failed to read input: {e}")))?;
                match choice.trim() {
                    "k" | "keep" => {
                        let name_display = spec.name.as_deref().unwrap_or(&boxfile_path);
                        println!("Kept (invalid) Boxfile for \"{name_display}\".");
                        return Ok(());
                    }
                    "a" | "abort" => {
                        // Restore original
                        std::fs::write(&boxfile_path, &original).map_err(|e| {
                            CboxError::ioerr(format!("Cannot restore {boxfile_path}: {e}"))
                        })?;
                        return Err(CboxError::dataerr(
                            "Edit aborted; Boxfile restored to original.",
                        ));
                    }
                    _ => {
                        // re-edit (default)
                        continue;
                    }
                }
            }
        }
    }
}

fn resolve_editor() -> Result<String, CboxError> {
    for var in &["VISUAL", "EDITOR"] {
        if let Ok(e) = std::env::var(var) {
            if !e.is_empty() {
                return Ok(e);
            }
        }
    }
    // Probe fallbacks
    for candidate in &["vi", "nano"] {
        if std::process::Command::new("which")
            .arg(candidate)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return Ok(candidate.to_string());
        }
    }
    Err(CboxError::software(
        "No editor found. Set $VISUAL or $EDITOR, or install vi/nano.",
    ))
}
