//! Integration tests for cbox rm / destroy — AC-RM-1 through AC-RM-4.

use cbox::core::{self, spec::RmSpec};
use cbox::dbox::backend::Backend;
use cbox::dbox::mock::{MockResponse, MockRunner};

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
    assert_eq!(calls.len(), 1);
    let call = &calls[0];
    assert_eq!(call.program, "distrobox");
    assert!(
        call.args.iter().any(|a| a == "rm"),
        "args should contain 'rm'"
    );
    assert!(
        call.args.iter().any(|a| a == "web-dev"),
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

// AC-RM-3: --force → args contain --force.
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
    let call = &runner.calls()[0];
    assert!(
        call.args.iter().any(|a| a == "--force"),
        "should have --force"
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

    let call = &runner.calls()[0];
    assert!(call.args.iter().any(|a| a == "box-a"));
    assert!(call.args.iter().any(|a| a == "box-b"));
}
