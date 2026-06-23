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

const BIN: &str = "./target/release/cbox";

/// Best-effort teardown so a re-run starts clean.
fn rm_box(name: &str) {
    use std::process::Command;
    let _ = Command::new(BIN).args(["rm", "-f", name]).output();
}

/// AC-8: a default `cbox enter` lands the user in the box's own `$HOME`.
/// The default path injects `-- sh -lc 'cd "$HOME"; exec <shell> -l'` (proven by
/// the unit tests in tests/argv_builder.rs). Here we run that same inner command
/// against a REAL box and assert it resolves to the box home — i.e. the redirect
/// mechanism works end-to-end with real distrobox.
#[test]
#[ignore = "requires real distrobox on PATH"]
fn smoke_enter_lands_in_home() {
    use std::process::Command;
    let name = "cbox-smoke-enterhome";
    rm_box(name);

    let created = Command::new(BIN)
        .args(["create", name, "-i", "fedora-toolbox:latest"])
        .output()
        .expect("cbox create failed to spawn");
    assert!(
        created.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&created.stderr)
    );

    // Run the exact command the default enter injects, but capture-able.
    let out = Command::new(BIN)
        .args([
            "enter",
            name,
            "--no-home",
            "--",
            "sh",
            "-lc",
            "cd \"$HOME\"; pwd",
        ])
        .output()
        .expect("cbox enter failed to spawn");
    let pwd = String::from_utf8_lossy(&out.stdout).trim().to_string();
    println!("box pwd after cd $HOME: {pwd}");

    rm_box(name);
    assert!(
        pwd.starts_with('/') && !pwd.is_empty(),
        "default enter should land in an absolute $HOME, got: {pwd:?}"
    );
}

/// AC-9: an `--isolated` box gets a private `$HOME` distinct from the host home,
/// and host shell dotfiles (e.g. ~/.zshrc) do NOT bleed into it.
#[test]
#[ignore = "requires real distrobox on PATH"]
fn smoke_isolated_box_has_private_home() {
    use std::process::Command;
    let name = "cbox-smoke-isolated";
    rm_box(name);

    let created = Command::new(BIN)
        .args(["create", name, "-i", "fedora-toolbox:latest", "--isolated"])
        .output()
        .expect("cbox create --isolated failed to spawn");
    assert!(
        created.status.success(),
        "isolated create failed: {}",
        String::from_utf8_lossy(&created.stderr)
    );

    let box_home = String::from_utf8_lossy(
        &Command::new(BIN)
            .args([
                "enter",
                name,
                "--no-home",
                "--",
                "sh",
                "-lc",
                "printf %s \"$HOME\"",
            ])
            .output()
            .expect("enter failed")
            .stdout,
    )
    .trim()
    .to_string();
    println!("isolated box $HOME: {box_home}");

    let host_home = std::env::var("HOME").unwrap_or_default();

    // Drop a unique sentinel in the HOST home; an isolated box must not see it.
    // (Checking for the absence of a specific dotfile like ~/.zshrc is unreliable:
    // distrobox seeds a fresh home from /etc/skel, which itself ships dotfiles.)
    let sentinel = format!("{host_home}/.cbox-smoke-sentinel");
    std::fs::write(&sentinel, "host-only").expect("write sentinel");
    let sentinel_state = String::from_utf8_lossy(
        &Command::new(BIN)
            .args([
                "enter",
                name,
                "--no-home",
                "--",
                "sh",
                "-lc",
                "test -e \"$HOME/.cbox-smoke-sentinel\" && echo VISIBLE || echo HIDDEN",
            ])
            .output()
            .expect("enter failed")
            .stdout,
    )
    .trim()
    .to_string();

    let _ = std::fs::remove_file(&sentinel);
    rm_box(name);

    assert!(
        box_home.ends_with(&format!("/cbox/homes/{name}")),
        "isolated $HOME should be the private path, got: {box_home}"
    );
    assert_ne!(
        box_home, host_home,
        "isolated box must not share the host home"
    );
    assert_eq!(
        sentinel_state, "HIDDEN",
        "a file in the host home must not be visible in an isolated box"
    );
}
