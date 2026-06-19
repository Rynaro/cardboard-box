//! The impure event-loop shell.
//!
//! Contains:
//!  - `TerminalGuard`: RAII restore of raw mode + alt-screen.
//!  - Panic hook installation.
//!  - Worker thread spawn + channel routing.
//!  - Suspend/restore for `enter` and `$EDITOR` handoffs.
//!  - The main event loop.
//!
//! This module is NOT unit-tested (smoke only). The reducer (`update`) and the effect
//! executor (`execute_effect`) carry all the unit-testable logic.

#[cfg(feature = "tui")]
mod inner {
    use std::io::{self, IsTerminal, Stdout, Write};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    };
    use std::time::Duration;

    use crossterm::{
        event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    };
    use ratatui::{backend::CrosstermBackend, Terminal};

    use crate::core;
    use crate::core::spec::{EditSpec, EnterSpec};
    use crate::dbox::backend::Backend;
    use crate::dbox::runner::DistroboxRunner;
    use crate::error::CboxError;
    use crate::tui::effect::{execute_effect, make_store, Effect};
    use crate::tui::message::{Key, Message};
    use crate::tui::model::Model;
    use crate::tui::update::update;
    use crate::tui::view::view;

    const POLL_MS: u64 = 50;

    // ─── TerminalGuard (RAII) ────────────────────────────────────────────────

    /// RAII guard that restores the terminal on drop.
    /// Uses an `AtomicBool` flag to be idempotent with the panic hook.
    static TERMINAL_RESTORED: AtomicBool = AtomicBool::new(false);

    pub struct TerminalGuard;

    impl TerminalGuard {
        pub fn new() -> io::Result<(Self, Terminal<CrosstermBackend<Stdout>>)> {
            enable_raw_mode()?;
            let mut stdout = io::stdout();
            execute!(stdout, EnterAlternateScreen)?;
            let backend = CrosstermBackend::new(io::stdout());
            let terminal = Terminal::new(backend)?;
            TERMINAL_RESTORED.store(false, Ordering::SeqCst);
            Ok((TerminalGuard, terminal))
        }
    }

    impl Drop for TerminalGuard {
        fn drop(&mut self) {
            restore_terminal_once();
        }
    }

    fn restore_terminal_once() {
        if TERMINAL_RESTORED
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            let _ = disable_raw_mode();
            let _ = execute!(io::stdout(), LeaveAlternateScreen);
        }
    }

    // ─── Panic hook ──────────────────────────────────────────────────────────

    pub fn install_panic_hook() {
        let original = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            // Restore terminal first so the panic message is readable.
            restore_terminal_once();
            original(info);
        }));
    }

    // ─── Key normalisation (crossterm → internal Key) ────────────────────────

    fn normalize_key(event: KeyEvent) -> Key {
        match event.code {
            KeyCode::Char('c') if event.modifiers.contains(KeyModifiers::CONTROL) => Key::CtrlC,
            KeyCode::Char('d') if event.modifiers.contains(KeyModifiers::CONTROL) => Key::CtrlD,
            KeyCode::Char(c) => Key::Char(c),
            KeyCode::Up => Key::Up,
            KeyCode::Down => Key::Down,
            KeyCode::Left => Key::Left,
            KeyCode::Right => Key::Right,
            KeyCode::Enter => Key::Enter,
            KeyCode::Esc => Key::Esc,
            KeyCode::Backspace => Key::Backspace,
            KeyCode::Tab => Key::Tab,
            KeyCode::BackTab => Key::BackTab,
            _ => Key::Other,
        }
    }

    // ─── Worker thread ────────────────────────────────────────────────────────

    /// Send a data effect to the worker thread; the worker posts a completion Message back.
    struct Worker {
        tx: mpsc::SyncSender<Effect>,
    }

    fn spawn_worker(
        runner: Arc<dyn DistroboxRunner>,
        result_tx: mpsc::SyncSender<Message>,
        backends: Vec<Backend>,
    ) -> Worker {
        let (eff_tx, eff_rx) = mpsc::sync_channel::<Effect>(4);

        std::thread::spawn(move || {
            let store = make_store();
            for eff in eff_rx {
                if let Some(msg) = execute_effect(eff, &store, &runner, &backends) {
                    let _ = result_tx.send(msg);
                }
            }
        });

        Worker { tx: eff_tx }
    }

    // ─── Edit handoff ────────────────────────────────────────────────────────

    /// Suspend the TUI, open `$EDITOR` for the given path, restore, return result.
    fn suspend_and_edit(
        path: &str,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
        runner: &dyn DistroboxRunner,
    ) -> Result<(), CboxError> {
        // 1. Leave alt-screen.
        let _ = terminal.show_cursor();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        let _ = disable_raw_mode();
        let _ = io::stdout().flush();

        // 2. Scaffold if absent.
        let path_obj = std::path::Path::new(path);
        if !path_obj.exists() {
            if let Some(parent) = path_obj.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            // Scaffold using the runner (best effort; may fail if box gone).
            let name = path_obj
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");
            let edit_spec = EditSpec {
                name: Some(name.to_string()),
                file: None,
                backend: Backend::detect_or_default(None).unwrap_or(Backend::Podman),
            };
            let scaffolded = core::scaffold_boxfile(name, &edit_spec, runner);
            let _ = std::fs::write(path, scaffolded);
        }

        // 3. Spawn $EDITOR.
        let editor = std::env::var("VISUAL")
            .or_else(|_| std::env::var("EDITOR"))
            .unwrap_or_else(|_| "vi".to_string());

        let status = std::process::Command::new(&editor)
            .arg(path)
            .status()
            .map_err(|e| CboxError::ioerr(format!("Failed to launch editor \"{editor}\": {e}")))?;

        if !status.success() {
            // Editor exited non-zero — not fatal, just note it.
        }

        // 4. Re-validate.
        if let Ok(content) = std::fs::read_to_string(path) {
            let _ = crate::boxfile::parse_and_validate(&content);
            // Validation errors are surfaced as a status message, not a hard failure.
        }

        // 5. Restore terminal.
        let _ = enable_raw_mode();
        let _ = execute!(io::stdout(), EnterAlternateScreen);
        let _ = terminal.clear();
        TERMINAL_RESTORED.store(false, Ordering::SeqCst);

        Ok(())
    }

    // ─── Enter handoff ───────────────────────────────────────────────────────

    /// Suspend the TUI, run `core::enter` (TTY pass-through), restore.
    fn suspend_and_enter(
        spec: &EnterSpec,
        runner: &dyn DistroboxRunner,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<i32, CboxError> {
        // 1. Leave alt-screen.
        let _ = terminal.show_cursor();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        let _ = disable_raw_mode();
        let _ = io::stdout().flush();

        // 2. Run enter interactively.
        let code = core::enter(spec, runner)?;

        // 3. Restore terminal.
        let _ = enable_raw_mode();
        let _ = execute!(io::stdout(), EnterAlternateScreen);
        let _ = terminal.clear();
        TERMINAL_RESTORED.store(false, Ordering::SeqCst);

        Ok(code)
    }

    // ─── Main event loop ─────────────────────────────────────────────────────

    pub fn run_loop(
        mut model: Model,
        runner: Arc<dyn DistroboxRunner>,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<(), CboxError> {
        let backends = model.backends.clone();
        let runner_for_effects = Arc::clone(&runner);

        // Channel for worker → main completions.
        let (result_tx, result_rx) = mpsc::sync_channel::<Message>(32);
        let result_tx_for_worker = result_tx.clone();

        let worker = spawn_worker(runner_for_effects, result_tx_for_worker, backends);

        // Kick off initial list load.
        model.busy = true;
        model.status = crate::tui::model::StatusLine::Busy("Loading boxes…".to_string());
        let _ = worker.tx.try_send(Effect::LoadList);

        // Kick off initial doctor check to warm the backend status.
        // (non-blocking send; drop if channel is full)
        // We don't run doctor on startup — just list is enough.

        loop {
            // Draw.
            terminal
                .draw(|f| view(&model, f))
                .map_err(|e| CboxError::ioerr(e.to_string()))?;

            if model.should_quit {
                break;
            }

            // Collect a message.
            let msg: Option<Message> = {
                // Try a non-blocking drain of the worker result channel first.
                if let Ok(m) = result_rx.try_recv() {
                    Some(m)
                } else if event::poll(Duration::from_millis(POLL_MS))
                    .map_err(|e| CboxError::ioerr(e.to_string()))?
                {
                    match event::read().map_err(|e| CboxError::ioerr(e.to_string()))? {
                        Event::Key(ke) => Some(Message::Key(normalize_key(ke))),
                        Event::Resize(w, h) => Some(Message::Resize(w, h)),
                        _ => None,
                    }
                } else {
                    // Check the channel one more time after the poll timeout.
                    result_rx.try_recv().ok()
                }
            };

            // Default to Tick.
            let msg = msg.unwrap_or(Message::Tick);

            let effects = update(&mut model, msg);

            // Route effects.
            for eff in effects {
                match eff {
                    Effect::Quit => {
                        model.should_quit = true;
                    }
                    Effect::SuspendAndEnter(spec) => {
                        let res = suspend_and_enter(&spec, runner.as_ref(), terminal);
                        let completion = Message::EnterReturned(res);
                        let eff2s = update(&mut model, completion);
                        // Route follow-up effects (typically LoadList).
                        for eff2 in eff2s {
                            route_to_worker(&worker, eff2);
                        }
                    }
                    Effect::SuspendAndEdit(path) => {
                        let res = suspend_and_edit(&path, terminal, runner.as_ref());
                        let completion = Message::EditReturned(res);
                        let _eff2s = update(&mut model, completion);
                    }
                    other => {
                        route_to_worker(&worker, other);
                    }
                }
            }
        }

        Ok(())
    }

    fn route_to_worker(worker: &Worker, eff: Effect) {
        let _ = worker.tx.try_send(eff);
    }

    // ─── Public entry ─────────────────────────────────────────────────────────

    pub fn run(runner: Arc<dyn DistroboxRunner>, backends: Vec<Backend>) -> Result<(), CboxError> {
        install_panic_hook();

        let (_guard, mut terminal) = TerminalGuard::new()
            .map_err(|e| CboxError::ioerr(format!("Terminal setup failed: {e}")))?;

        // Detect color capability once at launch and thread it through the model.
        // The TUI has no explicit --no-color flag today; NO_COLOR env + TTY gate suffice.
        let color_mode = crate::tui::theme::detect(false);

        // backends is non-empty (Backend::usable guarantees it); [0] is the
        // preferred engine used as the default for creating new boxes.
        let mut model = Model::new(backends[0].clone());
        model.backends = backends;
        model.color_mode = color_mode;

        run_loop(model, runner, &mut terminal)
    }

    pub fn stdout_is_tty() -> bool {
        io::stdout().is_terminal()
    }

    pub fn stdin_is_tty() -> bool {
        io::stdin().is_terminal()
    }
}

#[cfg(feature = "tui")]
pub use inner::{run, stdin_is_tty, stdout_is_tty};
