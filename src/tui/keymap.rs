//! Keymap-as-data: single source of truth for key bindings across the TUI.
//!
//! This module is always compiled (no `tui` feature gate) so:
//!  - Unit tests can import it without the full ratatui dep.
//!  - The lean (`make lint-lean`) build can lint it.
//!  - The cheatsheet overlay and the status bar both derive from ONE table.
//!
//! Voice rule: action labels use verbs only — no banned adjectives.
//! AC-ACTION-1 + AC-VOICE-1 assert this across every `Action::label()`.
#![allow(dead_code)]

use crate::tui::action::Action;

// ─── KeyBinding ───────────────────────────────────────────────────────────────

/// A single key → action entry in the keymap table.
pub struct KeyBinding {
    /// Display form of the key, e.g. `"?"`, `"enter"`, `"↑↓"`.
    pub key: &'static str,
    /// The action this key performs — single source of truth.
    pub action: Action,
}

// ─── KeyContext ───────────────────────────────────────────────────────────────

/// Logical context for keymap resolution — mirrors the reducer's screen dispatch
/// plus overlay nuances.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyContext {
    List,
    Detail,
    Wizard,
    ConfirmDestroy,
    Progress,
    DoctorPanel,
    /// Filter overlay active: type characters, arrows navigate, enter/esc close.
    FilterInput,
    /// Cheatsheet overlay: any key dismisses.
    Cheatsheet,
    /// Command-log overlay: scroll with arrows/j/k, esc/q/l closes.
    CommandLog,
    /// Palette overlay: type query, arrows navigate matches, enter dispatches.
    Palette,
    /// Bulk-confirm modal: type phrase or y/n/esc.
    BulkConfirm,
}

// ─── Keymap tables ────────────────────────────────────────────────────────────

static KEYMAP_LIST: &[KeyBinding] = &[
    KeyBinding {
        key: "↑↓ / j k",
        action: Action::MoveUp,
    },
    KeyBinding {
        key: "enter",
        action: Action::Open,
    },
    KeyBinding {
        key: "i",
        action: Action::Inspect,
    },
    KeyBinding {
        key: "c",
        action: Action::Create,
    },
    KeyBinding {
        key: "s",
        action: Action::Stop,
    },
    KeyBinding {
        key: "d",
        action: Action::Destroy,
    },
    KeyBinding {
        key: "a",
        action: Action::Apply,
    },
    KeyBinding {
        key: "e",
        action: Action::Edit,
    },
    KeyBinding {
        key: "r",
        action: Action::Refresh,
    },
    KeyBinding {
        key: "/",
        action: Action::Filter,
    },
    KeyBinding {
        key: "?",
        action: Action::Cheatsheet,
    },
    KeyBinding {
        key: "D",
        action: Action::Doctor,
    },
    KeyBinding {
        key: "t",
        action: Action::CycleSkin,
    },
    KeyBinding {
        key: "l",
        action: Action::CommandLog,
    },
    KeyBinding {
        key: ":",
        action: Action::Palette,
    },
    KeyBinding {
        key: "b",
        action: Action::Palette, // bulk-scoped fast-path (opens Palette with bulk_only=true)
    },
    KeyBinding {
        key: "q / esc",
        action: Action::Quit,
    },
];

static KEYMAP_DETAIL: &[KeyBinding] = &[
    KeyBinding {
        key: "esc / q",
        action: Action::Quit, // goes back on Detail
    },
    KeyBinding {
        key: "e",
        action: Action::Edit,
    },
    KeyBinding {
        key: "a",
        action: Action::Apply,
    },
    KeyBinding {
        key: "enter",
        action: Action::Open,
    },
    KeyBinding {
        key: "?",
        action: Action::Cheatsheet,
    },
    KeyBinding {
        key: "t",
        action: Action::CycleSkin,
    },
];

static KEYMAP_WIZARD: &[KeyBinding] = &[
    KeyBinding {
        key: "tab / enter",
        action: Action::Open, // next step
    },
    KeyBinding {
        key: "shift-tab",
        action: Action::MoveUp, // back
    },
    KeyBinding {
        key: "esc",
        action: Action::Quit, // cancel
    },
];

