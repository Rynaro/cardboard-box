//! AC-DIFF-* — Boxfile vs live diff classification tests.
//! All purely functional: no runner, no mock needed.

use cbox::boxfile::model::{Boxfile, DockerModeField, MountEntry, MountMode};
use cbox::core::{diff::diff_boxfile_vs_live, spec::InspectResult};

fn base_boxfile(name: &str, image: &str) -> Boxfile {
    Boxfile {
        name: name.to_string(),
        image: image.to_string(),
        packages: vec![],
        docker: DockerModeField::None,
        mounts: vec![],
        sandbox: Default::default(),
        box_config: Default::default(),
        provision: vec![],
        secrets: std::collections::BTreeMap::new(),
        env: std::collections::BTreeMap::new(),
    }
}

fn base_live(name: &str, image: &str) -> InspectResult {
    InspectResult {
        name: name.to_string(),
        status: "running".to_string(),
        image: image.to_string(),
        created: "2026-06-16T00:00:00Z".to_string(),
        docker_mode: "none".to_string(),
        mounts: vec![],
        packages: vec![],
        backend: "podman".to_string(),
        id: "abc123".to_string(),
        boxfile_path: None,
        cbox_image: None,
        home: None,
        hostname: None,
    }
}

// AC-DIFF-1: equal Boxfile and live → Incremental, no fields
#[test]
fn ac_diff_1_equal_no_change() {
    let bf = base_boxfile("web-dev", "fedora-toolbox:latest");
    let live = base_live("web-dev", "fedora-toolbox:latest");
    let diff = diff_boxfile_vs_live(&bf, &live);
    assert_eq!(diff.class, "Incremental");
    assert!(
        diff.fields.is_empty(),
        "no diff fields expected, got: {:?}",
        diff.fields
    );
}

// AC-DIFF-2: differing image → Recreate naming 'image'
#[test]
fn ac_diff_2_image_change() {
    let bf = base_boxfile("web-dev", "fedora-toolbox:40");
    let live = base_live("web-dev", "fedora-toolbox:latest");
    let diff = diff_boxfile_vs_live(&bf, &live);
    assert_eq!(diff.class, "Recreate");
    let image_field = diff.fields.iter().find(|f| f.field == "image");
    assert!(image_field.is_some(), "should have an 'image' diff field");
    let f = image_field.unwrap();
    assert_eq!(f.class, "Recreate");
    assert_eq!(f.old, "fedora-toolbox:latest");
    assert_eq!(f.new, "fedora-toolbox:40");
}

// AC-DIFF-3: differing docker mode → Recreate naming 'docker'
#[test]
fn ac_diff_3_docker_mode_change() {
    let mut bf = base_boxfile("web-dev", "fedora-toolbox:latest");
    bf.docker = DockerModeField::Host;
    let live = base_live("web-dev", "fedora-toolbox:latest"); // docker_mode = "none"

    let diff = diff_boxfile_vs_live(&bf, &live);
    assert_eq!(diff.class, "Recreate");
    let docker_field = diff.fields.iter().find(|f| f.field == "docker");
    assert!(docker_field.is_some(), "should have a 'docker' diff field");
    assert_eq!(docker_field.unwrap().class, "Recreate");
}

// AC-DIFF-4: added mount → Recreate naming 'mounts'
#[test]
fn ac_diff_4_added_mount() {
    let mut bf = base_boxfile("web-dev", "fedora-toolbox:latest");
    bf.mounts = vec![MountEntry {
        host: "/data".to_string(),
        guest: "/data".to_string(),
        mode: MountMode::Rw,
    }];
    let live = base_live("web-dev", "fedora-toolbox:latest"); // no mounts

    let diff = diff_boxfile_vs_live(&bf, &live);
    assert_eq!(diff.class, "Recreate");
    let mount_field = diff.fields.iter().find(|f| f.field == "mounts");
    assert!(mount_field.is_some(), "should have a 'mounts' diff field");
    assert_eq!(mount_field.unwrap().class, "Recreate");
}

// AC-DIFF-5: only added packages → Incremental
#[test]
fn ac_diff_5_added_packages_incremental() {
    let mut bf = base_boxfile("web-dev", "fedora-toolbox:latest");
    bf.packages = vec!["git".to_string(), "ripgrep".to_string()];
    let mut live = base_live("web-dev", "fedora-toolbox:latest");
    live.packages = vec!["git".to_string()]; // ripgrep is new

    let diff = diff_boxfile_vs_live(&bf, &live);
    assert_eq!(diff.class, "Incremental");
    let pkg_field = diff
        .fields
        .iter()
        .find(|f| f.field == "packages" && f.class == "Incremental");
    assert!(
        pkg_field.is_some(),
        "should have an incremental 'packages' diff field"
    );
    assert!(pkg_field.unwrap().new.contains("ripgrep"));
}

// AC-DIFF-6: removed package → Recreate
#[test]
fn ac_diff_6_removed_package_recreate() {
    let mut bf = base_boxfile("web-dev", "fedora-toolbox:latest");
    bf.packages = vec!["git".to_string()]; // ripgrep removed
    let mut live = base_live("web-dev", "fedora-toolbox:latest");
    live.packages = vec!["git".to_string(), "ripgrep".to_string()];

    let diff = diff_boxfile_vs_live(&bf, &live);
    assert_eq!(diff.class, "Recreate", "package removal should be Recreate");
    let pkg_field = diff
        .fields
        .iter()
        .find(|f| f.field == "packages" && f.class == "Recreate");
    assert!(
        pkg_field.is_some(),
        "should have a Recreate 'packages' diff field"
    );
}

// AC-DIFF-7: sandbox.unshare not recoverable from live → assumed unchanged (no recreate)
#[test]
fn ac_diff_7_sandbox_unshare_assumed_unchanged() {
    // We don't store sandbox.unshare in live labels in v2.0.
    // The diff should NOT trigger a recreate for sandbox fields.
    use cbox::boxfile::model::{SandboxConfig, UnshareSpec};
    let mut bf = base_boxfile("web-dev", "fedora-toolbox:latest");
    bf.sandbox = SandboxConfig {
        unshare: UnshareSpec::List(vec!["netns".to_string()]),
        init: false,
    };
    let live = base_live("web-dev", "fedora-toolbox:latest");

    let diff = diff_boxfile_vs_live(&bf, &live);
    // sandbox.unshare is not recoverable from labels → assume unchanged → no recreate triggered
    assert_eq!(
        diff.class, "Incremental",
        "sandbox.unshare change should NOT trigger recreate (R5 — assume unchanged)"
    );
}
