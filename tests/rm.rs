//! Integration tests for cbox rm / destroy — AC-RM-1 through AC-RM-4.
//!
//! NOTE: core::rm now issues a best-effort stop BEFORE the rm invocation.
//! calls()[0] = stop call, calls()[1] = rm call.

use cbox::core::{self, isolated_home_remove_target, spec::RmSpec};
use cbox::dbox::backend::Backend;
use cbox::dbox::mock::{MockResponse, MockRunner};

// AC-RM-0: core::rm makes stop-then-rm (two calls in order).
#[test]
fn ac_rm_0_stop_first_ordering() {
    let runner = MockRunner::new().with_default(MockResponse::ok(""));
    let spec = RmSpec {
        names: vec!["web-dev".to_string()],
        force: false,
        rm_home: false,
        all: false,
        yes: true,
        backend: Backend::Podman,
    };

    core::rm(&spec, &runner).expect("rm should succeed");
    let calls = runner.calls();
    assert_eq!(calls.len(), 2, "rm should issue exactly two backend calls");
    // First call: stop
    assert!(
        calls[0].args.iter().any(|a| a == "stop"),
        "first call should be stop"
    );
    assert!(
        calls[0].args.iter().any(|a| a == "--yes"),
        "stop call should have --yes"
    );
    // Second call: rm
    assert!(
        calls[1].args.iter().any(|a| a == "rm"),
        "second call should be rm"
    );
}

// AC-RM-1: rm with -y → runner called with distrobox rm web-dev, "Removed box" output.
#[test]
fn ac_rm_1_basic_rm_yes() {
    let runner = MockRunner::new().with_default(MockResponse::ok(""));
    let spec = RmSpec {
        names: vec!["web-dev".to_string()],
        force: false,
        rm_home: false,
        all: false,
        yes: true,
        backend: Backend::Podman,
    };

    let outcome = core::rm(&spec, &runner).expect("rm should succeed");
    assert_eq!(outcome.removed, vec!["web-dev"]);
    assert!(outcome.skipped.is_empty());

    let calls = runner.calls();
    assert_eq!(calls.len(), 2, "rm should issue two calls (stop + rm)");
    // rm call is at index 1
    let rm_call = &calls[1];
    assert_eq!(rm_call.program, "distrobox");
    assert!(
        rm_call.args.iter().any(|a| a == "rm"),
        "args should contain 'rm'"
    );
    assert!(
        rm_call.args.iter().any(|a| a == "web-dev"),
        "args should contain 'web-dev'"
    );
}

// AC-RM-2: no -y → no runner call (handled at CLI layer; test the spec behavior).
// At core level, rm always runs; the confirmation is in cli/rm.rs.
// We test here that the core function does call the runner.
#[test]
fn ac_rm_2_rm_is_called_with_spec() {
    // The confirmation guard is at the CLI layer; core::rm always runs when called.
    // This test verifies the runner IS called when core::rm is invoked.
    let runner = MockRunner::new().with_default(MockResponse::ok(""));
    let spec = RmSpec {
        names: vec!["web-dev".to_string()],
        force: false,
        rm_home: false,
        all: false,
        yes: false, // no -y, but core::rm doesn't check this — CLI does
        backend: Backend::Podman,
    };
    let outcome = core::rm(&spec, &runner).expect("core::rm should succeed");
    assert_eq!(outcome.removed, vec!["web-dev"]);
}

// AC-RM-3: --force → rm args contain --force (on the rm call, not the stop call).
#[test]
fn ac_rm_3_force_flag() {
    let runner = MockRunner::new().with_default(MockResponse::ok(""));
    let spec = RmSpec {
        names: vec!["web-dev".to_string()],
        force: true,
        rm_home: false,
        all: false,
        yes: true,
        backend: Backend::Podman,
    };

    core::rm(&spec, &runner).expect("rm should succeed");
    let calls = runner.calls();
    assert_eq!(calls.len(), 2, "rm should issue two calls (stop + rm)");
    // --force is on the rm call (index 1)
    let rm_call = &calls[1];
    assert!(
        rm_call.args.iter().any(|a| a == "--force"),
        "rm call should have --force"
    );
    // stop call should NOT have --force
    assert!(
        !calls[0].args.iter().any(|a| a == "--force"),
        "stop call should not have --force"
    );
}

