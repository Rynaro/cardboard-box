//! Centralized user-facing copy for the TUI.
//!
//! Single-source for voice compliance: the "show don't tell" rule requires
//! that copy demonstrates character via verbs + glyphs rather than advertising
//! qualities (no cozy/beautiful/friendly/delightful).  AC-COPY-1 mechanically
//! asserts that every public const here is free of those banned substrings.
//!
//! This module is compiled regardless of the `tui` feature so integration tests
//! can import it.  In the lean (no-tui) build the items are unused by the binary
//! itself, hence the module-level lint suppression.
#![allow(dead_code)]

// ─── Brand ────────────────────────────────────────────────────────────────────

pub const WORDMARK: &str = "cardboard-box";
pub const TAGLINE: &str = "Your Linux environments, unboxed.";
pub const LOGO_GLYPH: &str = "▣";

// ─── Empty / loading / error states ──────────────────────────────────────────

// EMPTY_LIST is the canonical source for the empty-list message.  view.rs renders
// it as two styled spans (with `c` highlighted) so the const is used indirectly;
// the allow suppresses clippy's dead_code lint in non-TUI feature builds.
#[allow(dead_code)]
pub const EMPTY_LIST: &str = "Nothing boxed up yet.  Press  c  to pack your first one.";
pub const EMPTY_DETAIL: &str = "No box selected.";
pub const LOADING_LIST: &str = "Unpacking your boxes\u{2026}";
pub const LOADING_DETAIL: &str = "Opening the box\u{2026}";
pub const LOADING_DOCTOR: &str = "Running a check-up\u{2026}";
pub const PROGRESS_RUNNING: &str = "Working\u{2026}";
pub const PROGRESS_DONE: &str = "All set.  Enter or Esc to head back.";
/// Prefix prepended to error messages — glyph carries error semantics in no-color.
pub const ERROR_PREFIX: &str = "\u{2717} ";

// ─── Help line (status bar) ───────────────────────────────────────────────────

pub const HELP: &str =
    "\u{2191}\u{2193} move \u{00b7} enter open \u{00b7} c create \u{00b7} s stop \u{00b7} d destroy \u{00b7} a apply \u{00b7} e edit \u{00b7} ? doctor \u{00b7} q quit";

// ─── Parameterized copy ───────────────────────────────────────────────────────

/// Status after a successful list load.
pub fn loaded(n: usize) -> String {
    format!("{n} box(es) packed and ready")
}

/// Status shown while the backend is unreachable and doctor is auto-launched.
pub fn backend_unreachable() -> &'static str {
    "Backend's gone quiet \u{2014} running a check-up\u{2026}"
}

/// Status after a box is successfully created.
pub fn created(name: &str) -> String {
    format!("Packed \"{name}\".")
}

/// Status after one or more boxes are removed.
pub fn removed(list: &str) -> String {
    format!("Cleared out: {list}")
}

/// Status after one or more boxes are stopped.
pub fn stopped(list: &str) -> String {
    format!("Sealed up: {list}")
}

/// Status after an apply completes.
pub fn applied(name: &str, ran: usize, skipped: usize, failed: usize) -> String {
    format!("Applied \"{name}\": {ran} ran, {skipped} skipped, {failed} failed")
}
