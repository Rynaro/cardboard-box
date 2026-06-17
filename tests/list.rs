//! Integration tests for cbox list — AC-LIST-1 through AC-LIST-3.

use cbox::core;
use cbox::dbox::{
    backend::Backend,
    mock::{MockResponse, MockRunner},
};

fn two_box_json() -> &'static str {
    r#"[
  {
    "Id": "abc123",
    "Names": ["web-dev"],
    "State": "running",
    "Image": "fedora-toolbox:latest",
    "Labels": {
      "manager": "distrobox",
      "cbox.managed": "true",
      "cbox.docker_mode": "host"
    }
  },
  {
    "Id": "def456",
    "Names": ["rust-box"],
    "State": "exited",
    "Image": "ubuntu:22.04",
    "Labels": {
      "manager": "distrobox",
      "cbox.managed": "true",
      "cbox.docker_mode": "none"
    }
  }
]"#
}

// AC-LIST-1: --json path returns valid JSON with boxes length 2, each with required fields.
#[test]
fn ac_list_1_json_two_boxes() {
    let runner = MockRunner::new().with_default(MockResponse::ok(two_box_json()));
    let backend = Backend::Podman;

    let outcome = core::list_machine(&backend, &runner).expect("list_machine should succeed");
    assert_eq!(outcome.boxes.len(), 2);

    let first = &outcome.boxes[0];
    assert_eq!(first.name, "web-dev");
    assert_eq!(first.status, "running");
    assert_eq!(first.image, "fedora-toolbox:latest");
    assert_eq!(first.docker_mode, "host");
    assert!(first.cbox_managed);
    assert!(!first.id.is_empty());

    let second = &outcome.boxes[1];
    assert_eq!(second.name, "rust-box");
    assert_eq!(second.docker_mode, "none");
}

// AC-LIST-2: human path — table header includes NAME STATUS IMAGE DOCKER CBOX?.
// (We test the data is correct; rendering is in output.rs.)
#[test]
fn ac_list_2_human_table_data() {
    let runner = MockRunner::new().with_default(MockResponse::ok(two_box_json()));
    let backend = Backend::Podman;

    let outcome = core::list_machine(&backend, &runner).expect("list should succeed");
    // Check the DOCKER column comes from cbox.docker_mode label
    assert_eq!(outcome.boxes[0].docker_mode, "host");
    assert_eq!(outcome.boxes[1].docker_mode, "none");
}

// AC-LIST-3: box without cbox.docker_mode label → docker_mode is "unknown", no crash.
#[test]
fn ac_list_3_unknown_docker_mode() {
    let json = r#"[
  {
    "Id": "zzz999",
    "Names": ["mystery-box"],
    "State": "running",
    "Image": "alpine:latest",
    "Labels": {
      "manager": "distrobox"
    }
  }
]"#;
    let runner = MockRunner::new().with_default(MockResponse::ok(json));
    let backend = Backend::Podman;

    let outcome = core::list_machine(&backend, &runner).expect("list should succeed");
    assert_eq!(outcome.boxes.len(), 1);
    assert_eq!(outcome.boxes[0].docker_mode, "unknown");
    assert!(!outcome.boxes[0].cbox_managed);
}
