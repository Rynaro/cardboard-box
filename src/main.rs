//! cbox — a cozy distrobox manager.
//! Entry point: parse clap → dispatch → map CboxError to exit code.

mod boxfile;
mod cli;
mod core;
mod dbox;
mod error;
mod tui;

use std::sync::Arc;

use clap::Parser;
use cli::{output::OutputCtx, Cli, Commands};
use dbox::real::RealRunner;
use error::CboxError;
use tui::TuiConfig;

fn main() {
    let cli = Cli::parse();

    let ctx = OutputCtx::new(cli.json, cli.quiet, cli.verbose, cli.no_color);

    // Wrap runner in Arc so it can be shared into the TUI worker thread.
    let runner: Arc<dyn dbox::runner::DistroboxRunner> = Arc::new(RealRunner);

    let result = dispatch(&cli, &ctx, runner);

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
                println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
            } else {
                eprintln!("error: {e}");
            }
            std::process::exit(code);
        }
    }
}

fn dispatch(
    cli: &Cli,
    ctx: &OutputCtx,
    runner: Arc<dyn dbox::runner::DistroboxRunner>,
) -> Result<(), CboxError> {
    let backend_str = cli.backend.as_deref();

    // Helper that still works with the &RealRunner reference for P1/P2 commands
    // (they take &dyn DistroboxRunner, not Arc).
    let runner_ref: &dyn dbox::runner::DistroboxRunner = runner.as_ref();

    match &cli.command {
        None => {
            // `cbox` with no args → launch TUI on a TTY; print help on non-TTY.
            launch_tui(cli, ctx, runner, backend_str, false)
        }
        Some(Commands::Tui(_args)) => {
            // `cbox tui` explicit → same TTY guard, but the error message differs.
            launch_tui(cli, ctx, runner, backend_str, true)
        }
        Some(Commands::Create(args)) => {
            cli::create::run(args, cli.dry_run, backend_str, ctx, runner_ref)
        }
        Some(Commands::List(args)) => cli::list::run(args, backend_str, ctx, runner_ref),
        Some(Commands::Rm(args)) => cli::rm::run(args, cli.yes, backend_str, ctx, runner_ref),
        Some(Commands::Enter(args)) => {
            cli::enter::run(args, cli.json, backend_str, ctx, runner_ref)
        }
        Some(Commands::Inspect(args)) => cli::inspect::run(args, backend_str, ctx, runner_ref),
        Some(Commands::Edit(args)) => cli::edit::run(args, cli.json, backend_str, ctx, runner_ref),
        Some(Commands::Doctor(args)) => cli::doctor::run(args, backend_str, ctx, runner_ref),
        Some(Commands::Apply(args)) => {
            cli::apply::run(args, cli.dry_run, backend_str, cli.yes, ctx, runner_ref)
        }
        Some(Commands::Up(args)) => {
            cli::up::run(args, cli.dry_run, backend_str, cli.yes, ctx, runner_ref)
        }
    }
}

fn launch_tui(
    cli: &Cli,
    _ctx: &OutputCtx,
    runner: Arc<dyn dbox::runner::DistroboxRunner>,
    backend_str: Option<&str>,
    explicit_tui_cmd: bool,
) -> Result<(), CboxError> {
    // --json is not meaningful for TUI.
    if cli.json {
        return Err(CboxError::usage("--json is not supported for the TUI"));
    }

    // TTY guard: require an interactive terminal.
    #[cfg(feature = "tui")]
    {
        if !tui::app::stdout_is_tty() || !tui::app::stdin_is_tty() {
            if explicit_tui_cmd {
                return Err(CboxError::usage("cbox tui needs an interactive terminal"));
            } else {
                // `cbox` with no args on a non-TTY → print clap help + exit 64.
                use clap::CommandFactory;
                let mut cmd = Cli::command();
                let mut help_buf = Vec::new();
                cmd.write_long_help(&mut help_buf).ok();
                let help_text = String::from_utf8_lossy(&help_buf);
                eprintln!("{help_text}");
                return Err(CboxError::usage(
                    "Not a TTY — use a subcommand (e.g. cbox list) or run in an interactive terminal",
                ));
            }
        }
    }

    #[cfg(not(feature = "tui"))]
    {
        // Silence unused variable warning when feature is off.
        let _ = explicit_tui_cmd;
    }

    let cfg = TuiConfig {
        backend_override: backend_str.map(|s| s.to_string()),
    };
    tui::run(cfg, runner)
}
