//! Bundle 1 "Retro Cockpit" acceptance-criteria tests.
//! Covers: AC-FILTER-*, AC-CHEAT-*, AC-TOAST-*, AC-CMDLOG-*, AC-REBIND-*, AC-SKIN-CYCLE.
//!
//! All tests target PURE helpers + the reducer (no terminal, no runner, no threads).
//! Mirrors the testing style of `tests/tui_theme.rs` and `tests/tui_update.rs`.
#![cfg(feature = "tui")]

use std::sync::{Arc, Mutex};

use cbox::core::spec::BoxRow;
use cbox::dbox::backend::Backend;
use cbox::tui::cmdlog::{CmdLog, LoggingRunner};
use cbox::tui::effect::Effect;
use cbox::tui::filter::fuzzy_rank;
use cbox::tui::history::HistoryStore;
use cbox::tui::message::{Key, Message};
use cbox::tui::model::{
    FilterState, Model, Overlay, StatusLine, ToastKind, TOAST_MAX, TOAST_TTL_ERROR,
    TOAST_TTL_SUCCESS,
};
use cbox::tui::theme::Skin;
use cbox::tui::update::update;

// ─── helpers ─────────────────────────────────────────────────────────────────

fn make_model() -> Model {
    Model::new(Backend::Podman)
}

fn make_box(name: &str) -> BoxRow {
    BoxRow {
        name: name.to_string(),
        status: "running".to_string(),
        image: "fedora-toolbox:latest".to_string(),
        docker_mode: "none".to_string(),
        cbox_managed: true,
        id: "abc123".to_string(),
        backend: "podman".to_string(),
    }
}

fn make_model_with_boxes(names: &[&str]) -> Model {
    let mut m = make_model();
    m.boxes = names.iter().map(|n| make_box(n)).collect();
    m.selected = if names.is_empty() { None } else { Some(0) };
    m
}

fn key_msg(k: Key) -> Message {
    Message::Key(k)
}

// ─── AC-FILTER-1: fuzzy_rank ranks a known set ────────────────────────────────

#[test]
fn ac_filter_1_fuzzy_rank_known_set() {
    let names = ["web-dev", "api", "webhook", "db"];
    let result = fuzzy_rank("web", &names);

    // Result must contain web-dev (0) and webhook (2) but NOT api (1) or db (3).
    assert!(
        result.contains(&0),
        "fuzzy_rank('web', ...) must include 'web-dev' (idx 0)"
    );
    assert!(
        result.contains(&2),
        "fuzzy_rank('web', ...) must include 'webhook' (idx 2)"
    );
    assert!(
        !result.contains(&1),
        "fuzzy_rank('web', ...) must NOT include 'api' (idx 1)"
    );
    assert!(
        !result.contains(&3),
        "fuzzy_rank('web', ...) must NOT include 'db' (idx 3)"
    );
}

// ─── AC-FILTER-2: empty query returns identity ────────────────────────────────

#[test]
fn ac_filter_2_empty_query_identity() {
    let names = ["web-dev", "api", "webhook", "db"];
    let result = fuzzy_rank("", &names);
    assert_eq!(
        result,
        vec![0, 1, 2, 3],
        "empty query must return all indices in order"
    );
}

#[test]
fn ac_filter_2_whitespace_query_identity() {
    let names = ["web-dev", "api", "webhook"];
    let result = fuzzy_rank("   ", &names);
    assert_eq!(result, vec![0, 1, 2]);
}

// ─── AC-FILTER-3: selection maps correctly under active filter ────────────────

