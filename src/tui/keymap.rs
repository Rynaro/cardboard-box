//! Keymap-as-data: single source of truth for key bindings across the TUI.
//!
//! This module is always compiled (no `tui` feature gate) so:
//!  - Unit tests can import it without the full ratatui dep.
//!  - The lean (`make lint-lean`) build can lint it.
//!  - The cheatsheet overlay and the status bar both derive from ONE table.
//!
//! Voice rule: action strings use verbs only — no banned adjectives.
//! AC-MAP-VOICE asserts this across every KeyBinding.action.
#![allow(dead_code)]

// ─── KeyBinding ───────────────────────────────────────────────────────────────

/// A single key → action entry in the keymap table.
pub struct KeyBinding {
    /// Display form of the key, e.g. `"?"`, `"enter"`, `"↑↓"`.
    pub key: &'static str,
    /// Verb phrase describing what it does, e.g. `"cheatsheet"`, `"open"`, `"move"`.
    pub action: &'static str,
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
}

// ─── Keymap tables ────────────────────────────────────────────────────────────

static KEYMAP_LIST: &[KeyBinding] = &[
    KeyBinding {
        key: "↑↓ / j k",
        action: "move",
    },
    KeyBinding {
        key: "enter",
        action: "open",
    },
    KeyBinding {
        key: "i",
        action: "inspect",
    },
    KeyBinding {
        key: "c",
        action: "pack (create)",
    },
    KeyBinding {
        key: "s",
        action: "seal (stop)",
    },
    KeyBinding {
        key: "d",
        action: "clear out (destroy)",
    },
    KeyBinding {
        key: "a",
        action: "apply",
    },
    KeyBinding {
        key: "e",
        action: "edit",
    },
    KeyBinding {
        key: "r",
        action: "refresh",
    },
    KeyBinding {
        key: "/",
        action: "filter",
    },
    KeyBinding {
        key: "?",
        action: "cheatsheet",
    },
    KeyBinding {
        key: "D",
        action: "doctor",
    },
    KeyBinding {
        key: "t",
        action: "skin",
    },
    KeyBinding {
        key: "l",
        action: "command-log",
    },
    KeyBinding {
        key: "q / esc",
        action: "quit",
    },
];

static KEYMAP_DETAIL: &[KeyBinding] = &[
    KeyBinding {
        key: "esc / q",
        action: "back",
    },
    KeyBinding {
        key: "e",
        action: "edit",
    },
    KeyBinding {
        key: "a",
        action: "apply",
    },
    KeyBinding {
        key: "enter",
        action: "open (if running)",
    },
    KeyBinding {
        key: "?",
        action: "cheatsheet",
    },
    KeyBinding {
        key: "t",
        action: "skin",
    },
];

static KEYMAP_WIZARD: &[KeyBinding] = &[
    KeyBinding {
        key: "tab / enter",
        action: "next step",
    },
    KeyBinding {
        key: "shift-tab",
        action: "back",
    },
    KeyBinding {
        key: "esc",
        action: "cancel",
    },
];

static KEYMAP_CONFIRM_DESTROY: &[KeyBinding] = &[
    KeyBinding {
        key: "y / enter",
        action: "confirm destroy",
    },
    KeyBinding {
        key: "n / esc",
        action: "cancel",
    },
    KeyBinding {
        key: "h",
        action: "toggle remove home",
    },
];

static KEYMAP_PROGRESS: &[KeyBinding] = &[KeyBinding {
    key: "enter / esc / q",
    action: "back (when done)",
}];

static KEYMAP_DOCTOR_PANEL: &[KeyBinding] = &[KeyBinding {
    key: "esc / q",
    action: "back",
}];

static KEYMAP_FILTER_INPUT: &[KeyBinding] = &[
    KeyBinding {
        key: "type",
        action: "narrow filter",
    },
    KeyBinding {
        key: "backspace",
        action: "delete character",
    },
    KeyBinding {
        key: "↑↓",
        action: "move within matches",
    },
    KeyBinding {
        key: "enter",
        action: "keep selection and close",
    },
    KeyBinding {
        key: "esc",
        action: "clear and close",
    },
];

static KEYMAP_CHEATSHEET: &[KeyBinding] = &[
    KeyBinding {
        key: "any key",
        action: "dismiss",
    },
    KeyBinding {
        key: "esc",
        action: "dismiss",
    },
];

static KEYMAP_COMMAND_LOG: &[KeyBinding] = &[
    KeyBinding {
        key: "↑↓ / j k",
        action: "scroll",
    },
    KeyBinding {
        key: "esc / q / l",
        action: "close",
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
    }
}

/// Derive a compact one-line help string from the List keymap.
/// Replaces the hand-written `strings::HELP` const (D-1 decision: derive, don't duplicate).
/// Returns a freshly-allocated `String`; callers may cache if needed.
pub fn help_line(ctx: KeyContext) -> String {
    keymap_for(ctx)
        .iter()
        .map(|kb| format!("{} {}", kb.key, kb.action))
        .collect::<Vec<_>>()
        .join(" · ")
}
