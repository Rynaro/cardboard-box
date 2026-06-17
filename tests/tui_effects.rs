//! Effect executor tests — `execute_effect` through MockRunner.
//! Zero real distrobox/podman/docker invocations (G-NO-NET, G-MOCK, G-ENTER).
#![cfg(feature = "tui")] // TUI internals only exist with the feature on.

use std::sync::Arc;

use cbox::core::spec::{ApplySpec, CreateSpec, EnterSpec, RmSpec};
use cbox::core::state_store::GuestStateStore;
use cbox::dbox::backend::Backend;
use cbox::dbox::mock::{MockMatcher, MockResponse, MockRunner};
use cbox::tui::effect::{execute_effect, Effect};
use cbox::tui::message::Message;

fn make_runner(r: MockRunner) -> Arc<dyn cbox::dbox::runner::DistroboxRunner> {
    Arc::new(r)
}

fn store() -> GuestStateStore {
    GuestStateStore
}

// ─── AC-EFF-LIST ─────────────────────────────────────────────────────────────

#[test]
fn ac_eff_list_loads_boxes() {
    // Two-box JSON as returned by `podman ps --format json`.
    let json = serde_json::json!([
        {
            "Names": ["web-dev"],
            "State": "running",
            "Image": "fedora-toolbox:latest",
            "Id": "abc",
            "Labels": {"manager": "distrobox", "cbox.managed": "true", "cbox.docker_mode": "none"}
        },
        {
            "Names": ["db-box"],
            "State": "exited",
            "Image": "ubuntu:22.04",
            "Id": "def",
            "Labels": {"manager": "distrobox", "cbox.managed": "false", "cbox.docker_mode": "none"}
        }
    ])
    .to_string();

    let runner = MockRunner::new().with_default(MockResponse::ok(json));
    let arc_runner = make_runner(runner);

    let msg = execute_effect(Effect::LoadList, &store(), &arc_runner, &[Backend::Podman])
        .expect("should return a message");

    match msg {
        Message::ListLoaded(Ok(rows)) => {
            assert_eq!(rows.len(), 2, "should have 2 rows");
            assert_eq!(rows[0].name, "web-dev");
            assert_eq!(rows[1].name, "db-box");
        }
        other => panic!("expected ListLoaded(Ok(..)), got {:?}", other),
    }

    // Verify the recorded call is `podman ps … --format json` (list_machine uses the backend binary).
    // We used Backend::Podman so the program should be "podman".
    let calls = arc_runner
        .as_ref()
        // We need to downcast — unfortunately the trait doesn't expose calls().
        // Use a separate raw MockRunner for call assertion.
        ;
    // Call assertion via a separate raw runner below.
    let _ = calls;
}

#[test]
fn ac_eff_list_call_shape() {
    // Verify the exact call shape via a dedicated runner we can inspect.
    let raw = MockRunner::new().with_default(MockResponse::ok("[]"));
    let runner: Arc<dyn cbox::dbox::runner::DistroboxRunner> = Arc::new(raw);

    execute_effect(Effect::LoadList, &store(), &runner, &[Backend::Podman]);

    // We can't downcast Arc<dyn Trait> back to MockRunner in safe Rust.
    // The alternative: drive the core function directly and assert on the mock.
    // The spec assertion is: `core::list_machine` calls `podman ps … --format json`.
    // We prove this via core::list_machine directly.
    let raw2 = MockRunner::new().with_default(MockResponse::ok("[]"));
    let _ = cbox::core::list_machine(&Backend::Podman, &raw2);
    let calls = raw2.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].program, "podman",
        "list_machine uses the backend binary"
    );
    assert!(
        calls[0].args.iter().any(|a| a == "ps"),
        "args should include 'ps'"
    );
    assert!(
        calls[0].args.iter().any(|a| a.contains("json")),
        "args should include json format"
    );
    assert!(!calls[0].interactive, "list should NOT be interactive");
}

// ─── AC-EFF-CREATE ───────────────────────────────────────────────────────────