static KEYMAP_CONFIRM_DESTROY: &[KeyBinding] = &[
    KeyBinding {
        key: "y / enter",
        action: Action::Destroy,
    },
    KeyBinding {
        key: "n / esc",
        action: Action::Quit,
    },
    KeyBinding {
        key: "h",
        action: Action::Apply, // toggle remove home
    },
];

static KEYMAP_PROGRESS: &[KeyBinding] = &[KeyBinding {
    key: "enter / esc / q",
    action: Action::Open, // back (when done)
}];

static KEYMAP_DOCTOR_PANEL: &[KeyBinding] = &[KeyBinding {
    key: "esc / q",
    action: Action::Quit,
}];

static KEYMAP_FILTER_INPUT: &[KeyBinding] = &[
    KeyBinding {
        key: "type",
        action: Action::Filter,
    },
    KeyBinding {
        key: "backspace",
        action: Action::DeleteChar,
    },
    KeyBinding {
        key: "↑↓",
        action: Action::MoveUp,
    },
    KeyBinding {
        key: "enter",
        action: Action::Open,
    },
    KeyBinding {
        key: "esc",
        action: Action::Quit,
    },
];

static KEYMAP_CHEATSHEET: &[KeyBinding] = &[
    KeyBinding {
        key: "any key",
        action: Action::Quit,
    },
    KeyBinding {
        key: "esc",
        action: Action::Quit,
    },
];

static KEYMAP_COMMAND_LOG: &[KeyBinding] = &[
    KeyBinding {
        key: "↑↓ / j k",
        action: Action::MoveUp,
    },
    KeyBinding {
        key: "esc / q / l",
        action: Action::Quit,
    },
];

static KEYMAP_PALETTE: &[KeyBinding] = &[
    KeyBinding {
        key: "type",
        action: Action::Filter,
    },
    KeyBinding {
        key: "↑↓",
        action: Action::MoveUp,
    },
    KeyBinding {
        key: "enter",
        action: Action::Open,
    },
    KeyBinding {
        key: "esc",
        action: Action::Quit,
    },
];

static KEYMAP_BULK_CONFIRM: &[KeyBinding] = &[
    KeyBinding {
        key: "y / enter",
        action: Action::Open,
    },
    KeyBinding {
        key: "n / esc",
        action: Action::Quit,
    },
];

// ─── Public API ───────────────────────────────────────────────────────────────

/// Return the slice of `KeyBinding` entries for the given context.
/// This is the single source of truth consumed by both the cheatsheet overlay
/// and the status bar help line.
pub fn keymap_for(ctx: KeyContext) -> &'static [KeyBinding] {
    match ctx {
        KeyContext::List => KEYMAP_LIST,
        KeyContext::Detail => KEYMAP_DETAIL,
        KeyContext::Wizard => KEYMAP_WIZARD,
        KeyContext::ConfirmDestroy => KEYMAP_CONFIRM_DESTROY,
        KeyContext::Progress => KEYMAP_PROGRESS,
        KeyContext::DoctorPanel => KEYMAP_DOCTOR_PANEL,
        KeyContext::FilterInput => KEYMAP_FILTER_INPUT,
        KeyContext::Cheatsheet => KEYMAP_CHEATSHEET,
        KeyContext::CommandLog => KEYMAP_COMMAND_LOG,
        KeyContext::Palette => KEYMAP_PALETTE,
        KeyContext::BulkConfirm => KEYMAP_BULK_CONFIRM,
    }
}

/// Derive a compact one-line help string from the given context's keymap.
/// Returns a freshly-allocated `String`; callers may cache if needed.
pub fn help_line(ctx: KeyContext) -> String {
    keymap_for(ctx)
        .iter()
        .map(|kb| format!("{} {}", kb.key, kb.action.label()))
        .collect::<Vec<_>>()
        .join(" · ")
}
