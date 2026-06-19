//! `cbox secret set|list|rm` — lean secret-management command surface.
//! Values are never printed; no `get`/`show` verb exists (defense in depth).

use super::output::OutputCtx;
use crate::boxfile::validate::{is_valid_env_name, is_valid_name};
use crate::error::CboxError;
use crate::secret::SecretStore;
use clap::{Args, Subcommand};
use std::io::IsTerminal;

#[derive(Args, Debug)]
pub struct SecretArgs {
    #[command(subcommand)]
    pub command: SecretCommands,
}

#[derive(Subcommand, Debug)]
pub enum SecretCommands {
    /// Store a secret value in the OS keyring (reads from TTY prompt or stdin).
    Set(SecretSetArgs),
    /// List secret KEY names stored for a box (names only, never values).
    List(SecretListArgs),
    /// Remove a secret from the keyring (idempotent).
    Rm(SecretRmArgs),
}

#[derive(Args, Debug)]
pub struct SecretSetArgs {
    /// Box name.
    #[arg(value_name = "BOX")]
    pub box_name: String,
    /// Secret key name (env-var style, e.g. DATABASE_URL).
    #[arg(value_name = "KEY")]
    pub key: String,
}

#[derive(Args, Debug)]
pub struct SecretListArgs {
    /// Box name.
    #[arg(value_name = "BOX")]
    pub box_name: String,
}

#[derive(Args, Debug)]
pub struct SecretRmArgs {
    /// Box name.
    #[arg(value_name = "BOX")]
    pub box_name: String,
    /// Secret key name to remove.
    #[arg(value_name = "KEY")]
    pub key: String,
}

pub fn run(args: &SecretArgs, ctx: &OutputCtx, store: &dyn SecretStore) -> Result<(), CboxError> {
    match &args.command {
        SecretCommands::Set(a) => run_set(a, ctx, store),
        SecretCommands::List(a) => run_list(a, ctx, store),
        SecretCommands::Rm(a) => run_rm(a, ctx, store),
    }
}

// ─── set ─────────────────────────────────────────────────────────────────────

fn run_set(
    args: &SecretSetArgs,
    ctx: &OutputCtx,
    store: &dyn SecretStore,
) -> Result<(), CboxError> {
    // Validate BOX name (exit 64 on bad name)
    if !is_valid_name(&args.box_name) {
        return Err(CboxError::usage(format!(
            "Invalid box name \"{}\". Names must match ^[a-zA-Z0-9][a-zA-Z0-9_.-]*$",
            args.box_name
        )));
    }

    // Validate KEY name (exit 65 on bad name)
    if !is_valid_env_name(&args.key) {
        return Err(CboxError::dataerr(format!(
            "Invalid secret key \"{}\". Must match ^[A-Za-z_][A-Za-z0-9_]*$",
            args.key
        )));
    }

    // Read value: hidden TTY prompt or stdin (piped/CI path)
    let value = read_secret_value(&args.key)?;

    // Empty value guard (exit 64)
    if value.is_empty() {
        return Err(CboxError::usage(format!(
            "Refusing to store an empty value for \"{}\".",
            args.key
        )));
    }

    // Store via SecretStore (exit 75 if unavailable, exit 70 other error)
    store_secret(store, &args.box_name, &args.key, &value)?;

    // Success output
    if ctx.json {
        let v = serde_json::json!({
            "ok": true,
            "action": "secret-set",
            "box": args.box_name,
            "key": args.key,
            // value is NEVER in JSON output
        });
        ctx.print_json(&v);
    } else if !ctx.quiet {
        ctx.success(&format!(
            "Stored secret \"{}\" for \"{}\".",
            args.key, args.box_name
        ));
        eprintln!(
            "hint: To apply a rotated value — persist=false secrets: re-enter / re-provision; \
             persist=true secrets: cbox apply {} --recreate",
            args.box_name
        );
    }

    Ok(())
}

