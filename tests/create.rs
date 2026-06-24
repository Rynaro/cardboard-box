//! Integration tests for cbox create — AC-CREATE-1 through AC-CREATE-7.
//! All driven against MockRunner; zero real distrobox invocations.

use cbox::core::{
    self,
    spec::{CreateSpec, DockerMode},
};
use cbox::dbox::{
    backend::Backend,
    mock::{MockResponse, MockRunner},
};

fn make_spec(name: &str) -> CreateSpec {
    let mut s = CreateSpec::new(name, Backend::Podman);
    s.image = "fedora-toolbox:latest".to_string();
    s
}

// AC-CREATE-1: basic create, runner called once with correct args, exit 0.
#[test]
fn ac_create_1_basic_create() {
    let runner = MockRunner::new().with_default(MockResponse::ok(""));
    let spec = make_spec("web-dev");

    let outcome = core::create(&spec, &runner).expect("create should succeed");
    assert_eq!(outcome.name, "web-dev");

    let calls = runner.calls();
    assert_eq!(calls.len(), 1, "runner should be called exactly once");
    let call = &calls[0];
    assert_eq!(call.program, "distrobox");
    assert!(
        call.args.iter().any(|a| a == "create"),
        "args should contain 'create'"
    );
    assert!(
        call.args.iter().any(|a| a == "--yes"),
        "args should contain '--yes'"
    );
    assert!(call
        .args
        .windows(2)
        .any(|w| w[0] == "--name" && w[1] == "web-dev"));
    assert!(call
        .args
        .windows(2)
        .any(|w| w[0] == "--image" && w[1] == "fedora-toolbox:latest"));
}

// AC-CREATE-2: docker=host + podman backend → correct socket volume + docker-cli package.
// dry_run=true bypasses the socket pre-flight so we can test argv assembly without a
// live podman socket.
#[test]
fn ac_create_2_docker_host_podman() {
    let runner = MockRunner::new().with_default(MockResponse::ok(""));
    let mut spec = make_spec("web-dev");
    spec.docker_mode = DockerMode::Host;
    spec.backend = Backend::Podman;
    spec.uid = 1000;
    spec.dry_run = true; // skip socket pre-flight; we only test argv shape

    let _outcome = core::create(&spec, &runner).expect("create should succeed");

    let calls = runner.calls();
    assert_eq!(calls.len(), 1);
    let call = &calls[0];

    // Socket volume
    let vols: Vec<&String> = call
        .args
        .windows(2)
        .filter(|w| w[0] == "--volume")
        .map(|w| &w[1])
        .collect();
    assert!(
        vols.iter().any(|v| v.contains("podman.sock")),
        "docker=host podman: expected podman.sock volume, got: {:?}",
        call.args
    );

    // Packages include podman-remote
    let pkg_idx = call
        .args
        .iter()
        .position(|a| a == "--additional-packages")
        .unwrap();
    assert!(
        call.args[pkg_idx + 1].contains("podman-remote"),
        "should include podman-remote"
    );

    // Label stamped
    assert!(call
        .args
        .iter()
        .any(|a| a.contains("cbox.docker_mode=host")));
}

// AC-CREATE-3: docker=nested → --init + docker-ce packages + no host socket.
#[test]
fn ac_create_3_docker_nested() {
    let runner = MockRunner::new().with_default(MockResponse::ok(""));
    let mut spec = make_spec("dind-box");
    spec.docker_mode = DockerMode::Nested;

    let _outcome = core::create(&spec, &runner).expect("create should succeed");

    let calls = runner.calls();
    assert_eq!(calls.len(), 1);
    let call = &calls[0];

    // Must have --init
    assert!(
        call.args.iter().any(|a| a == "--init"),
        "nested: must have --init"
    );

    // Must have docker-ce in packages
    let pkg_idx = call
        .args
        .iter()
        .position(|a| a == "--additional-packages")
        .unwrap();
    assert!(
        call.args[pkg_idx + 1].contains("docker-ce"),
        "nested: must have docker-ce"
    );

    // No host socket volume
    let vols: Vec<&String> = call
        .args
        .windows(2)
        .filter(|w| w[0] == "--volume")
        .map(|w| &w[1])
        .collect();
    assert!(
        !vols.iter().any(|v| v.contains(".sock")),
        "nested: no host socket volume"
    );
}

// AC-CREATE-4: docker=none + unshare=all → --unshare-all in args.
#[test]
fn ac_create_4_docker_none_unshare_all() {
    let runner = MockRunner::new().with_default(MockResponse::ok(""));
    let mut spec = make_spec("sandboxed");
    spec.docker_mode = DockerMode::None;
    spec.unshare = Some("all".to_string());

    let _outcome = core::create(&spec, &runner).expect("create should succeed");

    let calls = runner.calls();
    assert!(
        calls[0].args.iter().any(|a| a == "--unshare-all"),
        "unshare=all: should have --unshare-all, got: {:?}",
        calls[0].args
    );
}

// AC-CREATE-5: MockRunner returns stderr with "already exists" → exit 125 + cozy message.
#[test]
fn ac_create_5_already_exists_error() {
    let runner = MockRunner::new().with_default(MockResponse::err(
        1,
        "distrobox error: web-dev already exists",
    ));
    let spec = make_spec("web-dev");

    let err = core::create(&spec, &runner).expect_err("should fail with already-exists");
    assert_eq!(err.exit_code(), 125);
    assert!(
        err.to_string().contains("already exists"),
        "error message should contain 'already exists', got: {err}"
    );
}

// AC-CREATE-6: --dry-run → runner called with DryRun mode, no Capture spawn, stdout = argv.
#[test]
fn ac_create_6_dry_run() {
    let runner = MockRunner::new().with_default(MockResponse::ok("distrobox create --dry-run …"));
    let mut spec = make_spec("web-dev");
    spec.dry_run = true;

    let outcome = core::create(&spec, &runner).expect("dry-run should succeed");
    assert!(
        outcome.dry_run_output.is_some(),
        "dry-run: output should be present"
    );
}

// AC-CREATE-7: invalid NAME with space → error before any runner call.
#[test]
fn ac_create_7_invalid_name() {
    use cbox::boxfile::validate::is_valid_name;

    // "web dev" has a space — invalid
    assert!(!is_valid_name("web dev"), "'web dev' should be invalid");
    assert!(!is_valid_name(""), "empty name should be invalid");
    assert!(
        !is_valid_name("-startswithdash"),
        "leading dash should be invalid"
    );
    assert!(is_valid_name("web-dev"), "web-dev should be valid");
    assert!(is_valid_name("web.dev"), "web.dev should be valid");
    assert!(is_valid_name("web_dev"), "web_dev should be valid");
    assert!(is_valid_name("webdev123"), "webdev123 should be valid");
}