#[test]
fn ac_eff_create_calls_distrobox_create() {
    let runner = MockRunner::new().with_default(MockResponse::ok(""));
    let arc_runner = make_runner(runner);

    let mut spec = CreateSpec::new("web-dev", Backend::Podman);
    spec.image = "fedora-toolbox:latest".to_string();
    spec.dry_run = false;

    let msg = execute_effect(
        Effect::Create(spec),
        &store(),
        &arc_runner,
        &[Backend::Podman],
    )
    .expect("should produce a message");

    assert!(
        matches!(msg, Message::CreateDone(Ok(_))),
        "should return CreateDone(Ok(..))"
    );

    // Verify call shape via core directly.
    let raw2 = MockRunner::new().with_default(MockResponse::ok(""));
    let mut spec2 = CreateSpec::new("web-dev", Backend::Podman);
    spec2.image = "fedora-toolbox:latest".to_string();
    let _ = cbox::core::create(&spec2, &raw2);
    let calls = raw2.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].program, "distrobox");
    assert!(calls[0].args.iter().any(|a| a == "create"));
    assert!(calls[0].args.iter().any(|a| a == "--yes"));
    assert!(calls[0]
        .args
        .windows(2)
        .any(|w| w[0] == "--name" && w[1] == "web-dev"));
    assert!(calls[0]
        .args
        .windows(2)
        .any(|w| w[0] == "--image" && w[1] == "fedora-toolbox:latest"));
    assert!(!calls[0].interactive);
}

// ─── AC-EFF-ENTER (G-ENTER) ──────────────────────────────────────────────────

#[test]
fn ac_eff_enter_is_interactive() {
    // This test proves the ENTER gate: core::enter routes to run_interactive, not run.
    let runner = MockRunner::new().with_default_interactive(0);

    let spec = EnterSpec {
        name: "web-dev".to_string(),
        root: false,
        clean_path: false,
        cmd: vec![],
        backend: Backend::Podman,
    };

    let code = cbox::core::enter(&spec, &runner).expect("enter should succeed");
    assert_eq!(code, 0);

    let calls = runner.calls();
    assert_eq!(calls.len(), 1, "enter should produce exactly one call");
    assert!(
        calls[0].interactive,
        "enter call MUST be interactive (run_interactive, not run)"
    );
    assert_eq!(calls[0].program, "distrobox");
    assert!(calls[0].args.iter().any(|a| a == "enter"));
    assert!(calls[0]
        .args
        .windows(2)
        .any(|w| w[0] == "--name" && w[1] == "web-dev"));
}

/// AC-EFF-ENTER via SuspendAndEnter effect: the effect executor does NOT handle
/// SuspendAndEnter (it's routed to the main thread). Verify it returns None.
#[test]
fn ac_eff_suspend_enter_returns_none() {
    let runner = make_runner(MockRunner::new());
    let spec = EnterSpec {
        name: "web-dev".to_string(),
        root: false,
        clean_path: false,
        cmd: vec![],
        backend: Backend::Podman,
    };
    let result = execute_effect(
        Effect::SuspendAndEnter(spec),
        &store(),
        &runner,
        &[Backend::Podman],
    );
    assert!(
        result.is_none(),
        "SuspendAndEnter must be handled by the main thread, not the worker"
    );
}

// ─── AC-EFF-RM ───────────────────────────────────────────────────────────────

#[test]
fn ac_eff_rm_calls_distrobox_rm() {
    let runner = MockRunner::new().with_default(MockResponse::ok(""));
    let arc_runner = make_runner(runner);

    let spec = RmSpec {
        names: vec!["web-dev".to_string()],
        force: true,
        rm_home: false,
        all: false,
        yes: true,
        backend: Backend::Podman,
    };

    let msg = execute_effect(Effect::Rm(spec), &store(), &arc_runner, &[Backend::Podman])
        .expect("should produce a message");

    assert!(
        matches!(msg, Message::RmDone(Ok(_))),
        "should return RmDone(Ok(..))"
    );

    // Verify call shape.
    let raw2 = MockRunner::new().with_default(MockResponse::ok(""));
    let spec2 = RmSpec {
        names: vec!["web-dev".to_string()],
        force: true,
        rm_home: false,
        all: false,
        yes: true,
        backend: Backend::Podman,
    };
    let _ = cbox::core::rm(&spec2, &raw2);
    let calls = raw2.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].program, "distrobox");
    assert!(calls[0].args.iter().any(|a| a == "rm"));
    assert!(calls[0].args.iter().any(|a| a == "--force"));
    assert!(!calls[0].interactive);
}

