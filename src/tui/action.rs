//! First-class `Action` enum — single source of truth for keymap, palette, and cheatsheet.
//!
//! This module is always compiled (no `tui` feature gate) so it stays lean-clean.
//! It imports only pure types (no ratatui, no nucleo-matcher, no Effect).
//!
//! GAP-3: every place that needs a label string or a list of available actions reads
//! this enum — keymap, cheatsheet, palette, and `dispatch_action` all converge here.
#![allow(dead_code)]

// ─── Action ──────────────────────────────────────────────────────────────────

/// Every user-initiatable action the TUI supports.
///
/// The enum is the single source of truth: `KeyBinding.action` links a key to
/// one of these; the palette offers the `in_palette()` subset; the cheatsheet
/// renders `label()`; `dispatch_action` maps each variant to its effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    // ── Navigation / single-box ──────────────────────────────────────────────
    MoveUp,
    MoveDown,
    Open,
    Inspect,
    Create,
    Stop,
    Destroy,
    Apply,
    Edit,
    Refresh,
    // ── Overlays / global ────────────────────────────────────────────────────
    Filter,
    Cheatsheet,
    Doctor,
    CycleSkin,
    CommandLog,
    Palette,
    Quit,
    // ── Bulk operations (Feature 4) ──────────────────────────────────────────
    BulkPruneStopped,
    BulkStopRunning,
    BulkDestroyManaged,
    BulkDestroyUnmanaged,
    // ── Input helpers ────────────────────────────────────────────────────────
    /// Backspace in a text-input context (filter / palette / wizard).
    /// Appears in the FilterInput and Palette keymaps but NOT in the palette
    /// itself (not in_palette()).
    DeleteChar,
    // ── Bundle 3 ─────────────────────────────────────────────────────────────
    /// Open the cross-session action history overlay (fuzzy search, read-only).
    History,
    /// Open the live container-log streaming screen for the selected box.
    ViewLogs,
}

impl Action {
    /// Stable verb label shown in the palette and cheatsheet.
    ///
    /// Voice rule: verbs only, no banned adjectives (AC-VOICE-1).
    pub fn label(self) -> &'static str {
        match self {
            Action::MoveUp => "move up",
            Action::MoveDown => "move down",
            Action::Open => "open",
            Action::Inspect => "inspect",
            Action::Create => "pack (create)",
            Action::Stop => "seal (stop)",
            Action::Destroy => "clear out (destroy)",
            Action::Apply => "apply",
            Action::Edit => "edit",
            Action::Refresh => "refresh",
            Action::Filter => "filter",
            Action::Cheatsheet => "cheatsheet",
            Action::Doctor => "doctor",
            Action::CycleSkin => "skin",
            Action::CommandLog => "command-log",
            Action::Palette => "palette",
            Action::Quit => "quit",
            Action::BulkPruneStopped => "bulk: prune stopped",
            Action::BulkStopRunning => "bulk: stop all running",
            Action::BulkDestroyManaged => "bulk: destroy cbox-managed",
            Action::BulkDestroyUnmanaged => "bulk: destroy NON-managed",
            Action::DeleteChar => "delete character",
            Action::History => "history",
            Action::ViewLogs => "view logs",
        }
    }

    /// Whether this action should appear in the `:` command palette.
    ///
    /// Navigation and input-helper actions are omitted.
    pub fn in_palette(self) -> bool {
        !matches!(self, Action::MoveUp | Action::MoveDown | Action::DeleteChar)
    }

    /// Whether this action is a Bundle 3 action (for tests / enumeration).
    #[allow(dead_code)]
    pub fn is_bundle3(self) -> bool {
        matches!(self, Action::History | Action::ViewLogs)
    }

    /// The display key for this action in the default keymap, if one exists.
    pub fn default_key(self) -> Option<&'static str> {
        match self {
            Action::MoveUp => Some("↑↓ / j k"),
            Action::MoveDown => None, // grouped with MoveUp
            Action::Open => Some("enter"),
            Action::Inspect => Some("i"),
            Action::Create => Some("c"),
            Action::Stop => Some("s"),
            Action::Destroy => Some("d"),
            Action::Apply => Some("a"),
            Action::Edit => Some("e"),
            Action::Refresh => Some("r"),
            Action::Filter => Some("/"),
            Action::Cheatsheet => Some("?"),
            Action::Doctor => Some("D"),
            Action::CycleSkin => Some("t"),
            Action::CommandLog => Some("l"),
            Action::Palette => Some(":"),
            Action::Quit => Some("q / esc"),
            Action::BulkPruneStopped => None,
            Action::BulkStopRunning => None,
            Action::BulkDestroyManaged => None,
            Action::BulkDestroyUnmanaged => None,
            Action::DeleteChar => Some("backspace"),
            Action::History => Some("H"),
            Action::ViewLogs => Some("L"),
        }
    }
}

/// All `Action` variants in the canonical order (for iteration in tests etc.).
pub const ALL_ACTIONS: &[Action] = &[
    Action::MoveUp,
    Action::MoveDown,
    Action::Open,
    Action::Inspect,
    Action::Create,
    Action::Stop,
    Action::Destroy,
    Action::Apply,
    Action::Edit,
    Action::Refresh,
    Action::Filter,
    Action::Cheatsheet,
    Action::Doctor,
    Action::CycleSkin,
    Action::CommandLog,
    Action::Palette,
    Action::Quit,
    Action::BulkPruneStopped,
    Action::BulkStopRunning,
    Action::BulkDestroyManaged,
    Action::BulkDestroyUnmanaged,
    Action::DeleteChar,
    // Bundle 3
    Action::History,
    Action::ViewLogs,
];

/// The ordered list of actions offered in the `:` command palette.
///
/// Returns every `Action` for which `in_palette()` is true (excludes nav and
/// input helpers like `DeleteChar`).
pub fn palette_actions() -> &'static [Action] {
    PALETTE_ACTIONS
}

/// Stable static slice of palette-worthy actions.
/// Excludes MoveUp, MoveDown, and DeleteChar (they are not discoverable via palette).
const PALETTE_ACTIONS: &[Action] = &[
    Action::Open,
    Action::Inspect,
    Action::Create,
    Action::Stop,
    Action::Destroy,
    Action::Apply,
    Action::Edit,
    Action::Refresh,
    Action::Filter,
    Action::Cheatsheet,
    Action::Doctor,
    Action::CycleSkin,
    Action::CommandLog,
    Action::Palette,
    Action::Quit,
    Action::BulkPruneStopped,
    Action::BulkStopRunning,
    Action::BulkDestroyManaged,
    Action::BulkDestroyUnmanaged,
    // Bundle 3
    Action::History,
    Action::ViewLogs,
];

/// The four bulk actions (for `bulk_only` palette scope).
pub const BULK_ACTIONS: &[Action] = &[
    Action::BulkPruneStopped,
    Action::BulkStopRunning,
    Action::BulkDestroyManaged,
    Action::BulkDestroyUnmanaged,
];
