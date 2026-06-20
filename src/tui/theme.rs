//! Theme tokens + color-mode resolution for the TUI.
//!
//! `ColorMode`, `Skin`, and `detect_from` are always compiled (no feature gate) so
//! unit tests can import them without the full ratatui dep.
//! `Theme` and `Theme::resolve` are gated behind `#[cfg(feature = "tui")]`
//! because they hold ratatui `Style` values.
//!
//! In the lean (no-tui) build the non-gated items are unused by the binary;
//! the module-level lint suppression prevents spurious dead_code warnings in
//! `make lint-lean` without hiding real issues inside the tui feature.
#![allow(dead_code)]

// ─── ColorMode ────────────────────────────────────────────────────────────────

/// Color capability tier for the current terminal session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    TrueColor,
    Ansi16,
    NoColor,
}

/// Pure, testable core of color-mode detection.
///
/// Arguments mirror what the env/TTY supply:
/// - `no_color_flag`: explicit `--no-color` flag from the CLI (or `false` when unset).
/// - `no_color_env`: value of `NO_COLOR` env var (`Some(_)` means it is set).
/// - `is_tty`: whether stdout is a TTY.
/// - `term`: value of `TERM` env var (empty string if unset).
/// - `colorterm`: value of `COLORTERM` env var (empty string if unset).
///
/// Decision rule (mirrors `cli/output.rs:14-23`, extended for 16-color):
/// 1. `no_color_flag` OR `NO_COLOR` set OR not a TTY → `NoColor`.
/// 2. `COLORTERM` in {`truecolor`, `24bit`} OR `TERM` matches `*-256color`/`*-direct` → `TrueColor`.
/// 3. Otherwise → `Ansi16`.
pub fn detect_from(
    no_color_flag: bool,
    no_color_env: Option<&str>,
    is_tty: bool,
    term: &str,
    colorterm: &str,
) -> ColorMode {
    if no_color_flag || no_color_env.is_some() || !is_tty {
        return ColorMode::NoColor;
    }
    let colorterm_lc = colorterm.to_lowercase();
    if colorterm_lc == "truecolor" || colorterm_lc == "24bit" {
        return ColorMode::TrueColor;
    }
    let term_lc = term.to_lowercase();
    if term_lc.ends_with("-256color") || term_lc.ends_with("-direct") {
        return ColorMode::TrueColor;
    }
    ColorMode::Ansi16
}

/// Thin wrapper that reads the actual environment.
/// Called ONCE in `app::run`; result is stored on `Model`.
pub fn detect(no_color_flag: bool) -> ColorMode {
    use std::io::IsTerminal;
    let no_color_env = std::env::var("NO_COLOR").ok();
    let is_tty = std::io::stdout().is_terminal();
    let term = std::env::var("TERM").unwrap_or_default();
    let colorterm = std::env::var("COLORTERM").unwrap_or_default();
    detect_from(
        no_color_flag,
        no_color_env.as_deref(),
        is_tty,
        &term,
        &colorterm,
    )
}

// ─── Skin ─────────────────────────────────────────────────────────────────────

/// Named palette/skin for the TUI. Cycles with the `t` key.
///
/// `Skin` is always compiled (no feature gate) so tests can import it and
/// assert the cycle/name invariants without ratatui.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Skin {
    /// Kraft amber (the shipped default retro look). UNCHANGED from pre-bundle-1.
    #[default]
    Kraft,
    /// Carbon: dark slate / cool-gray, low-chroma.
    Carbon,
    /// Blueprint: cyan-on-dark drafting palette.
    Blueprint,
}

impl Skin {
    /// Cycle to the next skin: Kraft → Carbon → Blueprint → Kraft.
    pub fn next(self) -> Skin {
        match self {
            Skin::Kraft => Skin::Carbon,
            Skin::Carbon => Skin::Blueprint,
            Skin::Blueprint => Skin::Kraft,
        }
    }

    /// Short lowercase name shown in the toast when cycling (e.g. `"kraft"`).
    pub fn name(self) -> &'static str {
        match self {
            Skin::Kraft => "kraft",
            Skin::Carbon => "carbon",
            Skin::Blueprint => "blueprint",
        }
    }
}

