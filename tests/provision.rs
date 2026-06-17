//! AC-PROV-* — provision engine tests.
//! All driven against MockRunner; zero real distrobox.

use cbox::boxfile::model::{ProvisionStep, ProvisionType};
use cbox::core::provision::{hash_step, normalize_shell_run, provision, ProvisionPlan};
use cbox::core::state_store::{ProvisionState, ProvisionStateStore};
use cbox::dbox::{
    backend::Backend,
    mock::{MockMatcher, MockResponse, MockRunner},
    runner::DistroboxRunner,
};
use cbox::error::CboxError;
use std::path::Path;
use tempfile::TempDir;

// ─── Mock state store for testing ────────────────────────────────────────────

/// A simple in-memory state store for testing (no spawns needed).
struct MemoryStateStore {
    state: std::sync::Mutex<ProvisionState>,
    writes: std::sync::Mutex<Vec<ProvisionState>>,
}

impl MemoryStateStore {
    fn empty() -> Self {
        MemoryStateStore {
            state: std::sync::Mutex::new(ProvisionState::new()),
            writes: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn with_state(state: ProvisionState) -> Self {
        MemoryStateStore {
            state: std::sync::Mutex::new(state),
            writes: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn last_written_state(&self) -> Option<ProvisionState> {
        self.writes.lock().unwrap().last().cloned()
    }
}

impl ProvisionStateStore for MemoryStateStore {
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
        self.writes.lock().unwrap().push(state.clone());
        *self.state.lock().unwrap() = state.clone();
        Ok(())
    }
}

fn shell_step(run: &str) -> ProvisionStep {
    ProvisionStep {
        step_type: ProvisionType::Shell,
        run: Some(run.to_string()),
        src: None,
        dst: None,
    }
}

fn copy_step(src: &str, dst: &str) -> ProvisionStep {
    ProvisionStep {
        step_type: ProvisionType::Copy,
        run: None,
        src: Some(src.to_string()),
        dst: Some(dst.to_string()),
    }
}

// ─── AC-PROV-1: hashing pure deterministic function ──────────────────────────

#[test]
fn ac_prov_1_hash_shell_deterministic() {
    let step = shell_step("rustup default stable");
    let h1 = hash_step(&step, Path::new(".")).unwrap();
    let h2 = hash_step(&step, Path::new(".")).unwrap();
    assert_eq!(h1, h2, "identical run -> identical hash");

    // Different run -> different hash
    let step2 = shell_step("cargo install just");
    let h3 = hash_step(&step2, Path::new(".")).unwrap();
    assert_ne!(h1, h3, "different run -> different hash");
}

#[test]
fn ac_prov_1_hash_whitespace_normalized() {
    // Trailing whitespace stripped per line → same hash
    let step1 = shell_step("echo hello   ");
    let step2 = shell_step("echo hello");
    let h1 = hash_step(&step1, Path::new(".")).unwrap();
    let h2 = hash_step(&step2, Path::new(".")).unwrap();
    assert_eq!(h1, h2, "trailing whitespace normalized -> same hash");
}

#[test]
fn ac_prov_1_normalize_shell_run() {
    assert_eq!(normalize_shell_run("echo hi"), "echo hi\n");
    assert_eq!(normalize_shell_run("echo hi   "), "echo hi\n");
    assert_eq!(normalize_shell_run("line1\nline2"), "line1\nline2\n");
    assert_eq!(normalize_shell_run("line1\nline2\n"), "line1\nline2\n");
    // Leading spaces preserved
    assert_eq!(normalize_shell_run("  echo hi"), "  echo hi\n");
}

// ─── AC-PROV-2: copy hash changes with content / dst ─────────────────────────

#[test]
fn ac_prov_2_copy_hash_changes() {
    let dir = TempDir::new().unwrap();

    let src1 = dir.path().join("file1.txt");
    let src2 = dir.path().join("file2.txt");
    std::fs::write(&src1, b"content A").unwrap();
    std::fs::write(&src2, b"content B").unwrap();

    let step1 = copy_step(src1.to_str().unwrap(), "/home/user/.config");
    let step2 = copy_step(src2.to_str().unwrap(), "/home/user/.config");
    let step3 = copy_step(src1.to_str().unwrap(), "/home/user/.other");

    let h1 = hash_step(&step1, Path::new(".")).unwrap();
    let h2 = hash_step(&step2, Path::new(".")).unwrap();
    let h3 = hash_step(&step3, Path::new(".")).unwrap();

    assert_ne!(h1, h2, "different src content -> different hash");
    assert_ne!(h1, h3, "different dst -> different hash");

    // Same src and dst -> same hash
    let step1b = copy_step(src1.to_str().unwrap(), "/home/user/.config");
    let h1b = hash_step(&step1b, Path::new(".")).unwrap();
    assert_eq!(h1, h1b, "same src+dst -> same hash");
}

// ─── AC-PROV-3: copy step spawns correct backend cp argv ─────────────────────

#[test]
fn ac_prov_3_copy_spawn() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("dotfile");
    std::fs::write(&src, b"dotfile contents").unwrap();

    let runner = MockRunner::new().with_default(MockResponse::ok(""));
    let store = MemoryStateStore::empty();

    let step = copy_step(src.to_str().unwrap(), "/home/user/.vimrc");
    let plan = ProvisionPlan {
        name: "web-dev",
        steps: &[step],
        boxfile_dir: dir.path(),
        backend: &Backend::Podman,
        force: false,
        redo: &[],
        dry_run: false,
    };

    let mut state = ProvisionState::new();
    let results = provision(&plan, &store, &runner, &mut state).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].status, "copied");

