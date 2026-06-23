//! The pure reducer: `update(&mut Model, Message) -> Vec<Effect>`.
//!
//! Rules:
//!  - Pure over (Model, Message): no I/O, no thread spawn, no runner, no clock.
//!  - `busy == true` ignores conflicting Key messages but always honors Tick, Resize,
//!    and Ctrl-C → Quit.
//!  - Effect completions clear `busy`, update the relevant Model field, set status.
//!  - Key handling is screen-dispatched: match `model.screen` first, then the key.
//!  - Overlay pre-check runs BEFORE screen dispatch (§4.3 of the spec).

use crate::core::spec::{
    ApplySpec, DoctorSpec, EnterSpec, InspectSpec, RmSpec, StatsSpec, StopSpec,
};
use crate::dbox::backend::Backend;
use crate::error::CboxError;
use crate::tui::action::{Action, BULK_ACTIONS};
use crate::tui::bulk::{bulk_targets, is_running, BulkOp};
use crate::tui::effect::Effect;
use crate::tui::logstream::SCROLL_STEP;
use crate::tui::message::Mouse;
use crate::tui::message::{Key, Message};
use crate::tui::model::{
    BulkConfirmState, ConfirmState, FilterState, Model, Overlay, ProgressState, Screen,
    StatsHistory, StatusLine, WizardState, WizardStep,
};
use crate::tui::poll::{should_poll, PollGate, PollKind};
use crate::tui::strings;

#[cfg(feature = "tui")]
use crate::tui::filter::fuzzy_rank;

/// Resolve a box's engine from its stored backend string, falling back to the
/// create default when it's missing/unknown (e.g. mock rows in tests).
fn backend_of(row_backend: &str, fallback: &Backend) -> Backend {
    Backend::from_name(row_backend).unwrap_or_else(|| fallback.clone())
}

/// The pure reducer. Returns a list of effects for the shell to execute.
pub fn update(model: &mut Model, msg: Message) -> Vec<Effect> {
    match msg {
        // ── Tick: advance spinner, expire toasts, maybe fire silent poll ─────
        Message::Tick => {
            model.spinner_tick = model.spinner_tick.wrapping_add(1);
            if let Some(ref mut p) = model.progress {
                p.spinner_tick = model.spinner_tick;
            }
            // Expire toasts that have outlived their TTL.
            let now = model.spinner_tick;
            model
                .toasts
                .retain(|t| now.wrapping_sub(t.born_tick) < t.ttl_ticks);

            // Silent-poll decision (GAP-1 gate).
            let gate = build_poll_gate(model);
            if let Some(kind) = should_poll(&gate) {
                model.last_poll_tick = model.spinner_tick;
                model.poll_in_flight = true;
                // NEVER sets model.busy — that would block all keys.
                let eff = poll_kind_to_effect(kind);
                return vec![eff];
            }

            vec![]
        }

        // ── Resize: no-op (ratatui handles layout) ────────────────────────────
        Message::Resize(_, _) => vec![],

        // ── Key events ────────────────────────────────────────────────────────
        Message::Key(key) => handle_key(model, key),

        // ── Mouse scroll ──────────────────────────────────────────────────────
        Message::Mouse(mouse) => handle_mouse(model, mouse),

        // ── Effect completions ────────────────────────────────────────────────
        Message::ListLoaded(result) => handle_list_loaded(model, result),
        Message::DetailLoaded(result) => handle_detail_loaded(model, result),
        Message::CreateDone(result) => handle_create_done(model, result),
        Message::RmDone(result) => handle_rm_done(model, result),
        Message::StopDone(result) => handle_stop_done(model, result),
        Message::ApplyDone(result) => handle_apply_done(model, result),
        Message::UpDone(result) => handle_up_done(model, result),
        Message::DoctorDone(result) => handle_doctor_done(model, result),
        Message::EnterReturned(result) => handle_enter_returned(model, result),
        Message::EditReturned(result) => handle_edit_returned(model, result),
        // ── Bundle 2: silent poll completions ─────────────────────────────────
        Message::SilentListLoaded(result) => handle_silent_list_loaded(model, result),
        Message::StatsLoaded(result) => handle_stats_loaded(model, result),

        // ── Bundle 3: streaming log completions ────────────────────────────────
        Message::LogChunk(chunk) => handle_log_chunk(model, chunk),
        Message::LogStreamEnded(code) => handle_log_stream_ended(model, code),
    }
}

// ─── Key dispatch ────────────────────────────────────────────────────────────

fn handle_key(model: &mut Model, key: Key) -> Vec<Effect> {
    // 1. Ctrl-C / Ctrl-D always quits, even when busy.
    if key == Key::CtrlC || key == Key::CtrlD {
        model.should_quit = true;
        return vec![Effect::Quit];
    }

    // 2. When busy, drop all other keys (spinner is shown; the worker is running).
    if model.busy {
        return vec![];
    }

    // 3a. If the bulk-confirm modal is open, intercept ALL keys for it.
    if model.bulk_confirm.is_some() {
        return handle_key_bulk_confirm(model, key);
    }

    // 3b. If the filter overlay is open, intercept ALL keys for filter input.
    if model.filter.is_some() {
        return handle_key_filter(model, key);
    }

    // 4. Overlay pre-check — handles overlays before screen dispatch so `esc`
    //    closes the overlay rather than quitting.
    match &model.overlay.clone() {
        Overlay::None => {}
        Overlay::Cheatsheet => {
            // Any key or Esc dismisses the cheatsheet.
            model.overlay = Overlay::None;
            return vec![];
        }
        Overlay::CommandLog { scroll } => {
            let scroll = *scroll;
            return handle_key_cmdlog(model, key, scroll);
        }
        Overlay::Palette {
            cursor,
            matches,
            bulk_only,
            ..
        } => {
            let cursor = *cursor;
            let matches = matches.clone();
            let bulk_only = *bulk_only;
            return handle_key_palette(model, key, cursor, matches, bulk_only);
        }
        Overlay::History {
            cursor, matches, ..
        } => {
            let cursor = *cursor;
            let matches = matches.clone();
            return handle_key_history(model, key, cursor, matches);
        }
    }

    // 5. Global keys (available on every non-busy screen): skin cycle, cheatsheet, palette.
    match key {
        Key::Char('t') => {
            let next = model.skin.next();
            model.skin = next;
            let name = format!("Skin: {}", next.name());
            model.push_toast(crate::tui::model::ToastKind::Info, name);
            return vec![];
        }
        Key::Char('?') => {
            model.overlay = Overlay::Cheatsheet;
            return vec![];
        }
        Key::Char(':') => {
            return dispatch_action(model, Action::Palette);
        }
        _ => {}
    }

    // 6. Screen dispatch (existing behavior + new List arms).
    match model.screen {
        Screen::List => handle_key_list(model, key),
        Screen::Detail => handle_key_detail(model, key),
        Screen::Wizard => handle_key_wizard(model, key),
        Screen::ConfirmDestroy => handle_key_confirm(model, key),
        Screen::Progress => handle_key_progress(model, key),
        Screen::DoctorPanel => handle_key_doctor(model, key),
        Screen::Logs => handle_key_logs(model, key),
    }
}

// ─── Filter overlay key handler ───────────────────────────────────────────────

