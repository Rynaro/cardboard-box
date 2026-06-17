//! TUI smoke tests — all #[ignore]d (require a real terminal/TTY/distrobox).
//! Run with: cargo test --test tui_smoke -- --ignored
//! Or:       make smoke (if added to Makefile)
//!
//! These are NOT CI gates (G-RESTORE / G-SMOKE are manual).

/// AC-RESTORE: verify that terminal raw mode is disabled + alt-screen is left
/// on normal quit, on early error, and on panic.
///
/// This test requires a PTY; run it manually:
///   script -q -c 'cargo test --test tui_smoke -- --ignored ac_restore' /dev/null
#[test]
#[ignore = "requires a real PTY; run manually"]
fn ac_restore_terminal_on_quit() {
    // This is a manual/smoke test that would be driven by a PTY harness.
    // Assertion: after TUI exits, `stty` returns "not in raw mode".
    // Not automatable in CI without a PTY allocator.
    unimplemented!("manual smoke test — run in a real terminal")
}

/// AC-SMOKE: end-to-end create/list/enter/destroy a throwaway box.
/// Requires: distrobox ≥ 1.6, podman or docker, network access for image pulls.
#[test]
#[ignore = "requires real distrobox on PATH and network access"]
fn ac_smoke_e2e() {
    unimplemented!("manual smoke test — requires real distrobox environment")
}
