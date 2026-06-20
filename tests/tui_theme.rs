//! Unit tests for theme tokens, badge classifier, brand header helper, and
//! centralized copy — covering AC-THEME-1/2/3, AC-BADGE-1/2, AC-HEADER-1,
//! AC-COPY-1/2.
//!
//! These tests target PURE helpers only; no golden frame rendering.
#![cfg(feature = "tui")]

use cbox::tui::strings;
use cbox::tui::theme::{
    badge_glyph, badge_label, classify_status, detect_from, header_should_collapse, BadgeKind,
    ColorMode, Skin, Theme, HEADER_COLLAPSE_WIDTH,
};

// ─── AC-THEME-1: color mode detection ────────────────────────────────────────

#[test]
fn ac_theme_1_no_color_flag_returns_nocolor() {
    let mode = detect_from(true, None, true, "xterm-256color", "truecolor");
    assert_eq!(mode, ColorMode::NoColor);
}

#[test]
fn ac_theme_1_no_color_env_returns_nocolor() {
    // NO_COLOR env set (even empty string means "set")
    let mode = detect_from(false, Some(""), true, "xterm-256color", "truecolor");
    assert_eq!(mode, ColorMode::NoColor);
}

#[test]
fn ac_theme_1_non_tty_returns_nocolor() {
    let mode = detect_from(false, None, false, "xterm-256color", "truecolor");
    assert_eq!(mode, ColorMode::NoColor);
}

#[test]
fn ac_theme_1_colorterm_truecolor() {
    let mode = detect_from(false, None, true, "xterm", "truecolor");
    assert_eq!(mode, ColorMode::TrueColor);
}

#[test]
fn ac_theme_1_colorterm_24bit() {
    let mode = detect_from(false, None, true, "xterm", "24bit");
    assert_eq!(mode, ColorMode::TrueColor);
}

#[test]
fn ac_theme_1_term_256color() {
    let mode = detect_from(false, None, true, "xterm-256color", "");
    assert_eq!(mode, ColorMode::TrueColor);
}

#[test]
fn ac_theme_1_plain_term_fallback_to_ansi16() {
    let mode = detect_from(false, None, true, "xterm", "");
    assert_eq!(mode, ColorMode::Ansi16);
}

// ─── AC-THEME-2: TrueColor and Ansi16 token values ───────────────────────────

#[test]
fn ac_theme_2_truecolor_accent_rgb() {
    use ratatui::style::Color;
    let theme = Theme::resolve(Skin::Kraft, ColorMode::TrueColor);
    assert_eq!(
        theme.accent.fg,
        Some(Color::Rgb(214, 158, 92)),
        "TrueColor accent must be kraft amber Rgb(214,158,92)"
    );
}

#[test]
fn ac_theme_2_ansi16_accent_yellow() {
    use ratatui::style::Color;
    let theme = Theme::resolve(Skin::Kraft, ColorMode::Ansi16);
    assert_eq!(
        theme.accent.fg,
        Some(Color::Yellow),
        "Ansi16 accent must map to Color::Yellow"
    );
}

// ─── AC-SKIN-1: skin-specific accent anchors ─────────────────────────────────

#[test]
fn ac_skin_1_kraft_truecolor_accent() {
    use ratatui::style::Color;
    let theme = Theme::resolve(Skin::Kraft, ColorMode::TrueColor);
    assert_eq!(
        theme.accent.fg,
        Some(Color::Rgb(214, 158, 92)),
        "Kraft TrueColor accent must be Rgb(214,158,92)"
    );
}

#[test]
fn ac_skin_1_carbon_truecolor_accent() {
    use ratatui::style::Color;
    let theme = Theme::resolve(Skin::Carbon, ColorMode::TrueColor);
    assert_eq!(
        theme.accent.fg,
        Some(Color::Rgb(160, 170, 180)),
        "Carbon TrueColor accent must be Rgb(160,170,180)"
    );
}

#[test]
fn ac_skin_1_blueprint_truecolor_accent() {
    use ratatui::style::Color;
    let theme = Theme::resolve(Skin::Blueprint, ColorMode::TrueColor);
    assert_eq!(
        theme.accent.fg,
        Some(Color::Rgb(90, 170, 200)),
        "Blueprint TrueColor accent must be Rgb(90,170,200)"
    );
}

#[test]
fn ac_skin_1_kraft_ansi16_accent_yellow() {
    use ratatui::style::Color;
    let theme = Theme::resolve(Skin::Kraft, ColorMode::Ansi16);
    assert_eq!(theme.accent.fg, Some(Color::Yellow));
}

#[test]
fn ac_skin_1_carbon_ansi16_accent_white() {
    use ratatui::style::Color;
    let theme = Theme::resolve(Skin::Carbon, ColorMode::Ansi16);
    assert_eq!(theme.accent.fg, Some(Color::White));
}

