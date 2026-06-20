//! Bundle 2 "Living Box" acceptance-criteria tests.
//!
//! Tests pure helpers only (no real TTY, no distrobox). Mirrors the Bundle 1
//! style: GIVEN / WHEN / THEN over pure functions and reducers.
//!
//! This test file is compiled under `cfg(feature = "tui")` for the TUI-dependent
//! parts (update/model) while the pure-helper tests (bulk, poll, action, stats)
//! run unconditionally.
#![allow(dead_code)]

// ─────────────────────────────────────────────────────────────────────────────
// AC-TIMEOUT-1/2/3: Capture timeout seam
// ─────────────────────────────────────────────────────────────────────────────

use std::time::{Duration, Instant};

use cbox::dbox::runner::{CmdOutput, DistroboxRunner, Invocation, RunMode, RunnerError};
use cbox::error::exit;

/// A MockRunner whose `run()` blocks indefinitely (simulates a hung command).
/// Its `run_with_timeout` uses the DEFAULT impl (delegates to `run`) so the
/// test drives the REAL `RealRunner` watchdog via a slow command, OR we use
/// a HangingRunner that implements the watchdog itself for unit testing.
///
/// For AC-TIMEOUT-1 we need a runner that actually times out. The default
/// impl of `run_with_timeout` just delegates to `run`, so the mock can't
/// test the watchdog. The watchdog is in `RealRunner`. Instead, we test the
/// timeout path with a real process (sleep/cat) or a purpose-built runner.
///
/// Here we build a `HangingRunner` that overrides `run_with_timeout` with a
/// pure timeout implementation using a channel that never fires.
struct HangingRunner;

impl DistroboxRunner for HangingRunner {
    fn run(&self, inv: Invocation) -> Result<CmdOutput, RunnerError> {
        // Simulate an infinite block.
        std::thread::sleep(Duration::from_secs(3600));
        Ok(CmdOutput {
            status: 0,
            stdout: String::new(),
            stderr: String::new(),
            argv: inv.argv(),
        })
    }

    fn run_interactive(&self, _inv: Invocation) -> Result<i32, RunnerError> {
        std::thread::sleep(Duration::from_secs(3600));
        Ok(0)
    }

    /// Override run_with_timeout with a real watchdog over `run` via a thread.
    ///
    /// This lets us test the timeout behavior without needing a real process —
    /// we spawn a thread running `self.run()`, wait for the deadline, and return
    /// `Timeout` if the thread hasn't finished.
    fn run_with_timeout(
        &self,
        inv: Invocation,
        timeout: Duration,
    ) -> Result<CmdOutput, RunnerError> {
        let program = inv.program.clone();
        let (tx, rx) = std::sync::mpsc::channel::<Result<CmdOutput, RunnerError>>();
        let inv_clone = inv.clone();
        std::thread::spawn(move || {
            // The run() above sleeps forever. The tx will be dropped when the
            // thread exits (after a very long sleep), so recv_timeout below will
            // see a timeout rather than a result.
            let _ = tx.send(HangingRunner.run(inv_clone));
        });

        match rx.recv_timeout(timeout) {
            Ok(result) => result,
            Err(_) => Err(RunnerError::Timeout {
                program,
                seconds: timeout.as_secs(),
            }),
        }
    }
}

#[test]
fn ac_timeout_1_hung_runner_returns_error_within_deadline() {
    let runner = HangingRunner;
    let inv = Invocation::new("hang", vec![], RunMode::Capture);
    let timeout = Duration::from_millis(200);

    let start = Instant::now();
    let result = runner.run_with_timeout(inv, timeout);
    let elapsed = start.elapsed();

    // Must return (not hang forever).
    assert!(
        elapsed < Duration::from_secs(2),
        "run_with_timeout must return within 2s (elapsed: {elapsed:?})"
    );

    // Must return a Timeout error.
    match result {
        Err(RunnerError::Timeout { .. }) => {}
        other => panic!("Expected Timeout error, got: {other:?}"),
    }
}

#[test]
fn ac_timeout_2_default_impl_delegates_to_run() {
    // The plain MockRunner uses the default `run_with_timeout` impl (delegates to `run`).
    use cbox::dbox::mock::{MockResponse, MockRunner};

    let runner = MockRunner::new().with_default(MockResponse::ok("hello"));
    let inv = Invocation::new("test", vec![], RunMode::Capture);
    let result = runner.run_with_timeout(inv, Duration::from_secs(1));
    match result {
        Ok(out) => assert_eq!(out.stdout, "hello"),
        Err(e) => panic!("Default impl should not return an error: {e}"),
    }
}