/// Read a secret value: hidden prompt on TTY, or first line from stdin when piped.
fn read_secret_value(key: &str) -> Result<String, CboxError> {
    if std::io::stdin().is_terminal() {
        // TTY: use rpassword for hidden echo
        rpassword::prompt_password(format!("Value for {key} (input hidden): "))
            .map_err(|e| CboxError::ioerr(format!("Failed to read secret from TTY: {e}")))
    } else {
        // Non-TTY (piped/CI): read from stdin, trim one trailing newline
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .map_err(|e| CboxError::ioerr(format!("Failed to read secret from stdin: {e}")))?;
        // Trim a single trailing \n or \r\n
        if line.ends_with('\n') {
            line.pop();
            if line.ends_with('\r') {
                line.pop();
            }
        }
        Ok(line)
    }
}

fn store_secret(
    store: &dyn SecretStore,
    box_name: &str,
    key: &str,
    value: &str,
) -> Result<(), CboxError> {
    use crate::secret::SecretError;
    store.set(box_name, key, value).map_err(|e| match e {
        SecretError::Unavailable(msg) => CboxError::tempfail(format!("Keyring unavailable: {msg}")),
        SecretError::NotFound { .. } => unreachable!("set never returns NotFound"),
        SecretError::Backend(msg) => CboxError::software(format!("Keyring backend error: {msg}")),
    })
}

// ─── list ────────────────────────────────────────────────────────────────────

fn run_list(
    args: &SecretListArgs,
    ctx: &OutputCtx,
    store: &dyn SecretStore,
) -> Result<(), CboxError> {
    if !is_valid_name(&args.box_name) {
        return Err(CboxError::usage(format!(
            "Invalid box name \"{}\". Names must match ^[a-zA-Z0-9][a-zA-Z0-9_.-]*$",
            args.box_name
        )));
    }

    use crate::secret::SecretError;
    let keys = store.list(&args.box_name).map_err(|e| match e {
        SecretError::Unavailable(msg) => CboxError::tempfail(format!("Keyring unavailable: {msg}")),
        SecretError::NotFound { .. } => unreachable!(),
        SecretError::Backend(msg) => CboxError::software(format!("Keyring backend error: {msg}")),
    })?;

    if ctx.json {
        let v = serde_json::json!({
            "ok": true,
            "box": args.box_name,
            "keys": keys,
            // values are NEVER in JSON output
        });
        ctx.print_json(&v);
    } else if keys.is_empty() {
        if !ctx.quiet {
            println!("No secrets stored for \"{}\".", args.box_name);
        }
    } else {
        for k in &keys {
            println!("{k}");
        }
    }

    Ok(())
}

// ─── rm ──────────────────────────────────────────────────────────────────────

fn run_rm(args: &SecretRmArgs, ctx: &OutputCtx, store: &dyn SecretStore) -> Result<(), CboxError> {
    if !is_valid_name(&args.box_name) {
        return Err(CboxError::usage(format!(
            "Invalid box name \"{}\". Names must match ^[a-zA-Z0-9][a-zA-Z0-9_.-]*$",
            args.box_name
        )));
    }
    if !is_valid_env_name(&args.key) {
        return Err(CboxError::dataerr(format!(
            "Invalid secret key \"{}\". Must match ^[A-Za-z_][A-Za-z0-9_]*$",
            args.key
        )));
    }

    use crate::secret::SecretError;
    store
        .delete(&args.box_name, &args.key)
        .map_err(|e| match e {
            SecretError::Unavailable(msg) => {
                CboxError::tempfail(format!("Keyring unavailable: {msg}"))
            }
            SecretError::NotFound { .. } => unreachable!("delete is idempotent, NotFound is Ok"),
            SecretError::Backend(msg) => {
                CboxError::software(format!("Keyring backend error: {msg}"))
            }
        })?;

    if ctx.json {
        let v = serde_json::json!({
            "ok": true,
            "action": "secret-rm",
            "box": args.box_name,
            "key": args.key,
        });
        ctx.print_json(&v);
    } else if !ctx.quiet {
        ctx.success(&format!(
            "Removed secret \"{}\" for \"{}\" (or it was not set).",
            args.key, args.box_name
        ));
    }

    Ok(())
}
