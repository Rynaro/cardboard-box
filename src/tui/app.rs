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
    use std::time::{Duration, Instant};

    use crossterm::{
        event::{
            self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
            MouseEventKind,
        },
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    };
    use ratatui::{backend::CrosstermBackend, Terminal};

    use crate::core;
    use crate::core::spec::{EditSpec, EnterSpec};
    use crate::dbox::backend::Backend;
    use crate::dbox::runner::DistroboxRunner;
    use crate::error::CboxError;
    use crate::tui::cmdlog::{CmdLog, LoggingRunner};
    use crate::tui::effect::{execute_effect, make_store, Effect};
    use crate::tui::history::HistoryStore;
    use crate::tui::logstream::LogCoalescer;
    use crate::tui::message::{Key, Message, Mouse};
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
            execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
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
            // DisableMouseCapture is inside the guard so the panic hook path
            // (which calls restore_terminal_once) also disables mouse capture
            // — no terminal corruption on panic (G-MOUSE-RESTORE).
            let _ = execute!(io::stdout(), DisableMouseCapture, LeaveAlternateScreen);
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

    // ─── Mouse normalisation (crossterm → internal Mouse) ────────────────────

    fn normalize_mouse(event: crossterm::event::MouseEvent) -> Mouse {
        match event.kind {
            MouseEventKind::ScrollUp => Mouse::ScrollUp,
            MouseEventKind::ScrollDown => Mouse::ScrollDown,
            _ => Mouse::Other,
        }
    }

    // ─── LogStream: dedicated thread + child lifecycle ───────────────────────

    /// Shell-local state for the live log streaming thread.
    /// Lives in `run_loop` beside `Worker` — NOT on the pure `Model`.
    struct LogStream {
        /// Set to `true` to request cancellation; the stream thread polls this.
        stop: Arc<AtomicBool>,
        /// Join handle for the dedicated stream thread. ALWAYS joined on teardown
        /// — never detached — to prevent thread/child leaks (G-LOGLEAK).
        join: std::thread::JoinHandle<()>,
        /// Currently streaming target `(container_id, backend_str)`.
        /// Used to detect same-target re-opens (idempotent).
        target: (String, String),
    }

    /// Tear down the log stream: set the stop flag, join the thread.
    ///
    /// Setting the flag BEFORE joining: the stream thread sees `stop=true`,
    /// kills + reaps the child, and exits. This makes `join` bounded.
    /// Called on: pane-close, target-swap, Quit arm, AND post-loop teardown.
    fn stop_log_stream(log_stream: &mut Option<LogStream>) {
        if let Some(ls) = log_stream.take() {
            ls.stop.store(true, Ordering::SeqCst);
            // join() is bounded: the child is killed when the stop flag is set,
            // which closes the pipe and unblocks read_line with EOF.
            let _ = ls.join.join();
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
        // 1. Leave alt-screen. Disable mouse capture so the editor gets a clean terminal.
        let _ = terminal.show_cursor();
        let _ = execute!(io::stdout(), DisableMouseCapture, LeaveAlternateScreen);
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

        // 5. Restore terminal (re-enable mouse capture on return).
        let _ = enable_raw_mode();
        let _ = execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture);
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
        // 1. Leave alt-screen. Disable mouse so the shell gets a clean terminal.
        let _ = terminal.show_cursor();
        let _ = execute!(io::stdout(), DisableMouseCapture, LeaveAlternateScreen);
        let _ = disable_raw_mode();
        let _ = io::stdout().flush();

        // 2. Run enter interactively.
        let code = core::enter(spec, runner)?;

        // 3. Restore terminal (re-enable mouse capture on return).
        let _ = enable_raw_mode();
        let _ = execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture);
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

        // Shell-local log stream handle (beside Worker — NOT on Model).
        let mut log_stream: Option<LogStream> = None;

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
                        Event::Mouse(me) => {
                            let m = normalize_mouse(me);
                            // Ignore Mouse::Other to avoid pointless Tick cycles.
                            if m != Mouse::Other {
                                Some(Message::Mouse(m))
                            } else {
                                None
                            }
                        }
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
                        // Stop any active log stream on quit (G-LOGLEAK).
                        stop_log_stream(&mut log_stream);
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
                    // ── Bundle 3: shell-handled streaming effects ─────────────
                    Effect::StreamLogs { id, backend } => {
                        let new_target = (id.clone(), backend.clone());
                        // Idempotent: same target already streaming → skip.
                        if let Some(ref ls) = log_stream {
                            if ls.target == new_target {
                                continue;
                            }
                        }
                        // Tear down old stream first (target swap).
                        stop_log_stream(&mut log_stream);

                        // Spawn dedicated thread for the log stream.
                        let stop_flag = Arc::new(AtomicBool::new(false));
                        let stop_for_thread = Arc::clone(&stop_flag);
                        let runner_for_stream = Arc::clone(&runner);
                        let tx_for_stream = result_tx.clone();
                        let id_for_thread = id.clone();
                        let backend_for_thread = backend.clone();

                        let join = std::thread::spawn(move || {
                            let backend_obj =
                                crate::dbox::backend::Backend::from_name(&backend_for_thread)
                                    .unwrap_or(crate::dbox::backend::Backend::Podman);

                            let mut coalescer = LogCoalescer::new();
                            let mut last_flush = Instant::now();

                            let mut on_line = |line: String| {
                                let size_due = coalescer.push(line);
                                let time_elapsed = last_flush.elapsed().as_millis() as u64;
                                let time_due = coalescer.due_at(time_elapsed);

                                if size_due || time_due {
                                    let chunk = coalescer.drain();
                                    last_flush = Instant::now();
                                    if !chunk.is_empty() {
                                        let _ = tx_for_stream.try_send(Message::LogChunk(chunk));
                                    }
                                }
                            };

                            let code = crate::core::logs(
                                &id_for_thread,
                                &backend_obj,
                                runner_for_stream.as_ref(),
                                &mut on_line,
                                &stop_for_thread,
                            );

                            // Final drain: flush any remaining buffered lines.
                            let remaining = coalescer.drain();
                            if !remaining.is_empty() {
                                let _ = tx_for_stream.try_send(Message::LogChunk(remaining));
                            }

                            // Signal stream end.
                            let exit_code = code.ok();
                            let _ = tx_for_stream.try_send(Message::LogStreamEnded(exit_code));
                        });

                        log_stream = Some(LogStream {
                            stop: stop_flag,
                            join,
                            target: new_target,
                        });
                    }
                    Effect::StopLogs => {
                        stop_log_stream(&mut log_stream);
                    }
                    other => {
                        route_to_worker(&worker, other);
                    }
                }
            }
        }

        // Post-loop unconditional teardown: stop any lingering log stream.
        // This covers abrupt quit paths that bypass the Effect::Quit arm (G-LOGLEAK).
        stop_log_stream(&mut log_stream);

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

        // Build the shared command-log ring buffer (cap 200).
        // The same Arc is given to the LoggingRunner decorator (writer) and the Model (reader).
        let cmdlog = Arc::new(std::sync::Mutex::new(CmdLog::new(200)));

        // Build the shared history store (cross-session persist, redacted).
        // The same Arc is given to LoggingRunner (writer); the overlay reads via HistoryStore::load().
        let history = Arc::new(std::sync::Mutex::new(HistoryStore::new()));

        // Wrap the injected runner in the LoggingRunner decorator so every spawn
        // is captured at the single chokepoint (DistroboxRunner::run / run_interactive).
        let logging_runner: Arc<dyn DistroboxRunner> = Arc::new(LoggingRunner::new(
            runner,
            Arc::clone(&cmdlog),
            Arc::clone(&history),
        ));

        // backends is non-empty (Backend::usable guarantees it); [0] is the
        // preferred engine used as the default for creating new boxes.
        let mut model = Model::new(backends[0].clone());
        model.backends = backends;
        model.color_mode = color_mode;
        model.cmdlog = cmdlog;

        run_loop(model, logging_runner, &mut terminal)
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
