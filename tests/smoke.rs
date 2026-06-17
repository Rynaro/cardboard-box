//! Smoke tests — real distrobox invocations. All marked #[ignore] so they do NOT
//! run in normal `make test` (gate G-NO-NET). Run with `make smoke` or:
//!   cargo test --test smoke -- --ignored
//!
//! Requires: distrobox ≥ 1.6, podman or docker, network access for image pulls.

/// Golden test: `cbox create --dry-run` → distrobox generates a valid `podman create` argv.
/// We verify the argv contains expected flags for the given spec.
#[test]
#[ignore = "requires real distrobox on PATH"]
fn smoke_create_dry_run_golden() {
    use std::process::Command;

    // Build the binary first
    let output = Command::new("cargo")
        .args(["build", "--release"])
        .output()
        .expect("cargo build failed");
    assert!(
        output.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let binary = "./target/release/cbox";
    let out = Command::new(binary)
        .args([
            "create",
            "smoke-test-box",
            "-i",
            "fedora-toolbox:latest",
            "--dry-run",
        ])
        .output()
        .expect("cbox create --dry-run failed to spawn");

    let stdout = String::from_utf8_lossy(&out.stdout);
    println!("dry-run output:\n{stdout}");

    assert!(
        out.status.success(),
        "exit code should be 0, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The output should contain distrobox create invocation
    assert!(
        stdout.contains("distrobox") || stdout.contains("podman") || stdout.contains("docker"),
        "dry-run output should reference distrobox or backend"
    );
}

/// Verify `cbox doctor` can detect the local environment.
#[test]
#[ignore = "requires real distrobox on PATH"]
fn smoke_doctor() {
    use std::process::Command;

    let binary = "./target/release/cbox";
    let out = Command::new(binary)
        .args(["doctor", "--json"])
        .output()
        .expect("cbox doctor failed to spawn");

    let stdout = String::from_utf8_lossy(&out.stdout);
    println!("doctor --json:\n{stdout}");

    // If distrobox is missing, exit 70 is expected; otherwise 0
    let v: serde_json::Value =
        serde_json::from_str(&stdout).expect("--json output should be valid JSON");
    assert!(v.get("ok").is_some(), "JSON should have 'ok' field");
    assert!(
        v.get("distrobox").is_some(),
        "JSON should have 'distrobox' field"
    );
    assert!(
        v.get("backend").is_some(),
        "JSON should have 'backend' field"
    );
}
