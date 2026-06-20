//! Tests for the keymap-as-data module.
//! Covers AC-MAP-1, AC-MAP-2, AC-MAP-VOICE, AC-ACTION-1.
//! This module is always compiled (no feature gate) because keymap.rs is ungated.

use cbox::tui::action::{Action, ALL_ACTIONS};
use cbox::tui::keymap::{help_line, keymap_for, KeyContext};

// ─── AC-MAP-1: help_line is derived, not hand-written ────────────────────────

#[test]
fn ac_map_1_help_line_contains_required_actions() {
    let line = help_line(KeyContext::List);
    assert!(!line.is_empty(), "help_line must not be empty");
    assert!(line.contains("move"), "help line must contain 'move'");
    assert!(line.contains("pack"), "help line must contain 'pack'");
    assert!(line.contains("seal"), "help line must contain 'seal'");
    assert!(
        line.contains("cheatsheet"),
        "help line must contain 'cheatsheet'"
    );
    assert!(line.contains("quit"), "help line must contain 'quit'");
}

// ─── AC-MAP-2: per-context entry sets ────────────────────────────────────────

#[test]
fn ac_map_2_list_contains_all_required_keys() {
    let bindings = keymap_for(KeyContext::List);
    let keys: Vec<&str> = bindings.iter().map(|b| b.key).collect();

    // Check that key display strings for required bindings are present.
    let required_keys = ["/", "?", "D", "t", "l", "c", "d", "s", "a", "e"];
    for required in &required_keys {
        assert!(
            keys.iter().any(|k| k.contains(required)),
            "List keymap must contain key '{required}'"
        );
    }
}

#[test]
fn ac_map_2_list_contains_palette_and_bulk_keys() {
    let bindings = keymap_for(KeyContext::List);
    let keys: Vec<&str> = bindings.iter().map(|b| b.key).collect();
    assert!(keys.contains(&":"), "List keymap must have ':' for palette");
    assert!(
        keys.contains(&"b"),
        "List keymap must have 'b' for bulk-scoped palette"
    );
}

#[test]
fn ac_map_2_filter_input_context() {
    let bindings = keymap_for(KeyContext::FilterInput);
    let labels: Vec<&str> = bindings.iter().map(|b| b.action.label()).collect();
    assert!(
        labels
            .iter()
            .any(|a| a.contains("narrow") || a.contains("filter")),
        "FilterInput must advertise type/filter action"
    );
    assert!(
        labels
            .iter()
            .any(|a| a.contains("delete") || a.contains("character")),
        "FilterInput must advertise delete/backspace action"
    );
}

#[test]
fn ac_map_2_command_log_context() {
    let bindings = keymap_for(KeyContext::CommandLog);
    let labels: Vec<&str> = bindings.iter().map(|b| b.action.label()).collect();
    assert!(
        labels
            .iter()
            .any(|a| a.contains("scroll") || a.contains("move")),
        "CommandLog must advertise scroll action"
    );
}

#[test]
fn ac_map_2_filter_and_command_log_are_distinct() {
    let filter_bindings = keymap_for(KeyContext::FilterInput);
    let cmdlog_bindings = keymap_for(KeyContext::CommandLog);
    assert_ne!(
        filter_bindings.len(),
        0,
        "FilterInput must have at least one binding"
    );
    assert_ne!(
        cmdlog_bindings.len(),
        0,
        "CommandLog must have at least one binding"
    );
    // Different lengths confirm distinct tables
    assert_ne!(
        filter_bindings.len(),
        cmdlog_bindings.len(),
        "FilterInput and CommandLog must be distinct keymap tables"
    );
}

// ─── AC-MAP-VOICE / AC-VOICE-1: all action labels free of banned adjectives ──

const BANNED_ADJECTIVES: &[&str] = &[
    "cozy",
    "beautiful",
    "friendly",
    "delightful",
    "cute",
    "lovely",
];

#[test]
fn ac_map_voice_all_action_labels_free_of_banned_adjectives() {
    // AC-VOICE-1: every Action::label() is free of banned adjectives.
    for action in ALL_ACTIONS {
        let label = action.label();
        let lower = label.to_lowercase();
        assert!(!label.is_empty(), "{action:?}.label() must not be empty");
        for banned in BANNED_ADJECTIVES {
            assert!(
                !lower.contains(banned),
                "{action:?}.label() contains banned adjective {banned:?}: {label:?}"
            );
        }
    }
}

#[test]
fn ac_map_voice_keymap_labels_free_of_banned_adjectives() {
    let contexts = [
        KeyContext::List,
        KeyContext::Detail,
        KeyContext::Wizard,
        KeyContext::ConfirmDestroy,
        KeyContext::Progress,
        KeyContext::DoctorPanel,
        KeyContext::FilterInput,
        KeyContext::Cheatsheet,
        KeyContext::CommandLog,
        KeyContext::Palette,
        KeyContext::BulkConfirm,
    ];

    for ctx in &contexts {
        let bindings = keymap_for(*ctx);
        for binding in bindings {
            let label = binding.action.label();
            let lower = label.to_lowercase();
            for banned in BANNED_ADJECTIVES {
                assert!(
                    !lower.contains(banned),
                    "keymap action {:?} in context {:?} contains banned adjective {:?}",
                    label,
                    ctx,
                    banned
                );
            }
        }
    }
}

// ─── AC-ACTION-1: KEYMAP_LIST contains the expected Action variants ───────────

#[test]
fn ac_action_1_keymap_list_contains_required_actions() {
    let bindings = keymap_for(KeyContext::List);
    let actions: Vec<Action> = bindings.iter().map(|b| b.action).collect();

    let required = [
        Action::Filter,
        Action::Cheatsheet,
        Action::Doctor,
        Action::CycleSkin,
        Action::CommandLog,
        Action::Palette,
        Action::Create,
        Action::Stop,
        Action::Destroy,
        Action::Apply,
        Action::Edit,
        Action::Refresh,
        Action::Quit,
    ];

    for r in &required {
        assert!(actions.contains(r), "KEYMAP_LIST must contain {r:?}");
    }
}

// ─── AC-ACTION-2: palette_actions() contains expected actions ─────────────────

#[test]
fn ac_action_2_palette_actions_include_bulk_and_exclude_nav() {
    use cbox::tui::action::palette_actions;

    let actions = palette_actions();
    let labels: Vec<&str> = actions.iter().map(|a| a.label()).collect();

    // Must include bulk actions.
    assert!(
        actions.contains(&Action::BulkPruneStopped),
        "palette must contain BulkPruneStopped"
    );
    assert!(
        actions.contains(&Action::BulkStopRunning),
        "palette must contain BulkStopRunning"
    );
    assert!(
        actions.contains(&Action::BulkDestroyManaged),
        "palette must contain BulkDestroyManaged"
    );
    assert!(
        actions.contains(&Action::BulkDestroyUnmanaged),
        "palette must contain BulkDestroyUnmanaged"
    );

    // Must NOT include navigation-only actions.
    assert!(
        !actions.contains(&Action::MoveUp),
        "palette must not contain MoveUp"
    );
    assert!(
        !actions.contains(&Action::MoveDown),
        "palette must not contain MoveDown"
    );

    let _ = labels; // used for debugging
}
