//! Integration tests for cbox rm / destroy — AC-RM-1 through AC-RM-4.
//!
//! NOTE: core::rm now issues a best-effort stop BEFORE the rm invocation.
//! calls()[0] = stop call, calls()[1] = rm call.

use cbox::core::{self, spec::RmSpec};
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
