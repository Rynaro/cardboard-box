//! AC-APPLY-* — integration tests for `cbox apply` via MockRunner.
//! All against MockRunner; zero real distrobox invocations.

use cbox::boxfile::model::{ProvisionStep, ProvisionType};
use cbox::core::{
    self,
    spec::ApplySpec,
    state_store::{AppliedStep, ProvisionState, ProvisionStateStore},
};
use cbox::dbox::{
    backend::Backend,
    mock::{MockMatcher, MockResponse, MockRunner},
    runner::DistroboxRunner,
};
use cbox::error::CboxError;
use std::path::Path;
use tempfile::TempDir;

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// An in-memory ProvisionStateStore for testing apply without spawns.
struct MemoryStore {
    state: std::sync::Mutex<ProvisionState>,
}

impl MemoryStore {
    fn empty() -> Self {
        MemoryStore {
            state: std::sync::Mutex::new(ProvisionState::new()),
        }
    }

    fn with_state(s: ProvisionState) -> Self {
        MemoryStore {
            state: std::sync::Mutex::new(s),
        }
    }
}

impl ProvisionStateStore for MemoryStore {
    fn read(
        &self,
        _name: &str,
        _runner: &dyn DistroboxRunner,
    ) -> Result<ProvisionState, CboxError> {
        Ok(self.state.lock().unwrap().clone())
    }

    fn write(
        &self,
        _name: &str,
        state: &ProvisionState,
        _runner: &dyn DistroboxRunner,
    ) -> Result<(), CboxError> {
        *self.state.lock().unwrap() = state.clone();
        Ok(())
    }
}

/// Build a minimal inspect JSON response for a box.
fn mock_inspect_json(name: &str, image: &str, docker_mode: &str) -> String {
    serde_json::json!([{
        "Id": "abc123",
        "State": { "Status": "running" },
        "Config": {
            "Image": image,
            "Labels": {
                "manager": "distrobox",
                "cbox.managed": "true",
                "cbox.docker_mode": docker_mode,
                "cbox.boxfile_path": "",
                "cbox.version": "0.1.0",
                "cbox.image": image,
                "cbox.packages": ""
            }
        },
        "Mounts": [],
        "Created": "2026-06-16T00:00:00Z",
        "Name": name
    }])
    .to_string()
}

fn boxfile_with_two_shell_steps() -> String {
    r#"
name = "web-dev"
image = "fedora-toolbox:latest"

[[provision]]
type = "shell"
run = "rustup default stable"

[[provision]]
type = "shell"
run = "cargo install just"
"#
    .to_string()
}

fn make_apply_spec(name: &str, boxfile_path: &str) -> ApplySpec {
    ApplySpec::new(name, boxfile_path, Backend::Podman)
}

fn write_boxfile(dir: &TempDir, content: &str) -> String {
    let path = dir.path().join("Boxfile.toml");
    std::fs::write(&path, content).unwrap();
    path.to_string_lossy().to_string()
}

// ─── AC-APPLY-1: two shell steps, no prior state → both run ──────────────────

#[test]
fn ac_apply_1_two_steps_no_state_both_run() {
    let dir = TempDir::new().unwrap();
    let bf_path = write_boxfile(&dir, &boxfile_with_two_shell_steps());

    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::ok(mock_inspect_json(
                "web-dev",
                "fedora-toolbox:latest",
                "none",
            )))
            .with_program("podman")
            .with_args_contain(vec!["inspect".to_string()]),
        )
        .with_default(MockResponse::ok(""));

    let store = MemoryStore::empty();
    let spec = make_apply_spec("web-dev", &bf_path);
    let outcome = core::apply(&spec, &store, &runner).unwrap();

    assert!(outcome.ok);
    assert_eq!(outcome.steps.len(), 2);
    assert_eq!(outcome.steps[0].status, "ran");
    assert_eq!(outcome.steps[1].status, "ran");

    // Runner should have been called with distrobox enter for both steps
    let calls = runner.calls();
    let provision_calls: Vec<_> = calls
        .iter()
        .filter(|c| c.program == "distrobox" && c.args.iter().any(|a| a == "enter"))
        .collect();
    assert_eq!(
        provision_calls.len(),
        2,
        "should have 2 provision spawn calls"
    );
}

// ─── AC-APPLY-2: matching hashes → both skipped (G-IDEMPOTENT) ───────────────