    // The runner should have been called with 'cp <src> web-dev:/home/user/.vimrc'
    let calls = runner.calls();
    let cp_call = calls
        .iter()
        .find(|c| c.program == "podman" && c.args.iter().any(|a| a == "cp"));
    assert!(
        cp_call.is_some(),
        "should have a podman cp call, got: {:?}",
        calls
    );
    let cp = cp_call.unwrap();
    assert!(cp
        .args
        .iter()
        .any(|a| a.contains("web-dev:/home/user/.vimrc")));
}

// ─── AC-PROV-4: copy missing src -> exit 65 before any spawn ─────────────────

#[test]
fn ac_prov_4_copy_missing_src() {
    let runner = MockRunner::new().with_default(MockResponse::ok(""));
    let store = MemoryStateStore::empty();

    let step = copy_step("/nonexistent/path/file.txt", "/home/user/.config");
    let plan = ProvisionPlan {
        name: "web-dev",
        steps: &[step],
        boxfile_dir: Path::new("."),
        backend: &Backend::Podman,
        force: false,
        redo: &[],
        dry_run: false,
    };

    let mut state = ProvisionState::new();
    let err = provision(&plan, &store, &runner, &mut state).unwrap_err();
    assert_eq!(err.exit_code(), 65, "missing copy src -> DATAERR (65)");
    assert_eq!(
        runner.call_count(),
        0,
        "no spawn should occur before preflight fails"
    );
}

// ─── AC-PROV-5: partial-failure resume ───────────────────────────────────────