// ─── Theme ────────────────────────────────────────────────────────────────────

#[cfg(feature = "tui")]
pub use theme_inner::Theme;

#[cfg(feature = "tui")]
mod theme_inner {
    use ratatui::style::{Color, Modifier, Style};

    use super::{ColorMode, Skin};

    /// All named style tokens for the TUI.
    ///
    /// Build once per frame via `Theme::resolve(model.skin, model.color_mode)` and pass `&theme`
    /// into each `render_*` fn. Never stored as a global or static.
    ///
    /// Some fields are not yet consumed in the current view pass but are part of the
    /// public token API (tested in `tests/tui_theme.rs`) and available for future use.
    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    pub struct Theme {
        pub mode: ColorMode,

        // ── border + chrome ──
        pub border: Style,
        pub border_focus: Style,
        pub title: Style,

        // ── semantic accents ──
        pub accent: Style,
        pub accent_dim: Style,
        pub success: Style,
        pub warning: Style,
        pub danger: Style,
        pub muted: Style,

        // ── table ──
        pub header_cell: Style,
        pub selection: Style,

        // ── brand ──
        pub brand_logo: Style,
        pub brand_name: Style,
        pub brand_tagline: Style,

        // ── badges ──
        pub badge_running: Style,
        pub badge_stopped: Style,
        pub badge_error: Style,
        pub badge_unknown: Style,
    }

    impl Theme {
        /// Build the full token table for the given skin and color mode.
        /// Pure — no I/O. Unit-testable.
        ///
        /// NoColor arm is **skin-independent**: all skins collapse to the same
        /// zero-fg/bg modifier-only table. This is the P0 invariant (AC-SKIN-NOCOLOR).
        pub fn resolve(skin: Skin, mode: ColorMode) -> Self {
            match mode {
                ColorMode::NoColor => Self::nocolor(),
                ColorMode::TrueColor => match skin {
                    Skin::Kraft => Self::kraft_truecolor(),
                    Skin::Carbon => Self::carbon_truecolor(),
                    Skin::Blueprint => Self::blueprint_truecolor(),
                },
                ColorMode::Ansi16 => match skin {
                    Skin::Kraft => Self::kraft_ansi16(),
                    Skin::Carbon => Self::carbon_ansi16(),
                    Skin::Blueprint => Self::blueprint_ansi16(),
                },
            }
        }

        // ── Kraft / retro palette (default, UNCHANGED) ────────────────────────

        fn kraft_truecolor() -> Self {
            let accent = Style::default().fg(Color::Rgb(214, 158, 92));
            let accent_dim = Style::default().fg(Color::Rgb(150, 110, 66));
            let success = Style::default().fg(Color::Rgb(126, 184, 108));
            let warning = Style::default().fg(Color::Rgb(214, 138, 70));
            let danger = Style::default().fg(Color::Rgb(200, 86, 74));
            let muted = Style::default().fg(Color::Rgb(128, 128, 128));

            Theme {
                mode: ColorMode::TrueColor,
                border: Style::default().fg(Color::Rgb(150, 110, 66)),
                border_focus: Style::default().fg(Color::Rgb(214, 158, 92)),
                title: Style::default()
                    .fg(Color::Rgb(214, 158, 92))
                    .add_modifier(Modifier::BOLD),
                accent,
                accent_dim,
                success,
                warning,
                danger,
                muted,
                header_cell: Style::default()
                    .fg(Color::Rgb(214, 158, 92))
                    .add_modifier(Modifier::BOLD),
                selection: Style::default()
                    .bg(Color::Rgb(60, 46, 30))
                    .fg(Color::Rgb(235, 222, 200))
                    .add_modifier(Modifier::BOLD),
                brand_logo: Style::default().fg(Color::Rgb(214, 158, 92)),
                brand_name: Style::default()
                    .fg(Color::Rgb(214, 158, 92))
                    .add_modifier(Modifier::BOLD),
                brand_tagline: Style::default().fg(Color::Rgb(128, 128, 128)),
                badge_running: Style::default().fg(Color::Rgb(126, 184, 108)),
                badge_stopped: Style::default().fg(Color::Rgb(128, 128, 128)),
                badge_error: Style::default().fg(Color::Rgb(200, 86, 74)),
                badge_unknown: Style::default().fg(Color::Rgb(150, 110, 66)),
            }
        }

