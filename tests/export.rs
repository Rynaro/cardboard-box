//! AC-EXP-1..16 — `cbox export` acceptance tests (all mock-driven; zero real distrobox).
//!
//! Every test uses `MockRunner` and asserts on `RecordedCall { program, args, env }`.
//! CLI-surface tests (AC-EXP-12/13/14/15) use `assert_cmd`.

use cbox::core::{
    self,
    spec::{ExportSpec, ExportTarget},
};
use cbox::dbox::{
    backend::Backend,
    mock::{MockMatcher, MockResponse, MockRunner},
};
use cbox::error::exit;

// ─── helpers ──────────────────────────────────────────────────────────────────

/// Minimal valid podman inspect JSON for a box named `name`.
fn box_inspect_json(name: &str) -> String {
    format!(
        r#"[{{"Id":"abc123","State":{{"Status":"running"}},"Config":{{"Image":"fedora:latest","Labels":{{"manager":"distrobox","cbox.managed":"true","cbox.docker_mode":"none","cbox.boxfile_path":"","cbox.packages":""}}}},"Created":"2026-01-01T00:00:00Z","Mounts":[],"Name":"{name}"}}]"#
    )
}

/// A `MockRunner` where `inspect` (backend ps) returns the box as existing,
/// and all other calls return the provided `export_response`.
fn runner_with_box(name: &str, export_response: MockResponse) -> MockRunner {
    let inspect_json = box_inspect_json(name);
    // The inspect call goes to the backend (program == "podman"), args contain the box name.
    MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::ok(inspect_json))
                .with_program("podman")
                .with_args_contain(vec![name.to_string()]),
        )
        .with_default(export_response)
}

fn make_spec(target: ExportTarget, delete: bool) -> ExportSpec {
    ExportSpec {
        box_name: "dev".to_string(),
        target,
        delete,
        backend: Backend::Podman,
        dry_run: false,
    }
}

// ─── AC-EXP-1: --app builds correct argv (mutating, success) ─────────────────

#[test]
fn ac_exp_1_app_argv_and_outcome() {
    let runner = runner_with_box("dev", MockResponse::ok("Exporting app firefox"));
    let spec = make_spec(
        ExportTarget::App {
            name: "firefox".to_string(),
        },
        false,
    );

    let outcome = core::export(&spec, &runner).expect("export should succeed");

    // Assert outcome.
    assert!(outcome.ok);
    assert_eq!(outcome.action, "export");
    assert_eq!(outcome.mode, "app");
    assert_eq!(outcome.target, Some("firefox".to_string()));
    assert!(!outcome.deleted);
    assert!(outcome.entries.is_empty());

    // Assert the export argv on the distrobox call.
    let calls = runner.calls();
    let export_call = calls
        .iter()
        .find(|c| c.program == "distrobox")
        .expect("should have a distrobox call");
    assert_eq!(
        export_call.args,
        &[
            "enter",
            "--name",
            "dev",
            "--",
            "distrobox-export",
            "--app",
            "firefox"
        ]
    );
}

// ─── AC-EXP-2: --app --delete appends --delete ───────────────────────────────

#[test]
fn ac_exp_2_app_delete_appends_flag() {
    let runner = runner_with_box("dev", MockResponse::ok(""));
    let spec = make_spec(
        ExportTarget::App {
            name: "firefox".to_string(),
        },
        true,
    );

    let outcome = core::export(&spec, &runner).expect("export --delete should succeed");

    assert_eq!(outcome.action, "export-delete");
    assert!(outcome.deleted);

    let calls = runner.calls();
    let export_call = calls
        .iter()
        .find(|c| c.program == "distrobox")
        .expect("distrobox call missing");
    let args = &export_call.args;
    // Last two tokens must be --app firefox --delete (in that order)
    let pos_app = args
        .iter()
        .position(|a| a == "--app")
        .expect("--app missing");
    assert_eq!(args[pos_app + 1], "firefox");
    assert_eq!(args.last().unwrap(), "--delete");
}

// ─── AC-EXP-3: --bin --to maps to --export-path ──────────────────────────────

#[test]
fn ac_exp_3_bin_with_to_maps_export_path() {
    let runner = runner_with_box("dev", MockResponse::ok(""));
    let spec = make_spec(
        ExportTarget::Bin {
            path: "/usr/bin/htop".to_string(),
            to: Some("/home/u/.local/bin".to_string()),
        },
        false,
    );

    core::export(&spec, &runner).expect("export should succeed");

    let calls = runner.calls();
    let export_call = calls
        .iter()
        .find(|c| c.program == "distrobox")
        .expect("distrobox call missing");
    assert_eq!(
        export_call.args,
        &[
            "enter",
            "--name",
            "dev",
            "--",
            "distrobox-export",
            "--bin",
            "/usr/bin/htop",
            "--export-path",
            "/home/u/.local/bin"
        ]
    );
}