// ─── AC-EFF-APPLY ────────────────────────────────────────────────────────────

#[test]
fn ac_eff_apply_through_mock_runner() {
    use std::io::Write;
    use tempfile::NamedTempFile;

    // Write a minimal Boxfile.
    let mut bf_file = NamedTempFile::new().expect("tempfile");
    write!(
        bf_file,
        r#"name = "web-dev"
image = "fedora-toolbox:latest"
packages = []
"#
    )
    .unwrap();

    let boxfile_path = bf_file.path().to_string_lossy().to_string();

    // Mock responses needed by apply:
    // 1. inspect the live box
    let inspect_json = serde_json::json!([{
        "Id": "abc123",
        "State": {"Status": "running"},
        "Image": "fedora-toolbox:latest",
        "Created": "2024-01-01",
        "Config": {
            "Labels": {
                "cbox.docker_mode": "none",
                "cbox.managed": "true"
            }
        },
        "Mounts": []
    }])
    .to_string();

    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::ok(inspect_json))
                .with_program("podman")
                .with_args_contain(vec!["inspect".to_string()]),
        )
        .with_default(MockResponse::ok(""));

    let arc_runner: Arc<dyn cbox::dbox::runner::DistroboxRunner> = Arc::new(runner);

    let spec = ApplySpec {
        name: "web-dev".to_string(),
        boxfile_path,
        force: false,
        redo: vec![],
        no_provision: false,
        recreate: false,
        yes: true,
        dry_run: false,
        backend: Backend::Podman,
    };

    let msg = execute_effect(
        Effect::Apply(spec),
        &store(),
        &arc_runner,
        &[Backend::Podman],
    )
    .expect("should produce a message");

    assert!(
        matches!(msg, Message::ApplyDone(Ok(_))),
        "should return ApplyDone(Ok(..))"
    );
}

// ─── Quit / SuspendAndEdit return None ───────────────────────────────────────

#[test]
fn quit_effect_returns_none() {
    let runner = make_runner(MockRunner::new());
    let result = execute_effect(Effect::Quit, &store(), &runner, &[Backend::Podman]);
    assert!(
        result.is_none(),
        "Quit should return None (handled by event loop)"
    );
}

#[test]
fn suspend_edit_effect_returns_none() {
    let runner = make_runner(MockRunner::new());
    let result = execute_effect(
        Effect::SuspendAndEdit("/tmp/Boxfile.toml".to_string()),
        &store(),
        &runner,
        &[Backend::Podman],
    );
    assert!(
        result.is_none(),
        "SuspendAndEdit should return None (main thread)"
    );
}

// ─── Doctor effect ────────────────────────────────────────────────────────────

#[test]
fn doctor_effect_returns_doctor_done() {
    use cbox::core::spec::DoctorSpec;

    // Doctor calls distrobox version + podman --version + podman info etc.
    // Scripted to return success for distrobox + podman presence.
    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::ok("distrobox: 1.8.0"))
                .with_program("distrobox")
                .with_args_contain(vec!["version".to_string()]),
        )
        .with_matcher(
            MockMatcher::new(MockResponse::ok("podman version 4.0.0"))
                .with_program("podman")
                .with_args_contain(vec!["--version".to_string()]),
        )
        .with_matcher(
            MockMatcher::new(MockResponse::ok(""))
                .with_program("podman")
                .with_args_contain(vec!["info".to_string()]),
        )
        .with_default(MockResponse::ok(""));

    let arc_runner = make_runner(runner);
    let spec = DoctorSpec {
        backend_override: Some("podman".to_string()),
    };

    let msg = execute_effect(
        Effect::Doctor(spec),
        &store(),
        &arc_runner,
        &[Backend::Podman],
    )
    .expect("should produce a message");

    assert!(
        matches!(msg, Message::DoctorDone(Ok(_))),
        "should return DoctorDone(Ok(..))"
    );
}