        fn kraft_ansi16() -> Self {
            Theme {
                mode: ColorMode::Ansi16,
                border: Style::default().fg(Color::DarkGray),
                border_focus: Style::default().fg(Color::Yellow),
                title: Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
                accent: Style::default().fg(Color::Yellow),
                accent_dim: Style::default().fg(Color::DarkGray),
                success: Style::default().fg(Color::Green),
                warning: Style::default().fg(Color::Yellow),
                danger: Style::default().fg(Color::Red),
                muted: Style::default().fg(Color::DarkGray),
                header_cell: Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
                selection: Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
                brand_logo: Style::default().fg(Color::Yellow),
                brand_name: Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
                brand_tagline: Style::default().fg(Color::DarkGray),
                badge_running: Style::default().fg(Color::Green),
                badge_stopped: Style::default().fg(Color::DarkGray),
                badge_error: Style::default().fg(Color::Red),
                badge_unknown: Style::default().fg(Color::DarkGray),
            }
        }

        // ── Carbon palette (dark slate / cool-gray) ───────────────────────────

        fn carbon_truecolor() -> Self {
            let accent = Style::default().fg(Color::Rgb(160, 170, 180)); // AC-SKIN-1 anchor
            let accent_dim = Style::default().fg(Color::Rgb(110, 118, 128));
            let success = Style::default().fg(Color::Rgb(120, 180, 140));
            let warning = Style::default().fg(Color::Rgb(210, 180, 110));
            let danger = Style::default().fg(Color::Rgb(205, 100, 95));
            let muted = Style::default().fg(Color::Rgb(100, 106, 115));

            Theme {
                mode: ColorMode::TrueColor,
                border: Style::default().fg(Color::Rgb(80, 86, 94)),
                border_focus: Style::default().fg(Color::Rgb(160, 170, 180)),
                title: Style::default()
                    .fg(Color::Rgb(160, 170, 180))
                    .add_modifier(Modifier::BOLD),
                accent,
                accent_dim,
                success,
                warning,
                danger,
                muted,
                header_cell: Style::default()
                    .fg(Color::Rgb(160, 170, 180))
                    .add_modifier(Modifier::BOLD),
                selection: Style::default()
                    .bg(Color::Rgb(40, 44, 50))
                    .fg(Color::Rgb(220, 224, 230))
                    .add_modifier(Modifier::BOLD),
                brand_logo: Style::default().fg(Color::Rgb(160, 170, 180)),
                brand_name: Style::default()
                    .fg(Color::Rgb(160, 170, 180))
                    .add_modifier(Modifier::BOLD),
                brand_tagline: Style::default().fg(Color::Rgb(100, 106, 115)),
                badge_running: Style::default().fg(Color::Rgb(120, 180, 140)),
                badge_stopped: Style::default().fg(Color::Rgb(100, 106, 115)),
                badge_error: Style::default().fg(Color::Rgb(205, 100, 95)),
                badge_unknown: Style::default().fg(Color::Rgb(110, 118, 128)),
            }
        }

        fn carbon_ansi16() -> Self {
            Theme {
                mode: ColorMode::Ansi16,
                border: Style::default().fg(Color::DarkGray),
                border_focus: Style::default().fg(Color::White),
                title: Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
                accent: Style::default().fg(Color::White), // AC-SKIN-1 anchor
                accent_dim: Style::default().fg(Color::Gray),
                success: Style::default().fg(Color::Green),
                warning: Style::default().fg(Color::Yellow),
                danger: Style::default().fg(Color::Red),
                muted: Style::default().fg(Color::DarkGray),
                header_cell: Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
                selection: Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
                brand_logo: Style::default().fg(Color::White),
                brand_name: Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
                brand_tagline: Style::default().fg(Color::DarkGray),
                badge_running: Style::default().fg(Color::Green),
                badge_stopped: Style::default().fg(Color::DarkGray),
                badge_error: Style::default().fg(Color::Red),
                badge_unknown: Style::default().fg(Color::DarkGray),
            }
        }