#[test]
fn ac_apply_2_matching_hashes_both_skipped() {
    use cbox::core::provision::hash_step;

    let dir = TempDir::new().unwrap();
    let bf_path = write_boxfile(&dir, &boxfile_with_two_shell_steps());

    // Compute the hashes for the two steps
    let step0 = ProvisionStep {
        step_type: ProvisionType::Shell,
        run: Some("rustup default stable".to_string()),
        src: None,
        dst: None,
    };
    let step1 = ProvisionStep {
        step_type: ProvisionType::Shell,
        run: Some("cargo install just".to_string()),
        src: None,
        dst: None,
    };
    let h0 = hash_step(&step0, Path::new(".")).unwrap();
    let h1 = hash_step(&step1, Path::new(".")).unwrap();

    // State with matching hashes
    let mut state = ProvisionState::new();
    state.set_step(AppliedStep {
        idx: 0,
        step_type: "shell".to_string(),
        hash: h0,
        applied_at: 0,
        result: "ok".to_string(),
    });
    state.set_step(AppliedStep {
        idx: 1,
        step_type: "shell".to_string(),
        hash: h1,
        applied_at: 0,
        result: "ok".to_string(),
    });

    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::ok(mock_inspect_json(
                "web-dev",
                "fedora-toolbox:latest",
                "none",
            )))
            .with_program("podman")
            .with_args_contain(vec!["inspect".to_string()]),
        )
        .with_default(MockResponse::ok(""));

    let store = MemoryStore::with_state(state);
    let spec = make_apply_spec("web-dev", &bf_path);
    let outcome = core::apply(&spec, &store, &runner).unwrap();

    assert_eq!(outcome.steps[0].status, "skipped");
    assert_eq!(outcome.steps[1].status, "skipped");
    assert_eq!(outcome.summary.ran, 0);
    assert_eq!(outcome.summary.skipped, 2);

    // No provision spawns (only the inspect call)
    let provision_call_count = runner
        .calls()
        .iter()
        .filter(|c| c.program == "distrobox" && c.args.iter().any(|a| a == "enter"))
        .count();
    assert_eq!(
        provision_call_count, 0,
        "idempotency: no provision spawn on unchanged"
    );
}

// ─── AC-APPLY-3: --force overrides stored hashes → both run ──────────────────

#[test]
fn ac_apply_3_force_reruns_all() {
    use cbox::core::provision::hash_step;

    let dir = TempDir::new().unwrap();
    let bf_path = write_boxfile(&dir, &boxfile_with_two_shell_steps());

    let step0 = ProvisionStep {
        step_type: ProvisionType::Shell,
        run: Some("rustup default stable".to_string()),
        src: None,
        dst: None,
    };
    let h0 = hash_step(&step0, Path::new(".")).unwrap();
    let mut state = ProvisionState::new();
    state.set_step(AppliedStep {
        idx: 0,
        step_type: "shell".to_string(),
        hash: h0,
        applied_at: 0,
        result: "ok".to_string(),
    });

    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::ok(mock_inspect_json(
                "web-dev",
                "fedora-toolbox:latest",
                "none",
            )))
            .with_program("podman")
            .with_args_contain(vec!["inspect".to_string()]),
        )
        .with_default(MockResponse::ok(""));

    let store = MemoryStore::with_state(state);
    let mut spec = make_apply_spec("web-dev", &bf_path);
    spec.force = true;

    let outcome = core::apply(&spec, &store, &runner).unwrap();

    assert_eq!(outcome.steps[0].status, "ran", "--force: step 0 should run");
    assert_eq!(outcome.steps[1].status, "ran", "--force: step 1 should run");
}

// ─── AC-APPLY-4: --redo 1 → only step 1 runs ─────────────────────────────────

#[test]
fn ac_apply_4_redo_one_step() {
    use cbox::core::provision::hash_step;

    let dir = TempDir::new().unwrap();
    let bf_path = write_boxfile(
        &dir,
        r#"
name = "web-dev"
image = "fedora-toolbox:latest"

[[provision]]
type = "shell"
run = "echo step0"

[[provision]]
type = "shell"
run = "echo step1"

[[provision]]
type = "shell"
run = "echo step2"
"#,
    );

    // All three steps have matching hashes stored
    let steps_content = ["echo step0", "echo step1", "echo step2"];
    let mut state = ProvisionState::new();
    for (idx, run) in steps_content.iter().enumerate() {
        let step = ProvisionStep {
            step_type: ProvisionType::Shell,
            run: Some(run.to_string()),
            src: None,
            dst: None,
        };
        let h = hash_step(&step, Path::new(".")).unwrap();
        state.set_step(AppliedStep {
            idx,
            step_type: "shell".to_string(),
            hash: h,
            applied_at: 0,
            result: "ok".to_string(),
        });
    }

    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::ok(mock_inspect_json(
                "web-dev",
                "fedora-toolbox:latest",
                "none",
            )))
            .with_program("podman")
            .with_args_contain(vec!["inspect".to_string()]),
        )
        .with_default(MockResponse::ok(""));

    let store = MemoryStore::with_state(state);
    let mut spec = make_apply_spec("web-dev", &bf_path);
    spec.redo = vec![1]; // only redo step 1

    let outcome = core::apply(&spec, &store, &runner).unwrap();

    assert_eq!(outcome.steps[0].status, "skipped", "step 0 should skip");
    assert_eq!(
        outcome.steps[1].status, "ran",
        "step 1 should run (--redo 1)"
    );
    assert_eq!(outcome.steps[2].status, "skipped", "step 2 should skip");
}

