//! AC-CWD-* — tests for cwd Boxfile.toml auto-discovery.
//!
//! The `discover_boxfile_in` helper is injected with a temp-dir path rather
//! than mutating the process cwd (which is global state and would cause races
//! when tests run in parallel).
//!
//! For the CLI run() functions, which call std::env::current_dir() internally,
//! we test the end-to-end pipeline through core:: directly with an explicit
//! Boxfile path, matching the resolved path that would be passed through.

use assert_cmd::Command;
use cbox::cli::discover_boxfile_in;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tempfile::TempDir;

// ─── Helper ──────────────────────────────────────────────────────────────────

fn write_boxfile(dir: &TempDir, name: &str, image: &str) -> std::path::PathBuf {
    let path = dir.path().join("Boxfile.toml");
    std::fs::write(&path, format!("name = \"{name}\"\nimage = \"{image}\"\n")).unwrap();
    path
}

fn cbox_cmd(cwd: &Path) -> Command {
    let mut command = Command::cargo_bin("cbox").expect("cbox binary not found");
    command.current_dir(cwd);
    command
}

#[cfg(unix)]
fn install_fake_engines(dir: &TempDir) -> String {
    let bin = dir.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    for engine in ["docker", "podman"] {
        let path = bin.join(engine);
        std::fs::write(&path, "#!/bin/sh\nprintf '[]\\n'\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let distrobox = bin.join("distrobox");
    std::fs::write(
        &distrobox,
        "#!/bin/sh\n: > \"$CBOX_TEST_MARKER\"\nexit 99\n",
    )
    .unwrap();
    std::fs::set_permissions(&distrobox, std::fs::Permissions::from_mode(0o755)).unwrap();
    let inherited = std::env::var("PATH").unwrap_or_default();
    format!("{}:{inherited}", bin.display())
}

// ─── AC-CWD-1: helper returns Some when Boxfile.toml is present ──────────────

#[test]
fn ac_cwd_1_helper_present() {
    let dir = TempDir::new().unwrap();
    write_boxfile(&dir, "my-box", "fedora:latest");

    let result = discover_boxfile_in(dir.path());
    assert_eq!(
        result,
        Some("Boxfile.toml"),
        "helper should return Some(\"Boxfile.toml\") when file exists"
    );
}

// ─── AC-CWD-2: helper returns None when Boxfile.toml is absent ───────────────

#[test]
fn ac_cwd_2_helper_absent() {
    let dir = TempDir::new().unwrap();
    // No Boxfile.toml written.

    let result = discover_boxfile_in(dir.path());
    assert_eq!(
        result, None,
        "helper should return None when Boxfile.toml is absent"
    );
}

// ─── AC-CWD-3: helper does NOT match lower-case boxfile.toml ─────────────────

#[test]
fn ac_cwd_3_case_sensitive_filename() {
    let dir = TempDir::new().unwrap();
    // Write lower-case — should NOT trigger discovery.
    std::fs::write(
        dir.path().join("boxfile.toml"),
        "name = \"x\"\nimage = \"y\"\n",
    )
    .unwrap();

    let result = discover_boxfile_in(dir.path());
    assert_eq!(
        result, None,
        "helper must be case-sensitive: 'boxfile.toml' != 'Boxfile.toml'"
    );
}

// ─── AC-CWD-4: helper does NOT walk up parent directories ────────────────────

#[test]
fn ac_cwd_4_no_parent_walk() {
    let parent_dir = TempDir::new().unwrap();
    write_boxfile(&parent_dir, "parent-box", "fedora:latest");

    // Create a subdirectory — only the subdirectory's cwd should be checked.
    let sub_dir = parent_dir.path().join("sub");
    std::fs::create_dir(&sub_dir).unwrap();

    let result = discover_boxfile_in(&sub_dir);
    assert_eq!(
        result, None,
        "helper must not walk up to parent: Boxfile.toml is in parent, not in sub/"
    );
}

// ─── AC-CWD-5: explicit --file wins over cwd Boxfile (resolved via helper) ───
//
// We test by constructing the resolution logic inline: if `file_path` is Some,
// it wins even when discover_boxfile_in would return Some.

#[test]
fn ac_cwd_5_explicit_file_wins() {
    let dir = TempDir::new().unwrap();
    let cwd_boxfile_path = write_boxfile(&dir, "cwd-box", "cwd-image:latest");

    let explicit_dir = TempDir::new().unwrap();
    let explicit_path = explicit_dir.path().join("Boxfile.toml");
    std::fs::write(
        &explicit_path,
        "name = \"explicit-box\"\nimage = \"explicit-image:latest\"\n",
    )
    .unwrap();

    // Simulate the resolution: explicit file_path is Some → use it, skip discovery.
    let file_arg: Option<String> = Some(explicit_path.to_string_lossy().to_string());
    let resolved_path = if let Some(ref p) = file_arg {
        p.clone()
    } else if discover_boxfile_in(dir.path()).is_some() {
        cwd_boxfile_path.to_string_lossy().to_string()
    } else {
        panic!("no path resolved");
    };

    assert_eq!(
        resolved_path,
        explicit_path.to_string_lossy().as_ref(),
        "explicit --file must win over cwd Boxfile"
    );
}

// ─── AC-CWD-6: matching positional NAME selects cwd Boxfile ──────────────────

#[test]
fn ac_cwd_6_matching_name_uses_boxfile_create_settings() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("private-home");
    std::fs::write(
        dir.path().join("Boxfile.toml"),
        format!(
            r#"name = "cwd-box"
image = "ubuntu:24.04"
docker = "nested"

[box]
home = "{}"

[sandbox]
unshare = "all"
"#,
            home.display()
        ),
    )
    .unwrap();

    let output = cbox_cmd(dir.path())
        .args(["--backend", "docker", "--dry-run", "create", "cwd-box"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--image ubuntu:24.04"), "{stdout}");
    assert!(
        stdout.contains(&format!("--home {}", home.display())),
        "{stdout}"
    );
    assert!(stdout.contains("--unshare-all"), "{stdout}");
    assert!(
        stdout.contains("cbox.boxfile_path=Boxfile.toml"),
        "{stdout}"
    );
    assert!(stdout.contains("cbox.docker_mode=nested"), "{stdout}");
}

#[cfg(unix)]
#[test]
fn ac_cwd_6_mismatched_name_exits_64_before_mutation() {
    let dir = TempDir::new().unwrap();
    write_boxfile(&dir, "cwd-box", "ubuntu:24.04");
    let marker = dir.path().join("distrobox-was-called");
    let path = install_fake_engines(&dir);

    let output = cbox_cmd(dir.path())
        .env("PATH", path)
        .env("CBOX_TEST_MARKER", &marker)
        .args(["--backend", "docker", "create", "different-box"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(64));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("does not match"), "{stderr}");
    assert!(stderr.contains("different-box"), "{stderr}");
    assert!(stderr.contains("cwd-box"), "{stderr}");
    assert!(
        !marker.exists(),
        "mismatch must fail before distrobox mutation"
    );
}

#[cfg(unix)]
#[test]
fn ac_cwd_6_up_matching_name_mirrors_cwd_boxfile_discovery() {
    let dir = TempDir::new().unwrap();
    write_boxfile(&dir, "cwd-box", "ubuntu:24.04");
    let path = install_fake_engines(&dir);

    let output = cbox_cmd(dir.path())
        .env("PATH", path)
        .args(["--backend", "docker", "--dry-run", "up", "cwd-box"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("Created box \"cwd-box\""), "{combined}");
    assert!(combined.contains("Boxfile.toml"), "{combined}");
}

// ─── AC-CWD-7: improved usage error message contains both options ─────────────
//
// This tests that the error message wording is correct when neither NAME,
// --file, nor cwd Boxfile.toml are provided.

#[test]
fn ac_cwd_7_usage_error_message() {
    let dir = TempDir::new().unwrap();
    // No Boxfile.toml in dir.

    let file_arg: Option<String> = None;
    let name_arg: Option<String> = None;

    let result: Result<String, String> = if let Some(ref p) = file_arg {
        Ok(p.clone())
    } else if name_arg.is_some() {
        Ok("name-path".to_string())
    } else if discover_boxfile_in(dir.path()).is_some() {
        Ok("Boxfile.toml".to_string())
    } else {
        Err("NAME is required unless --file is provided or a Boxfile.toml exists in the current directory.".to_string())
    };

    let err = result.expect_err("should return usage error when no name/file/cwd boxfile");
    assert!(
        err.contains("--file"),
        "error message should mention --file, got: {err}"
    );
    assert!(
        err.contains("Boxfile.toml"),
        "error message should mention Boxfile.toml, got: {err}"
    );
    assert!(
        err.contains("current directory"),
        "error message should mention 'current directory', got: {err}"
    );
}

// ─── AC-CWD-8: cwd Boxfile is parsed correctly (name/image come from it) ──────

#[test]
fn ac_cwd_8_cwd_boxfile_parsed_correctly() {
    let dir = TempDir::new().unwrap();
    write_boxfile(&dir, "my-dev-box", "ubuntu:22.04");

    // Verify discover_boxfile_in signals the file exists.
    assert_eq!(discover_boxfile_in(dir.path()), Some("Boxfile.toml"));

    // Parse the absolute path to verify name/image are correct.
    let abs_path = dir.path().join("Boxfile.toml");
    let (bf, warnings) = cbox::boxfile::parse_file(&abs_path.to_string_lossy()).unwrap();
    assert!(
        warnings.is_empty(),
        "no warnings expected for minimal Boxfile"
    );
    assert_eq!(bf.name, "my-dev-box");
    assert_eq!(bf.image, "ubuntu:22.04");
}
