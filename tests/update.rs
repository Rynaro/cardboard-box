use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn update_help_exposes_check_mode() {
    Command::cargo_bin("cbox")
        .unwrap()
        .args(["update", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--check"));
}

#[test]
fn update_rejects_unknown_arguments_without_network() {
    Command::cargo_bin("cbox")
        .unwrap()
        .args(["update", "--not-a-real-flag"])
        .assert()
        .code(2);
}

#[cfg(unix)]
fn fake_curl(version: &str) -> TempDir {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new().unwrap();
    let script = dir.path().join("curl");
    std::fs::write(
        &script,
        format!("#!/bin/sh\nprintf '%s' '{{\"tag_name\":\"v{version}\",\"assets\":[]}}'\n"),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    dir
}

#[cfg(unix)]
#[test]
fn check_json_is_machine_readable_and_quiet_is_silent() {
    let bin = fake_curl("99.0.0");
    let output = Command::cargo_bin("cbox")
        .unwrap()
        .env("PATH", bin.path())
        .args(["--json", "update", "--check"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["update_available"], true);
    assert_eq!(value["updated"], false);

    Command::cargo_bin("cbox")
        .unwrap()
        .env("PATH", bin.path())
        .args(["--quiet", "update", "--check"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[cfg(unix)]
#[test]
fn global_dry_run_reports_plan_without_downloading_assets() {
    let bin = fake_curl("99.0.0");
    Command::cargo_bin("cbox")
        .unwrap()
        .env("PATH", bin.path())
        .args(["--dry-run", "update"])
        .assert()
        .success()
        .stdout(predicate::str::contains("would update"));
}