#[test]
fn ac_filter_3_selection_maps_under_filter() {
    // Boxes: [A(0), B(1), C(2), D(3)]. Filter matches [B(1), D(3)] → cursor 0 → selected=1.
    let mut model = make_model_with_boxes(&["A", "B", "C", "D"]);
    // Inject a pre-computed filter state with matches [1, 3].
    model.filter = Some(FilterState {
        query: "B".to_string(),
        matches: vec![1, 3],
        cursor: 0,
    });
    model.selected = Some(1);

    // move_down: cursor 0→1, selected→3.
    model.move_down();
    assert_eq!(
        model.filter.as_ref().unwrap().cursor,
        1,
        "filter cursor must advance to 1"
    );
    assert_eq!(
        model.selected,
        Some(3),
        "selected must map to matches[1] = 3"
    );

    // move_down again: cursor stays at 1 (clamped), selected stays 3.
    model.move_down();
    assert_eq!(model.filter.as_ref().unwrap().cursor, 1, "cursor clamps");
    assert_eq!(model.selected, Some(3), "selected stays 3");

    // move_up: cursor 1→0, selected→1.
    model.move_up();
    assert_eq!(model.filter.as_ref().unwrap().cursor, 0);
    assert_eq!(model.selected, Some(1));
}

// ─── AC-FILTER-4: close semantics ────────────────────────────────────────────

#[test]
fn ac_filter_4_enter_keeps_selection_and_closes() {
    let mut model = make_model_with_boxes(&["A", "B", "C"]);
    model.filter = Some(FilterState {
        query: "B".to_string(),
        matches: vec![1],
        cursor: 0,
    });
    model.selected = Some(1);

    // Enter: close filter, keep selected = 1.
    update(&mut model, key_msg(Key::Enter));

    assert!(model.filter.is_none(), "filter must be None after Enter");
    assert_eq!(
        model.selected,
        Some(1),
        "selected must be preserved after Enter"
    );
}

#[test]
fn ac_filter_4_esc_closes_and_restores() {
    let mut model = make_model_with_boxes(&["A", "B", "C"]);
    model.filter = Some(FilterState {
        query: "B".to_string(),
        matches: vec![1],
        cursor: 0,
    });
    model.selected = Some(1);

    // Esc: close filter. Selection falls back to match index (still valid → Some(1)).
    update(&mut model, key_msg(Key::Esc));

    assert!(model.filter.is_none(), "filter must be None after Esc");
    // The box at index 1 is still present, so selected stays Some(1).
    assert_eq!(model.selected, Some(1));
}

// ─── AC-FILTER-5: refresh recomputes filter matches ──────────────────────────

#[test]
fn ac_filter_5_list_loaded_recomputes_filter() {
    let mut model = make_model_with_boxes(&["web-dev", "api"]);
    // Open filter with query "web".
    update(&mut model, key_msg(Key::Char('/')));
    // Type 'w', 'e', 'b'.
    for c in ['w', 'e', 'b'] {
        update(&mut model, key_msg(Key::Char(c)));
    }
    // Verify filter is active.
    assert!(model.filter.is_some());

    // Simulate ListLoaded with new rows (web-dev gone, api still here).
    let new_rows = vec![make_box("api"), make_box("webhook"), make_box("backend")];
    update(&mut model, Message::ListLoaded(Ok(new_rows)));

    // Filter must still be active (query unchanged) and matches recomputed.
    let f = model.filter.as_ref().expect("filter must still be open");
    // "web" should match "webhook" (idx 1 in the new list), not "api" (0) or "backend" (2).
    assert!(
        f.matches.contains(&1),
        "after refresh, 'web' should match 'webhook' at new idx 1"
    );
    assert!(!f.matches.contains(&0), "'api' should not match 'web'");
    // cursor must be within valid range.
    assert!(
        f.cursor < f.matches.len().max(1),
        "cursor must be within valid range"
    );
    // selected must not be out of range.
    if let Some(sel) = model.selected {
        assert!(sel < model.boxes.len(), "selected must be in bounds");
    }
}

// ─── AC-CHEAT-1: cheatsheet overlay open/dismiss ────────────────────────────

#[test]
fn ac_cheat_1_question_mark_opens_cheatsheet() {
    let mut model = make_model();
    assert_eq!(model.overlay, Overlay::None);

    update(&mut model, key_msg(Key::Char('?')));
    assert_eq!(
        model.overlay,
        Overlay::Cheatsheet,
        "? must open Overlay::Cheatsheet"
    );
}