#[test]
fn ac_timeout_3_exit_mapping() {
    let err = RunnerError::Timeout {
        program: "test".to_string(),
        seconds: 5,
    };
    assert_eq!(
        err.exit_code(),
        exit::TEMPFAIL,
        "RunnerError::Timeout must map to TEMPFAIL (75)"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-POLL-1/2/3/4/5: Silent-poll gating
// ─────────────────────────────────────────────────────────────────────────────

use cbox::tui::poll::{should_poll, PollGate, PollKind, POLL_INTERVAL_TICKS};

fn make_gate(
    spinner_tick: usize,
    last_poll_tick: usize,
    busy: bool,
    poll_in_flight: bool,
) -> PollGate {
    PollGate {
        spinner_tick,
        last_poll_tick,
        busy,
        poll_in_flight,
        detail_running: None,
    }
}

#[test]
fn ac_poll_1_fires_at_interval() {
    // Below the interval: no poll.
    let gate_early = make_gate(POLL_INTERVAL_TICKS - 1, 0, false, false);
    assert_eq!(
        should_poll(&gate_early),
        None,
        "should not fire before interval"
    );

    // At exactly the interval: poll.
    let gate_at = make_gate(POLL_INTERVAL_TICKS, 0, false, false);
    assert_eq!(
        should_poll(&gate_at),
        Some(PollKind::List),
        "should fire at POLL_INTERVAL_TICKS"
    );
}

#[test]
fn ac_poll_2_skips_while_busy() {
    let gate = make_gate(POLL_INTERVAL_TICKS * 2, 0, true, false);
    assert_eq!(
        should_poll(&gate),
        None,
        "should not poll while busy (AC-POLL-2)"
    );
}

#[test]
fn ac_poll_3_coalesces_in_flight() {
    let gate = make_gate(POLL_INTERVAL_TICKS * 2, 0, false, true);
    assert_eq!(
        should_poll(&gate),
        None,
        "should not poll while in-flight (AC-POLL-3)"
    );
}

#[test]
fn ac_poll_stats_when_detail_running() {
    let gate = PollGate {
        spinner_tick: POLL_INTERVAL_TICKS,
        last_poll_tick: 0,
        busy: false,
        poll_in_flight: false,
        detail_running: Some(("abc123".to_string(), "podman".to_string())),
    };
    match should_poll(&gate) {
        Some(PollKind::Stats { id, backend }) => {
            assert_eq!(id, "abc123");
            assert_eq!(backend, "podman");
        }
        other => panic!("Expected Stats poll, got: {other:?}"),
    }
}

#[test]
fn ac_poll_no_stats_for_stopped_box() {
    // detail_running = None means stopped box → expect List poll (not Stats).
    let gate = PollGate {
        spinner_tick: POLL_INTERVAL_TICKS,
        last_poll_tick: 0,
        busy: false,
        poll_in_flight: false,
        detail_running: None,
    };
    assert_eq!(should_poll(&gate), Some(PollKind::List));
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-POLL-4/5: silent completion handlers (reducer)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "tui")]
mod poll_reducer_tests {
    use cbox::core::spec::BoxRow;
    use cbox::dbox::backend::Backend;
    use cbox::error::CboxError;
    use cbox::tui::message::Message;
    use cbox::tui::model::{Model, StatusLine};
    use cbox::tui::update::update;

    fn make_row(name: &str) -> BoxRow {
        BoxRow {
            name: name.to_string(),
            status: "running".to_string(),
            image: "test".to_string(),
            docker_mode: "none".to_string(),
            cbox_managed: true,
            id: name.to_string(),
            backend: "podman".to_string(),
        }
    }

    #[test]
    fn ac_poll_4_silent_ok_does_not_set_busy_or_status() {
        let mut model = Model::new(Backend::Podman);
        model.poll_in_flight = true;
        model.status = StatusLine::Ok("original status".to_string());
        let rows = vec![make_row("box1"), make_row("box2")];

        let effects = update(&mut model, Message::SilentListLoaded(Ok(rows)));

        assert!(!model.poll_in_flight, "poll_in_flight must be cleared");
        assert!(!model.busy, "busy must NOT be set by silent completion");
        // Status must be UNCHANGED (not updated to Ok with new count).
        assert_eq!(
            model.status,
            StatusLine::Ok("original status".to_string()),
            "status must not be clobbered by silent refresh"
        );
        assert!(
            model.toasts.is_empty(),
            "silent refresh must not push a toast"
        );
        assert!(
            effects.is_empty(),
            "silent completion must return no effects"
        );
        assert_eq!(model.boxes.len(), 2);
    }

    #[test]
    fn ac_poll_4_silent_err_clears_in_flight_only() {
        let mut model = Model::new(Backend::Podman);
        model.poll_in_flight = true;
        model.status = StatusLine::Ok("original".to_string());

        let effects = update(
            &mut model,
            Message::SilentListLoaded(Err(CboxError::usage("timeout"))),
        );

        assert!(
            !model.poll_in_flight,
            "poll_in_flight must be cleared on err"
        );
        assert!(!model.busy, "busy must not be set");
        assert_eq!(
            model.status,
            StatusLine::Ok("original".to_string()),
            "status must be unchanged on silent err"
        );
        assert!(effects.is_empty(), "silent err must return no effects");
    }

    #[test]
    fn ac_poll_5_selection_clamped_after_silent_refresh() {
        let mut model = Model::new(Backend::Podman);
        model.boxes = vec![make_row("a"), make_row("b"), make_row("c")];
        model.selected = Some(2); // currently last
        model.poll_in_flight = true;

        // Silent refresh with only 1 row — selection must clamp.
        let new_rows = vec![make_row("only")];
        update(&mut model, Message::SilentListLoaded(Ok(new_rows)));

        assert_eq!(model.boxes.len(), 1);
        assert!(
            model
                .selected
                .map(|i| i < model.boxes.len())
                .unwrap_or(false),
            "selection must be clamped within bounds after silent refresh"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-ACTION-1/2/3: Action enum + dispatch
// ─────────────────────────────────────────────────────────────────────────────

use cbox::tui::action::{palette_actions, Action, ALL_ACTIONS, BULK_ACTIONS};

#[test]
fn ac_action_2_palette_source_includes_bulk_excludes_nav() {
    let actions = palette_actions();
    assert!(actions.contains(&Action::BulkPruneStopped));
    assert!(actions.contains(&Action::BulkStopRunning));
    assert!(actions.contains(&Action::BulkDestroyManaged));
    assert!(actions.contains(&Action::BulkDestroyUnmanaged));
    assert!(!actions.contains(&Action::MoveUp));
    assert!(!actions.contains(&Action::MoveDown));
}

#[test]
fn ac_action_2_bulk_actions_is_exactly_four() {
    assert_eq!(BULK_ACTIONS.len(), 4);
    assert!(BULK_ACTIONS.contains(&Action::BulkPruneStopped));
    assert!(BULK_ACTIONS.contains(&Action::BulkStopRunning));
    assert!(BULK_ACTIONS.contains(&Action::BulkDestroyManaged));
    assert!(BULK_ACTIONS.contains(&Action::BulkDestroyUnmanaged));
}

#[test]
fn ac_action_labels_all_non_empty() {
    for action in ALL_ACTIONS {
        assert!(
            !action.label().is_empty(),
            "{action:?}.label() must not be empty"
        );
    }
}

#[cfg(feature = "tui")]
mod action_dispatch_tests {
    use cbox::dbox::backend::Backend;
    use cbox::tui::action::Action;
    use cbox::tui::effect::Effect;
    use cbox::tui::model::{Model, Overlay, StatusLine};
    use cbox::tui::update::dispatch_action;

    #[test]
    fn ac_action_3_dispatch_refresh() {
        let mut model = Model::new(Backend::Podman);
        let effects = dispatch_action(&mut model, Action::Refresh);
        assert!(model.busy, "Refresh must set busy=true");
        assert_eq!(model.status, StatusLine::Busy("Refreshing…".to_string()));
        assert!(
            matches!(effects.as_slice(), [Effect::LoadList]),
            "Refresh must return [Effect::LoadList]"
        );
    }

    #[test]
    fn ac_action_3_dispatch_cheatsheet() {
        let mut model = Model::new(Backend::Podman);
        let effects = dispatch_action(&mut model, Action::Cheatsheet);
        assert_eq!(model.overlay, Overlay::Cheatsheet);
        assert!(effects.is_empty(), "Cheatsheet must return no effects");
    }

    #[test]
    fn ac_action_3_dispatch_quit() {
        let mut model = Model::new(Backend::Podman);
        let effects = dispatch_action(&mut model, Action::Quit);
        assert!(model.should_quit, "Quit must set should_quit");
        assert!(
            matches!(effects.as_slice(), [Effect::Quit]),
            "Quit must return [Effect::Quit]"
        );
    }

    #[test]
    fn ac_action_3_dispatch_bulk_stop_running_opens_confirm() {
        use cbox::core::spec::BoxRow;
        let mut model = Model::new(Backend::Podman);
        // Add a running box.
        model.boxes = vec![BoxRow {
            name: "runner".to_string(),
            status: "running".to_string(),
            image: "test".to_string(),
            docker_mode: "none".to_string(),
            cbox_managed: true,
            id: "abc".to_string(),
            backend: "podman".to_string(),
        }];

        let effects = dispatch_action(&mut model, Action::BulkStopRunning);

        // With a running box, bulk confirm modal should open.
        assert!(
            model.bulk_confirm.is_some(),
            "BulkStopRunning must open bulk_confirm"
        );
        let bc = model.bulk_confirm.as_ref().unwrap();
        assert!(bc.targets.contains(&"runner".to_string()));
        assert!(effects.is_empty(), "Opening confirm returns no effects");
    }

    #[test]
    fn ac_action_3_dispatch_palette_opens_overlay() {
        let mut model = Model::new(Backend::Podman);
        dispatch_action(&mut model, Action::Palette);
        assert!(
            matches!(
                model.overlay,
                Overlay::Palette {
                    bulk_only: false,
                    ..
                }
            ),
            "Palette action must open palette overlay with bulk_only=false"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-PALETTE-1/2/3/4: Command palette
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "tui")]
mod palette_tests {
    use cbox::dbox::backend::Backend;
    use cbox::tui::message::{Key, Message};
    use cbox::tui::model::{Model, Overlay};
    use cbox::tui::update::update;

    #[test]
    fn ac_palette_1_colon_opens_palette() {
        let mut model = Model::new(Backend::Podman);
        update(&mut model, Message::Key(Key::Char(':')));
        assert!(
            matches!(
                model.overlay,
                Overlay::Palette {
                    bulk_only: false,
                    ..
                }
            ),
            "':' must open palette with bulk_only=false"
        );
    }

    #[test]
    fn ac_palette_1_esc_closes_palette() {
        let mut model = Model::new(Backend::Podman);
        update(&mut model, Message::Key(Key::Char(':')));
        assert!(matches!(model.overlay, Overlay::Palette { .. }));
        update(&mut model, Message::Key(Key::Esc));
        assert_eq!(
            model.overlay,
            cbox::tui::model::Overlay::None,
            "Esc must close palette"
        );
    }

    #[test]
    fn ac_palette_4_b_opens_bulk_scoped_palette() {
        let mut model = Model::new(Backend::Podman);
        update(&mut model, Message::Key(Key::Char('b')));
        assert!(
            matches!(
                model.overlay,
                Overlay::Palette {
                    bulk_only: true,
                    ..
                }
            ),
            "'b' must open palette with bulk_only=true"
        );
    }

    #[test]
    fn ac_palette_4_bulk_scoped_has_four_matches() {
        let mut model = Model::new(Backend::Podman);
        update(&mut model, Message::Key(Key::Char('b')));
        if let Overlay::Palette {
            matches, bulk_only, ..
        } = &model.overlay
        {
            assert!(*bulk_only, "must be bulk_only");
            assert_eq!(matches.len(), 4, "bulk palette must have exactly 4 matches");
        } else {
            panic!("expected Palette overlay");
        }
    }

    #[test]
    fn ac_palette_2_typing_narrows_matches() {
        use cbox::tui::action::palette_actions;
        let mut model = Model::new(Backend::Podman);
        update(&mut model, Message::Key(Key::Char(':')));

        // Type "stop" — should narrow to actions containing "stop".
        for c in "stop".chars() {
            update(&mut model, Message::Key(Key::Char(c)));
        }

        if let Overlay::Palette { matches, query, .. } = &model.overlay {
            assert_eq!(query, "stop");
            // Must have at least one match (Stop, BulkStopRunning).
            assert!(
                !matches.is_empty(),
                "typing 'stop' must produce at least one match"
            );
            // Verify all returned indices are within palette range.
            let source = palette_actions();
            for &idx in matches {
                assert!(
                    idx < source.len(),
                    "match index {idx} out of bounds (source len {})",
                    source.len()
                );
            }
        } else {
            panic!("expected Palette overlay after typing");
        }
    }

    #[test]
    fn ac_palette_3_enter_dispatches_cheatsheet() {
        use cbox::tui::action::{palette_actions, Action};
        let mut model = Model::new(Backend::Podman);
        update(&mut model, Message::Key(Key::Char(':')));

        // Find the index of Action::Cheatsheet in the palette.
        let source = palette_actions();
        let cheatsheet_idx = source
            .iter()
            .position(|a| *a == Action::Cheatsheet)
            .expect("Cheatsheet must be in palette");

        // Navigate to it: the cursor starts at 0, matches = all indices in order.
        // We just set cursor via Down keys or directly test that Enter on the
        // currently-selected item (cursor=0) dispatches the right action.
        // More robustly: if Cheatsheet is at position N, navigate Down N times.
        for _ in 0..cheatsheet_idx {
            update(&mut model, Message::Key(Key::Down));
        }

        update(&mut model, Message::Key(Key::Enter));

        assert_eq!(
            model.overlay,
            cbox::tui::model::Overlay::Cheatsheet,
            "Enter on Cheatsheet must open the cheatsheet overlay"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-BULK-FILTER-1: predicate correctness
// ─────────────────────────────────────────────────────────────────────────────

use cbox::core::spec::BoxRow;
use cbox::tui::bulk::{bulk_targets, BulkOp};

fn make_test_boxes() -> Vec<BoxRow> {
    vec![
        BoxRow {
            name: "a".to_string(),
            status: "running".to_string(),
            image: "img".to_string(),
            docker_mode: "none".to_string(),
            cbox_managed: true,
            id: "a".to_string(),
            backend: "podman".to_string(),
        },
        BoxRow {
            name: "b".to_string(),
            status: "stopped".to_string(),
            image: "img".to_string(),
            docker_mode: "none".to_string(),
            cbox_managed: true,
            id: "b".to_string(),
            backend: "podman".to_string(),
        },
        BoxRow {
            name: "c".to_string(),
            status: "Up 2 hours".to_string(),
            image: "img".to_string(),
            docker_mode: "none".to_string(),
            cbox_managed: false,
            id: "c".to_string(),
            backend: "docker".to_string(),
        },
        BoxRow {
            name: "d".to_string(),
            status: "exited".to_string(),
            image: "img".to_string(),
            docker_mode: "none".to_string(),
            cbox_managed: false,
            id: "d".to_string(),
            backend: "docker".to_string(),
        },
    ]
}

#[test]
fn ac_bulk_filter_1_prune_stopped_selects_b_and_d() {
    let boxes = make_test_boxes();
    let indices = bulk_targets(BulkOp::PruneStopped, &boxes);
    let names: Vec<&str> = indices.iter().map(|&i| boxes[i].name.as_str()).collect();
    assert!(
        names.contains(&"b"),
        "PruneStopped must include 'b' (stopped, managed)"
    );
    assert!(
        names.contains(&"d"),
        "PruneStopped must include 'd' (stopped, unmanaged)"
    );
    assert!(
        !names.contains(&"a"),
        "PruneStopped must not include 'a' (running)"
    );
    assert!(
        !names.contains(&"c"),
        "PruneStopped must not include 'c' (running)"
    );
}

#[test]
fn ac_bulk_filter_1_stop_running_selects_a_and_c() {
    let boxes = make_test_boxes();
    let indices = bulk_targets(BulkOp::StopRunning, &boxes);
    let names: Vec<&str> = indices.iter().map(|&i| boxes[i].name.as_str()).collect();
    assert!(
        names.contains(&"a"),
        "StopRunning must include 'a' (running, managed)"
    );
    assert!(
        names.contains(&"c"),
        "StopRunning must include 'c' (running, unmanaged)"
    );
    assert!(!names.contains(&"b"));
    assert!(!names.contains(&"d"));
}

#[test]
fn ac_bulk_filter_1_destroy_managed_selects_a_and_b() {
    let boxes = make_test_boxes();
    let indices = bulk_targets(BulkOp::DestroyManaged, &boxes);
    let names: Vec<&str> = indices.iter().map(|&i| boxes[i].name.as_str()).collect();
    assert!(
        names.contains(&"a"),
        "DestroyManaged must include 'a' (cbox_managed)"
    );
    assert!(
        names.contains(&"b"),
        "DestroyManaged must include 'b' (cbox_managed)"
    );
    assert!(!names.contains(&"c"));
    assert!(!names.contains(&"d"));
}

#[test]
fn ac_bulk_filter_1_destroy_unmanaged_selects_c_and_d() {
    let boxes = make_test_boxes();
    let indices = bulk_targets(BulkOp::DestroyUnmanaged, &boxes);
    let names: Vec<&str> = indices.iter().map(|&i| boxes[i].name.as_str()).collect();
    assert!(
        names.contains(&"c"),
        "DestroyUnmanaged must include 'c' (!cbox_managed)"
    );
    assert!(
        names.contains(&"d"),
        "DestroyUnmanaged must include 'd' (!cbox_managed)"
    );
    assert!(!names.contains(&"a"));
    assert!(!names.contains(&"b"));
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-BULK-EMPTY: no targets → no modal, info toast
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "tui")]
mod bulk_confirm_tests {
    use cbox::core::spec::BoxRow;
    use cbox::dbox::backend::Backend;
    use cbox::tui::action::Action;
    use cbox::tui::message::{Key, Message};
    use cbox::tui::model::Model;
    use cbox::tui::strings;
    use cbox::tui::update::{dispatch_action, update};

    fn stopped_box(name: &str) -> BoxRow {
        BoxRow {
            name: name.to_string(),
            status: "stopped".to_string(),
            image: "img".to_string(),
            docker_mode: "none".to_string(),
            cbox_managed: true,
            id: name.to_string(),
            backend: "podman".to_string(),
        }
    }

    fn running_unmanaged(name: &str) -> BoxRow {
        BoxRow {
            name: name.to_string(),
            status: "running".to_string(),
            image: "img".to_string(),
            docker_mode: "none".to_string(),
            cbox_managed: false,
            id: name.to_string(),
            backend: "podman".to_string(),
        }
    }

    #[test]
    fn ac_bulk_empty_no_stopped_boxes_gives_toast() {
        let mut model = Model::new(Backend::Podman);
        // All running — no stopped boxes to prune.
        model.boxes = vec![BoxRow {
            name: "runner".to_string(),
            status: "running".to_string(),
            image: "img".to_string(),
            docker_mode: "none".to_string(),
            cbox_managed: true,
            id: "r".to_string(),
            backend: "podman".to_string(),
        }];

        dispatch_action(&mut model, Action::BulkPruneStopped);

        assert!(
            model.bulk_confirm.is_none(),
            "No stopped boxes → bulk_confirm must stay None"
        );
        assert!(
            !model.toasts.is_empty(),
            "Must push an info toast when there are no targets"
        );
    }

    // ─── AC-BULK-DANGER-1: typed phrase required ──────────────────────────────

    #[test]
    fn ac_bulk_danger_1_y_key_does_not_execute_without_phrase() {
        let mut model = Model::new(Backend::Podman);
        model.boxes = vec![running_unmanaged("foreign")];
        dispatch_action(&mut model, Action::BulkDestroyUnmanaged);
        assert!(model.bulk_confirm.is_some(), "must open bulk confirm modal");

        // Press 'y' — must NOT execute (wrong phrase).
        let effects = update(&mut model, Message::Key(Key::Char('y')));
        assert!(
            model.bulk_confirm.is_some(),
            "'y' alone must not execute (typed_confirm != phrase)"
        );
        assert!(
            effects.is_empty(),
            "'y' alone must return no effects for dangerous op"
        );
    }

    #[test]
    fn ac_bulk_danger_1_enter_with_wrong_phrase_does_not_execute() {
        let mut model = Model::new(Backend::Podman);
        model.boxes = vec![running_unmanaged("foreign")];
        dispatch_action(&mut model, Action::BulkDestroyUnmanaged);

        // Type wrong phrase.
        for c in "WRONG PHRASE".chars() {
            update(&mut model, Message::Key(Key::Char(c)));
        }
        let effects = update(&mut model, Message::Key(Key::Enter));
        assert!(
            model.bulk_confirm.is_some(),
            "Enter with wrong phrase must keep modal open"
        );
        assert!(effects.is_empty(), "wrong phrase must return no effects");
    }

    #[test]
    fn ac_bulk_danger_1_correct_phrase_and_enter_executes() {
        let mut model = Model::new(Backend::Podman);
        model.boxes = vec![running_unmanaged("foreign")];
        dispatch_action(&mut model, Action::BulkDestroyUnmanaged);

        // Type the exact phrase.
        for c in strings::BULK_UNMANAGED_PHRASE.chars() {
            update(&mut model, Message::Key(Key::Char(c)));
        }
        let effects = update(&mut model, Message::Key(Key::Enter));
        assert!(
            model.bulk_confirm.is_none(),
            "Correct phrase + Enter must close the modal"
        );
        assert!(
            !effects.is_empty(),
            "Correct phrase + Enter must return fan-out effects"
        );
    }

    // ─── AC-BULK-DANGER-2: single char only appends ───────────────────────────

    #[test]
    fn ac_bulk_danger_2_single_char_only_appends_to_typed_confirm() {
        let mut model = Model::new(Backend::Podman);
        model.boxes = vec![running_unmanaged("foreign")];
        dispatch_action(&mut model, Action::BulkDestroyUnmanaged);
        assert!(model.bulk_confirm.is_some());

        // Single char 'D' must only append.
        update(&mut model, Message::Key(Key::Char('D')));
        let bc = model.bulk_confirm.as_ref().unwrap();
        assert_eq!(
            bc.typed_confirm, "D",
            "Single char must append to typed_confirm"
        );
        // Modal must still be open.
        assert!(
            model.bulk_confirm.is_some(),
            "Modal must remain open after single char"
        );
    }

    // ─── AC-BULK-CONFIRM-NONDANGEROUS: y/esc on safe ops ─────────────────────

    #[test]
    fn ac_bulk_confirm_nondangerous_y_executes() {
        let mut model = Model::new(Backend::Podman);
        model.boxes = vec![BoxRow {
            name: "managed".to_string(),
            status: "running".to_string(),
            image: "img".to_string(),
            docker_mode: "none".to_string(),
            cbox_managed: true,
            id: "m".to_string(),
            backend: "podman".to_string(),
        }];
        dispatch_action(&mut model, Action::BulkStopRunning);
        assert!(model.bulk_confirm.is_some());

        let effects = update(&mut model, Message::Key(Key::Char('y')));
        assert!(
            model.bulk_confirm.is_none(),
            "'y' on non-dangerous op must confirm and close modal"
        );
        assert!(
            !effects.is_empty(),
            "'y' on non-dangerous op must return effects"
        );
    }

    #[test]
    fn ac_bulk_confirm_nondangerous_esc_cancels() {
        let mut model = Model::new(Backend::Podman);
        model.boxes = vec![stopped_box("managed")];
        dispatch_action(&mut model, Action::BulkPruneStopped);
        assert!(model.bulk_confirm.is_some());

        let effects = update(&mut model, Message::Key(Key::Esc));
        assert!(
            model.bulk_confirm.is_none(),
            "Esc must cancel the non-dangerous bulk confirm"
        );
        assert!(effects.is_empty(), "Cancel must return no effects");
    }

    // ─── AC-BULK-FANOUT-1: grouped by backend ─────────────────────────────────

    #[test]
    fn ac_bulk_fanout_1_grouped_by_backend() {
        use cbox::tui::effect::Effect;

        let mut model = Model::new(Backend::Podman);
        // Two stopped boxes on different backends.
        model.boxes = vec![
            BoxRow {
                name: "pod-box".to_string(),
                status: "stopped".to_string(),
                image: "img".to_string(),
                docker_mode: "none".to_string(),
                cbox_managed: true,
                id: "p".to_string(),
                backend: "podman".to_string(),
            },
            BoxRow {
                name: "doc-box".to_string(),
                status: "stopped".to_string(),
                image: "img".to_string(),
                docker_mode: "none".to_string(),
                cbox_managed: true,
                id: "d".to_string(),
                backend: "docker".to_string(),
            },
        ];

        dispatch_action(&mut model, Action::BulkPruneStopped);
        assert!(model.bulk_confirm.is_some());

        let effects = update(&mut model, Message::Key(Key::Char('y')));

        // Must have confirmed.
        assert!(model.bulk_confirm.is_none());
        // ≤ 2 effects (one per backend group, under the 4-deep channel).
        assert!(
            effects.len() <= 2,
            "fan-out must emit ≤2 effects (got {})",
            effects.len()
        );
        assert!(!effects.is_empty(), "fan-out must emit at least one effect");

        // All effects must be Rm.
        for eff in &effects {
            assert!(
                matches!(eff, Effect::Rm(_)),
                "bulk prune must use Effect::Rm"
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-STATS-ARGV-1: build_stats_argv
// ─────────────────────────────────────────────────────────────────────────────

use cbox::dbox::argv::build_stats_argv;

#[test]
fn ac_stats_argv_1_correct_args() {
    let args = build_stats_argv("abc123");
    assert_eq!(
        args,
        vec!["stats", "abc123", "--no-stream", "--format", "json"],
        "build_stats_argv must produce the correct argv"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-STATS-PARSE-1/2/3: parse_stats_json
// ─────────────────────────────────────────────────────────────────────────────

use cbox::core::parse_stats_json;

#[test]
fn ac_stats_parse_1_podman_shaped() {
    let json = r#"[{"CPU":"12.5%","MemUsage":"240MiB / 1944MiB"}]"#;
    let sample = parse_stats_json(json).expect("podman-shaped stats must parse");
    assert!(
        (sample.cpu_pct - 12.5).abs() < 0.01,
        "cpu_pct must be ~12.5, got {}",
        sample.cpu_pct
    );
    let expected_used = 240u64 * 1024 * 1024;
    let tolerance = 1024 * 1024; // 1 MiB
    assert!(
        (sample.mem_used as i64 - expected_used as i64).unsigned_abs() <= tolerance,
        "mem_used must be ~240MiB ({}), got {}",
        expected_used,
        sample.mem_used
    );
}

#[test]
fn ac_stats_parse_2_docker_shaped() {
    let json = r#"{"CPUPerc":"3.1%","MemUsage":"12MiB / 512MiB"}"#;
    let sample = parse_stats_json(json).expect("docker-shaped stats must parse");
    assert!(
        (sample.cpu_pct - 3.1).abs() < 0.01,
        "cpu_pct must be ~3.1, got {}",
        sample.cpu_pct
    );
    let expected_used = 12u64 * 1024 * 1024;
    let tolerance = 1024 * 1024;
    assert!(
        (sample.mem_used as i64 - expected_used as i64).unsigned_abs() <= tolerance,
        "mem_used must be ~12MiB, got {}",
        sample.mem_used
    );
}

#[test]
fn ac_stats_parse_3_empty_returns_err() {
    assert!(
        parse_stats_json("").is_err(),
        "empty string must return Err"
    );
}

#[test]
fn ac_stats_parse_3_null_returns_err() {
    assert!(parse_stats_json("null").is_err(), "'null' must return Err");
}

#[test]
fn ac_stats_parse_3_empty_array_returns_err() {
    assert!(parse_stats_json("[]").is_err(), "'[]' must return Err");
}

#[test]
fn ac_stats_parse_3_malformed_returns_err_no_panic() {
    assert!(
        parse_stats_json("not json at all").is_err(),
        "malformed must return Err"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-HIST-1: bounded ring drops oldest
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "tui")]
mod history_tests {
    use cbox::core::spec::StatsSample;
    use cbox::tui::model::{StatsHistory, STATS_HISTORY_CAP};

    #[test]
    fn ac_hist_1_bounded_ring_drops_oldest() {
        let mut h = StatsHistory::new("box1");
        for i in 0..(STATS_HISTORY_CAP + 5) {
            h.push_sample(&StatsSample {
                cpu_pct: i as f64,
                mem_used: i as u64 * 1024,
                mem_limit: 1024 * 1024 * 1024,
            });
        }
        assert_eq!(
            h.cpu.len(),
            STATS_HISTORY_CAP,
            "cpu buffer must be capped at STATS_HISTORY_CAP"
        );
        assert_eq!(
            h.mem_used.len(),
            STATS_HISTORY_CAP,
            "mem_used buffer must be capped at STATS_HISTORY_CAP"
        );
        // The newest sample (index STATS_HISTORY_CAP + 4) should be at the back.
        let expected_last_cpu = ((STATS_HISTORY_CAP + 4) as f64 * 100.0).round() as u64;
        assert_eq!(
            *h.cpu.back().unwrap(),
            expected_last_cpu,
            "newest sample must be at back of ring"
        );
    }

    // ─── AC-HIST-2: reset on box change ───────────────────────────────────────

    #[test]
    fn ac_hist_2_reset_on_box_change() {
        use cbox::dbox::backend::Backend;
        use cbox::tui::message::Message;
        use cbox::tui::model::Model;
        use cbox::tui::update::update;

        // We test the logic indirectly: if a model has stats for box "x" and we
        // open Detail for "y", the stats_history should reset.
        let mut model = Model::new(Backend::Podman);
        model.stats_history = Some(StatsHistory::new("x"));
        if let Some(ref mut h) = model.stats_history {
            h.push_sample(&StatsSample {
                cpu_pct: 5.0,
                mem_used: 100,
                mem_limit: 1000,
            });
        }

        // Simulate a DetailLoaded for a RUNNING box with different id.
        let detail = cbox::core::spec::InspectResult {
            name: "y".to_string(),
            status: "running".to_string(),
            image: "img".to_string(),
            created: "now".to_string(),
            docker_mode: "none".to_string(),
            mounts: vec![],
            packages: vec![],
            backend: "podman".to_string(),
            id: "y-id".to_string(),
            boxfile_path: None,
            cbox_image: None,
            home: None,
            hostname: None,
        };
        update(&mut model, Message::DetailLoaded(Ok(detail)));

        let h = model
            .stats_history
            .as_ref()
            .expect("stats_history must be Some for running box");
        assert_eq!(h.box_id, "y-id", "box_id must be reset to new box's id");
        assert!(h.cpu.is_empty(), "cpu buffer must be empty after reset");
    }

    #[test]
    fn ac_hist_2_reset_to_none_when_stopped() {
        use cbox::dbox::backend::Backend;
        use cbox::tui::message::Message;
        use cbox::tui::model::Model;
        use cbox::tui::update::update;

        let mut model = Model::new(Backend::Podman);
        model.stats_history = Some(StatsHistory::new("x"));

        let detail = cbox::core::spec::InspectResult {
            name: "stopped-box".to_string(),
            status: "stopped".to_string(),
            image: "img".to_string(),
            created: "now".to_string(),
            docker_mode: "none".to_string(),
            mounts: vec![],
            packages: vec![],
            backend: "podman".to_string(),
            id: "s-id".to_string(),
            boxfile_path: None,
            cbox_image: None,
            home: None,
            hostname: None,
        };
        update(&mut model, Message::DetailLoaded(Ok(detail)));

        assert!(
            model.stats_history.is_none(),
            "stats_history must be None for stopped box in Detail"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-VOICE-1: BULK_UNMANAGED_PHRASE const value
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn ac_bulk_phrase_const_value() {
    use cbox::tui::strings;
    assert_eq!(
        strings::BULK_UNMANAGED_PHRASE,
        "DESTROY UNMANAGED",
        "BULK_UNMANAGED_PHRASE must be exactly 'DESTROY UNMANAGED'"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// AC-STATS-STOPPED: should_poll never returns Stats for stopped/non-Detail
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn ac_stats_stopped_no_stats_poll_when_stopped() {
    // detail_running = None (stopped or not in Detail) → must not return Stats.
    let gate = PollGate {
        spinner_tick: POLL_INTERVAL_TICKS,
        last_poll_tick: 0,
        busy: false,
        poll_in_flight: false,
        detail_running: None,
    };
    let kind = should_poll(&gate);
    assert!(
        !matches!(kind, Some(PollKind::Stats { .. })),
        "should_poll must not return Stats when detail_running is None"
    );
}