        // ── Blueprint palette (cyan-on-dark drafting look) ────────────────────

        fn blueprint_truecolor() -> Self {
            let accent = Style::default().fg(Color::Rgb(90, 170, 200)); // AC-SKIN-1 anchor
            let accent_dim = Style::default().fg(Color::Rgb(60, 120, 150));
            let success = Style::default().fg(Color::Rgb(110, 190, 160));
            let warning = Style::default().fg(Color::Rgb(220, 180, 90));
            let danger = Style::default().fg(Color::Rgb(210, 95, 90));
            let muted = Style::default().fg(Color::Rgb(90, 110, 130));

            Theme {
                mode: ColorMode::TrueColor,
                border: Style::default().fg(Color::Rgb(60, 100, 130)),
                border_focus: Style::default().fg(Color::Rgb(90, 170, 200)),
                title: Style::default()
                    .fg(Color::Rgb(90, 170, 200))
                    .add_modifier(Modifier::BOLD),
                accent,
                accent_dim,
                success,
                warning,
                danger,
                muted,
                header_cell: Style::default()
                    .fg(Color::Rgb(90, 170, 200))
                    .add_modifier(Modifier::BOLD),
                selection: Style::default()
                    .bg(Color::Rgb(20, 45, 60))
                    .fg(Color::Rgb(210, 235, 245))
                    .add_modifier(Modifier::BOLD),
                brand_logo: Style::default().fg(Color::Rgb(90, 170, 200)),
                brand_name: Style::default()
                    .fg(Color::Rgb(90, 170, 200))
                    .add_modifier(Modifier::BOLD),
                brand_tagline: Style::default().fg(Color::Rgb(90, 110, 130)),
                badge_running: Style::default().fg(Color::Rgb(110, 190, 160)),
                badge_stopped: Style::default().fg(Color::Rgb(60, 120, 150)),
                badge_error: Style::default().fg(Color::Rgb(210, 95, 90)),
                badge_unknown: Style::default().fg(Color::Rgb(60, 100, 130)),
            }
        }

        fn blueprint_ansi16() -> Self {
            Theme {
                mode: ColorMode::Ansi16,
                border: Style::default().fg(Color::Blue),
                border_focus: Style::default().fg(Color::Cyan),
                title: Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
                accent: Style::default().fg(Color::Cyan), // AC-SKIN-1 anchor
                accent_dim: Style::default().fg(Color::Blue),
                success: Style::default().fg(Color::Green),
                warning: Style::default().fg(Color::Yellow),
                danger: Style::default().fg(Color::Red),
                muted: Style::default().fg(Color::DarkGray),
                header_cell: Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
                selection: Style::default()
                    .bg(Color::Cyan)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
                brand_logo: Style::default().fg(Color::Cyan),
                brand_name: Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
                brand_tagline: Style::default().fg(Color::Blue),
                badge_running: Style::default().fg(Color::Green),
                badge_stopped: Style::default().fg(Color::Blue),
                badge_error: Style::default().fg(Color::Red),
                badge_unknown: Style::default().fg(Color::Blue),
            }
        }

        // ── NoColor tier — SKIN-INDEPENDENT (P0 invariant) ───────────────────