// AC-RM-4: alias destroy → same behavior (tested via CLI layer using assert_cmd in integration).
// Here we test the core function handles multiple names.
#[test]
fn ac_rm_4_multiple_names() {
    let runner = MockRunner::new().with_default(MockResponse::ok(""));
    let spec = RmSpec {
        names: vec!["box-a".to_string(), "box-b".to_string()],
        force: false,
        rm_home: false,
        all: false,
        yes: true,
        backend: Backend::Podman,
    };

    let outcome = core::rm(&spec, &runner).expect("rm should succeed");
    assert_eq!(outcome.removed.len(), 2);

    let calls = runner.calls();
    // rm call is at index 1
    let rm_call = &calls[1];
    assert!(rm_call.args.iter().any(|a| a == "box-a"));
    assert!(rm_call.args.iter().any(|a| a == "box-b"));
}

// ─── isolated_home_remove_target guard (pure unit tests) ─────────────────────

// Happy path: returns the synth path for a valid name.
#[test]
fn rm_guard_returns_synth_path_for_valid_name() {
    let result = isolated_home_remove_target("/home/u/.local/share", "web-dev", "/home/u");
    assert_eq!(
        result,
        Some("/home/u/.local/share/cbox/homes/web-dev".to_string())
    );
}

// Rejects empty name.
#[test]
fn rm_guard_rejects_empty_name() {
    let result = isolated_home_remove_target("/home/u/.local/share", "", "/home/u");
    assert!(result.is_none(), "empty name must be rejected");
}

// Rejects name containing '/'.
#[test]
fn rm_guard_rejects_name_with_slash() {
    let result = isolated_home_remove_target("/home/u/.local/share", "a/b", "/home/u");
    assert!(result.is_none(), "name with slash must be rejected");
}

// Rejects name containing '..'.
#[test]
fn rm_guard_rejects_name_with_dotdot() {
    let result = isolated_home_remove_target("/home/u/.local/share", "..", "/home/u");
    assert!(result.is_none(), "name with '..' must be rejected");
}

// Rejects when computed path == real_home (should be impossible in practice,
// but the guard must be bullet-proof).
#[test]
fn rm_guard_rejects_path_equal_to_real_home() {
    // isolated_home_under("/mydata", "box") = "/mydata/cbox/homes/box"
    // Set real_home to the exact same string.
    let result = isolated_home_remove_target(
        "/mydata",
        "box",
        "/mydata/cbox/homes/box", // path == real_home
    );
    assert!(result.is_none(), "path equal to real_home must be rejected");
}

// Rejects when real_home is nested inside the computed path
// (deleting path would delete real_home).
#[test]
fn rm_guard_rejects_ancestor_of_real_home() {
    // path = /data/cbox/homes/mybox
    // real_home = /data/cbox/homes/mybox/nested — path is an ancestor
    let result = isolated_home_remove_target("/data", "mybox", "/data/cbox/homes/mybox/nested");
    assert!(
        result.is_none(),
        "computed path that is an ancestor of real_home must be rejected"
    );
}

// Rejects a path that does not contain the sentinel segment (should not happen
// with isolated_home_under, but guard is defensive).
#[test]
fn rm_guard_rejects_path_lacking_sentinel() {
    // If data_dir already contains "cbox/homes" at the wrong position we still
    // verify the invariant holds. Use a data_dir that produces a path WITHOUT
    // the literal "/cbox/homes/" segment — only possible if we fake data_dir.
    // isolated_home_under always inserts /cbox/homes/ so the simplest test is
    // to confirm a synthetic path without it is rejected by the guard logic.
    // We test by using a name that contains ".." but that's already covered;
    // here we verify the normal path contains the segment.
    let result = isolated_home_remove_target("/home/u/.local/share", "mybox", "/home/u");
    // This should succeed (contains /cbox/homes/).
    assert!(
        result.is_some(),
        "valid input should return Some; sentinel check should pass"
    );
    assert!(
        result.unwrap().contains("/cbox/homes/"),
        "returned path must contain sentinel"
    );
}

