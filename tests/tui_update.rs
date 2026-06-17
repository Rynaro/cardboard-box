//! Reducer unit tests — pure `update(&mut Model, Message) -> Vec<Effect>`.
//! No terminal, no runner, no threads.
//! Covers all acceptance criteria from the spec §10 (reducer section).
#![cfg(feature = "tui")] // TUI internals only exist with the feature on.

use cbox::core::spec::{ApplyOutcome, ApplySummary, BoxRow, DiffResult, ProvisionStepResult};
use cbox::dbox::backend::Backend;
use cbox::tui::effect::Effect;
use cbox::tui::message::{Key, Message};
use cbox::tui::model::{Model, Screen, StatusLine, WizardStep};
use cbox::tui::update::update;

// ─── helpers ─────────────────────────────────────────────────────────────────

fn make_model() -> Model {
    Model::new(Backend::Podman)
}

fn make_running_box(name: &str) -> BoxRow {
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

fn make_stopped_box(name: &str) -> BoxRow {
    BoxRow {
        name: name.to_string(),
        status: "exited".to_string(),
        image: "fedora-toolbox:latest".to_string(),
        docker_mode: "none".to_string(),
        cbox_managed: true,
        id: "def456".to_string(),
        backend: "podman".to_string(),
    }
}

fn make_model_with_boxes(boxes: Vec<BoxRow>) -> Model {
    let mut m = make_model();
    m.boxes = boxes;
    m.selected = Some(0);
    m
}

fn key_msg(k: Key) -> Message {
    Message::Key(k)
}

// ─── AC-NAV-1 ────────────────────────────────────────────────────────────────

#[test]
fn ac_nav_1_down_moves_selection() {
    let boxes = vec![
        make_running_box("box-a"),
        make_running_box("box-b"),
        make_running_box("box-c"),
    ];
    let mut model = make_model_with_boxes(boxes);
    model.selected = Some(0);

    let effects = update(&mut model, key_msg(Key::Down));

    assert_eq!(model.selected, Some(1), "selection should advance to 1");
    assert!(effects.is_empty(), "nav should produce no effects");
}

#[test]
fn ac_nav_1_up_from_second() {
    let boxes = vec![make_running_box("box-a"), make_running_box("box-b")];
    let mut model = make_model_with_boxes(boxes);
    model.selected = Some(1);

    let effects = update(&mut model, key_msg(Key::Up));

    assert_eq!(model.selected, Some(0));
    assert!(effects.is_empty());
}

// ─── AC-NAV-2 ────────────────────────────────────────────────────────────────

#[test]
fn ac_nav_2_i_opens_detail() {
    let boxes = vec![make_running_box("web-dev")];
    let mut model = make_model_with_boxes(boxes);
    model.selected = Some(0);

    let effects = update(&mut model, key_msg(Key::Char('i')));

    assert_eq!(model.screen, Screen::Detail);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::LoadDetail(spec) if spec.name == "web-dev")),
        "should emit LoadDetail for web-dev"
    );
    assert!(model.busy, "should be busy while loading detail");
}

// ─── AC-LIST-1 ───────────────────────────────────────────────────────────────

#[test]
fn ac_list_1_list_loaded_ok() {
    let mut model = make_model();
    model.busy = true;

    let rows = vec![make_running_box("box-a"), make_stopped_box("box-b")];
    let effects = update(&mut model, Message::ListLoaded(Ok(rows.clone())));

    assert_eq!(model.boxes.len(), 2);
    assert!(!model.busy);
    assert!(matches!(model.status, StatusLine::Ok(_)));
    assert!(effects.is_empty());
}

// ─── AC-EMPTY-1 ──────────────────────────────────────────────────────────────

#[test]
fn ac_empty_1_empty_list() {
    let mut model = make_model();
    let effects = update(&mut model, Message::ListLoaded(Ok(vec![])));

    assert!(model.boxes.is_empty());
    assert!(model.is_empty_list());
    assert!(effects.is_empty());
}

// ─── AC-WIZ-1 ────────────────────────────────────────────────────────────────