        /// NoColor invariant (P0): NO style carries any fg/bg color, for ANY skin.
        /// Differentiation is ONLY via Modifier (BOLD/DIM/REVERSED).
        /// The skin argument is intentionally ignored in `resolve` when mode is NoColor.
        fn nocolor() -> Self {
            Theme {
                mode: ColorMode::NoColor,
                border: Style::default(),
                border_focus: Style::default().add_modifier(Modifier::BOLD),
                title: Style::default().add_modifier(Modifier::BOLD),
                accent: Style::default().add_modifier(Modifier::BOLD),
                accent_dim: Style::default(),
                success: Style::default().add_modifier(Modifier::BOLD),
                warning: Style::default().add_modifier(Modifier::BOLD),
                danger: Style::default().add_modifier(Modifier::BOLD),
                muted: Style::default().add_modifier(Modifier::DIM),
                header_cell: Style::default().add_modifier(Modifier::BOLD),
                selection: Style::default()
                    .add_modifier(Modifier::REVERSED)
                    .add_modifier(Modifier::BOLD),
                brand_logo: Style::default().add_modifier(Modifier::BOLD),
                brand_name: Style::default().add_modifier(Modifier::BOLD),
                brand_tagline: Style::default().add_modifier(Modifier::DIM),
                badge_running: Style::default().add_modifier(Modifier::BOLD),
                badge_stopped: Style::default().add_modifier(Modifier::DIM),
                badge_error: Style::default().add_modifier(Modifier::BOLD),
                badge_unknown: Style::default(),
            }
        }
    }
}

// ─── Badge component ──────────────────────────────────────────────────────────

/// The visual kind of a status badge — pure, testable, independent of ratatui.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeKind {
    Running,
    Stopped,
    Error,
    Unknown,
}

/// Classify a raw distrobox/podman status string into a `BadgeKind`.
///
/// Pure function — unit-testable without a terminal (AC-BADGE-1).
/// Preserves existing running/up → green, else dim semantics from `view.rs:87-93`.
pub fn classify_status(raw: &str) -> BadgeKind {
    let s = raw.to_lowercase();
    if s.contains("running") || s.contains("up") {
        BadgeKind::Running
    } else if s.contains("exit") || s.contains("stopped") || s.contains("created") {
        BadgeKind::Stopped
    } else if s.contains("error") || s.contains("dead") {
        BadgeKind::Error
    } else {
        BadgeKind::Unknown
    }
}

/// Glyph string for a `BadgeKind` (single-cell, degrades cleanly in no-color).
pub fn badge_glyph(kind: BadgeKind) -> &'static str {
    match kind {
        BadgeKind::Running => "●",
        BadgeKind::Stopped => "○",
        BadgeKind::Error => "✗",
        BadgeKind::Unknown => "·",
    }
}

/// Short label for a `BadgeKind`.
pub fn badge_label(kind: BadgeKind) -> &'static str {
    match kind {
        BadgeKind::Running => "up",
        BadgeKind::Stopped => "sealed",
        BadgeKind::Error => "trouble",
        BadgeKind::Unknown => "unknown",
    }
}

/// Return a styled `Span` for the given raw status string.
/// The span content is `"{glyph} {label}"`.
#[cfg(feature = "tui")]
pub fn badge_span<'a>(raw: &str, theme: &Theme) -> ratatui::text::Span<'a> {
    let kind = classify_status(raw);
    let style = match kind {
        BadgeKind::Running => theme.badge_running,
        BadgeKind::Stopped => theme.badge_stopped,
        BadgeKind::Error => theme.badge_error,
        BadgeKind::Unknown => theme.badge_unknown,
    };
    let content = format!("{} {}", badge_glyph(kind), badge_label(kind));
    ratatui::text::Span::styled(content, style)
}

/// Return a styled `Span` for a boolean ok/fail check-mark (used in DoctorPanel).
#[cfg(feature = "tui")]
pub fn ok_glyph<'a>(b: bool, theme: &Theme) -> ratatui::text::Span<'a> {
    if b {
        ratatui::text::Span::styled("✓", theme.success)
    } else {
        ratatui::text::Span::styled("✗", theme.danger)
    }
}

// ─── Brand header helpers ──────────────────────────────────────────────────────

/// Width threshold below which the brand header collapses to logo + wordmark only.
pub const HEADER_COLLAPSE_WIDTH: u16 = 60;

/// Pure collapse predicate — unit-testable (AC-HEADER-1).
pub fn header_should_collapse(width: u16) -> bool {
    width < HEADER_COLLAPSE_WIDTH
}