#[test]
fn ac_skin_1_blueprint_ansi16_accent_cyan() {
    use ratatui::style::Color;
    let theme = Theme::resolve(Skin::Blueprint, ColorMode::Ansi16);
    assert_eq!(theme.accent.fg, Some(Color::Cyan));
}

// ─── AC-THEME-3 / AC-SKIN-NOCOLOR: NoColor P0 invariant for EVERY skin ───────

/// For every skin, `Theme::resolve(skin, NoColor)` must have fg==None && bg==None
/// for ALL 18 style fields.  The NoColor arm is skin-independent (same table),
/// but we assert it for each skin to pin the guarantee mechanically (AC-SKIN-NOCOLOR).
fn assert_nocolor_invariant(skin: Skin) {
    let theme = Theme::resolve(skin, ColorMode::NoColor);

    let styles = [
        ("border", theme.border),
        ("border_focus", theme.border_focus),
        ("title", theme.title),
        ("accent", theme.accent),
        ("accent_dim", theme.accent_dim),
        ("success", theme.success),
        ("warning", theme.warning),
        ("danger", theme.danger),
        ("muted", theme.muted),
        ("header_cell", theme.header_cell),
        ("selection", theme.selection),
        ("brand_logo", theme.brand_logo),
        ("brand_name", theme.brand_name),
        ("brand_tagline", theme.brand_tagline),
        ("badge_running", theme.badge_running),
        ("badge_stopped", theme.badge_stopped),
        ("badge_error", theme.badge_error),
        ("badge_unknown", theme.badge_unknown),
    ];

    for (name, style) in &styles {
        assert!(
            style.fg.is_none(),
            "NoColor theme field `{name}` (skin {:?}) must have fg=None, got {:?}",
            skin,
            style.fg
        );
        assert!(
            style.bg.is_none(),
            "NoColor theme field `{name}` (skin {:?}) must have bg=None, got {:?}",
            skin,
            style.bg
        );
    }
}

#[test]
fn ac_theme_3_nocolor_no_fg_bg_anywhere() {
    // Legacy name kept for continuity; now loops all skins (AC-SKIN-NOCOLOR).
    for skin in [Skin::Kraft, Skin::Carbon, Skin::Blueprint] {
        assert_nocolor_invariant(skin);
    }
}

#[test]
fn ac_skin_nocolor_kraft() {
    assert_nocolor_invariant(Skin::Kraft);
}

#[test]
fn ac_skin_nocolor_carbon() {
    assert_nocolor_invariant(Skin::Carbon);
}

#[test]
fn ac_skin_nocolor_blueprint() {
    assert_nocolor_invariant(Skin::Blueprint);
}

// ─── AC-SKIN-CYCLE: skin cycle order ─────────────────────────────────────────

#[test]
fn ac_skin_cycle_kraft_to_carbon() {
    assert_eq!(Skin::Kraft.next(), Skin::Carbon);
}

#[test]
fn ac_skin_cycle_carbon_to_blueprint() {
    assert_eq!(Skin::Carbon.next(), Skin::Blueprint);
}

#[test]
fn ac_skin_cycle_blueprint_to_kraft() {
    assert_eq!(Skin::Blueprint.next(), Skin::Kraft);
}

#[test]
fn ac_skin_name_all() {
    assert_eq!(Skin::Kraft.name(), "kraft");
    assert_eq!(Skin::Carbon.name(), "carbon");
    assert_eq!(Skin::Blueprint.name(), "blueprint");
}

// ─── AC-BADGE-1: classify_status ─────────────────────────────────────────────

#[test]
fn ac_badge_1_running_lowercase() {
    assert_eq!(classify_status("running"), BadgeKind::Running);
}

#[test]
fn ac_badge_1_up_3_minutes() {
    assert_eq!(classify_status("Up 3 minutes"), BadgeKind::Running);
}

#[test]
fn ac_badge_1_exited_0() {
    assert_eq!(classify_status("exited (0)"), BadgeKind::Stopped);
}

#[test]
fn ac_badge_1_created() {
    assert_eq!(classify_status("created"), BadgeKind::Stopped);
}

#[test]
fn ac_badge_1_stopped_status() {
    assert_eq!(classify_status("stopped"), BadgeKind::Stopped);
}

#[test]
fn ac_badge_1_dead() {
    assert_eq!(classify_status("dead"), BadgeKind::Error);
}

#[test]
fn ac_badge_1_error_status() {
    assert_eq!(
        classify_status("error starting container"),
        BadgeKind::Error
    );
}

#[test]
fn ac_badge_1_unknown_weird() {
    assert_eq!(classify_status("weird"), BadgeKind::Unknown);
}

// ─── AC-BADGE-2: glyph/label per kind ────────────────────────────────────────

#[test]
fn ac_badge_2_running_glyph_label() {
    assert_eq!(badge_glyph(BadgeKind::Running), "●");
    assert_eq!(badge_label(BadgeKind::Running), "up");
}

#[test]
fn ac_badge_2_stopped_glyph_label() {
    assert_eq!(badge_glyph(BadgeKind::Stopped), "○");
    assert_eq!(badge_label(BadgeKind::Stopped), "sealed");
}