fn handle_key_filter(model: &mut Model, key: Key) -> Vec<Effect> {
    match key {
        Key::Esc => {
            // Clear filter; restore selection.
            let prev_selected = model.filter.as_ref().and_then(|f| {
                if f.matches.is_empty() {
                    None
                } else {
                    Some(f.matches[f.cursor])
                }
            });
            model.filter = None;
            // Restore a sensible selection (previous box if still present, else 0).
            if let Some(idx) = prev_selected {
                if idx < model.boxes.len() {
                    model.selected = Some(idx);
                } else if !model.boxes.is_empty() {
                    model.selected = Some(0);
                } else {
                    model.selected = None;
                }
            }
            vec![]
        }
        Key::Enter => {
            // Commit the current selection and close the filter.
            // model.selected is already synced to the correct box index.
            model.filter = None;
            vec![]
        }
        Key::Up => {
            model.move_up();
            vec![]
        }
        Key::Down => {
            model.move_down();
            vec![]
        }
        Key::Backspace => {
            if let Some(ref mut f) = model.filter {
                f.query.pop();
                let query = f.query.clone();
                recompute_filter(model, &query);
            }
            vec![]
        }
        Key::Char(c) => {
            // In FilterInput: ALL chars (including j/k) are typed into the query.
            // Navigation within matches is ↑/↓ ONLY (§4.4 of the spec, R-8).
            if let Some(ref mut f) = model.filter {
                f.query.push(c);
                let query = f.query.clone();
                recompute_filter(model, &query);
            }
            vec![]
        }
        _ => vec![],
    }
}

/// Recompute `filter.matches` from the current query and clamp `filter.cursor`.
/// Also updates `model.selected` to reflect the new cursor position.
fn recompute_filter(model: &mut Model, query: &str) {
    let names: Vec<&str> = model.boxes.iter().map(|b| b.name.as_str()).collect();

    // Use the tui-gated fuzzy_rank when the feature is on.
    #[cfg(feature = "tui")]
    let matches = fuzzy_rank(query, &names);

    // Fallback for non-tui builds (substring match, keeps lean build clean).
    #[cfg(not(feature = "tui"))]
    let matches: Vec<usize> = if query.is_empty() {
        (0..names.len()).collect()
    } else {
        names
            .iter()
            .enumerate()
            .filter(|(_, n)| n.to_lowercase().contains(&query.to_lowercase()))
            .map(|(i, _)| i)
            .collect()
    };

    if let Some(ref mut f) = model.filter {
        f.matches = matches;
        // Clamp cursor to valid range.
        if f.matches.is_empty() {
            f.cursor = 0;
            model.selected = None;
        } else {
            f.cursor = f.cursor.min(f.matches.len() - 1);
            model.selected = Some(f.matches[f.cursor]);
        }
    }
}

// ─── Command-log overlay key handler ──────────────────────────────────────────

fn handle_key_cmdlog(model: &mut Model, key: Key, scroll: usize) -> Vec<Effect> {
    match key {
        Key::Esc | Key::Char('q') | Key::Char('l') => {
            model.overlay = Overlay::None;
            vec![]
        }
        Key::Up | Key::Char('k') => {
            let new_scroll = scroll.saturating_sub(1);
            model.overlay = Overlay::CommandLog { scroll: new_scroll };
            vec![]
        }
        Key::Down | Key::Char('j') => {
            // Get the log entry count under the lock.
            let entry_count = model.cmdlog.lock().map(|log| log.len()).unwrap_or(0);
            let max_scroll = entry_count.saturating_sub(1);
            let new_scroll = (scroll + 1).min(max_scroll);
            model.overlay = Overlay::CommandLog { scroll: new_scroll };
            vec![]
        }
        _ => vec![],
    }
}

// ─── List screen ─────────────────────────────────────────────────────────────

fn handle_key_list(model: &mut Model, key: Key) -> Vec<Effect> {
    match key {
        Key::Up | Key::Char('k') => {
            model.move_up();
            vec![]
        }
        Key::Down | Key::Char('j') => {
            model.move_down();
            vec![]
        }
        Key::Enter => open_selected(model),
        Key::Char('i') => inspect_selected(model),
        Key::Char('c') => {
            model.screen = Screen::Wizard;
            model.wizard = Some(WizardState::new());
            vec![]
        }
        Key::Char('d') => confirm_destroy_selected(model),
        Key::Char('s') => stop_selected(model),
        Key::Char('a') => apply_selected(model),
        Key::Char('u') => {
            // Up is not fully wired in v3.0 list screen (needs boxfile path).
            // Treat as apply for now.
            apply_selected(model)
        }
        Key::Char('e') => edit_selected(model),
        Key::Char('r') => start_manual_refresh(model),
        // Doctor moved from `?` to uppercase `D` (AC-REBIND-1).
        Key::Char('D') => start_doctor(model),
        // `/` opens the fuzzy filter overlay.
        Key::Char('/') => {
            open_filter(model);
            vec![]
        }
        // `l` opens the command-log overlay.
        Key::Char('l') => {
            model.overlay = Overlay::CommandLog { scroll: 0 };
            vec![]
        }
        // `L` opens the live log streaming screen for the selected box.
        Key::Char('L') => open_logs_screen(model),
        // `H` opens the cross-session action history overlay.
        Key::Char('H') => open_history_overlay(model),
        // `b` opens the palette scoped to bulk actions only (fast-path for bulk ops).
        Key::Char('b') => {
            open_palette(model, true);
            vec![]
        }
        Key::Char('q') | Key::Esc => {
            model.should_quit = true;
            vec![Effect::Quit]
        }
        _ => vec![],
    }
}

// ─── Detail screen ───────────────────────────────────────────────────────────

fn handle_key_detail(model: &mut Model, key: Key) -> Vec<Effect> {
    match key {
        Key::Esc | Key::Char('q') => {
            // Esc from Detail goes back to List (never quits the app).
            model.screen = Screen::List;
            model.detail = None;
            // Reset stats history when leaving Detail (AC-HIST-2).
            model.stats_history = None;
            vec![]
        }
        Key::Char('e') => {
            if let Some(ref detail) = model.detail.clone() {
                start_edit(model, &detail.name.clone())
            } else {
                vec![]
            }
        }
        Key::Char('a') => {
            if let Some(ref detail) = model.detail.clone() {
                let name = detail.name.clone();
                let backend = backend_of(&detail.backend, &model.backend);
                start_apply(model, &name, false, backend)
            } else {
                vec![]
            }
        }
        Key::Enter => {
            if let Some(ref detail) = model.detail.clone() {
                if is_running(&detail.status) {
                    let spec = EnterSpec {
                        name: detail.name.clone(),
                        root: false,
                        clean_path: false,
                        cmd: vec![],
                        home_landing: true,
                        backend: backend_of(&detail.backend, &model.backend),
                    };
                    vec![Effect::SuspendAndEnter(spec)]
                } else {
                    vec![]
                }
            } else {
                vec![]
            }
        }
        _ => vec![],
    }
}

// ─── Wizard screen ───────────────────────────────────────────────────────────

