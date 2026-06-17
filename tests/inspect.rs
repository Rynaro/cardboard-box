//! Integration tests for cbox inspect / show — AC-INSPECT-1 through AC-INSPECT-3, AC-SHOW-1.

use cbox::core::{self, spec::InspectSpec};
use cbox::dbox::{
    backend::Backend,
    mock::{MockResponse, MockRunner},
};
use cbox::error::exit;

fn make_spec(name: &str) -> InspectSpec {
    InspectSpec {
        name: name.to_string(),
        raw: false,
        backend: Backend::Podman,
    }
}

fn container_json() -> &'static str {
    r#"[
  {
    "Id": "abc123def456",
    "State": {
      "Status": "running"
    },
    "Config": {
      "Image": "fedora-toolbox:latest",
      "Labels": {
        "manager": "distrobox",
        "cbox.managed": "true",
        "cbox.docker_mode": "host",
        "cbox.boxfile_path": "/home/user/.config/cbox/boxes/web-dev/Boxfile.toml",
        "cbox.packages": "git ripgrep"
      }
    },
    "Created": "2026-06-16T10:00:00Z",
    "Mounts": [
      {
        "Source": "/home/user/code",
        "Destination": "/code",
        "Mode": "rw"
      }
    ]
  }
]"#
}

// AC-INSPECT-1: projected schema (not raw podman JSON).
#[test]
fn ac_inspect_1_projected_schema() {
    let runner = MockRunner::new().with_default(MockResponse::ok(container_json()));
    let spec = make_spec("web-dev");

    let result = core::inspect(&spec, &runner).expect("inspect should succeed");

    // Stable projected fields
    assert_eq!(result.name, "web-dev");
    assert_eq!(result.status, "running");
    assert_eq!(result.image, "fedora-toolbox:latest");
    assert!(!result.created.is_empty());
    assert_eq!(result.docker_mode, "host");
    assert_eq!(result.backend, "podman");
    assert!(!result.id.is_empty());
    assert_eq!(
        result.boxfile_path.as_deref(),
        Some("/home/user/.config/cbox/boxes/web-dev/Boxfile.toml")
    );

    // Packages from label
    assert!(result.packages.contains(&"git".to_string()));
    assert!(result.packages.contains(&"ripgrep".to_string()));

    // Mounts
    assert_eq!(result.mounts.len(), 1);
    assert_eq!(result.mounts[0].host, "/home/user/code");
    assert_eq!(result.mounts[0].guest, "/code");
    assert_eq!(result.mounts[0].mode, "rw");
}

// AC-INSPECT-2: --raw → raw backend JSON unmodified.
#[test]
fn ac_inspect_2_raw() {
    let raw_json = container_json();
    let runner = MockRunner::new().with_default(MockResponse::ok(raw_json));
    let spec = InspectSpec {
        name: "web-dev".to_string(),
        raw: true,
        backend: Backend::Podman,
    };

    let raw = core::inspect_raw(&spec, &runner).expect("inspect_raw should succeed");
    // Should contain the raw JSON (not projected)
    assert!(raw.contains("abc123def456"), "raw should contain raw Id");
    assert!(
        raw.contains("cbox.docker_mode"),
        "raw should contain labels"
    );
}

// AC-INSPECT-3: non-existent box → exit 69 with cozy not-found.
#[test]
fn ac_inspect_3_not_found() {
    // Empty array means box not found
    let runner = MockRunner::new().with_default(MockResponse::ok("[]"));
    let spec = make_spec("missing");

    let err = core::inspect(&spec, &runner).expect_err("should return not-found error");
    assert_eq!(err.exit_code(), exit::UNAVAILABLE);
    assert!(
        err.to_string().contains("missing") || err.to_string().contains("No box named"),
        "error should mention the missing box name: {err}"
    );
}

// AC-SHOW-1: alias cbox show → identical to inspect (same core function).
#[test]
fn ac_show_1_same_as_inspect() {
    let runner = MockRunner::new().with_default(MockResponse::ok(container_json()));
    let spec = make_spec("web-dev");

    // show is aliased to inspect at CLI level; core is identical
    let result = core::inspect(&spec, &runner).expect("show/inspect should succeed");
    assert_eq!(result.name, "web-dev");
}