#[test]
fn ac_cheat_1_any_key_dismisses_cheatsheet() {
    let mut model = make_model();
    model.overlay = Overlay::Cheatsheet;

    // Any key (e.g. 'x') must dismiss.
    update(&mut model, key_msg(Key::Char('x')));
    assert_eq!(
        model.overlay,
        Overlay::None,
        "any key must dismiss the cheatsheet"
    );
}

#[test]
fn ac_cheat_1_esc_dismisses_cheatsheet() {
    let mut model = make_model();
    model.overlay = Overlay::Cheatsheet;

    update(&mut model, key_msg(Key::Esc));
    assert_eq!(model.overlay, Overlay::None);
}

// ─── AC-CHEAT-2: cheatsheet content from keymap ──────────────────────────────

#[test]
fn ac_cheat_2_list_keymap_has_required_bindings() {
    use cbox::tui::keymap::{keymap_for, KeyContext};
    let bindings = keymap_for(KeyContext::List);
    let keys: Vec<&str> = bindings.iter().map(|b| b.key).collect();

    for required in &["/", "?", "D", "t", "l", "c", "d", "s", "a", "e"] {
        assert!(
            keys.iter().any(|k| k.contains(required)),
            "List keymap must include key '{required}'"
        );
    }
}

// ─── AC-REBIND-1: uppercase D opens doctor ────────────────────────────────────

#[test]
fn ac_rebind_1_uppercase_d_opens_doctor() {
    use cbox::tui::model::Screen;
    let mut model = make_model();
    let effects = update(&mut model, key_msg(Key::Char('D')));
    assert_eq!(
        model.screen,
        Screen::DoctorPanel,
        "D must navigate to DoctorPanel"
    );
    assert!(
        effects.iter().any(|e| matches!(e, Effect::Doctor(_))),
        "D must emit Doctor effect"
    );
}

// ─── AC-REBIND-2: ? no longer opens doctor, opens cheatsheet ─────────────────

#[test]
fn ac_rebind_2_question_mark_opens_cheatsheet_not_doctor() {
    use cbox::tui::model::Screen;
    let mut model = make_model();
    let effects = update(&mut model, key_msg(Key::Char('?')));

    // Must NOT go to DoctorPanel.
    assert_ne!(
        model.screen,
        Screen::DoctorPanel,
        "? must NOT open DoctorPanel"
    );
    // Must NOT emit Doctor effect.
    assert!(
        !effects.iter().any(|e| matches!(e, Effect::Doctor(_))),
        "? must NOT emit Doctor effect"
    );
    // Must open cheatsheet.
    assert_eq!(
        model.overlay,
        Overlay::Cheatsheet,
        "? must open Overlay::Cheatsheet"
    );
}

// ─── AC-SKIN-CYCLE: skin cycle driven by 't' ─────────────────────────────────

#[test]
fn ac_skin_cycle_t_key_advances_skin() {
    let mut model = make_model();
    assert_eq!(model.skin, Skin::Kraft, "default skin must be Kraft");

    update(&mut model, key_msg(Key::Char('t')));
    assert_eq!(
        model.skin,
        Skin::Carbon,
        "after 1st 't', skin must be Carbon"
    );

    // Pressing 't' also pushes an Info toast with the new skin name.
    assert!(
        !model.toasts.is_empty(),
        "pressing 't' must push an Info toast"
    );
    assert_eq!(model.toasts.last().unwrap().kind, ToastKind::Info);
    assert!(
        model.toasts.last().unwrap().text.contains("carbon"),
        "toast text must contain the new skin name"
    );

    update(&mut model, key_msg(Key::Char('t')));
    assert_eq!(model.skin, Skin::Blueprint);

    update(&mut model, key_msg(Key::Char('t')));
    assert_eq!(model.skin, Skin::Kraft, "cycle must wrap back to Kraft");
}