fn handle_key_wizard(model: &mut Model, key: Key) -> Vec<Effect> {
    if model.wizard.is_none() {
        return vec![];
    }

    match key {
        Key::Esc => {
            // Cancel wizard.
            model.screen = Screen::List;
            model.wizard = None;
            return vec![];
        }
        Key::Tab | Key::Enter => {
            // Advance step (with validation).
            return wizard_advance(model);
        }
        Key::BackTab => {
            return wizard_back(model);
        }
        Key::Left => {
            if let Some(ref mut w) = model.wizard {
                if w.step == WizardStep::DockerMode && w.docker_mode_idx > 0 {
                    w.docker_mode_idx -= 1;
                }
            }
        }
        Key::Right => {
            if let Some(ref mut w) = model.wizard {
                if w.step == WizardStep::DockerMode && w.docker_mode_idx < 2 {
                    w.docker_mode_idx += 1;
                }
            }
        }
        Key::Backspace => {
            if let Some(ref mut w) = model.wizard {
                match w.step {
                    WizardStep::Name => {
                        w.name.pop();
                        w.dirty = true;
                    }
                    WizardStep::Image => {
                        w.image.pop();
                        w.dirty = true;
                    }
                    WizardStep::Packages => {
                        w.packages_raw.pop();
                        w.dirty = true;
                    }
                    _ => {}
                }
            }
        }
        Key::Char(c) => {
            if let Some(ref mut w) = model.wizard {
                match w.step {
                    WizardStep::Name => {
                        w.name.push(c);
                        w.dirty = true;
                    }
                    WizardStep::Image => {
                        w.image.push(c);
                        w.dirty = true;
                    }
                    WizardStep::Packages => {
                        w.packages_raw.push(c);
                        w.dirty = true;
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
    vec![]
}

fn wizard_advance(model: &mut Model) -> Vec<Effect> {
    let wizard = match model.wizard.as_mut() {
        Some(w) => w,
        None => return vec![],
    };

    match wizard.step.clone() {
        WizardStep::Name => {
            // Validate name.
            if !validate_box_name(&wizard.name) {
                model.status = StatusLine::Error(
                    "Invalid name: must start with a letter or digit and contain only [a-zA-Z0-9_.-]"
                        .to_string(),
                );
                return vec![];
            }
            if let Some(ref mut w) = model.wizard {
                w.step = WizardStep::Image;
            }
        }
        WizardStep::Image => {
            if let Some(ref mut w) = model.wizard {
                w.step = WizardStep::Packages;
            }
        }
        WizardStep::Packages => {
            if let Some(ref mut w) = model.wizard {
                w.step = WizardStep::DockerMode;
            }
        }
        WizardStep::DockerMode => {
            if let Some(ref mut w) = model.wizard {
                w.step = WizardStep::Confirm;
            }
        }
        WizardStep::Confirm => {
            // Submit the wizard.
            return wizard_submit(model);
        }
    }
    vec![]
}

fn wizard_back(model: &mut Model) -> Vec<Effect> {
    let wizard = match model.wizard.as_mut() {
        Some(w) => w,
        None => return vec![],
    };

    match wizard.step.clone() {
        WizardStep::Name => {
            // Already at first step; cancel.
            model.screen = Screen::List;
            model.wizard = None;
        }
        WizardStep::Image => {
            if let Some(ref mut w) = model.wizard {
                w.step = WizardStep::Name;
            }
        }
        WizardStep::Packages => {
            if let Some(ref mut w) = model.wizard {
                w.step = WizardStep::Image;
            }
        }
        WizardStep::DockerMode => {
            if let Some(ref mut w) = model.wizard {
                w.step = WizardStep::Packages;
            }
        }
        WizardStep::Confirm => {
            if let Some(ref mut w) = model.wizard {
                w.step = WizardStep::DockerMode;
            }
        }
    }
    vec![]
}

fn wizard_submit(model: &mut Model) -> Vec<Effect> {
    let wizard = match model.wizard.take() {
        Some(w) => w,
        None => return vec![],
    };

    let packages: Vec<String> = wizard
        .packages_raw
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();

    let docker_mode = match wizard.docker_mode_idx {
        1 => crate::core::spec::DockerMode::Host,
        2 => crate::core::spec::DockerMode::Nested,
        _ => crate::core::spec::DockerMode::None,
    };

    let mut spec = crate::core::spec::CreateSpec::new(wizard.name.clone(), model.backend.clone());
    spec.image = wizard.image.clone();
    spec.packages = packages;
    spec.docker_mode = docker_mode;
    spec.dry_run = false;

    model.screen = Screen::Progress;
    model.busy = true;
    model.status = StatusLine::Busy(format!("Creating \"{}\"…", wizard.name));
    model.progress = Some(ProgressState {
        title: format!("Creating \"{}\"", wizard.name),
        steps: vec![],
        spinner_tick: 0,
        recreate_needed: false,
        recreate_msg: None,
        recreate_confirm: false,
        pending_spec: None,
    });

    vec![Effect::Create(spec)]
}

fn validate_box_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut chars = name.chars();
    // Must start with alphanumeric.
    if !chars.next().map(|c| c.is_alphanumeric()).unwrap_or(false) {
        return false;
    }
    // Rest: alphanumeric, underscore, hyphen, dot.
    chars.all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
}

// ─── ConfirmDestroy screen ───────────────────────────────────────────────────

fn handle_key_confirm(model: &mut Model, key: Key) -> Vec<Effect> {
    match key {
        Key::Char('y') | Key::Enter => {
            if let Some(ref confirm) = model.confirm.clone() {
                let spec = RmSpec {
                    names: vec![confirm.name.clone()],
                    force: true,
                    rm_home: confirm.rm_home,
                    all: false,
                    yes: true,
                    backend: confirm.backend.clone(),
                };
                model.screen = Screen::Progress;
                model.busy = true;
                model.status = StatusLine::Busy(format!("Destroying \"{}\"…", confirm.name));
                model.progress = Some(ProgressState {
                    title: format!("Destroying \"{}\"", confirm.name),
                    steps: vec![],
                    spinner_tick: 0,
                    recreate_needed: false,
                    recreate_msg: None,
                    recreate_confirm: false,
                    pending_spec: None,
                });
                model.confirm = None;
                vec![Effect::Rm(spec)]
            } else {
                vec![]
            }
        }
        Key::Char('n') | Key::Esc => {
            model.screen = Screen::List;
            model.confirm = None;
            vec![]
        }
        Key::Char('h') => {
            if let Some(ref mut confirm) = model.confirm {
                confirm.rm_home = !confirm.rm_home;
            }
            vec![]
        }
        _ => vec![],
    }
}

// ─── Progress screen ──────────────────────────────────────────────────────────

fn handle_key_progress(model: &mut Model, key: Key) -> Vec<Effect> {
    // If recreate confirm modal is showing.
    if let Some(ref progress) = model.progress {
        if progress.recreate_confirm {
            return handle_key_recreate_confirm(model, key);
        }
    }

    match key {
        Key::Enter | Key::Esc | Key::Char('q') => {
            // Only allow backing out when not busy.
            if !model.busy {
                model.screen = Screen::List;
                model.progress = None;
                model.busy = true;
                model.status = StatusLine::Busy("Refreshing…".to_string());
                return vec![Effect::LoadList];
            }
            vec![]
        }
        _ => vec![],
    }
}

fn handle_key_recreate_confirm(model: &mut Model, key: Key) -> Vec<Effect> {
    match key {
        Key::Char('y') | Key::Enter => {
            // Re-issue apply with recreate:true.
            let pending_spec = model.progress.as_ref().and_then(|p| p.pending_spec.clone());
            if let Some(mut spec) = pending_spec {
                spec.recreate = true;
                if let Some(ref mut p) = model.progress {
                    p.recreate_confirm = false;
                    p.recreate_needed = false;
                    p.recreate_msg = None;
                    p.pending_spec = Some(spec.clone());
                }
                model.busy = true;
                model.status = StatusLine::Busy(format!("Recreating \"{}\"…", spec.name));
                vec![Effect::Apply(spec)]
            } else {
                vec![]
            }
        }
        Key::Char('n') | Key::Esc => {
            if let Some(ref mut p) = model.progress {
                p.recreate_confirm = false;
            }
            model.screen = Screen::List;
            model.progress = None;
            vec![]
        }
        _ => vec![],
    }
}

// ─── DoctorPanel screen ───────────────────────────────────────────────────────

fn handle_key_doctor(model: &mut Model, key: Key) -> Vec<Effect> {
    match key {
        Key::Esc | Key::Char('q') => {
            model.screen = Screen::List;
            vec![]
        }
        _ => vec![],
    }
}

// ─── Bundle 3: Logs screen ────────────────────────────────────────────────────

fn handle_key_logs(model: &mut Model, key: Key) -> Vec<Effect> {
    match key {
        Key::Esc | Key::Char('q') => {
            model.screen = Screen::List;
            model.log_target = None;
            vec![Effect::StopLogs]
        }
        // Autoscroll toggle.
        Key::Char('a') => {
            model.log_autoscroll = !model.log_autoscroll;
            vec![]
        }
        // Wrap toggle.
        Key::Char('w') => {
            model.log_wrap = !model.log_wrap;
            vec![]
        }
        Key::Up | Key::Char('k') => {
            model.log_scroll = model.log_scroll.saturating_add(SCROLL_STEP);
            // Manual scroll up disables autoscroll.
            model.log_autoscroll = false;
            vec![]
        }
        Key::Down | Key::Char('j') => {
            if model.log_scroll >= SCROLL_STEP {
                model.log_scroll -= SCROLL_STEP;
            } else {
                model.log_scroll = 0;
            }
            // Re-enable autoscroll when reaching the bottom.
            if model.log_scroll == 0 {
                model.log_autoscroll = true;
            }
            vec![]
        }
        Key::Left => {
            // Page up (by a larger step).
            model.log_scroll = model.log_scroll.saturating_add(SCROLL_STEP * 5);
            model.log_autoscroll = false;
            vec![]
        }
        Key::Right => {
            // Page down.
            if model.log_scroll >= SCROLL_STEP * 5 {
                model.log_scroll -= SCROLL_STEP * 5;
            } else {
                model.log_scroll = 0;
            }
            if model.log_scroll == 0 {
                model.log_autoscroll = true;
            }
            vec![]
        }
        _ => vec![],
    }
}

/// Open the live log streaming screen for the selected box (AC-LOGSCREEN-1).
/// If no box is selected → no-op (AC-LOGOPEN-1).
fn open_logs_screen(model: &mut Model) -> Vec<Effect> {
    let row = match model.selected_box().cloned() {
        Some(r) => r,
        None => return vec![],
    };

    let id = row.id.clone();
    let backend = row.backend.clone();

    // Determine the new target.
    let new_target = (id.clone(), backend.clone());

    // Swap detection: if already streaming a different target, emit StopLogs first.
    let mut effects: Vec<Effect> = Vec::new();
    if let Some(ref existing) = model.log_target {
        if existing != &new_target {
            effects.push(Effect::StopLogs);
        } else {
            // Same target already streaming — just transition to the screen.
            model.screen = Screen::Logs;
            return vec![];
        }
    }

    model.screen = Screen::Logs;
    model.log_target = Some(new_target);
    model.log_buffer.clear();
    model.log_scroll = 0;
    model.log_autoscroll = true;
    model.log_ended = false;

    effects.push(Effect::StreamLogs { id, backend });
    effects
}

// ─── Bundle 3: History overlay ────────────────────────────────────────────────

fn open_history_overlay(model: &mut Model) -> Vec<Effect> {
    let store = crate::tui::history::HistoryStore::new();
    let entries = store.load();
    let n = entries.len();
    model.overlay = Overlay::History {
        query: String::new(),
        matches: (0..n).collect(),
        cursor: 0,
        entries,
    };
    vec![]
}

fn handle_key_history(
    model: &mut Model,
    key: Key,
    cursor: usize,
    matches: Vec<usize>,
) -> Vec<Effect> {
    match key {
        Key::Esc | Key::Char('q') => {
            model.overlay = Overlay::None;
            vec![]
        }
        Key::Up | Key::Char('k') => {
            let new_cursor = cursor.saturating_sub(1);
            if let Overlay::History {
                cursor: ref mut c, ..
            } = model.overlay
            {
                *c = new_cursor;
            }
            vec![]
        }
        Key::Down | Key::Char('j') => {
            let max = matches.len().saturating_sub(1);
            let new_cursor = (cursor + 1).min(max);
            if let Overlay::History {
                cursor: ref mut c, ..
            } = model.overlay
            {
                *c = new_cursor;
            }
            vec![]
        }
        Key::Backspace => {
            if let Overlay::History {
                ref mut query,
                ref mut matches,
                ref mut cursor,
                ref entries,
            } = model.overlay
            {
                query.pop();
                let q = query.clone();
                let argvs: Vec<&str> = entries.iter().map(|e| e.argv.as_str()).collect();
                *matches = history_rank(&q, &argvs);
                *cursor = 0;
            }
            vec![]
        }
        Key::Enter => {
            // Read-only review — Enter is a no-op (spec: no re-run, out of scope).
            vec![]
        }
        Key::Char(c) => {
            if let Overlay::History {
                ref mut query,
                ref mut matches,
                ref mut cursor,
                ref entries,
            } = model.overlay
            {
                query.push(c);
                let q = query.clone();
                let argvs: Vec<&str> = entries.iter().map(|e| e.argv.as_str()).collect();
                *matches = history_rank(&q, &argvs);
                *cursor = 0;
            }
            vec![]
        }
        _ => vec![],
    }
}

fn history_rank(query: &str, argvs: &[&str]) -> Vec<usize> {
    #[cfg(feature = "tui")]
    {
        fuzzy_rank(query, argvs)
    }
    #[cfg(not(feature = "tui"))]
    {
        if query.trim().is_empty() {
            return (0..argvs.len()).collect();
        }
        argvs
            .iter()
            .enumerate()
            .filter(|(_, a)| a.to_lowercase().contains(&query.to_lowercase()))
            .map(|(i, _)| i)
            .collect()
    }
}

// ─── Bundle 3: mouse scroll handler ──────────────────────────────────────────

/// Return the scroll delta for a mouse event: +1 = scroll down, -1 = scroll up, 0 = ignore.
pub fn scroll_delta(mouse: &Mouse) -> i32 {
    match mouse {
        Mouse::ScrollDown => 1,
        Mouse::ScrollUp => -1,
        Mouse::Other => 0,
    }
}

fn handle_mouse(model: &mut Model, mouse: Mouse) -> Vec<Effect> {
    let delta = scroll_delta(&mouse);
    if delta == 0 {
        return vec![];
    }

    // Route by current overlay / screen.
    match model.overlay.clone() {
        Overlay::CommandLog { scroll } => {
            // Scroll the command-log overlay.
            let new_scroll = if delta > 0 {
                // ScrollDown → advance (newer entries).
                let entry_count = model.cmdlog.lock().map(|log| log.len()).unwrap_or(0);
                let max_scroll = entry_count.saturating_sub(1);
                (scroll + SCROLL_STEP).min(max_scroll)
            } else {
                scroll.saturating_sub(SCROLL_STEP)
            };
            model.overlay = Overlay::CommandLog { scroll: new_scroll };
            return vec![];
        }
        Overlay::History {
            cursor, matches, ..
        } => {
            let new_cursor = if delta > 0 {
                let max = matches.len().saturating_sub(1);
                (cursor + 1).min(max)
            } else {
                cursor.saturating_sub(1)
            };
            if let Overlay::History {
                cursor: ref mut c, ..
            } = model.overlay
            {
                *c = new_cursor;
            }
            return vec![];
        }
        _ => {}
    }

    match model.screen {
        Screen::List | Screen::Detail => {
            if delta > 0 {
                model.move_down();
            } else {
                model.move_up();
            }
        }
        Screen::Logs => {
            // delta > 0 = scroll down (towards newer) = decrease scroll offset.
            // delta < 0 = scroll up (towards older) = increase scroll offset.
            if delta < 0 {
                // Scroll up → older lines.
                model.log_scroll = model.log_scroll.saturating_add(SCROLL_STEP);
                model.log_autoscroll = false;
            } else {
                // Scroll down → newer lines.
                if model.log_scroll >= SCROLL_STEP {
                    model.log_scroll -= SCROLL_STEP;
                } else {
                    model.log_scroll = 0;
                }
                // Re-enable autoscroll when we reach the bottom (offset = 0).
                if model.log_scroll == 0 {
                    model.log_autoscroll = true;
                }
            }
        }
        _ => {}
    }
    vec![]
}

// ─── Bundle 3: log message handlers ──────────────────────────────────────────

fn handle_log_chunk(model: &mut Model, chunk: Vec<String>) -> Vec<Effect> {
    model.log_buffer.push_chunk(chunk);
    if model.log_autoscroll {
        model.log_scroll = 0; // snap to bottom (0 = bottom-most position)
    }
    vec![]
}

fn handle_log_stream_ended(model: &mut Model, _code: Option<i32>) -> Vec<Effect> {
    model.log_ended = true;
    vec![]
}

// ─── Bundle 2: dispatch_action (single reducer dispatch) ─────────────────────

/// Map an `Action` to its effects and model mutations.
///
/// Both the key path (`handle_key_list` arms) and the palette (Enter over a command)
/// route through this function — no behavior duplication (R-2).
pub fn dispatch_action(model: &mut Model, action: Action) -> Vec<Effect> {
    match action {
        Action::Filter => {
            open_filter(model);
            vec![]
        }
        Action::Cheatsheet => {
            model.overlay = Overlay::Cheatsheet;
            vec![]
        }
        Action::CommandLog => {
            model.overlay = Overlay::CommandLog { scroll: 0 };
            vec![]
        }
        Action::CycleSkin => {
            let next = model.skin.next();
            model.skin = next;
            let name = format!("Skin: {}", next.name());
            model.push_toast(crate::tui::model::ToastKind::Info, name);
            vec![]
        }
        Action::Doctor => start_doctor(model),
        Action::Refresh => start_manual_refresh(model),
        Action::Create => {
            model.screen = Screen::Wizard;
            model.wizard = Some(WizardState::new());
            vec![]
        }
        Action::Stop => stop_selected(model),
        Action::Destroy => confirm_destroy_selected(model),
        Action::Apply => apply_selected(model),
        Action::Edit => edit_selected(model),
        Action::Inspect => inspect_selected(model),
        Action::Open => open_selected(model),
        Action::MoveUp => {
            model.move_up();
            vec![]
        }
        Action::MoveDown => {
            model.move_down();
            vec![]
        }
        Action::Palette => {
            open_palette(model, false);
            vec![]
        }
        Action::Quit => {
            model.should_quit = true;
            vec![Effect::Quit]
        }
        // DeleteChar is handled inline in text-input contexts (filter / palette / wizard).
        // Dispatching it globally is a no-op.
        Action::DeleteChar => vec![],
        // Bulk: open bulk-confirm modal pre-loaded with the filtered target set.
        Action::BulkPruneStopped => open_bulk_confirm(model, BulkOp::PruneStopped),
        Action::BulkStopRunning => open_bulk_confirm(model, BulkOp::StopRunning),
        Action::BulkDestroyManaged => open_bulk_confirm(model, BulkOp::DestroyManaged),
        Action::BulkDestroyUnmanaged => open_bulk_confirm(model, BulkOp::DestroyUnmanaged),
        // Bundle 3.
        Action::History => open_history_overlay(model),
        Action::ViewLogs => open_logs_screen(model),
    }
}

// ─── Bundle 2: single-box action helpers (extracted so key path + palette agree) ──

fn open_filter(model: &mut Model) {
    model.overlay = Overlay::None;
    let names: Vec<&str> = model.boxes.iter().map(|b| b.name.as_str()).collect();
    let all_indices: Vec<usize> = (0..names.len()).collect();
    model.filter = Some(FilterState {
        query: String::new(),
        matches: all_indices,
        cursor: model
            .selected
            .unwrap_or(0)
            .min(model.boxes.len().saturating_sub(1)),
    });
}

fn start_doctor(model: &mut Model) -> Vec<Effect> {
    model.screen = Screen::DoctorPanel;
    model.busy = true;
    model.status = StatusLine::Busy("Running doctor…".to_string());
    vec![Effect::Doctor(DoctorSpec {
        backend_override: None,
    })]
}

fn start_manual_refresh(model: &mut Model) -> Vec<Effect> {
    model.busy = true;
    model.status = StatusLine::Busy("Refreshing…".to_string());
    vec![Effect::LoadList]
}

fn stop_selected(model: &mut Model) -> Vec<Effect> {
    if let Some(row) = model.selected_box().cloned() {
        let spec = StopSpec {
            names: vec![row.name.clone()],
            all: false,
            backend: backend_of(&row.backend, &model.backend),
        };
        model.busy = true;
        model.status = StatusLine::Busy(format!("Stopping \"{}\"…", row.name));
        vec![Effect::Stop(spec)]
    } else {
        vec![]
    }
}

fn confirm_destroy_selected(model: &mut Model) -> Vec<Effect> {
    if let Some(row) = model.selected_box().cloned() {
        model.screen = Screen::ConfirmDestroy;
        model.confirm = Some(ConfirmState {
            name: row.name.clone(),
            rm_home: false,
            backend: backend_of(&row.backend, &model.backend),
        });
    }
    vec![]
}

fn apply_selected(model: &mut Model) -> Vec<Effect> {
    if let Some(row) = model.selected_box().cloned() {
        let backend = backend_of(&row.backend, &model.backend);
        start_apply(model, &row.name, false, backend)
    } else {
        vec![]
    }
}

fn edit_selected(model: &mut Model) -> Vec<Effect> {
    if let Some(row) = model.selected_box().cloned() {
        start_edit(model, &row.name)
    } else {
        vec![]
    }
}

fn inspect_selected(model: &mut Model) -> Vec<Effect> {
    if let Some(row) = model.selected_box().cloned() {
        let spec = InspectSpec {
            name: row.name.clone(),
            raw: false,
            backend: backend_of(&row.backend, &model.backend),
        };
        model.screen = Screen::Detail;
        model.busy = true;
        model.status = StatusLine::Busy("Loading detail…".to_string());
        vec![Effect::LoadDetail(spec)]
    } else {
        vec![]
    }
}

fn open_selected(model: &mut Model) -> Vec<Effect> {
    if let Some(row) = model.selected_box().cloned() {
        let row_is_running = is_running(&row.status);
        if row_is_running {
            let spec = EnterSpec {
                name: row.name.clone(),
                root: false,
                clean_path: false,
                cmd: vec![],
                home_landing: true,
                backend: backend_of(&row.backend, &model.backend),
            };
            vec![Effect::SuspendAndEnter(spec)]
        } else {
            inspect_selected(model)
        }
    } else {
        vec![]
    }
}

// ─── Bundle 2: palette ────────────────────────────────────────────────────────

/// Open the command palette.
///
/// `bulk_only`: when `true`, restricts the palette source to the four bulk actions
/// (fast-path opened via `b`).
fn open_palette(model: &mut Model, bulk_only: bool) {
    model.filter = None; // close filter if open
    let source = if bulk_only {
        crate::tui::action::BULK_ACTIONS
    } else {
        crate::tui::action::palette_actions()
    };
    let n = source.len();
    model.overlay = Overlay::Palette {
        query: String::new(),
        matches: (0..n).collect(),
        cursor: 0,
        bulk_only,
    };
}

fn handle_key_palette(
    model: &mut Model,
    key: Key,
    cursor: usize,
    matches: Vec<usize>,
    bulk_only: bool,
) -> Vec<Effect> {
    match key {
        Key::Esc => {
            model.overlay = Overlay::None;
            vec![]
        }
        Key::Enter => {
            // Dispatch the selected action.
            let source = palette_source(bulk_only);
            if let Some(&action_idx) = matches.get(cursor) {
                if let Some(&action) = source.get(action_idx) {
                    model.overlay = Overlay::None;
                    return dispatch_action(model, action);
                }
            }
            vec![]
        }
        Key::Up => {
            let new_cursor = cursor.saturating_sub(1);
            if let Overlay::Palette {
                cursor: ref mut c, ..
            } = model.overlay
            {
                *c = new_cursor;
            }
            vec![]
        }
        Key::Down => {
            let max = matches.len().saturating_sub(1);
            let new_cursor = (cursor + 1).min(max);
            if let Overlay::Palette {
                cursor: ref mut c, ..
            } = model.overlay
            {
                *c = new_cursor;
            }
            vec![]
        }
        Key::Backspace => {
            if let Overlay::Palette {
                ref mut query,
                ref mut matches,
                ref mut cursor,
                bulk_only,
            } = model.overlay
            {
                query.pop();
                let q = query.clone();
                let source = palette_source(bulk_only);
                let labels: Vec<&str> = source.iter().map(|a| a.label()).collect();
                *matches = palette_rank(&q, &labels);
                *cursor = 0;
            }
            vec![]
        }
        Key::Char(c) => {
            // In palette, ALL chars (including j/k) feed the query (consistent with filter).
            if let Overlay::Palette {
                ref mut query,
                ref mut matches,
                ref mut cursor,
                bulk_only,
            } = model.overlay
            {
                query.push(c);
                let q = query.clone();
                let source = palette_source(bulk_only);
                let labels: Vec<&str> = source.iter().map(|a| a.label()).collect();
                *matches = palette_rank(&q, &labels);
                *cursor = 0;
            }
            vec![]
        }
        _ => vec![],
    }
}

fn palette_source(bulk_only: bool) -> &'static [Action] {
    if bulk_only {
        BULK_ACTIONS
    } else {
        crate::tui::action::palette_actions()
    }
}

/// Rank palette entries by query using fuzzy_rank when tui is enabled, or
/// a simple substring fallback for lean builds.
fn palette_rank(query: &str, labels: &[&str]) -> Vec<usize> {
    #[cfg(feature = "tui")]
    {
        fuzzy_rank(query, labels)
    }
    #[cfg(not(feature = "tui"))]
    {
        if query.trim().is_empty() {
            return (0..labels.len()).collect();
        }
        labels
            .iter()
            .enumerate()
            .filter(|(_, l)| l.to_lowercase().contains(&query.to_lowercase()))
            .map(|(i, _)| i)
            .collect()
    }
}

// ─── Bundle 2: bulk confirm ───────────────────────────────────────────────────

/// Open the bulk-confirm modal for the given op.
///
/// Computes the target set from `model.boxes`. If empty, pushes an info toast
/// and does NOT open the modal (AC-BULK-EMPTY).
fn open_bulk_confirm(model: &mut Model, op: BulkOp) -> Vec<Effect> {
    let indices = bulk_targets(op, &model.boxes);
    if indices.is_empty() {
        model.push_toast(
            crate::tui::model::ToastKind::Info,
            strings::BULK_EMPTY.to_string(),
        );
        return vec![];
    }

    let targets: Vec<String> = indices
        .iter()
        .map(|&i| model.boxes[i].name.clone())
        .collect();
    let target_backends: Vec<String> = indices
        .iter()
        .map(|&i| model.boxes[i].backend.clone())
        .collect();

    // Close palette if open.
    model.overlay = Overlay::None;

    model.bulk_confirm = Some(BulkConfirmState {
        op,
        targets,
        target_backends,
        typed_confirm: String::new(),
    });
    vec![]
}

/// Key handler for the bulk-confirm modal.
///
/// For the dangerous op (`DestroyUnmanaged`): ALL chars feed `typed_confirm`;
/// Enter executes ONLY if `typed_confirm == BULK_UNMANAGED_PHRASE`.
/// For non-dangerous ops: y/Enter confirms; n/Esc cancels.
fn handle_key_bulk_confirm(model: &mut Model, key: Key) -> Vec<Effect> {
    let op = match model.bulk_confirm.as_ref().map(|b| b.op) {
        Some(op) => op,
        None => return vec![],
    };

    if op == BulkOp::DestroyUnmanaged {
        // DANGEROUS path: typed phrase required.
        match key {
            Key::Esc | Key::Char('n') => {
                model.bulk_confirm = None;
                vec![]
            }
            Key::Backspace => {
                if let Some(ref mut b) = model.bulk_confirm {
                    b.typed_confirm.pop();
                }
                vec![]
            }
            Key::Enter => {
                let confirmed = model
                    .bulk_confirm
                    .as_ref()
                    .map(|b| b.typed_confirm == strings::BULK_UNMANAGED_PHRASE)
                    .unwrap_or(false);
                if confirmed {
                    execute_bulk_confirm(model)
                } else {
                    // Wrong phrase — stay in modal.
                    vec![]
                }
            }
            Key::Char(c) => {
                // Single char only appends — never destroys on its own (AC-BULK-DANGER-2).
                if let Some(ref mut b) = model.bulk_confirm {
                    b.typed_confirm.push(c);
                }
                vec![]
            }
            _ => vec![],
        }
    } else {
        // Non-dangerous: y/enter confirms; n/esc cancels.
        match key {
            Key::Char('y') | Key::Enter => execute_bulk_confirm(model),
            Key::Char('n') | Key::Esc => {
                model.bulk_confirm = None;
                vec![]
            }
            _ => vec![],
        }
    }
}

/// Fan-out confirmed bulk op — builds one Effect::Rm/Stop per backend group.
fn execute_bulk_confirm(model: &mut Model) -> Vec<Effect> {
    let bulk = match model.bulk_confirm.take() {
        Some(b) => b,
        None => return vec![],
    };

    // Group targets by backend (≤ 2 groups — podman + docker, under 4-deep channel).
    let mut by_backend: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for (name, backend_str) in bulk.targets.iter().zip(bulk.target_backends.iter()) {
        by_backend
            .entry(backend_str.clone())
            .or_default()
            .push(name.clone());
    }

    let n = bulk.targets.len();
    let op = bulk.op;

    let busy_msg = match op {
        BulkOp::PruneStopped => format!("Pruning {n} stopped boxes…"),
        BulkOp::StopRunning => format!("Stopping {n} running boxes…"),
        BulkOp::DestroyManaged => format!("Destroying {n} cbox-managed boxes…"),
        BulkOp::DestroyUnmanaged => format!("Destroying {n} NON-managed boxes…"),
    };
    let title = match op {
        BulkOp::PruneStopped => format!("Pruning {n} boxes"),
        BulkOp::StopRunning => format!("Stopping {n} boxes"),
        BulkOp::DestroyManaged => format!("Destroying {n} boxes"),
        BulkOp::DestroyUnmanaged => format!("Destroying {n} NON-managed boxes"),
    };

    model.screen = Screen::Progress;
    model.busy = true;
    model.status = StatusLine::Busy(busy_msg);
    model.progress = Some(ProgressState {
        title,
        steps: vec![],
        spinner_tick: 0,
        recreate_needed: false,
        recreate_msg: None,
        recreate_confirm: false,
        pending_spec: None,
    });

    let mut effects = Vec::new();
    for (backend_str, names) in by_backend {
        let backend = Backend::from_name(&backend_str).unwrap_or_else(|| model.backend.clone());
        match op {
            BulkOp::PruneStopped | BulkOp::DestroyManaged | BulkOp::DestroyUnmanaged => {
                effects.push(Effect::Rm(RmSpec {
                    names,
                    force: true,
                    rm_home: false,
                    all: false,
                    yes: true,
                    backend,
                }));
            }
            BulkOp::StopRunning => {
                effects.push(Effect::Stop(StopSpec {
                    names,
                    all: false,
                    backend,
                }));
            }
        }
    }

    effects
}

// ─── Bundle 2: poll helpers ───────────────────────────────────────────────────

/// Build the `PollGate` view from the current model state.
fn build_poll_gate(model: &Model) -> PollGate {
    // Determine if we're on Detail with a running box.
    let detail_running = if model.screen == Screen::Detail {
        model.detail.as_ref().and_then(|d| {
            if is_running(&d.status) {
                Some((d.id.clone(), d.backend.clone()))
            } else {
                None
            }
        })
    } else {
        None
    };

    PollGate {
        spinner_tick: model.spinner_tick,
        last_poll_tick: model.last_poll_tick,
        busy: model.busy,
        poll_in_flight: model.poll_in_flight,
        detail_running,
    }
}

/// Map a `PollKind` to the concrete `Effect` to dispatch.
fn poll_kind_to_effect(kind: PollKind) -> Effect {
    match kind {
        PollKind::List => Effect::SilentLoadList,
        PollKind::Stats { id, backend } => {
            let b = Backend::from_name(&backend).unwrap_or(Backend::Podman);
            Effect::StatsPoll(StatsSpec { id, backend: b })
        }
    }
}

// ─── Bundle 2: silent poll completion handlers ────────────────────────────────

fn handle_silent_list_loaded(
    model: &mut Model,
    result: Result<Vec<crate::core::spec::BoxRow>, CboxError>,
) -> Vec<Effect> {
    // Always clear the in-flight guard (AC-POLL-4).
    model.poll_in_flight = false;
    match result {
        Ok(rows) => {
            model.boxes = rows;
            // Clamp selection — mirror handle_list_loaded (AC-POLL-5).
            if let Some(i) = model.selected {
                if i >= model.boxes.len() {
                    model.selected = if model.boxes.is_empty() {
                        None
                    } else {
                        Some(model.boxes.len() - 1)
                    };
                }
            }
            if model.selected.is_none() && !model.boxes.is_empty() {
                model.selected = Some(0);
            }
            // Recompute filter if open (AC-POLL-5).
            if model.filter.is_some() {
                let query = model.filter.as_ref().unwrap().query.clone();
                recompute_filter(model, &query);
            }
            // NEVER set model.status, NEVER set model.busy, NEVER push a toast (AC-POLL-4).
        }
        Err(_) => {
            // Swallow silently — a transient refresh failure is invisible.
            // Next poll will retry.
        }
    }
    vec![]
}

fn handle_stats_loaded(
    model: &mut Model,
    result: Result<crate::core::spec::StatsSample, CboxError>,
) -> Vec<Effect> {
    // Always clear the in-flight guard.
    model.poll_in_flight = false;
    if let Ok(sample) = result {
        // Push into the bounded history buffer.
        if let Some(ref mut history) = model.stats_history {
            history.push_sample(&sample);
        }
        // If history hasn't been initialized yet (shouldn't happen), initialize it now.
        // (detail sets it in handle_detail_loaded; this is a safety net.)
    }
    // On Err: push nothing — history goes stale → renders gracefully (AC-STATS-PARSE-3).
    vec![]
}

// ─── Helper: start apply ─────────────────────────────────────────────────────

fn start_apply(model: &mut Model, name: &str, recreate: bool, backend: Backend) -> Vec<Effect> {
    // Resolve boxfile path (XDG fallback — we don't have a runner here since reducer is pure).
    let config_home = std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
        format!("{home}/.config")
    });
    let boxfile_path = format!("{config_home}/cbox/boxes/{name}/Boxfile.toml");

    let spec = ApplySpec {
        recreate,
        yes: true,
        ..ApplySpec::new(name, boxfile_path, backend)
    };

    model.screen = Screen::Progress;
    model.busy = true;
    model.status = StatusLine::Busy(format!("Applying \"{}\"…", name));
    model.progress = Some(ProgressState {
        title: format!("Applying \"{}\"", name),
        steps: vec![],
        spinner_tick: 0,
        recreate_needed: false,
        recreate_msg: None,
        recreate_confirm: false,
        pending_spec: Some(spec.clone()),
    });

    vec![Effect::Apply(spec)]
}

// ─── Helper: start edit ──────────────────────────────────────────────────────

fn start_edit(model: &mut Model, name: &str) -> Vec<Effect> {
    // XDG fallback path (same as CLI edit / resolve_boxfile_path XDG fallback).
    let config_home = std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
        format!("{home}/.config")
    });
    let path = format!("{config_home}/cbox/boxes/{name}/Boxfile.toml");
    model.pending_edit = Some(path.clone());
    vec![Effect::SuspendAndEdit(path)]
}