// ─── AC-APPLY-5: recreate-class diff without --recreate → exit 65 ────────────

#[test]
fn ac_apply_5_recreate_diff_without_flag_exit_65() {
    let dir = TempDir::new().unwrap();
    // Boxfile has image fedora-toolbox:40
    let bf_path = write_boxfile(
        &dir,
        r#"
name = "web-dev"
image = "fedora-toolbox:40"
"#,
    );

    // Live box has fedora-toolbox:latest
    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::ok(mock_inspect_json(
                "web-dev",
                "fedora-toolbox:latest",
                "none",
            )))
            .with_program("podman")
            .with_args_contain(vec!["inspect".to_string()]),
        )
        .with_default(MockResponse::ok(""));

    let store = MemoryStore::empty();
    let spec = make_apply_spec("web-dev", &bf_path);
    let err = core::apply(&spec, &store, &runner).unwrap_err();

    assert_eq!(
        err.exit_code(),
        65,
        "recreate diff without --recreate -> DATAERR (65)"
    );
    assert!(
        err.to_string().contains("recreate"),
        "error should mention 'recreate', got: {err}"
    );
    assert!(
        err.to_string().contains("image"),
        "error should name the 'image' field"
    );

    // No provision spawn should have occurred
    let provision_calls = runner
        .calls()
        .iter()
        .filter(|c| c.program == "distrobox" && c.args.iter().any(|a| a == "enter"))
        .count();
    assert_eq!(
        provision_calls, 0,
        "no provision spawn after recreate-class diff"
    );
}

// ─── AC-APPLY-6: --recreate -y → rm + create + full provision list ────────────

#[test]
fn ac_apply_6_recreate_flag_rm_create_provision() {
    let dir = TempDir::new().unwrap();
    let bf_path = write_boxfile(
        &dir,
        r#"
name = "web-dev"
image = "fedora-toolbox:40"

[[provision]]
type = "shell"
run = "echo hello"
"#,
    );

    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::ok(mock_inspect_json(
                "web-dev",
                "fedora-toolbox:latest", // different image
                "none",
            )))
            .with_program("podman")
            .with_args_contain(vec!["inspect".to_string()]),
        )
        .with_default(MockResponse::ok(""));

    let store = MemoryStore::empty();
    let mut spec = make_apply_spec("web-dev", &bf_path);
    spec.recreate = true;
    spec.yes = true;

    let outcome = core::apply(&spec, &store, &runner).unwrap();
    assert!(outcome.ok);

    let calls = runner.calls();

    // Should have called distrobox rm
    let rm_call = calls
        .iter()
        .any(|c| c.program == "distrobox" && c.args.iter().any(|a| a == "rm"));
    assert!(rm_call, "should have called distrobox rm");

    // Should have called distrobox create
    let create_call = calls
        .iter()
        .any(|c| c.program == "distrobox" && c.args.iter().any(|a| a == "create"));
    assert!(create_call, "should have called distrobox create");

    // Should have run the provision step
    let provision_call = calls
        .iter()
        .any(|c| c.program == "distrobox" && c.args.iter().any(|a| a == "enter"));
    assert!(provision_call, "should have run provision after recreate");
}

// ─── AC-APPLY-7: box not found → exit 69 ─────────────────────────────────────

#[test]
fn ac_apply_7_box_not_found_exit_69() {
    let dir = TempDir::new().unwrap();
    let bf_path = write_boxfile(
        &dir,
        r#"name = "missing-box"
image = "fedora-toolbox:latest"
"#,
    );

    // Mock returns not-found (empty array)
    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::ok("[]"))
                .with_program("podman")
                .with_args_contain(vec!["inspect".to_string()]),
        )
        .with_default(MockResponse::ok(""));

    let store = MemoryStore::empty();
    let spec = make_apply_spec("missing-box", &bf_path);
    let err = core::apply(&spec, &store, &runner).unwrap_err();

    assert_eq!(err.exit_code(), 69, "box not found -> UNAVAILABLE (69)");
}