#[test]
fn ac_prov_5_partial_failure_resume() {
    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::ok(""))
                .with_program("distrobox")
                .with_args_contain(vec!["echo step0".to_string()]),
        )
        .with_matcher(
            MockMatcher::new(MockResponse::err(1, "cargo: error"))
                .with_program("distrobox")
                .with_args_contain(vec!["cargo install just".to_string()]),
        )
        .with_default(MockResponse::ok(""));

    let store = MemoryStateStore::empty();
    let steps = vec![
        shell_step("echo step0"),
        shell_step("cargo install just"),
        shell_step("echo step2"),
    ];

    let plan = ProvisionPlan {
        name: "web-dev",
        steps: &steps,
        boxfile_dir: Path::new("."),
        backend: &Backend::Podman,
        force: false,
        redo: &[],
        dry_run: false,
    };

    let mut state = ProvisionState::new();
    let err = provision(&plan, &store, &runner, &mut state).unwrap_err();
    assert_eq!(err.exit_code(), 125, "step failure -> exit 125");

    // Step 0 should have been recorded as ok
    let written = store
        .last_written_state()
        .expect("state should have been written");
    let step0 = written.steps.iter().find(|s| s.idx == 0);
    assert!(step0.is_some(), "step 0 should be recorded");
    assert_eq!(step0.unwrap().result, "ok");

    // Step 2 should NOT have been attempted
    let calls = runner.calls();
    let step2_called = calls
        .iter()
        .any(|c| c.args.iter().any(|a| a == "echo step2"));
    assert!(
        !step2_called,
        "step 2 should not have been attempted after step 1 failure"
    );

    // --- Re-apply with the partial state should SKIP step 0, resume at step 1 ---
    let partial_state = written.clone();
    let store2 = MemoryStateStore::with_state(partial_state);

    let runner2 = MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::ok(""))
                .with_program("distrobox")
                .with_args_contain(vec!["cargo install just".to_string()]),
        )
        .with_default(MockResponse::ok(""));

    let mut state2 = store2.read("web-dev", &runner2).unwrap();
    let plan2 = ProvisionPlan {
        name: "web-dev",
        steps: &steps,
        boxfile_dir: Path::new("."),
        backend: &Backend::Podman,
        force: false,
        redo: &[],
        dry_run: false,
    };

    let results2 = provision(&plan2, &store2, &runner2, &mut state2).unwrap();

    // Step 0: skipped (hash matches stored ok)
    assert_eq!(
        results2[0].status, "skipped",
        "step 0 should be skipped on resume"
    );
    // Steps 1 and 2 ran
    assert_eq!(results2[1].status, "ran", "step 1 should run on resume");
    assert_eq!(results2[2].status, "ran", "step 2 should run on resume");
}

// ─── AC-PROV-6: state read parses correctly ──────────────────────────────────

#[test]
fn ac_prov_6_state_read_parses() {
    use cbox::core::state_store::GuestStateStore;

    let stored_hash = "abc123def456";
    let state_json = serde_json::json!({
        "cbox_state_version": 1,
        "boxfile_sha": "",
        "packages_applied": [],
        "steps": [
            { "idx": 0, "type": "shell", "hash": stored_hash, "applied_at": 0, "result": "ok" }
        ]
    })
    .to_string();

    // Mock runner that returns the state JSON for any distrobox enter call to "web-dev"
    // (state reads are 'distrobox enter --name web-dev -- sh -c "cat ...provision.json..."')
    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::ok(state_json.clone()))
                .with_program("distrobox")
                .with_args_contain(vec![
                    "enter".to_string(),
                    "--name".to_string(),
                    "web-dev".to_string(),
                ]),
        )
        .with_default(MockResponse::ok(""));

    let store = GuestStateStore;
    let state = store.read("web-dev", &runner).unwrap();
    assert_eq!(
        state.steps.len(),
        1,
        "should parse 1 step from returned JSON"
    );
    assert_eq!(state.steps[0].idx, 0);
    assert_eq!(state.steps[0].hash, stored_hash);
    assert_eq!(state.steps[0].result, "ok");
}

// ─── AC-PROV-7: state write escaping ─────────────────────────────────────────