// ─── Effect completion handlers ───────────────────────────────────────────────

fn handle_list_loaded(
    model: &mut Model,
    result: Result<Vec<crate::core::spec::BoxRow>, CboxError>,
) -> Vec<Effect> {
    model.busy = false;
    match result {
        Ok(rows) => {
            model.boxes = rows;
            // Clamp selection.
            if let Some(i) = model.selected {
                if i >= model.boxes.len() {
                    model.selected = if model.boxes.is_empty() {
                        None
                    } else {
                        Some(model.boxes.len() - 1)
                    };
                }
            }
            if model.selected.is_none() && !model.boxes.is_empty() {
                model.selected = Some(0);
            }
            // If a filter is open, recompute matches against the new rows.
            if model.filter.is_some() {
                let query = model.filter.as_ref().unwrap().query.clone();
                recompute_filter(model, &query);
            }
            model.status = StatusLine::Ok(strings::loaded(model.boxes.len()));
            vec![]
        }
        Err(e) => {
            // Backend unreachable → auto-route to DoctorPanel.
            let is_tempfail =
                matches!(&e, CboxError::TempFail { .. }) || matches!(&e, CboxError::Backend { .. });
            model.status = StatusLine::Error(e.to_string());
            if is_tempfail {
                model.screen = Screen::DoctorPanel;
                model.busy = true;
                model.status = StatusLine::Busy(strings::backend_unreachable().to_string());
                vec![Effect::Doctor(DoctorSpec {
                    backend_override: None,
                })]
            } else {
                vec![]
            }
        }
    }
}