#[test]
fn ac_wiz_1_type_name_and_tab_advances() {
    let mut model = make_model();
    model.screen = Screen::Wizard;
    model.wizard = Some(cbox::tui::model::WizardState::new());

    // Type "web-dev"
    for c in "web-dev".chars() {
        update(&mut model, key_msg(Key::Char(c)));
    }

    // Press Tab to advance.
    update(&mut model, key_msg(Key::Tab));

    let w = model.wizard.as_ref().expect("wizard should still exist");
    assert_eq!(w.step, WizardStep::Image, "step should advance to Image");
    assert_eq!(w.name, "web-dev");
}

// ─── AC-WIZ-2 ────────────────────────────────────────────────────────────────

#[test]
fn ac_wiz_2_confirm_step_submits() {
    let mut model = make_model();
    model.screen = Screen::Wizard;
    let mut w = cbox::tui::model::WizardState::new();
    w.name = "web-dev".to_string();
    w.step = WizardStep::Confirm;
    model.wizard = Some(w);

    let effects = update(&mut model, key_msg(Key::Enter));

    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::Create(spec) if spec.name == "web-dev")),
        "should emit Create(spec) with name=web-dev"
    );
    assert!(model.busy, "should be busy after submitting");
    // Wizard is consumed on submit.
    assert!(model.wizard.is_none());
}

// ─── AC-WIZ-3 ────────────────────────────────────────────────────────────────

#[test]
fn ac_wiz_3_invalid_name_no_advance() {
    let mut model = make_model();
    model.screen = Screen::Wizard;
    let mut w = cbox::tui::model::WizardState::new();
    w.name = "-bad".to_string(); // invalid: starts with '-'
    model.wizard = Some(w);

    update(&mut model, key_msg(Key::Tab));

    let wiz = model.wizard.as_ref().expect("wizard should still exist");
    assert_eq!(
        wiz.step,
        WizardStep::Name,
        "step should NOT advance on invalid name"
    );
    assert!(
        matches!(model.status, StatusLine::Error(_)),
        "status should be Error"
    );
}

// ─── AC-DESTROY-1 ────────────────────────────────────────────────────────────

#[test]
fn ac_destroy_1_d_opens_confirm() {
    let boxes = vec![make_stopped_box("web-dev")];
    let mut model = make_model_with_boxes(boxes);
    model.selected = Some(0);

    update(&mut model, key_msg(Key::Char('d')));

    assert_eq!(model.screen, Screen::ConfirmDestroy);
    let confirm = model.confirm.as_ref().expect("confirm should be set");
    assert_eq!(confirm.name, "web-dev");
}

// ─── AC-DESTROY-2 ────────────────────────────────────────────────────────────

#[test]
fn ac_destroy_2_y_emits_rm() {
    let mut model = make_model();
    model.screen = Screen::ConfirmDestroy;
    model.confirm = Some(cbox::tui::model::ConfirmState {
        name: "web-dev".to_string(),
        rm_home: false,
        backend: Backend::Podman,
    });

    let effects = update(&mut model, key_msg(Key::Char('y')));

    assert!(
        effects.iter().any(|e| matches!(
            e,
            Effect::Rm(spec)
            if spec.names == vec!["web-dev"] && spec.force && spec.yes && !spec.rm_home
        )),
        "should emit Rm(RmSpec{{names:[web-dev], force:true, yes:true, rm_home:false}})"
    );
    assert!(model.busy);
}

// ─── AC-DESTROY-3 ────────────────────────────────────────────────────────────

#[test]
fn ac_destroy_3_n_cancels() {
    let mut model = make_model();
    model.screen = Screen::ConfirmDestroy;
    model.confirm = Some(cbox::tui::model::ConfirmState {
        name: "web-dev".to_string(),
        rm_home: false,
        backend: Backend::Podman,
    });

    let effects = update(&mut model, key_msg(Key::Char('n')));

    assert_eq!(model.screen, Screen::List, "should return to List on 'n'");
    assert!(effects.is_empty(), "should produce no effects");
    assert!(model.confirm.is_none());
}

// ─── AC-ENTER-SELECT ─────────────────────────────────────────────────────────

