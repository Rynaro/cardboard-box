//! Integration tests for cbox stop — AC-STOP-1 through AC-STOP-3.

use cbox::core::{self, spec::StopSpec};
use cbox::dbox::backend::Backend;
use cbox::dbox::mock::{MockResponse, MockRunner};

// AC-STOP-1: stop a named box → runner called with distrobox stop --yes <name>.
#[test]
fn ac_stop_1_basic_stop() {
    let runner = MockRunner::new().with_default(MockResponse::ok(""));
    let spec = StopSpec {
        names: vec!["web-dev".to_string()],
        all: false,
        backend: Backend::Podman,
    };

    let outcome = core::stop(&spec, &runner).expect("stop should succeed");
    assert_eq!(outcome.stopped, vec!["web-dev"]);

    let calls = runner.calls();
    assert_eq!(calls.len(), 1);
    let call = &calls[0];
    assert_eq!(call.program, "distrobox");
    assert!(
        call.args.iter().any(|a| a == "stop"),
        "args should contain 'stop'"
    );
    assert!(
        call.args.iter().any(|a| a == "--yes"),
        "args should contain '--yes'"
    );
    assert!(
        call.args.iter().any(|a| a == "web-dev"),
        "args should contain 'web-dev'"
    );
}

// AC-STOP-2: stop --all → args contain --all.
#[test]
fn ac_stop_2_all_flag() {
    let runner = MockRunner::new().with_default(MockResponse::ok(""));
    let spec = StopSpec {
        names: vec![],
        all: true,
        backend: Backend::Podman,
    };

    core::stop(&spec, &runner).expect("stop --all should succeed");
    let calls = runner.calls();
    assert_eq!(calls.len(), 1);
    let call = &calls[0];
    assert_eq!(call.program, "distrobox");
    assert!(
        call.args.iter().any(|a| a == "stop"),
        "args should contain 'stop'"
    );
    assert!(
        call.args.iter().any(|a| a == "--yes"),
        "args should contain '--yes'"
    );
    assert!(
        call.args.iter().any(|a| a == "--all"),
        "args should contain '--all'"
    );
}

// AC-STOP-3: stop with multiple names.
#[test]
fn ac_stop_3_multiple_names() {
    let runner = MockRunner::new().with_default(MockResponse::ok(""));
    let spec = StopSpec {
        names: vec!["box-a".to_string(), "box-b".to_string()],
        all: false,
        backend: Backend::Podman,
    };

    let outcome = core::stop(&spec, &runner).expect("stop should succeed");
    assert_eq!(outcome.stopped.len(), 2);

    let call = &runner.calls()[0];
    assert!(call.args.iter().any(|a| a == "box-a"));
    assert!(call.args.iter().any(|a| a == "box-b"));
}
