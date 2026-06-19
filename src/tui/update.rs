//! The pure reducer: `update(&mut Model, Message) -> Vec<Effect>`.
//!
//! Rules:
//!  - Pure over (Model, Message): no I/O, no thread spawn, no runner, no clock.
//!  - `busy == true` ignores conflicting Key messages but always honors Tick, Resize,
//!    and Ctrl-C → Quit.
//!  - Effect completions clear `busy`, update the relevant Model field, set status.
//!  - Key handling is screen-dispatched: match `model.screen` first, then the key.

use crate::core::spec::{ApplySpec, DoctorSpec, EnterSpec, InspectSpec, RmSpec, StopSpec};
use crate::dbox::backend::Backend;
use crate::error::CboxError;
use crate::tui::effect::Effect;
use crate::tui::message::{Key, Message};
use crate::tui::model::{
    ConfirmState, Model, ProgressState, Screen, StatusLine, WizardState, WizardStep,
};

/// Resolve a box's engine from its stored backend string, falling back to the
/// create default when it's missing/unknown (e.g. mock rows in tests).
fn backend_of(row_backend: &str, fallback: &Backend) -> Backend {
    Backend::from_name(row_backend).unwrap_or_else(|| fallback.clone())
}

/// The pure reducer. Returns a list of effects for the shell to execute.
pub fn update(model: &mut Model, msg: Message) -> Vec<Effect> {
    match msg {
        // ── Tick: advance spinner ──────────────────────────────────────────────
        Message::Tick => {
            model.spinner_tick = model.spinner_tick.wrapping_add(1);
            if let Some(ref mut p) = model.progress {
                p.spinner_tick = model.spinner_tick;
            }
            vec![]
        }

        // ── Resize: no-op (ratatui handles layout) ────────────────────────────
        Message::Resize(_, _) => vec![],

        // ── Key events ────────────────────────────────────────────────────────
        Message::Key(key) => handle_key(model, key),

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
    }
}

// ─── Key dispatch ────────────────────────────────────────────────────────────