fn handle_detail_loaded(
    model: &mut Model,
    result: Result<crate::core::spec::InspectResult, CboxError>,
) -> Vec<Effect> {
    model.busy = false;
    match result {
        Ok(detail) => {
            let msg = format!("Inspected \"{}\"", detail.name);
            model.set_status_ok(msg);

            // Initialize stats history for RUNNING boxes (AC-HIST-2, AC-STATS-STOPPED).
            // Stopped boxes get no history — should_poll won't fire StatsPoll for them.
            let running = is_running(&detail.status);
            if running {
                // Reset to a fresh buffer if we're looking at a different box.
                let needs_reset = model
                    .stats_history
                    .as_ref()
                    .map(|h| h.box_id != detail.id)
                    .unwrap_or(true);
                if needs_reset {
                    model.stats_history = Some(StatsHistory::new(detail.id.clone()));
                }
            } else {
                model.stats_history = None;
            }

            model.detail = Some(detail);
            vec![]
        }
        Err(e) => {
            model.set_status_error(e.to_string());
            model.screen = Screen::List;
            model.stats_history = None;
            vec![]
        }
    }
}

fn handle_create_done(
    model: &mut Model,
    result: Result<crate::tui::message::CreateOutcome, CboxError>,
) -> Vec<Effect> {
    model.busy = false;
    match result {
        Ok(outcome) => {
            let msg = strings::created(&outcome.name);
            model.set_status_ok(msg);
            model.screen = Screen::List;
            model.progress = None;
            // Refresh the list.
            model.busy = true;
            vec![Effect::LoadList]
        }
        Err(e) => {
            model.set_status_error(e.to_string());
            model.screen = Screen::List;
            model.progress = None;
            vec![]
        }
    }
}

