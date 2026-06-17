//! Integration tests for cbox doctor — AC-DOCTOR-1 through AC-DOCTOR-3.

use cbox::core::{self, spec::DoctorSpec};
use cbox::dbox::mock::{MockMatcher, MockResponse, MockRunner};
use cbox::error::exit;

fn good_runner() -> MockRunner {
    MockRunner::new()
        // distrobox version → present
        .with_matcher(
            MockMatcher::new(MockResponse::ok("distrobox: 1.8.2.4"))
                .with_program("distrobox")
                .with_args_contain(vec!["version".to_string()]),
        )
        // podman --version → present
        .with_matcher(
            MockMatcher::new(MockResponse::ok("podman version 5.8.2"))
                .with_program("podman")
                .with_args_contain(vec!["--version".to_string()]),
        )
        // podman info → reachable
        .with_matcher(
            MockMatcher::new(MockResponse::ok("host:\n  os: linux"))
                .with_program("podman")
                .with_args_contain(vec!["info".to_string()]),
        )
        // docker --version → not present (default returns empty/exit 1)
        .with_matcher(
            MockMatcher::new(MockResponse::err(127, "command not found: docker"))
                .with_program("docker")
                .with_args_contain(vec!["--version".to_string()]),
        )
        .with_default(MockResponse::ok(""))
}

// AC-DOCTOR-1: distrobox 1.8.2.4 + podman info exit 0 → ok:true, backend:podman, supported:true.
#[test]
fn ac_doctor_1_good_env() {
    let runner = good_runner();
    let spec = DoctorSpec {
        backend_override: None,
    };

    let result = core::doctor(&spec, &runner).expect("doctor should succeed");
    assert!(result.ok, "ok should be true");
    assert_eq!(result.backend.selected.as_deref(), Some("podman"));
    assert!(result.distrobox.present);
    assert!(result.distrobox.supported, "1.8.2.4 is above floor 1.6");
    assert_eq!(result.distrobox.version.as_deref(), Some("1.8.2.4"));
}

// AC-DOCTOR-2: distrobox absent → exit 70 with install guidance.
#[test]
fn ac_doctor_2_distrobox_missing() {
    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::err(127, "command not found: distrobox"))
                .with_program("distrobox")
                .with_args_contain(vec!["version".to_string()]),
        )
        .with_default(MockResponse::err(1, "not found"));

    let spec = DoctorSpec {
        backend_override: None,
    };
    let err = core::doctor(&spec, &runner).expect_err("should fail when distrobox missing");
    assert_eq!(err.exit_code(), exit::SOFTWARE, "exit 70 expected");
    assert!(
        err.to_string().contains("distrobox"),
        "error should mention distrobox"
    );
}

// AC-DOCTOR-3: distrobox 1.4 (below floor 1.6) → warning, not error, exit 0 if backend usable.
#[test]
fn ac_doctor_3_old_distrobox_version() {
    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::ok("distrobox: 1.4.2"))
                .with_program("distrobox")
                .with_args_contain(vec!["version".to_string()]),
        )
        .with_matcher(
            MockMatcher::new(MockResponse::ok("podman version 5.0"))
                .with_program("podman")
                .with_args_contain(vec!["--version".to_string()]),
        )
        .with_matcher(
            MockMatcher::new(MockResponse::ok("ok"))
                .with_program("podman")
                .with_args_contain(vec!["info".to_string()]),
        )
        .with_default(MockResponse::err(1, "not found"));

    let spec = DoctorSpec {
        backend_override: None,
    };
    let result = core::doctor(&spec, &runner).expect("doctor should succeed despite old version");

    assert!(result.distrobox.present, "distrobox is present");
    assert!(!result.distrobox.supported, "1.4.2 is below floor 1.6");
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.contains("1.6") || w.contains("floor") || w.contains("below")),
        "should warn about old version, warnings: {:?}",
        result.warnings
    );
    // Backend should still be usable
    assert_eq!(result.backend.selected.as_deref(), Some("podman"));
}
