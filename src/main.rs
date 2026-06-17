//! cbox — a cozy distrobox manager.
//! Entry point: parse clap → dispatch → map CboxError to exit code.

mod boxfile;
mod cli;
mod core;
mod dbox;
mod error;
mod tui;

use clap::Parser;
use cli::{output::OutputCtx, Cli, Commands};
use dbox::real::RealRunner;
use error::CboxError;

fn main() {
    let cli = Cli::parse();

    let ctx = OutputCtx::new(cli.json, cli.quiet, cli.verbose, cli.no_color);

    let runner = RealRunner;

    let result = dispatch(&cli, &ctx, &runner);

    match result {
        Ok(()) => {}
        Err(e) => {
            let code = e.exit_code();
            if ctx.json {
                let v = serde_json::json!({
                    "ok": false,
                    "error": e.to_string(),
                    "exit_code": code,
                });
                // Print error JSON to stdout (so --json consumers always get valid JSON)
                println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
            } else {
                eprintln!("error: {e}");
            }
            std::process::exit(code);
        }
    }
}

fn dispatch(cli: &Cli, ctx: &OutputCtx, runner: &RealRunner) -> Result<(), CboxError> {
    let backend_str = cli.backend.as_deref();

    match &cli.command {
        Commands::Create(args) => cli::create::run(args, cli.dry_run, backend_str, ctx, runner),
        Commands::List(args) => cli::list::run(args, backend_str, ctx, runner),
        Commands::Rm(args) => cli::rm::run(args, cli.yes, ctx, runner),
        Commands::Enter(args) => cli::enter::run(args, cli.json, ctx, runner),
        Commands::Inspect(args) => cli::inspect::run(args, backend_str, ctx, runner),
        Commands::Edit(args) => cli::edit::run(args, cli.json, backend_str, ctx, runner),
        Commands::Doctor(args) => cli::doctor::run(args, backend_str, ctx, runner),
        Commands::Apply(args) => {
            cli::apply::run(args, cli.dry_run, backend_str, cli.yes, ctx, runner)
        }
        Commands::Up(args) => cli::up::run(args, cli.dry_run, backend_str, cli.yes, ctx, runner),
    }
}