fn handle_rm_done(
    model: &mut Model,
    result: Result<crate::tui::message::RmOutcome, CboxError>,
) -> Vec<Effect> {
    model.busy = false;
    match result {
        Ok(outcome) => {
            let msg = strings::removed(&outcome.removed.join(", "));
            model.set_status_ok(msg);
            model.screen = Screen::List;
            model.progress = None;
            model.selected = None;
            model.busy = true;
            vec![Effect::LoadList]
        }
        Err(e) => {
            model.set_status_error(e.to_string());
            model.screen = Screen::List;
            model.progress = None;
            vec![]
        }
    }
}

fn handle_stop_done(
    model: &mut Model,
    result: Result<crate::tui::message::StopOutcome, CboxError>,
) -> Vec<Effect> {
    model.busy = false;
    match result {
        Ok(outcome) => {
            let msg = strings::stopped(&outcome.stopped.join(", "));
            model.set_status_ok(msg);
            model.busy = true;
            vec![Effect::LoadList]
        }
        Err(e) => {
            model.set_status_error(e.to_string());
            vec![]
        }
    }
}

fn handle_apply_done(
    model: &mut Model,
    result: Result<crate::core::spec::ApplyOutcome, CboxError>,
) -> Vec<Effect> {
    model.busy = false;
    match result {
        Ok(outcome) => {
            let msg = strings::applied(
                &outcome.name,
                outcome.summary.ran,
                outcome.summary.skipped,
                outcome.summary.failed,
            );
            model.set_status_ok(msg);
            if let Some(ref mut p) = model.progress {
                p.steps = outcome.steps;
            }
            vec![]
        }
        Err(e) => {
            // Check if this is a recreate-class error (exit 65 / DataErr).
            let is_recreate = matches!(&e, CboxError::DataErr { .. });
            if is_recreate {
                let msg = e.to_string();
                if let Some(ref mut p) = model.progress {
                    p.recreate_needed = true;
                    p.recreate_msg = Some(msg.clone());
                    p.recreate_confirm = true;
                }
                model.set_status_error(msg);
            } else {
                model.set_status_error(e.to_string());
                model.screen = Screen::List;
                model.progress = None;
            }
            vec![]
        }
    }
}