#[test]
fn ac_prov_7_state_write_escaping() {
    use cbox::dbox::argv::{build_state_write_argv, escape_single_quotes};

    // JSON with a single quote
    let json = r#"{"key":"it's a test"}"#;
    let escaped = escape_single_quotes(json);
    // Must not have unescaped single quotes
    assert!(
        !escaped.contains("it's"),
        "raw single quote should be escaped"
    );
    assert!(
        escaped.contains("it'\\''s"),
        "should use the '\\'\\'' escape pattern"
    );

    // The full write argv must produce a shell-safe command
    let args = build_state_write_argv("web-dev", json);
    // The sh -c argument (last element) must not cause parse errors
    // Visual check: last element contains the escaped json
    let sh_cmd = args.last().unwrap();
    assert!(sh_cmd.contains("printf"), "should use printf");
    assert!(
        !sh_cmd.contains("it's "),
        "raw single-quote must not appear unescaped in shell cmd"
    );
}

// ─── AC-PROV-8: corrupt state → exit 74 unless --force ───────────────────────

#[test]
fn ac_prov_8_corrupt_state() {
    use cbox::core::state_store::GuestStateStore;

    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::ok("not json at all {{{{"))
                .with_program("distrobox")
                .with_args_contain(vec![
                    "enter".to_string(),
                    "--name".to_string(),
                    "web-dev".to_string(),
                ]),
        )
        .with_default(MockResponse::ok(""));

    let store = GuestStateStore;
    let err = store.read("web-dev", &runner).unwrap_err();
    assert_eq!(err.exit_code(), 74, "corrupt state -> IOERR (74)");
    assert!(
        err.to_string().contains("corrupt") || err.to_string().contains("Provision state"),
        "should mention corrupt state"
    );
}

// ─── AC-PROV-9: failure error contains captured stderr + step index + exit code ──

#[test]
fn ac_prov_9_failure_error_surfaces_stderr_and_index() {
    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::err(2, "bundle: Could not find gem 'psepho'"))
                .with_program("distrobox")
                .with_args_contain(vec!["bundle install".to_string()]),
        )
        .with_default(MockResponse::ok(""));

    let store = MemoryStateStore::empty();
    let steps = vec![shell_step("bundle install")];

    let plan = ProvisionPlan {
        name: "electionbuddy",
        steps: &steps,
        boxfile_dir: Path::new("."),
        backend: &Backend::Podman,
        force: false,
        redo: &[],
        dry_run: false,
    };

    let mut state = ProvisionState::new();
    let err = provision(&plan, &store, &runner, &mut state).unwrap_err();

    // Exit code must be 125 (BACKEND_NONZERO)
    assert_eq!(err.exit_code(), 125, "provision failure -> exit 125");

    let msg = err.to_string();
    // Headline must contain step index
    assert!(
        msg.contains("[0]"),
        "error should name step index [0], got: {msg}"
    );
    // Headline must contain exit code
    assert!(
        msg.contains("exit 2"),
        "error should include exit code 2, got: {msg}"
    );
    // Error message must surface the captured stderr
    assert!(
        msg.contains("psepho"),
        "error should include captured stderr 'psepho', got: {msg}"
    );
}

// ─── AC-PROV-10: step recorded as "failed" re-runs on the next apply ──────────

#[test]
fn ac_prov_10_failed_step_reruns_not_skipped() {
    use cbox::core::provision::hash_step;
    use cbox::core::state_store::AppliedStep;

    // Compute the hash for the step
    let step = shell_step("bundle install");
    let h = hash_step(&step, Path::new(".")).unwrap();

    // Pre-load state that records step 0 as "failed" (with the same hash)
    // This simulates a previous run that failed at step 0.
    let mut initial_state = ProvisionState::new();
    initial_state.set_step(AppliedStep {
        idx: 0,
        step_type: "shell".to_string(),
        hash: h,
        applied_at: 0,
        result: "failed".to_string(),
    });

    let store = MemoryStateStore::with_state(initial_state);

    // This time the step succeeds
    let runner = MockRunner::new().with_default(MockResponse::ok("Bundle complete!"));

    let steps = vec![step];
    let plan = ProvisionPlan {
        name: "electionbuddy",
        steps: &steps,
        boxfile_dir: Path::new("."),
        backend: &Backend::Podman,
        force: false,
        redo: &[],
        dry_run: false,
    };

    let mut state = store.read("electionbuddy", &runner).unwrap();
    let results = provision(&plan, &store, &runner, &mut state).unwrap();

    // Must NOT be skipped — must re-run despite hash match
    assert_eq!(
        results[0].status, "ran",
        "a step recorded as 'failed' must re-run on next apply, not be skipped"
    );

    // Runner must have been called (the step actually executed)
    assert!(
        runner.call_count() > 0,
        "runner should have been called for the re-run"
    );

    // After success the state should record "ok"
    let written = store.last_written_state().expect("state should be written");
    let recorded = written.steps.iter().find(|s| s.idx == 0).unwrap();
    assert_eq!(
        recorded.result, "ok",
        "after success the state result should be 'ok'"
    );
}

