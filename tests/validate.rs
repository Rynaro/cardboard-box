use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn cbox() -> Command {
    Command::cargo_bin("cbox").expect("cbox binary not found")
}

#[test]
fn validates_default_boxfile_with_real_parser() {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("Boxfile.toml"),
        r#"
name = "project-dev"
image = "registry.fedoraproject.org/fedora-toolbox:latest"
packages = ["git", "make"]
docker = "none"

[[mounts]]
host = "/home/user/src"
guest = "/workspace"
mode = "rw"

[box]
isolated = true

[[provision]]
type = "shell"
run = "make bootstrap"

[env]
RUST_LOG = "info"

[secrets]
API_TOKEN = { persist = false, from = "keyring" }
"#,
    )
    .unwrap();

    cbox()
        .current_dir(dir.path())
        .arg("validate")
        .assert()
        .success()
        .stdout(predicate::str::contains("Boxfile.toml is valid"));
}

#[test]
fn explicit_file_and_json_report_warnings() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("dev.toml");
    std::fs::write(&path, "name = \"dev\"\nfuture_field = true\n").unwrap();

    let output = cbox()
        .args(["--json", "validate", "--file"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["ok"], true);
    assert_eq!(value["name"], "dev");
    assert_eq!(value["warnings"].as_array().unwrap().len(), 1);
}

#[test]
fn invalid_boxfile_exits_dataerr() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bad.toml");
    std::fs::write(&path, "name = \"bad name\"\n").unwrap();

    cbox()
        .args(["validate", "--file"])
        .arg(&path)
        .assert()
        .code(65)
        .stderr(predicate::str::contains("invalid"));
}

#[cfg(unix)]
#[test]
fn validation_never_invokes_backend_or_keyring_helpers() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("Boxfile.toml"), "name = \"offline\"\n").unwrap();
    let bin_dir = dir.path().join("bin");
    std::fs::create_dir(&bin_dir).unwrap();
    let marker = dir.path().join("called");
    for helper in ["distrobox", "podman", "docker", "secret-tool"] {
        let path = bin_dir.join(helper);
        std::fs::write(
            &path,
            "#!/bin/sh\ntouch \"$CBOX_VALIDATE_MARKER\"\nexit 99\n",
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    cbox()
        .current_dir(dir.path())
        .env("PATH", &bin_dir)
        .env("CBOX_VALIDATE_MARKER", &marker)
        .arg("validate")
        .assert()
        .success();
    assert!(!marker.exists(), "validate must remain parser-only");
}
