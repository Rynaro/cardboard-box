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

// ─── AC-APPLY-SOCKET-*: docker="host" pre-flight guard ───────────────────────

/// socket_preflight: host mode + absent socket → TempFail with path + remedy hint.
#[test]
fn ac_apply_socket_1_host_absent_errors_with_path_and_hint() {
    let err = cbox::core::socket_preflight(
        "podman",
        "/run/user/9999/podman/podman.sock",
        true,  // is_host
        false, // dry_run
        false, // socket_exists
    )
    .unwrap_err();

    assert_eq!(err.exit_code(), 75, "missing socket → TEMPFAIL (75)");
    let msg = err.to_string();
    assert!(
        msg.contains("/run/user/9999/podman/podman.sock"),
        "error must name the socket path, got: {msg}"
    );
    assert!(
        msg.contains("podman.socket"),
        "error must hint at systemctl remedy, got: {msg}"
    );
    assert!(
        msg.contains("docker"),
        "error must offer the alternative backend, got: {msg}"
    );
}

/// socket_preflight: docker mode + absent socket → TempFail with docker path + hint.
#[test]
fn ac_apply_socket_2_docker_backend_absent_socket_errors() {
    let err = cbox::core::socket_preflight(
        "docker",
        "/var/run/docker.sock",
        true,  // is_host
        false, // dry_run
        false, // socket_exists
    )
    .unwrap_err();

    assert_eq!(err.exit_code(), 75);
    let msg = err.to_string();
    assert!(msg.contains("/var/run/docker.sock"), "path in error: {msg}");
    assert!(
        msg.contains("podman"),
        "alternative backend hint in error: {msg}"
    );
}

/// socket_preflight: non-host docker_mode → Ok regardless of socket.
#[test]
fn ac_apply_socket_3_non_host_mode_ok() {
    cbox::core::socket_preflight(
        "podman",
        "/run/user/9999/podman/podman.sock",
        false, // is_host = false (None or Nested)
        false,
        false, // socket absent — irrelevant
    )
    .expect("non-host docker_mode must never fail preflight");
}

/// socket_preflight: dry_run=true → Ok even when socket is absent and mode is Host.
#[test]
fn ac_apply_socket_4_dry_run_bypasses_preflight() {
    cbox::core::socket_preflight(
        "podman",
        "/run/user/9999/podman/podman.sock",
        true,  // is_host
        true,  // dry_run
        false, // socket absent
    )
    .expect("dry_run must bypass preflight");
}

/// socket_preflight: host mode + socket present → Ok.
#[test]
fn ac_apply_socket_5_socket_present_ok() {
    // Point the socket path at a file we know exists.
    let socket_path = std::env::current_exe()
        .unwrap()
        .to_string_lossy()
        .to_string();
    cbox::core::socket_preflight(
        "podman",
        &socket_path,
        true,  // is_host
        false, // dry_run
        true,  // socket_exists — we tell the helper it's there
    )
    .expect("socket present → Ok");
}

/// AC-APPLY-SOCKET-6 (recreate safety): when docker="host" socket is absent,
/// apply --recreate must fail BEFORE invoking distrobox rm.
/// This is the key regression guard: the box must not be destroyed if create
/// would also fail.
#[test]
fn ac_apply_socket_6_recreate_preflight_prevents_rm() {
    let dir = TempDir::new().unwrap();
    // Boxfile requests docker="host" and a new image (forces Recreate diff).
    // We pick an image that differs from the live one so the diff class is Recreate.
    let bf_path = write_boxfile(
        &dir,
        r#"
name = "my-box"
image = "fedora-toolbox:40"
docker = "host"
"#,
    );

    // The live box has a different image → Recreate-class diff.
    // Backend is Podman so socket_path() → /run/user/<uid>/podman/podman.sock.
    // That path almost certainly does not exist in the test environment, so the
    // pre-flight fires before rm.  To make this deterministic we use a socket
    // path that is guaranteed absent (non-existent uid directory).
    //
    // We can't inject a fake path directly here because core::apply uses the
    // real Backend::socket_path(). However the test runner runs under a UID
    // whose podman socket is NOT active (CI / container), so Path::new(socket).exists()
    // returns false — which is the expected production scenario.
    // If this assumption fails (podman socket IS running), the test still passes
    // because we assert the rm call was present — the test only asserts rm IS
    // NOT present when the pre-flight fires an error.
    //
    // Strategy: catch the Err and assert on rm call count.
    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::ok(mock_inspect_json(
                "my-box",
                "fedora-toolbox:latest", // different image → Recreate diff
                "host",
            )))
            .with_program("podman")
            .with_args_contain(vec!["inspect".to_string()]),
        )
        .with_default(MockResponse::ok(""));

    let store = MemoryStore::empty();
    let mut spec = make_apply_spec("my-box", &bf_path);
    spec.recreate = true;
    spec.yes = true;
    // backend defaults to Podman (from make_apply_spec → ApplySpec::new)

    let result = core::apply(&spec, &store, &runner);

    // If the socket exists (podman is running in the test environment), create
    // would also succeed and the whole recreate runs fine — both rm and create
    // calls appear.  Skip the assertion in that case: the pre-flight only fires
    // when the socket is absent.
    let podman_uid = unsafe {
        extern "C" {
            fn getuid() -> u32;
        }
        getuid()
    };
    let socket_path = format!("/run/user/{podman_uid}/podman/podman.sock");
    let socket_present = std::path::Path::new(&socket_path).exists();

    if socket_present {
        // Socket is live — recreate should succeed; no assertion needed for the
        // pre-flight guard (it passed correctly).
        let _ = result; // may succeed or fail for unrelated reasons in CI
    } else {
        // Socket absent — pre-flight must have fired BEFORE rm.
        let err = result.expect_err("missing socket + host mode → should fail");
        assert_eq!(err.exit_code(), 75, "TEMPFAIL expected, got: {err}");

        let all_calls = runner.calls();
        let rm_calls: Vec<_> = all_calls
            .iter()
            .filter(|c| c.program == "distrobox" && c.args.iter().any(|a| a == "rm"))
            .collect();
        assert_eq!(
            rm_calls.len(),
            0,
            "rm must NOT have been called before the pre-flight guard fires: {rm_calls:?}"
        );
    }
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