#[test]
fn ac_badge_2_error_glyph_label() {
    assert_eq!(badge_glyph(BadgeKind::Error), "✗");
    assert_eq!(badge_label(BadgeKind::Error), "trouble");
}

#[test]
fn ac_badge_2_unknown_glyph_label() {
    assert_eq!(badge_glyph(BadgeKind::Unknown), "·");
    assert_eq!(badge_label(BadgeKind::Unknown), "unknown");
}

/// badge_span for Running uses theme.badge_running style.
#[test]
fn ac_badge_2_badge_span_running_style() {
    use cbox::tui::theme::badge_span;
    let theme = Theme::resolve(Skin::Kraft, ColorMode::TrueColor);
    let span = badge_span("running", &theme);
    assert_eq!(span.style, theme.badge_running);
    assert!(span.content.contains("●"));
    assert!(span.content.contains("up"));
}

#[test]
fn ac_badge_2_badge_span_stopped_style() {
    use cbox::tui::theme::badge_span;
    let theme = Theme::resolve(Skin::Kraft, ColorMode::TrueColor);
    let span = badge_span("exited (0)", &theme);
    assert_eq!(span.style, theme.badge_stopped);
    assert!(span.content.contains("○"));
    assert!(span.content.contains("sealed"));
}

// ─── AC-HEADER-1: collapse threshold ─────────────────────────────────────────

#[test]
fn ac_header_1_collapse_at_59() {
    assert!(
        header_should_collapse(59),
        "width 59 (below threshold {HEADER_COLLAPSE_WIDTH}) must collapse"
    );
}

#[test]
fn ac_header_1_no_collapse_at_60() {
    assert!(
        !header_should_collapse(60),
        "width 60 (at threshold {HEADER_COLLAPSE_WIDTH}) must NOT collapse"
    );
}

#[test]
fn ac_header_1_no_collapse_at_120() {
    assert!(
        !header_should_collapse(120),
        "width 120 (well above threshold) must NOT collapse"
    );
}

// ─── AC-COPY-1: voice compliance + non-empty ─────────────────────────────────

const BANNED: &[&str] = &[
    "cozy",
    "beautiful",
    "friendly",
    "delightful",
    "cute",
    "lovely",
];

/// Every public copy const must be non-empty and must not contain a banned adjective.
#[test]
fn ac_copy_1_all_consts_non_empty_and_compliant() {
    let consts: &[(&str, &str)] = &[
        ("WORDMARK", strings::WORDMARK),
        ("TAGLINE", strings::TAGLINE),
        ("LOGO_GLYPH", strings::LOGO_GLYPH),
        ("EMPTY_LIST", strings::EMPTY_LIST),
        ("EMPTY_DETAIL", strings::EMPTY_DETAIL),
        ("LOADING_LIST", strings::LOADING_LIST),
        ("LOADING_DETAIL", strings::LOADING_DETAIL),
        ("LOADING_DOCTOR", strings::LOADING_DOCTOR),
        ("PROGRESS_RUNNING", strings::PROGRESS_RUNNING),
        ("PROGRESS_DONE", strings::PROGRESS_DONE),
        ("ERROR_PREFIX", strings::ERROR_PREFIX),
        ("HELP", strings::HELP),
        // Bundle 1: command-log copy consts (R-7 — must be voice-compliant).
        ("CMDLOG_TITLE", strings::CMDLOG_TITLE),
        ("CMDLOG_EMPTY", strings::CMDLOG_EMPTY),
        ("CMDLOG_HINT", strings::CMDLOG_HINT),
    ];

    for (name, value) in consts {
        assert!(!value.is_empty(), "strings::{name} must not be empty");
        let lower = value.to_lowercase();
        for banned in BANNED {
            assert!(
                !lower.contains(banned),
                "strings::{name} contains banned adjective \"{banned}\": {value:?}"
            );
        }
    }
}

// ─── AC-COPY-2: formatter output shapes ──────────────────────────────────────

#[test]
fn ac_copy_2_loaded_contains_n() {
    let s = strings::loaded(2);
    assert!(!s.is_empty());
    assert!(s.contains("2"), "loaded(2) must contain '2', got: {s:?}");
}

#[test]
fn ac_copy_2_created_contains_name() {
    let s = strings::created("web");
    assert!(!s.is_empty());
    assert!(
        s.contains("web"),
        "created(\"web\") must contain 'web', got: {s:?}"
    );
}

#[test]
fn ac_copy_2_removed_contains_list() {
    let s = strings::removed("box-a, box-b");
    assert!(s.contains("box-a"));
}

#[test]
fn ac_copy_2_stopped_contains_list() {
    let s = strings::stopped("box-a");
    assert!(s.contains("box-a"));
}

#[test]
fn ac_copy_2_applied_shape() {
    let s = strings::applied("mybox", 3, 1, 0);
    assert!(s.contains("mybox"));
    assert!(s.contains("3"));
}
