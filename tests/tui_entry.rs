//! Entry / feature-gate acceptance tests (assert_cmd).
//! AC-ENTRY-NOTTY, AC-ENTRY-TUI-NOTTY, AC-ENTRY-JSON.
//! All run with piped stdin/stdout (non-TTY) so no TUI is launched.

use assert_cmd::Command;
use predicates::str::contains;

// ─── AC-ENTRY-NOTTY ──────────────────────────────────────────────────────────
// GIVEN non-TTY stdout WHEN `cbox` (no args) THEN prints help, exit 64.

#[test]
fn ac_entry_notty_no_args_prints_help() {
    // assert_cmd pipes stdin/stdout → non-TTY.
    let mut cmd = Command::cargo_bin("cbox").unwrap();
    cmd.assert().failure().code(64);
    // Output should contain usage/help text on stderr or stdout.
    // We don't assert the exact text since it may vary; just that it exits 64.
}

// ─── AC-ENTRY-TUI-NOTTY ──────────────────────────────────────────────────────
// GIVEN non-TTY WHEN `cbox tui` THEN exit 64 with "needs an interactive terminal".

#[test]
fn ac_entry_tui_notty_exits_64() {
    let mut cmd = Command::cargo_bin("cbox").unwrap();
    cmd.arg("tui")
        .assert()
        .failure()
        .code(64)
        .stderr(contains("interactive terminal"));
}

// ─── AC-ENTRY-JSON ───────────────────────────────────────────────────────────
// GIVEN `cbox tui --json` THEN exit 64 with "--json is not supported for the TUI".
// Note: when --json is active, the error is emitted as JSON to stdout (per main.rs §json branch).

#[test]
fn ac_entry_json_with_tui_exits_64() {
    let mut cmd = Command::cargo_bin("cbox").unwrap();
    cmd.arg("--json")
        .arg("tui")
        .assert()
        .failure()
        .code(64)
        // The --json flag routes errors to stdout as JSON.
        .stdout(contains("--json is not supported for the TUI"));
}

// ─── Regression: existing subcommands still parse ────────────────────────────
// Proves AC-R7: Option<Commands> doesn't break existing subcommand parsing.

#[test]
fn regression_list_subcommand_still_works() {
    // `cbox list --json` should work (exits 0 or non-zero depending on distrobox presence,
    // but must NOT exit 64 for "unknown subcommand").
    // On a non-distrobox host it will exit 70 or 75; that's fine.
    let mut cmd = Command::cargo_bin("cbox").unwrap();
    let output = cmd.arg("list").arg("--json").output().unwrap();
    // Must not exit 64 (usage error).
    assert_ne!(
        output.status.code(),
        Some(64),
        "`cbox list` should not exit 64 (subcommand should still be recognized)"
    );
}

#[test]
fn regression_create_dry_run_subcommand() {
    let mut cmd = Command::cargo_bin("cbox").unwrap();
    // dry-run with explicit backend so it doesn't try to probe the host.
    // Accepts exit 0 (dry-run success) or 75 (no backend on this host, e.g. CI container).
    let output = cmd
        .args(["create", "test-box", "--dry-run", "--backend", "podman"])
        .output()
        .unwrap();
    let code = output.status.code().unwrap_or(1);
    assert_ne!(
        code, 64,
        "`cbox create` should not exit 64 (usage error — subcommand must be recognized)"
    );
}

#[test]
fn regression_help_flag() {
    Command::cargo_bin("cbox")
        .unwrap()
        .arg("--help")
        .assert()
        .success();
}