#[test]
fn ac_enter_select_running_box() {
    let boxes = vec![make_running_box("web-dev")];
    let mut model = make_model_with_boxes(boxes);
    model.selected = Some(0);

    let effects = update(&mut model, key_msg(Key::Enter));

    assert!(
        effects.iter().any(|e| matches!(
            e,
            Effect::SuspendAndEnter(spec) if spec.name == "web-dev" && !spec.root
        )),
        "should emit SuspendAndEnter for running box (the interactive path)"
    );
}

// ─── AC-ENTER-STOPPED ────────────────────────────────────────────────────────

#[test]
fn ac_enter_stopped_box_opens_detail() {
    let boxes = vec![make_stopped_box("web-dev")];
    let mut model = make_model_with_boxes(boxes);
    model.selected = Some(0);

    let effects = update(&mut model, key_msg(Key::Enter));

    assert!(
        effects.iter().any(|e| matches!(e, Effect::LoadDetail(_))),
        "stopped box should open detail, not enter"
    );
    assert_eq!(model.screen, Screen::Detail);
}

// ─── AC-APPLY-1 ──────────────────────────────────────────────────────────────

#[test]
fn ac_apply_1_a_emits_apply() {
    let boxes = vec![make_stopped_box("web-dev")];
    let mut model = make_model_with_boxes(boxes);
    model.selected = Some(0);

    let effects = update(&mut model, key_msg(Key::Char('a')));

    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::Apply(spec) if spec.name == "web-dev" && !spec.recreate)),
        "should emit Apply spec for web-dev"
    );
    assert_eq!(model.screen, Screen::Progress);
    assert!(model.busy);
}

// ─── AC-APPLY-2 ──────────────────────────────────────────────────────────────

#[test]
fn ac_apply_2_apply_done_updates_progress() {
    let mut model = make_model();
    model.screen = Screen::Progress;
    model.busy = true;

    use cbox::tui::model::ProgressState;
    model.progress = Some(ProgressState {
        title: "Applying…".to_string(),
        steps: vec![],
        spinner_tick: 0,
        recreate_needed: false,
        recreate_msg: None,
        recreate_confirm: false,
        pending_spec: None,
    });

    let steps = vec![ProvisionStepResult {
        idx: 0,
        step_type: "run".to_string(),
        status: "ran".to_string(),
        hash: "abc".to_string(),
        duration_ms: 100,
        exit_code: Some(0),
        captured_stderr: String::new(),
        captured_stdout: String::new(),
        argv: Vec::new(),
    }];

    let outcome = ApplyOutcome {
        ok: true,
        action: "apply".to_string(),
        name: "web-dev".to_string(),
        diff: DiffResult {
            class: "Incremental".to_string(),
            fields: vec![],
        },
        recreate_required: false,
        steps: steps.clone(),
        summary: ApplySummary {
            ran: 1,
            skipped: 0,
            copied: 0,
            failed: 0,
        },
    };

    update(&mut model, Message::ApplyDone(Ok(outcome)));

    assert!(!model.busy, "should not be busy after ApplyDone");
    let progress = model.progress.as_ref().expect("progress should remain");
    assert_eq!(progress.steps.len(), 1);
    assert_eq!(progress.steps[0].status, "ran");
}

// ─── AC-APPLY-RECREATE ───────────────────────────────────────────────────────

#[test]
fn ac_apply_recreate_err_sets_confirm_modal() {
    use cbox::core::spec::ApplySpec;
    use cbox::error::CboxError;
    use cbox::tui::model::ProgressState;

    let apply_spec = ApplySpec {
        name: "web-dev".to_string(),
        boxfile_path: "/tmp/Boxfile.toml".to_string(),
        force: false,
        redo: vec![],
        no_provision: false,
        recreate: false,
        yes: true,
        dry_run: false,
        backend: Backend::Podman,
    };

    let mut model = make_model();
    model.screen = Screen::Progress;
    model.busy = true;
    model.progress = Some(ProgressState {
        title: "Applying…".to_string(),
        steps: vec![],
        spinner_tick: 0,
        recreate_needed: false,
        recreate_msg: None,
        recreate_confirm: false,
        pending_spec: Some(apply_spec.clone()),
    });

    // A DataErr triggers the recreate path.
    let err = CboxError::dataerr("web-dev needs a recreate: image changed");
    update(&mut model, Message::ApplyDone(Err(err)));

    let p = model.progress.as_ref().expect("progress should exist");
    assert!(p.recreate_needed, "recreate_needed should be set");
    assert!(p.recreate_confirm, "recreate_confirm should be set");

    // Now press 'y' to confirm recreate.
    let effects = update(&mut model, key_msg(Key::Char('y')));

    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::Apply(spec) if spec.recreate && spec.name == "web-dev")),
        "should emit Apply with recreate:true"
    );
}