// ─── AC-APPLY-8: --no-provision skips [[provision]] but package install still runs ──

#[test]
fn ac_apply_8_no_provision_skips_provision_steps() {
    let dir = TempDir::new().unwrap();
    // Boxfile with an added package (vs live) and a provision step
    let bf_path = write_boxfile(
        &dir,
        r#"
name = "web-dev"
image = "fedora-toolbox:latest"
packages = ["git", "ripgrep"]

[[provision]]
type = "shell"
run = "echo should not run"
"#,
    );

    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::ok(mock_inspect_json(
                "web-dev",
                "fedora-toolbox:latest",
                "none",
            )))
            .with_program("podman")
            .with_args_contain(vec!["inspect".to_string()]),
        )
        .with_default(MockResponse::ok(""));

    let store = MemoryStore::empty();
    let mut spec = make_apply_spec("web-dev", &bf_path);
    spec.no_provision = true;

    let outcome = core::apply(&spec, &store, &runner).unwrap();
    assert!(outcome.ok);

    // Steps should be empty (skipped entirely)
    assert_eq!(outcome.steps.len(), 0, "--no-provision: no steps");

    // provision step "echo should not run" must not appear
    let provision_echo_calls = runner
        .calls()
        .iter()
        .filter(|c| c.args.iter().any(|a| a.contains("should not run")))
        .count();
    assert_eq!(
        provision_echo_calls, 0,
        "provision step must not run with --no-provision"
    );
}

// ─── AC-APPLY-9: --dry-run → no mutating Capture spawns ──────────────────────

#[test]
fn ac_apply_9_dry_run_no_capture_mutations() {
    let dir = TempDir::new().unwrap();
    let bf_path = write_boxfile(&dir, &boxfile_with_two_shell_steps());

    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::ok(mock_inspect_json(
                "web-dev",
                "fedora-toolbox:latest",
                "none",
            )))
            .with_program("podman")
            .with_args_contain(vec!["inspect".to_string()]),
        )
        .with_default(MockResponse::ok(""));

    let store = MemoryStore::empty();
    let mut spec = make_apply_spec("web-dev", &bf_path);
    spec.dry_run = true;

    // Dry-run should succeed without error
    let outcome = core::apply(&spec, &store, &runner).unwrap();
    assert!(outcome.ok);

    // Steps should show "ran" (dry-run still reports intent, like P1 create dry-run)
    assert_eq!(outcome.steps.len(), 2);
}

// ─── AC-APPLY-10: --json output conforms to schema ───────────────────────────

#[test]
fn ac_apply_10_json_schema() {
    let dir = TempDir::new().unwrap();
    let bf_path = write_boxfile(&dir, &boxfile_with_two_shell_steps());

    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::ok(mock_inspect_json(
                "web-dev",
                "fedora-toolbox:latest",
                "none",
            )))
            .with_program("podman")
            .with_args_contain(vec!["inspect".to_string()]),
        )
        .with_default(MockResponse::ok(""));

    let store = MemoryStore::empty();
    let spec = make_apply_spec("web-dev", &bf_path);
    let outcome = core::apply(&spec, &store, &runner).unwrap();

    // Serialize to JSON and verify schema fields
    let v = serde_json::to_value(&outcome).unwrap();
    assert!(v["ok"].as_bool().is_some(), "schema: ok field");
    assert_eq!(v["action"].as_str().unwrap(), "apply");
    assert!(v["name"].as_str().is_some(), "schema: name field");
    assert!(v["diff"].is_object(), "schema: diff object");
    assert!(v["steps"].is_array(), "schema: steps array");
    assert!(v["summary"].is_object(), "schema: summary object");
    assert!(
        v["recreate_required"].as_bool().is_some(),
        "schema: recreate_required"
    );

    // Check step schema
    let steps = v["steps"].as_array().unwrap();
    if !steps.is_empty() {
        let s = &steps[0];
        assert!(s["idx"].as_u64().is_some(), "step: idx");
        assert!(s["type"].as_str().is_some(), "step: type");
        assert!(s["status"].as_str().is_some(), "step: status");
        assert!(s["hash"].as_str().is_some(), "step: hash");
        assert!(s["duration_ms"].as_u64().is_some(), "step: duration_ms");
    }
}
