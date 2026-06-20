//! Tests for the keymap-as-data module.
//! Covers AC-MAP-1, AC-MAP-2, AC-MAP-VOICE.
//! This module is always compiled (no feature gate) because keymap.rs is ungated.

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
fn ac_map_2_filter_input_context() {
    let bindings = keymap_for(KeyContext::FilterInput);
    let actions: Vec<&str> = bindings.iter().map(|b| b.action).collect();
    assert!(
        actions
            .iter()
            .any(|a| a.contains("narrow") || a.contains("filter")),
        "FilterInput must advertise type/filter action"
    );
    assert!(
        actions.iter().any(|a| a.contains("delete")),
        "FilterInput must advertise delete/backspace action"
    );
    assert!(
        actions.iter().any(|a| a.contains("close")),
        "FilterInput must advertise close/esc action"
    );
}

#[test]
fn ac_map_2_command_log_context() {
    let bindings = keymap_for(KeyContext::CommandLog);
    let actions: Vec<&str> = bindings.iter().map(|b| b.action).collect();
    assert!(
        actions.iter().any(|a| a.contains("scroll")),
        "CommandLog must advertise scroll action"
    );
    assert!(
        actions.iter().any(|a| a.contains("close")),
        "CommandLog must advertise close action"
    );
}

#[test]
fn ac_map_2_filter_and_command_log_are_distinct() {
    let filter_bindings = keymap_for(KeyContext::FilterInput);
    let cmdlog_bindings = keymap_for(KeyContext::CommandLog);
    // They should not be the same slice
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

// ─── AC-MAP-VOICE: all action strings free of banned adjectives ───────────────

const BANNED_ADJECTIVES: &[&str] = &[
    "cozy",
    "beautiful",
    "friendly",
    "delightful",
    "cute",
    "lovely",
];

#[test]
fn ac_map_voice_all_actions_free_of_banned_adjectives() {
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
    ];

    for ctx in &contexts {
        let bindings = keymap_for(*ctx);
        for binding in bindings {
            let action_lower = binding.action.to_lowercase();
            for banned in BANNED_ADJECTIVES {
                assert!(
                    !action_lower.contains(banned),
                    "keymap action {:?} in context {:?} contains banned adjective {:?}",
                    binding.action,
                    ctx,
                    banned
                );
            }
        }
    }
}