// ─── AC-TOAST-1: TTL expiry on Tick ─────────────────────────────────────────

#[test]
fn ac_toast_1_expires_after_ttl() {
    let mut model = make_model();
    model.spinner_tick = 0;
    let ttl = TOAST_TTL_SUCCESS; // 60

    model.push_toast(ToastKind::Success, "box packed".to_string());
    assert_eq!(model.toasts.len(), 1, "toast must be present initially");

    // Apply (ttl - 1) Ticks: toast must still be present.
    for _ in 0..(ttl - 1) {
        update(&mut model, Message::Tick);
    }
    assert_eq!(
        model.toasts.len(),
        1,
        "toast must still be present after ttl-1 ticks"
    );

    // One more Tick: expires.
    update(&mut model, Message::Tick);
    assert!(
        model.toasts.is_empty(),
        "toast must have expired after ttl ticks"
    );
}

#[test]
fn ac_toast_1_error_ttl_longer() {
    let mut model = make_model();
    model.spinner_tick = 0;
    let ttl = TOAST_TTL_ERROR; // 120

    model.push_toast(ToastKind::Error, "something went wrong".to_string());

    // After 119 ticks: still present.
    for _ in 0..(ttl - 1) {
        update(&mut model, Message::Tick);
    }
    assert_eq!(
        model.toasts.len(),
        1,
        "error toast must survive ttl-1 ticks"
    );

    // After 1 more: expired.
    update(&mut model, Message::Tick);
    assert!(model.toasts.is_empty(), "error toast must expire at ttl");
}

// ─── AC-TOAST-2: completion handlers preserve StatusLine AND push toast ───────

#[test]
fn ac_toast_2_stop_done_ok_preserves_status_and_pushes_toast() {
    use cbox::tui::message::StopOutcome;
    let mut model = make_model();
    model.busy = true;

    let outcome = StopOutcome {
        stopped: vec!["web-dev".to_string()],
    };
    update(&mut model, Message::StopDone(Ok(outcome)));

    // StatusLine variant must be Ok (unchanged assertion from existing tests).
    assert!(
        matches!(model.status, StatusLine::Ok(_)),
        "status must be Ok after successful stop"
    );
    // Toast must also have been pushed.
    assert!(
        !model.toasts.is_empty(),
        "a Success toast must be pushed alongside StatusLine::Ok"
    );
    assert_eq!(model.toasts.last().unwrap().kind, ToastKind::Success);
}

#[test]
fn ac_toast_2_stop_done_err_sets_error_status_and_toast() {
    use cbox::error::CboxError;
    let mut model = make_model();
    model.busy = true;

    update(
        &mut model,
        Message::StopDone(Err(CboxError::software("stop failed"))),
    );

    assert!(
        matches!(model.status, StatusLine::Error(_)),
        "status must be Error after failed stop"
    );
    assert!(
        !model.toasts.is_empty(),
        "an Error toast must be pushed alongside StatusLine::Error"
    );
    assert_eq!(model.toasts.last().unwrap().kind, ToastKind::Error);
}

// ─── AC-TOAST-3: bounded queue drops oldest ──────────────────────────────────

#[test]
fn ac_toast_3_bounded_queue() {
    let mut model = make_model();

    // Push TOAST_MAX + 2 toasts.
    for i in 0..(TOAST_MAX + 2) {
        model.push_toast(ToastKind::Info, format!("toast {i}"));
    }

    assert!(
        model.toasts.len() <= TOAST_MAX,
        "toasts queue must not exceed TOAST_MAX={TOAST_MAX}"
    );
}

// ─── AC-TOAST-4: Busy does not push a toast ───────────────────────────────────

#[test]
fn ac_toast_4_busy_status_no_toast() {
    use cbox::tui::model::StatusLine;
    let mut model = make_model();

    // Trigger a refresh (sets StatusLine::Busy — no toast expected).
    update(&mut model, key_msg(Key::Char('r')));

    // Model is now busy with LoadList effect emitted.
    assert!(
        matches!(model.status, StatusLine::Busy(_)),
        "r key must set Busy status"
    );
    assert!(model.toasts.is_empty(), "Busy status must NOT push a toast");
}