// ─── AC-DOCTOR-AUTO ──────────────────────────────────────────────────────────

#[test]
fn ac_doctor_auto_tempfail_routes_to_doctor() {
    use cbox::error::CboxError;

    let mut model = make_model();

    let err = CboxError::tempfail("No backend found");
    let effects = update(&mut model, Message::ListLoaded(Err(err)));

    assert_eq!(
        model.screen,
        Screen::DoctorPanel,
        "TempFail should auto-route to DoctorPanel"
    );
    assert!(
        effects.iter().any(|e| matches!(e, Effect::Doctor(_))),
        "should also emit Doctor effect"
    );
    assert!(model.busy);
}

// ─── AC-BUSY-GATE ────────────────────────────────────────────────────────────

#[test]
fn ac_busy_gate_blocks_create() {
    let mut model = make_model();
    model.busy = true;

    let effects = update(&mut model, key_msg(Key::Char('c')));

    assert!(effects.is_empty(), "create should be blocked when busy");
    // Wizard should NOT have opened.
    assert!(model.wizard.is_none());
}

#[test]
fn ac_busy_gate_ctrl_c_still_quits() {
    let mut model = make_model();
    model.busy = true;

    let effects = update(&mut model, key_msg(Key::CtrlC));

    assert!(
        effects.iter().any(|e| matches!(e, Effect::Quit)),
        "Ctrl-C should always produce Quit even when busy"
    );
    assert!(model.should_quit);
}

// ─── AC-QUIT-1 ───────────────────────────────────────────────────────────────

#[test]
fn ac_quit_1_q_on_list_quits() {
    let mut model = make_model();
    model.screen = Screen::List;

    let effects = update(&mut model, key_msg(Key::Char('q')));

    assert!(
        effects.iter().any(|e| matches!(e, Effect::Quit)),
        "q on List should produce Quit"
    );
}

#[test]
fn ac_quit_1_esc_on_detail_backs_to_list() {
    let mut model = make_model();
    model.screen = Screen::Detail;

    let effects = update(&mut model, key_msg(Key::Esc));

    assert_eq!(
        model.screen,
        Screen::List,
        "Esc on Detail should go back to List, NOT quit"
    );
    assert!(
        !effects.iter().any(|e| matches!(e, Effect::Quit)),
        "Esc on Detail should NOT produce Quit"
    );
}

// ─── Extra: refresh triggers LoadList ────────────────────────────────────────

#[test]
fn refresh_r_emits_load_list() {
    let mut model = make_model();
    let effects = update(&mut model, key_msg(Key::Char('r')));
    assert!(effects.iter().any(|e| matches!(e, Effect::LoadList)));
    assert!(model.busy);
}

// ─── Extra: doctor panel ─────────────────────────────────────────────────────

#[test]
fn doctor_question_mark_opens_panel() {
    let mut model = make_model();
    let effects = update(&mut model, key_msg(Key::Char('?')));
    assert_eq!(model.screen, Screen::DoctorPanel);
    assert!(effects.iter().any(|e| matches!(e, Effect::Doctor(_))));
}

// ─── Extra: ListLoaded clamps selection ──────────────────────────────────────

#[test]
fn list_loaded_clamps_selection() {
    let mut model = make_model();
    model.selected = Some(5); // out of range

    update(
        &mut model,
        Message::ListLoaded(Ok(vec![make_running_box("only-one")])),
    );

    assert_eq!(model.selected, Some(0), "selection should be clamped to 0");
}

// ─── Extra: Tick advances spinner ────────────────────────────────────────────

#[test]
fn tick_advances_spinner() {
    let mut model = make_model();
    assert_eq!(model.spinner_tick, 0);
    update(&mut model, Message::Tick);
    assert_eq!(model.spinner_tick, 1);
}