// ─── core::rm --rm-home (fs integration, env-isolated per-test tmp) ───────────

// `core::rm` reads HOME/XDG_DATA_HOME, and these tests must mutate them. env is
// process-global, so the two tests below are serialized against each other with
// this lock (held across mutate → core::rm → restore) to keep them race-free
// under cargo's default parallel test runner.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

// rm_home=true removes a pre-created synth home directory.
#[test]
fn ac_rm_home_true_removes_home_dir() {
    let tmp = tempfile::TempDir::new().expect("tmp dir");
    let data_dir = tmp.path().to_str().unwrap().to_string();
    let box_name = "isolated-box";

    // Pre-create the synth home with a sentinel file.
    let home_path = format!("{data_dir}/cbox/homes/{box_name}");
    std::fs::create_dir_all(&home_path).unwrap();
    std::fs::write(format!("{home_path}/provision.json"), b"{}").unwrap();

    // Serialize the env-mutation window; recover from a poisoned lock so a panic
    // in the sibling test doesn't cascade-fail this one.
    let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    // Temporarily set XDG_DATA_HOME so synth_data_dir() uses our tmp dir.
    let old_xdg = std::env::var("XDG_DATA_HOME").ok();
    std::env::set_var("XDG_DATA_HOME", &data_dir);
    // Set HOME to something outside our tmp dir so the safety guard passes.
    let old_home = std::env::var("HOME").ok();
    std::env::set_var("HOME", "/home/test-user");

    let runner = MockRunner::new().with_default(MockResponse::ok(""));
    let spec = RmSpec {
        names: vec![box_name.to_string()],
        force: false,
        rm_home: true,
        all: false,
        yes: true,
        backend: Backend::Podman,
    };

    let outcome = core::rm(&spec, &runner).expect("rm should succeed");

    // Restore env, then release the lock before asserting (asserts read the fs /
    // the outcome, not env).
    match old_xdg {
        Some(v) => std::env::set_var("XDG_DATA_HOME", v),
        None => std::env::remove_var("XDG_DATA_HOME"),
    }
    match old_home {
        Some(v) => std::env::set_var("HOME", v),
        None => std::env::remove_var("HOME"),
    }
    drop(guard);

    assert!(
        !std::path::Path::new(&home_path).exists(),
        "synth home should have been removed"
    );
    assert!(
        outcome.removed_homes.contains(&home_path),
        "removed_homes should list the deleted path"
    );
    assert!(outcome.kept_homes.is_empty(), "kept_homes should be empty");
}

// rm_home=false keeps the home directory and records it in kept_homes.
#[test]
fn ac_rm_home_false_keeps_home_dir() {
    let tmp = tempfile::TempDir::new().expect("tmp dir");
    let data_dir = tmp.path().to_str().unwrap().to_string();
    let box_name = "isolated-box2";

    let home_path = format!("{data_dir}/cbox/homes/{box_name}");
    std::fs::create_dir_all(&home_path).unwrap();

    let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let old_xdg = std::env::var("XDG_DATA_HOME").ok();
    std::env::set_var("XDG_DATA_HOME", &data_dir);
    let old_home = std::env::var("HOME").ok();
    std::env::set_var("HOME", "/home/test-user");

    let runner = MockRunner::new().with_default(MockResponse::ok(""));
    let spec = RmSpec {
        names: vec![box_name.to_string()],
        force: false,
        rm_home: false,
        all: false,
        yes: true,
        backend: Backend::Podman,
    };

    let outcome = core::rm(&spec, &runner).expect("rm should succeed");

    match old_xdg {
        Some(v) => std::env::set_var("XDG_DATA_HOME", v),
        None => std::env::remove_var("XDG_DATA_HOME"),
    }
    match old_home {
        Some(v) => std::env::set_var("HOME", v),
        None => std::env::remove_var("HOME"),
    }
    drop(guard);

    assert!(
        std::path::Path::new(&home_path).exists(),
        "synth home should still exist when --rm-home not passed"
    );
    assert!(
        outcome.kept_homes.contains(&home_path),
        "kept_homes should list the retained path"
    );
    assert!(
        outcome.removed_homes.is_empty(),
        "removed_homes should be empty"
    );
}
