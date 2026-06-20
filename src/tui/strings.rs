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

// NOTE (D-1): The canonical help text is derived from `keymap::help_line(KeyContext::List)`
// at render time in view.rs so the status bar and the cheatsheet never drift.
// This const is kept as a legacy alias so AC-COPY-1 can assert voice compliance
// on a stable value. Keep it in sync with the List keymap manually if verbs change.
pub const HELP: &str =
    "\u{2191}\u{2193} move \u{00b7} enter open \u{00b7} c pack \u{00b7} s seal \u{00b7} d clear out \u{00b7} a apply \u{00b7} e edit \u{00b7} ? cheatsheet \u{00b7} D doctor \u{00b7} q quit";

// ─── Command-log overlay copy ────────────────────────────────────────────────

pub const CMDLOG_TITLE: &str = " command log ";
pub const CMDLOG_EMPTY: &str = "Nothing has run yet.";
pub const CMDLOG_HINT: &str =
    "What cbox ran, newest last. This is a record - it can't undo anything.";

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

// ─── Bundle 2: Bulk operation copy ───────────────────────────────────────────

/// Title for the prune-stopped bulk confirm modal.
pub const BULK_PRUNE_TITLE: &str = " prune stopped ";
/// Title for the stop-running bulk confirm modal.
pub const BULK_STOP_TITLE: &str = " stop all running ";
/// Title for the destroy-cbox-managed bulk confirm modal.
pub const BULK_DESTROY_MANAGED_TITLE: &str = " destroy cbox-managed ";
/// Title for the destroy-NON-managed bulk confirm modal (dangerous).
pub const BULK_DESTROY_UNMANAGED_TITLE: &str = " destroy NON-managed ";
/// Warning shown in the dangerous destroy-unmanaged modal.
pub const BULK_UNMANAGED_WARN: &str =
    "These boxes were not packed by cbox. Destroying them is permanent.";
/// The exact phrase the user must type to confirm the dangerous destroy-unmanaged op.
/// AC-BULK-DANGER-1/2: tests assert `typed_confirm == BULK_UNMANAGED_PHRASE`.
pub const BULK_UNMANAGED_PHRASE: &str = "DESTROY UNMANAGED";
/// Shown when a bulk op finds no matching boxes.
pub const BULK_EMPTY: &str = "Nothing to do — no boxes match.";

// ─── Bundle 2: Palette copy ───────────────────────────────────────────────────

/// Title for the command-palette overlay.
pub const PALETTE_TITLE: &str = " : command palette ";
/// Hint shown when the palette has no matches.
pub const PALETTE_NO_MATCH: &str = "No matching actions.";
/// Footer shown in the palette overlay.
pub const PALETTE_HINT: &str = "↑↓ move · enter run · esc cancel";