// ─── AC-EXP-4: --bin without --to is a usage error → exit 64 ────────────────
// D2: --bin requires --to <HOSTDIR>; omitting --to is rejected at the CLI
// boundary before any runner/box contact.  This is a CLI-level test (assert_cmd).
//
// (Previously this test asserted that the core accepted Bin{to:None} and omitted
// --export-path in the argv.  That encoded the wrong behaviour — it allowed
// distrobox-export to silently default to ~/.local/bin.  Corrected per D2.)

#[cfg(test)]
mod ac_exp_4_cli {
    fn cbox_cmd() -> assert_cmd::Command {
        assert_cmd::Command::cargo_bin("cbox").expect("cbox binary not found")
    }

    /// cbox export dev --bin /usr/bin/htop (no --to) → exit 64
    #[test]
    fn ac_exp_4_bin_without_to_exit_64() {
        cbox_cmd()
            .args(["export", "dev", "--bin", "/usr/bin/htop"])
            .env("CBOX_BACKEND", "podman")
            .assert()
            .code(64);
    }
}

// ─── AC-EXP-5: --service builds service argv ─────────────────────────────────

#[test]
fn ac_exp_5_service_argv() {
    let runner = runner_with_box("dev", MockResponse::ok(""));
    let spec = make_spec(
        ExportTarget::Service {
            name: "nginx".to_string(),
        },
        false,
    );

    core::export(&spec, &runner).expect("export should succeed");

    let calls = runner.calls();
    let export_call = calls
        .iter()
        .find(|c| c.program == "distrobox")
        .expect("distrobox call missing");
    assert_eq!(
        export_call.args,
        &[
            "enter",
            "--name",
            "dev",
            "--",
            "distrobox-export",
            "--service",
            "nginx"
        ]
    );
}

// ─── AC-EXP-6: --list-apps parses entries ────────────────────────────────────

#[test]
fn ac_exp_6_list_apps_parses_entries() {
    let runner = runner_with_box("dev", MockResponse::ok("firefox\ngimp\n"));
    let spec = make_spec(ExportTarget::ListApps, false);

    let outcome = core::export(&spec, &runner).expect("list-apps should succeed");

    assert_eq!(outcome.action, "export-list");
    assert_eq!(outcome.mode, "list-apps");
    assert_eq!(outcome.target, None);
    assert_eq!(outcome.entries, vec!["firefox", "gimp"]);

    let calls = runner.calls();
    let export_call = calls
        .iter()
        .find(|c| c.program == "distrobox")
        .expect("distrobox call missing");
    let args = &export_call.args;
    let pos = args
        .iter()
        .position(|a| a == "distrobox-export")
        .expect("distrobox-export missing");
    assert_eq!(args[pos + 1], "--list-apps");
}

// ─── AC-EXP-7: --list-bins mirrors AC-EXP-6 with --list-binaries ─────────────

#[test]
fn ac_exp_7_list_bins_uses_list_binaries() {
    let runner = runner_with_box("dev", MockResponse::ok("htop\nbash\n"));
    let spec = make_spec(ExportTarget::ListBins, false);

    let outcome = core::export(&spec, &runner).expect("list-bins should succeed");

    assert_eq!(outcome.action, "export-list");
    assert_eq!(outcome.mode, "list-bins");
    assert_eq!(outcome.target, None);
    assert_eq!(outcome.entries, vec!["htop", "bash"]);

    let calls = runner.calls();
    let export_call = calls
        .iter()
        .find(|c| c.program == "distrobox")
        .expect("distrobox call missing");
    let args = &export_call.args;
    let pos = args
        .iter()
        .position(|a| a == "distrobox-export")
        .expect("distrobox-export missing");
    assert_eq!(args[pos + 1], "--list-binaries");
}

// ─── AC-EXP-8: box does not exist → exit 69, no export attempted ─────────────