fn handle_up_done(
    model: &mut Model,
    result: Result<crate::core::spec::UpOutcome, CboxError>,
) -> Vec<Effect> {
    model.busy = false;
    match result {
        Ok(outcome) => {
            let action = if outcome.created {
                "Created+applied"
            } else {
                "Applied"
            };
            let msg = format!("{} \"{}\"", action, outcome.name);
            model.set_status_ok(msg);
            if let Some(ref mut p) = model.progress {
                p.steps = outcome.apply.steps;
            }
            vec![]
        }
        Err(e) => {
            model.set_status_error(e.to_string());
            model.screen = Screen::List;
            model.progress = None;
            vec![]
        }
    }
}

fn handle_doctor_done(
    model: &mut Model,
    result: Result<crate::core::spec::DoctorResult, CboxError>,
) -> Vec<Effect> {
    model.busy = false;
    match result {
        Ok(dr) => {
            model.doctor = Some(dr);
            model.status = StatusLine::Ok("Doctor complete".to_string());
            // Doctor completion sets status only (no toast — informational, not an op result).
            vec![]
        }
        Err(e) => {
            model.set_status_error(e.to_string());
            vec![]
        }
    }
}

fn handle_enter_returned(model: &mut Model, result: Result<i32, CboxError>) -> Vec<Effect> {
    model.pending_enter = None;
    match result {
        Ok(code) => {
            if code == 0 {
                model.set_status_ok("Returned from box".to_string());
            } else {
                model.set_status_error(format!("Box exited with code {code}"));
            }
        }
        Err(e) => {
            model.set_status_error(e.to_string());
        }
    }
    // Refresh after enter.
    model.busy = true;
    vec![Effect::LoadList]
}

fn handle_edit_returned(model: &mut Model, result: Result<(), CboxError>) -> Vec<Effect> {
    model.pending_edit = None;
    match result {
        Ok(()) => {
            model.set_status_ok("Boxfile saved".to_string());
        }
        Err(e) => {
            model.set_status_error(e.to_string());
        }
    }
    vec![]
}