// ─── AC-CMDLOG-1: bounded ring drops oldest ──────────────────────────────────

#[test]
fn ac_cmdlog_1_bounded_ring_drops_oldest() {
    let mut log = CmdLog::new(3);

    log.push("cmd1".to_string(), Some(0));
    log.push("cmd2".to_string(), Some(0));
    log.push("cmd3".to_string(), Some(0));
    log.push("cmd4".to_string(), Some(0)); // should drop cmd1
    log.push("cmd5".to_string(), Some(0)); // should drop cmd2

    assert_eq!(log.len(), 3, "log must not exceed cap=3");

    // The remaining entries must be the 3 newest (cmd3, cmd4, cmd5).
    let entries: Vec<_> = log.entries().map(|e| e.argv.clone()).collect();
    assert!(
        entries.contains(&"cmd3".to_string()),
        "cmd3 must be retained"
    );
    assert!(
        entries.contains(&"cmd4".to_string()),
        "cmd4 must be retained"
    );
    assert!(
        entries.contains(&"cmd5".to_string()),
        "cmd5 must be retained"
    );
    assert!(
        !entries.contains(&"cmd1".to_string()),
        "cmd1 must have been dropped"
    );
    assert!(
        !entries.contains(&"cmd2".to_string()),
        "cmd2 must have been dropped"
    );
}

// ─── AC-CMDLOG-2: decorator captures real argv + status ───────────────────────

#[test]
fn ac_cmdlog_2_decorator_captures_argv_and_status() {
    use cbox::dbox::mock::MockRunner;
    use cbox::dbox::runner::{DistroboxRunner, Invocation, RunMode};

    let inner = Arc::new(MockRunner::new());
    let log = Arc::new(Mutex::new(CmdLog::new(200)));
    let history = Arc::new(Mutex::new(HistoryStore::with_path(
        std::path::PathBuf::from("/dev/null"),
    )));
    let runner = LoggingRunner::new(inner, Arc::clone(&log), history);

    let inv = Invocation::new(
        "distrobox",
        vec![
            "create".to_string(),
            "--name".to_string(),
            "web".to_string(),
        ],
        RunMode::Capture,
    );
    let _ = runner.run(inv);

    let log = log.lock().unwrap();
    assert_eq!(log.len(), 1, "one entry must be logged");

    let entry = log.entries().next().unwrap();
    assert_eq!(
        entry.argv, "distrobox create --name web",
        "argv must be space-joined"
    );
    assert_eq!(
        entry.status,
        Some(0),
        "status must be Some(0) from MockRunner default"
    );
}

// ─── AC-CMDLOG-3: dry-run invocations are NOT logged ─────────────────────────

#[test]
fn ac_cmdlog_3_dry_run_not_logged() {
    use cbox::dbox::mock::MockRunner;
    use cbox::dbox::runner::{DistroboxRunner, Invocation, RunMode};

    let inner = Arc::new(MockRunner::new());
    let log = Arc::new(Mutex::new(CmdLog::new(200)));
    let history = Arc::new(Mutex::new(HistoryStore::with_path(
        std::path::PathBuf::from("/dev/null"),
    )));
    let runner = LoggingRunner::new(inner, Arc::clone(&log), history);

    let inv = Invocation::new("distrobox", vec!["create".to_string()], RunMode::DryRun);
    let _ = runner.run(inv);

    let log = log.lock().unwrap();
    assert_eq!(log.len(), 0, "dry-run must NOT be logged");
}

// ─── AC-CMDLOG-4: interactive spawn logs exit code ───────────────────────────