fn handle_key(model: &mut Model, key: Key) -> Vec<Effect> {
    // Ctrl-C always quits, even when busy.
    if key == Key::CtrlC || key == Key::CtrlD {
        model.should_quit = true;
        return vec![Effect::Quit];
    }

    // When busy, drop all other keys (spinner is shown; the worker is running).
    if model.busy {
        return vec![];
    }

    match model.screen {
        Screen::List => handle_key_list(model, key),
        Screen::Detail => handle_key_detail(model, key),
        Screen::Wizard => handle_key_wizard(model, key),
        Screen::ConfirmDestroy => handle_key_confirm(model, key),
        Screen::Progress => handle_key_progress(model, key),
        Screen::DoctorPanel => handle_key_doctor(model, key),
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
        Key::Enter => {
            if let Some(row) = model.selected_box().cloned() {
                let is_running = row.status.to_lowercase().contains("running")
                    || row.status.to_lowercase().contains("up");
                if is_running {
                    let spec = EnterSpec {
                        name: row.name.clone(),
                        root: false,
                        clean_path: false,
                        cmd: vec![],
                        backend: backend_of(&row.backend, &model.backend),
                    };
                    vec![Effect::SuspendAndEnter(spec)]
                } else {
                    // stopped → inspect/detail
                    let spec = InspectSpec {
                        name: row.name.clone(),
                        raw: false,
                        backend: backend_of(&row.backend, &model.backend),
                    };
                    model.screen = Screen::Detail;
                    model.busy = true;
                    model.status = StatusLine::Busy("Loading detail…".to_string());
                    vec![Effect::LoadDetail(spec)]
                }
            } else {
                vec![]
            }
        }
        Key::Char('i') => {
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
        Key::Char('c') => {
            model.screen = Screen::Wizard;
            model.wizard = Some(WizardState::new());
            vec![]
        }
        Key::Char('d') => {
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
        Key::Char('s') => {
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
        Key::Char('a') => {
            if let Some(row) = model.selected_box().cloned() {
                let backend = backend_of(&row.backend, &model.backend);
                start_apply(model, &row.name, false, backend)
            } else {
                vec![]
            }
        }
        Key::Char('u') => {
            // Up is not fully wired in v3.0 list screen (needs boxfile path).
            // Treat as apply for now.
            if let Some(row) = model.selected_box().cloned() {
                let backend = backend_of(&row.backend, &model.backend);
                start_apply(model, &row.name, false, backend)
            } else {
                vec![]
            }
        }
        Key::Char('e') => {
            if let Some(row) = model.selected_box().cloned() {
                start_edit(model, &row.name)
            } else {
                vec![]
            }
        }
        Key::Char('r') => {
            model.busy = true;
            model.status = StatusLine::Busy("Refreshing…".to_string());
            vec![Effect::LoadList]
        }
        Key::Char('?') => {
            model.screen = Screen::DoctorPanel;
            model.busy = true;
            model.status = StatusLine::Busy("Running doctor…".to_string());
            vec![Effect::Doctor(DoctorSpec {
                backend_override: None,
            })]
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
                let is_running = detail.status.to_lowercase().contains("running")
                    || detail.status.to_lowercase().contains("up");
                if is_running {
                    let spec = EnterSpec {
                        name: detail.name.clone(),
                        root: false,
                        clean_path: false,
                        cmd: vec![],
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

// ─── Helper: start apply ─────────────────────────────────────────────────────

fn start_apply(model: &mut Model, name: &str, recreate: bool, backend: Backend) -> Vec<Effect> {
    // Resolve boxfile path (XDG fallback — we don't have a runner here since reducer is pure).
    let config_home = std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
        format!("{home}/.config")
    });
    let boxfile_path = format!("{config_home}/cbox/boxes/{name}/Boxfile.toml");

    let spec = ApplySpec {
        name: name.to_string(),
        boxfile_path,
        force: false,
        redo: vec![],
        no_provision: false,
        recreate,
        yes: true,
        dry_run: false,
        backend,
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
            model.status = StatusLine::Ok(format!("{} box(es) loaded", model.boxes.len()));
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
                model.status =
                    StatusLine::Busy("Can't reach backend — running doctor…".to_string());
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
            model.status = StatusLine::Ok(format!("Inspected \"{}\"", detail.name));
            model.detail = Some(detail);
            vec![]
        }
        Err(e) => {
            model.status = StatusLine::Error(e.to_string());
            model.screen = Screen::List;
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
            model.status = StatusLine::Ok(format!("Created \"{}\"", outcome.name));
            model.screen = Screen::List;
            model.progress = None;
            // Refresh the list.
            model.busy = true;
            vec![Effect::LoadList]
        }
        Err(e) => {
            model.status = StatusLine::Error(e.to_string());
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
            model.status = StatusLine::Ok(format!("Removed: {}", outcome.removed.join(", ")));
            model.screen = Screen::List;
            model.progress = None;
            model.selected = None;
            model.busy = true;
            vec![Effect::LoadList]
        }
        Err(e) => {
            model.status = StatusLine::Error(e.to_string());
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
            model.status = StatusLine::Ok(format!("Stopped: {}", outcome.stopped.join(", ")));
            model.busy = true;
            vec![Effect::LoadList]
        }
        Err(e) => {
            model.status = StatusLine::Error(e.to_string());
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
            model.status = StatusLine::Ok(format!(
                "Applied \"{}\": {} ran, {} skipped, {} failed",
                outcome.name, outcome.summary.ran, outcome.summary.skipped, outcome.summary.failed
            ));
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
                model.status = StatusLine::Error(msg);
            } else {
                model.status = StatusLine::Error(e.to_string());
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
            model.status = StatusLine::Ok(format!("{} \"{}\"", action, outcome.name));
            if let Some(ref mut p) = model.progress {
                p.steps = outcome.apply.steps;
            }
            vec![]
        }
        Err(e) => {
            model.status = StatusLine::Error(e.to_string());
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
            vec![]
        }
        Err(e) => {
            model.status = StatusLine::Error(e.to_string());
            vec![]
        }
    }
}

fn handle_enter_returned(model: &mut Model, result: Result<i32, CboxError>) -> Vec<Effect> {
    model.pending_enter = None;
    match result {
        Ok(code) => {
            if code == 0 {
                model.status = StatusLine::Ok("Returned from box".to_string());
            } else {
                model.status = StatusLine::Error(format!("Box exited with code {code}"));
            }
        }
        Err(e) => {
            model.status = StatusLine::Error(e.to_string());
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
            model.status = StatusLine::Ok("Boxfile saved".to_string());
        }
        Err(e) => {
            model.status = StatusLine::Error(e.to_string());
        }
    }
    vec![]
}