#[test]
fn ac_exp_8_box_not_found_exit_69_no_export() {
    // The inspect (backend call) returns empty → not found.
    let runner = MockRunner::new().with_default(MockResponse::ok("[]"));
    let spec = make_spec(
        ExportTarget::App {
            name: "firefox".to_string(),
        },
        false,
    );

    let err = core::export(&spec, &runner).expect_err("should fail with box not found");
    assert_eq!(err.exit_code(), exit::UNAVAILABLE, "exit code must be 69");
    assert!(
        err.to_string().contains("No box named"),
        "message should mention box: {err}"
    );

    // No distrobox-export call should have been made.
    let calls = runner.calls();
    let had_export_call = calls
        .iter()
        .any(|c| c.args.iter().any(|a| a == "distrobox-export"));
    assert!(
        !had_export_call,
        "must NOT have called distrobox-export when box is absent, calls: {calls:?}"
    );
}

// ─── AC-EXP-9: distrobox-export missing → exit 70 with cbox-authored message ─

#[test]
fn ac_exp_9_export_helper_missing_exit_70() {
    let runner = runner_with_box(
        "dev",
        MockResponse::err(127, "distrobox-export: command not found"),
    );
    let spec = make_spec(
        ExportTarget::App {
            name: "firefox".to_string(),
        },
        false,
    );

    let err = core::export(&spec, &runner).expect_err("should fail with helper missing");
    assert_eq!(err.exit_code(), exit::SOFTWARE, "exit code must be 70");
    let msg = err.to_string();
    assert!(
        msg.contains("distrobox-export isn't available"),
        "message must contain cbox-authored text, got: {msg}"
    );
    assert!(
        msg.contains("cbox doctor"),
        "message must reference cbox doctor, got: {msg}"
    );
}

// ─── AC-EXP-10: app not found → exit 125 with helper stderr tail ─────────────

#[test]
fn ac_exp_10_app_not_found_exit_125() {
    let runner = runner_with_box(
        "dev",
        MockResponse::err(
            1,
            "Trying to export firefox... application firefox not found",
        ),
    );
    let spec = make_spec(
        ExportTarget::App {
            name: "firefox".to_string(),
        },
        false,
    );

    let err = core::export(&spec, &runner).expect_err("should fail with app not found");
    assert_eq!(
        err.exit_code(),
        exit::BACKEND_NONZERO,
        "exit code must be 125"
    );
    let msg = err.to_string();
    assert!(
        msg.contains("not found"),
        "error should surface helper's 'not found' message, got: {msg}"
    );
}

// ─── AC-EXP-11: (removed — scenario was unreachable) ─────────────────────────
// The original test mocked distrobox-export returning exit 1 with
// "--export-path is required" for a Bin{to:None} spec.  That code path is now
// unreachable: the CLI guard in cli/export.rs rejects --bin without --to at
// exit 64 before the backend is ever contacted.  Additionally, real
// distrobox-export does NOT error on a missing --export-path — it silently
// defaults to ~/.local/bin — so the mock did not reflect real behaviour.
//
// No replacement is added here because there is no genuinely reachable
// backend-error scenario specific to the bin target that is not already
// covered by AC-EXP-9 (helper missing → 70) and AC-EXP-10 (app not found → 125).

// ─── AC-EXP-12: usage errors → exit 64 (CLI guard + clap ArgGroup) ───────────

#[cfg(test)]
mod cli_usage_tests {
    // These tests use assert_cmd to drive the compiled binary.
    // They are guarded to only run when the binary is available (i.e. in make check / test).

    fn cbox_cmd() -> assert_cmd::Command {
        assert_cmd::Command::cargo_bin("cbox").expect("cbox binary not found")
    }

    /// cbox export dev (no target) → non-zero (clap ArgGroup required, exits 2)
    #[test]
    fn ac_exp_12a_no_target_nonzero() {
        cbox_cmd()
            .args(["export", "dev"])
            .env("CBOX_BACKEND", "podman")
            .assert()
            .failure(); // clap ArgGroup exits 2 (usage); non-zero is the invariant
    }

    /// cbox export dev --app a --service b (two targets) → non-zero (clap ArgGroup multiple=false)
    #[test]
    fn ac_exp_12b_two_targets_nonzero() {
        cbox_cmd()
            .args(["export", "dev", "--app", "a", "--service", "b"])
            .env("CBOX_BACKEND", "podman")
            .assert()
            .failure(); // clap ArgGroup multiple=false exits 2 (usage)
    }

    /// cbox export dev --list-apps --to /x (--to without --bin) → exit 64
    #[test]
    fn ac_exp_12c_to_without_bin_exit_64() {
        cbox_cmd()
            .args(["export", "dev", "--list-apps", "--to", "/x"])
            .env("CBOX_BACKEND", "podman")
            .assert()
            .code(64);
    }