// ─── AC-PROV-11: step recorded as "ok" with matching hash IS skipped ──────────

#[test]
fn ac_prov_11_ok_step_with_matching_hash_is_skipped() {
    use cbox::core::provision::hash_step;
    use cbox::core::state_store::AppliedStep;

    let step = shell_step("echo hello");
    let h = hash_step(&step, Path::new(".")).unwrap();

    let mut initial_state = ProvisionState::new();
    initial_state.set_step(AppliedStep {
        idx: 0,
        step_type: "shell".to_string(),
        hash: h,
        applied_at: 0,
        result: "ok".to_string(),
    });

    let store = MemoryStateStore::with_state(initial_state);
    let runner = MockRunner::new().with_default(MockResponse::ok(""));

    let steps = vec![step];
    let plan = ProvisionPlan {
        name: "web-dev",
        steps: &steps,
        boxfile_dir: Path::new("."),
        backend: &Backend::Podman,
        force: false,
        redo: &[],
        dry_run: false,
    };

    let mut state = store.read("web-dev", &runner).unwrap();
    let results = provision(&plan, &store, &runner, &mut state).unwrap();

    assert_eq!(
        results[0].status, "skipped",
        "step with result='ok' and matching hash must be skipped"
    );
    assert_eq!(runner.call_count(), 0, "no runner calls for a skipped step");
}

// ─── Idempotency proof (G-IDEMPOTENT) ────────────────────────────────────────

#[test]
fn g_idempotent_second_apply_makes_zero_run_spawns() {
    // First apply
    let steps = vec![
        shell_step("rustup default stable"),
        shell_step("cargo install just"),
    ];

    let runner1 = MockRunner::new().with_default(MockResponse::ok(""));
    let store1 = MemoryStateStore::empty();

    let plan1 = ProvisionPlan {
        name: "web-dev",
        steps: &steps,
        boxfile_dir: Path::new("."),
        backend: &Backend::Podman,
        force: false,
        redo: &[],
        dry_run: false,
    };

    let mut state1 = ProvisionState::new();
    let results1 = provision(&plan1, &store1, &runner1, &mut state1).unwrap();
    assert_eq!(results1[0].status, "ran");
    assert_eq!(results1[1].status, "ran");

    // Second apply with the persisted state
    let persisted = store1.last_written_state().unwrap();
    let store2 = MemoryStateStore::with_state(persisted);
    let runner2 = MockRunner::new().with_default(MockResponse::ok(""));

    let mut state2 = store2.read("web-dev", &runner2).unwrap();
    let plan2 = ProvisionPlan {
        name: "web-dev",
        steps: &steps,
        boxfile_dir: Path::new("."),
        backend: &Backend::Podman,
        force: false,
        redo: &[],
        dry_run: false,
    };

    let results2 = provision(&plan2, &store2, &runner2, &mut state2).unwrap();
    assert_eq!(results2[0].status, "skipped");
    assert_eq!(results2[1].status, "skipped");

    // Zero RUN spawns (the runner should have zero calls for the second apply)
    assert_eq!(
        runner2.call_count(),
        0,
        "second apply should make zero provision spawns"
    );
}