#[test]
fn ac_cmdlog_4_interactive_logs_exit_code() {
    use cbox::dbox::mock::MockRunner;
    use cbox::dbox::runner::{DistroboxRunner, Invocation, RunMode};

    let inner = Arc::new(MockRunner::new());
    let log = Arc::new(Mutex::new(CmdLog::new(200)));
    let history = Arc::new(Mutex::new(HistoryStore::with_path(
        std::path::PathBuf::from("/dev/null"),
    )));
    let runner = LoggingRunner::new(inner, Arc::clone(&log), history);

    let inv = Invocation::new(
        "distrobox",
        vec!["enter".to_string(), "mybox".to_string()],
        RunMode::Interactive,
    );
    let _ = runner.run_interactive(inv);

    let log = log.lock().unwrap();
    assert_eq!(log.len(), 1, "interactive spawn must log one entry");
    let entry = log.entries().next().unwrap();
    assert_eq!(entry.argv, "distrobox enter mybox");
    assert_eq!(
        entry.status,
        Some(0),
        "exit code must be Some(0) from MockRunner"
    );
}

// ─── AC-CMDLOG-5: overlay open/scroll/close ──────────────────────────────────

#[test]
fn ac_cmdlog_5_l_opens_command_log_overlay() {
    let mut model = make_model();
    update(&mut model, key_msg(Key::Char('l')));
    assert_eq!(
        model.overlay,
        Overlay::CommandLog { scroll: 0 },
        "l must open CommandLog overlay"
    );
}

#[test]
fn ac_cmdlog_5_esc_closes_command_log_overlay() {
    let mut model = make_model();
    model.overlay = Overlay::CommandLog { scroll: 0 };
    update(&mut model, key_msg(Key::Esc));
    assert_eq!(
        model.overlay,
        Overlay::None,
        "Esc must close CommandLog overlay"
    );
}

#[test]
fn ac_cmdlog_5_down_scrolls_command_log() {
    let mut model = make_model();
    // Pre-populate the log so scroll can advance.
    {
        let mut log = model.cmdlog.lock().unwrap();
        for i in 0..10 {
            log.push(format!("cmd{i}"), Some(0));
        }
    }
    model.overlay = Overlay::CommandLog { scroll: 0 };

    update(&mut model, key_msg(Key::Down));

    match model.overlay {
        Overlay::CommandLog { scroll } => {
            assert!(scroll > 0, "scroll must increase after Down");
        }
        _ => panic!("overlay must remain CommandLog"),
    }
}

// ─── Overlay precedence: esc closes overlay before screen-dispatch ────────────

#[test]
fn overlay_precedence_esc_closes_cheatsheet_not_app() {
    use cbox::tui::model::Screen;
    let mut model = make_model();
    model.overlay = Overlay::Cheatsheet;

    // Esc with cheatsheet open must NOT quit the app.
    update(&mut model, key_msg(Key::Esc));

    assert_eq!(model.overlay, Overlay::None, "cheatsheet must close");
    assert!(!model.should_quit, "app must NOT quit");
    assert_eq!(model.screen, Screen::List, "screen must remain List");
}

// ─── Filter ↔ overlay mutual exclusion ───────────────────────────────────────

#[test]
fn filter_closes_overlay_when_opening() {
    let mut model = make_model_with_boxes(&["web-dev", "api"]);
    // When the cheatsheet is open, the overlay pre-check (step 4) intercepts all keys.
    // The first '/' closes the cheatsheet; the second '/' opens the filter.
    // This is the expected key-precedence behavior (§4.3 of the spec).
    model.overlay = Overlay::Cheatsheet;

    // First key dismisses the cheatsheet.
    update(&mut model, key_msg(Key::Char('/')));
    assert_eq!(
        model.overlay,
        Overlay::None,
        "first key must close the cheatsheet overlay"
    );
    assert!(
        model.filter.is_none(),
        "filter must not open while the cheatsheet intercepted the key"
    );

    // Second '/' (with no overlay active) must open the filter.
    update(&mut model, key_msg(Key::Char('/')));
    assert!(model.filter.is_some(), "second '/' must open the filter");
}