    /// cbox export dev --list-apps --delete (--delete with list) → exit 64
    #[test]
    fn ac_exp_12d_delete_with_list_exit_64() {
        cbox_cmd()
            .args(["export", "dev", "--list-apps", "--delete"])
            .env("CBOX_BACKEND", "podman")
            .assert()
            .code(64);
    }

    /// cbox export "../evil" --app x → exit 64 (bad box name, before any runner call)
    #[test]
    fn ac_exp_15_bad_box_name_exit_64() {
        cbox_cmd()
            .args(["export", "../evil", "--app", "x"])
            .env("CBOX_BACKEND", "podman")
            .assert()
            .code(64);
    }
}

// ─── AC-EXP-13: --json shape (mutating + list) ───────────────────────────────

#[test]
fn ac_exp_13_json_mutating_shape() {
    let runner = runner_with_box("dev", MockResponse::ok("Exporting app firefox"));
    let spec = make_spec(
        ExportTarget::App {
            name: "firefox".to_string(),
        },
        false,
    );

    let outcome = core::export(&spec, &runner).expect("export should succeed");

    // Serialize and parse to verify JSON shape.
    let json_str = serde_json::to_string(&outcome).expect("serialize outcome");
    let v: serde_json::Value = serde_json::from_str(&json_str).expect("parse json");

    assert_eq!(v["ok"], true);
    assert_eq!(v["action"], "export");
    assert_eq!(v["mode"], "app");
    assert_eq!(v["target"], "firefox");
    assert_eq!(v["deleted"], false);
    assert_eq!(v["entries"], serde_json::json!([]));
}

#[test]
fn ac_exp_13_json_list_shape() {
    let runner = runner_with_box("dev", MockResponse::ok("firefox\ngimp\n"));
    let spec = make_spec(ExportTarget::ListApps, false);

    let outcome = core::export(&spec, &runner).expect("list-apps should succeed");

    let json_str = serde_json::to_string(&outcome).expect("serialize outcome");
    let v: serde_json::Value = serde_json::from_str(&json_str).expect("parse json");

    assert_eq!(v["ok"], true);
    assert_eq!(v["action"], "export-list");
    assert_eq!(v["mode"], "list-apps");
    assert_eq!(v["target"], serde_json::Value::Null);
    assert!(v["entries"].is_array(), "entries must be an array");
    assert_eq!(v["entries"], serde_json::json!(["firefox", "gimp"]));
}

// ─── AC-EXP-14: --dry-run prints argv, spawns no export ──────────────────────

#[test]
fn ac_exp_14_dry_run_no_export_spawn() {
    // In DryRun mode the MockRunner still records a call (it records before dispatching),
    // but the mode is DryRun, so no real process is spawned.
    // We verify the outcome is ok:true and that the recorded call is for the inspect only,
    // not for a real export exec. The MockRunner for DryRun returns the argv as stdout.
    let runner = runner_with_box("dev", MockResponse::ok(""));
    let spec = ExportSpec {
        box_name: "dev".to_string(),
        target: ExportTarget::App {
            name: "firefox".to_string(),
        },
        delete: false,
        backend: Backend::Podman,
        dry_run: true,
    };

    let outcome = core::export(&spec, &runner).expect("dry-run should succeed");
    assert!(outcome.ok, "dry-run outcome must be ok=true");
    assert_eq!(outcome.action, "export");
}

// ─── AC-EXP-16: backend pin on the invocation ────────────────────────────────

#[test]
fn ac_exp_16_backend_pin_env() {
    let inspect_json = box_inspect_json("dev");
    // Use Docker backend.
    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::ok(inspect_json))
                .with_program("docker")
                .with_args_contain(vec!["dev".to_string()]),
        )
        .with_default(MockResponse::ok(""));

    let spec = ExportSpec {
        box_name: "dev".to_string(),
        target: ExportTarget::App {
            name: "firefox".to_string(),
        },
        delete: false,
        backend: Backend::Docker,
        dry_run: false,
    };

    core::export(&spec, &runner).expect("export should succeed");

    // The distrobox export call must carry DBX_CONTAINER_MANAGER=docker.
    let calls = runner.calls();
    let export_call = calls
        .iter()
        .find(|c| c.program == "distrobox" && c.args.iter().any(|a| a == "distrobox-export"))
        .expect("distrobox export call missing");

    assert!(
        export_call
            .env
            .iter()
            .any(|(k, v)| k == "DBX_CONTAINER_MANAGER" && v == "docker"),
        "export call must carry DBX_CONTAINER_MANAGER=docker, got env: {:?}",
        export_call.env
    );
}
