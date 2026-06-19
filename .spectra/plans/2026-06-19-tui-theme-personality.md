# cbox TUI Personality + Theme Foundation — SPECTRA Spec

- **Spec ID:** `2026-06-19-tui-theme-personality`
- **Intent:** REQUEST / CHANGE (direction LOCKED by human; this spec makes it precise + buildable)
- **Coder:** Vivi (feature work) / Kupo (flagged micro-tasks)
- **Complexity:** 8/12 (extended depth, standard tier, single-pass cycle)
- **Confidence:** 87% → AUTO_PROCEED
- **Build constraint (pass to coder):** Builds are CONTAINERIZED. NEVER run host `cargo`. Use `make` targets only (`make build`, `make release`, `make test`, `make check`). The `tui` feature is default-on (`Cargo.toml:17-18`), so plain `make build`/`make test` exercises this code.
- **Aesthetic (LOCKED, do not relitigate):** Playful / retro — heavy box-drawing, ASCII wordmark, characterful state badges, whimsy in glyphs + status copy, WITHIN the "show don't tell" voice rule (never advertise cozy/beautiful/friendly/delightful; demonstrate via layout, glyphs, concrete wording).

---

## 1. Scope

### In scope (this pass)
1. New `src/tui/theme.rs` — semantic palette tokens + named styles + color-mode resolution. Replaces ALL inline styling in `view.rs`.
2. New `src/tui/strings.rs` — centralized user-facing copy (header tagline, empty/loading/error states, help line, badge labels). See §4 for the centralization decision and exact strings.
3. Reusable **state badge** component (status string → glyph + label + style).
4. Reusable **brand header** strip (logo glyph + wordmark + tagline), collapsible by width, drawn straight above the list — NO splash.
5. Restyle all 6 screens + status bar against named theme tokens (`view.rs:47-558`).
6. Richer empty / loading / error states (concrete copy).
7. NO_COLOR + `--no-color` + low-color (16-color) + true/256-color resolution, porting the gate from `cli/output.rs:14-23`.
8. New unit tests for `theme.rs`, the badge mapper, and the header-collapse helper (pure functions only — no golden frame rendering).

### Out of scope (note as future work; do NOT spec in detail)
- New About / Splash screen.
- Animated transitions.
- Responsive / small-terminal reflow beyond the single header-collapse rule defined here. (Note: `Resize` is currently a no-op at `update.rs:38`; this spec deliberately keeps reflow out and computes collapse from the per-frame `Rect` width inside `view`, not from resize events.)
- Changing TUI control flow, keybindings, the reducer, effects, or the worker thread.

### Deferred / explicitly NOT touched
- `model.rs`, `update.rs` logic, `effect.rs`, `app.rs`, `message.rs` — UNCHANGED except: (a) the ONE copy-centralization move in §4 touches the status-string call sites in `update.rs`; (b) `theme.rs` needs a color-mode value computed once at launch in `app.rs` and threaded to `view`. Both are surgical and called out per-task.

---

## 2. Approach + Rationale

**Selected hypothesis: H2 — "Theme-by-value + per-frame width, copy centralized" (ADAPT from cbox's existing `OutputCtx` pattern).**

Three angles drove this:
- **Lowest blast radius:** `view.rs` is pure and untested (`view.rs:1-3`), the highest-leverage / lowest-risk surface. Concentrate change there.
- **Pattern fit:** cbox already has a color-gate idiom (`OutputCtx::color()`, `cli/output.rs:14-27`). The TUI theme is the same idea expressed as ratatui `Style`s instead of raw `\x1b[..m`. Porting (not inventing) keeps it idiomatic and review-cheap.
- **Testability:** the redesign is only safe if it is assertable. The view frame is not unit-testable, so the design pushes every decision into PURE helpers — `Theme::resolve(mode)`, `badge_for(status)`, `header_should_collapse(width)` — that return plain values and can be unit-tested without a terminal. This is the load-bearing architectural choice.

### Threading model (decided)
- A `ColorMode` enum (`TrueColor | Ansi16 | NoColor`) is computed ONCE at launch in `app.rs` (next to `Model::new`, `app.rs:328`) using the ported gate, and stored on `Model` as a new field `color_mode: ColorMode`.
- `view(&Model, &mut Frame)` builds a `Theme` from `model.color_mode` at the top of `view()` (`view.rs:19`) — `let theme = Theme::resolve(model.color_mode);` — and passes `&theme` by reference into each `render_*` fn. **By-ref, not global.** No `static`, no `thread_local`, no `OnceCell`. Rationale: `view` already takes `&Model`; adding one `&Theme` param per render fn is the smallest, most explicit, test-friendliest change and keeps the render layer a pure function of (Model, Theme).
- `Theme` is cheap to build (a struct of `Style`/`Color` values); rebuilding per frame is negligible and avoids lifetime/state plumbing. No memoization needed.

### Why not the alternatives (see §8 Rejected Alternatives for full scoring)
- **H1 (global/`OnceCell` theme):** less plumbing but hides the dependency, harder to test in isolation, and a singleton in a render path is a smell. Rejected.
- **H3 (full inline-style refactor only, no copy module):** leaves voice scattered across `update.rs` + `view.rs`, fails the "show don't tell" centralization goal, and makes copy untestable. Rejected as incomplete.
- **H4 (`Resize`-driven responsive layout):** out of scope, higher risk, touches the reducer. Deferred.

---

## 3. `theme.rs` design (detailed)

### 3.1 Types

```
// src/tui/theme.rs  (available regardless of `tui` feature so tests can import the
// pure parts; the ratatui-typed parts are #[cfg(feature = "tui")]-gated like view.rs)

pub enum ColorMode { TrueColor, Ansi16, NoColor }   // Copy + Eq + Debug

pub struct Theme {            // #[cfg(feature="tui")] — holds ratatui Styles
    pub mode: ColorMode,
    // border + chrome
    pub border:        Style,   pub border_focus:  Style,
    pub title:         Style,
    // semantic accents (per-screen accent is chosen from these, see §3.4)
    pub accent:        Style,   pub accent_dim:    Style,
    pub success:       Style,   pub warning:       Style,   pub danger: Style,
    pub muted:         Style,
    // table
    pub header_cell:   Style,   pub selection:     Style,   // bg+fg+BOLD
    // brand
    pub brand_logo:    Style,   pub brand_name:    Style,   pub brand_tagline: Style,
    // badges (built by badge component, but the base styles live here)
    pub badge_running: Style,   pub badge_stopped: Style,   pub badge_error:   Style,
    pub badge_unknown: Style,
}
```

### 3.2 Construction + color-mode resolution

- `ColorMode::detect(no_color_flag: bool) -> ColorMode` — the ported gate. **Decision rule (mirrors `cli/output.rs:14-23`, extended for 16-color):**
  1. If `no_color_flag == true` **OR** `std::env::var("NO_COLOR").is_ok()` **OR** `!std::io::stdout().is_terminal()` → `NoColor`.
  2. Else if low-color terminal — detected via `TERM` containing none of `256`/`truecolor` AND `COLORTERM` unset/empty (i.e. `COLORTERM` not in {`truecolor`,`24bit`} and `TERM` not matching `*-256color`/`*-direct`) → `Ansi16`.
  3. Else → `TrueColor`.
  - Keep the detection deterministic and env-only (no terminal capability queries) — matches the CLI's simple, testable posture.
  - **Where it's called:** ONCE in `app::run` right after backends are resolved (`app.rs:320-329`), before `Model::new`. The TUI has no `--no-color` flag wired today; the launch path should read the same global `no_color` the CLI uses if available, else pass `false` and rely on `NO_COLOR` + TTY. (Task T6 wires the flag; if the global flag is not plumbed into the TUI entry, default `no_color_flag=false` — `NO_COLOR` env still works.)
- `Theme::resolve(mode: ColorMode) -> Theme` — pure mapping from mode to the token table in §3.3. No I/O. Unit-testable.

### 3.3 Token color table (kraft / retro-leaning) per tier

Retro "kraft cardboard" palette: warm tan/amber primary, terminal-green success, rust/orange warning, brick-red danger. Values chosen to read on both dark and light terminals (avoid pure white/black backgrounds).

| Token | TrueColor (RGB) | Ansi16 (named) | NoColor |
|---|---|---|---|
| `accent` (kraft amber) | `Rgb(214,158,92)` | `Yellow` | default fg + `BOLD` |
| `accent_dim` | `Rgb(150,110,66)` | `DarkGray` | default fg |
| `success` (box up) | `Rgb(126,184,108)` | `Green` | default fg + `BOLD` |
| `warning` (rust) | `Rgb(214,138,70)` | `Yellow` | default fg + `BOLD` |
| `danger` (brick) | `Rgb(200,86,74)` | `Red` | default fg + `BOLD` |
| `muted` | `Rgb(128,128,128)` | `DarkGray` | default fg + `DIM` |
| `border` | `Rgb(150,110,66)` (kraft edge) | `DarkGray` | default fg |
| `border_focus` | `Rgb(214,158,92)` | `Yellow` | default fg + `BOLD` |
| `title` | `Rgb(214,158,92)` + `BOLD` | `Yellow` + `BOLD` | default fg + `BOLD` |
| `selection` (bg/fg) | `bg Rgb(60,46,30)` / `fg Rgb(235,222,200)` + `BOLD` | `bg Blue` / `fg White` + `BOLD` | `REVERSED` + `BOLD` (no color) |
| `header_cell` | `accent` + `BOLD` | `Yellow` + `BOLD` | default fg + `BOLD` |
| `brand_logo` | `accent` | `Yellow` | default fg + `BOLD` |
| `brand_name` | `accent` + `BOLD` | `Yellow` + `BOLD` | default fg + `BOLD` |
| `brand_tagline` | `muted` | `DarkGray` | default fg + `DIM` |
| `badge_running` | `success` | `Green` | default fg + `BOLD` (glyph carries meaning) |
| `badge_stopped` | `muted` | `DarkGray` | default fg + `DIM` |
| `badge_error` | `danger` | `Red` | default fg + `BOLD` |
| `badge_unknown` | `accent_dim` | `DarkGray` | default fg |

**NoColor invariant (P0):** in `NoColor` mode, NO `Style` carries any `Color::Rgb`/named color — only `Modifier` (`BOLD`/`DIM`/`REVERSED`) and default fg/bg. This is the testable contract in AC-THEME-3.

### 3.4 Per-screen accent assignment
Today each screen hardcodes a different color (Cyan/Green/Red/Magenta/Yellow). Keep the *idea* of per-screen identity but route it through tokens. Mapping (Vivi picks the field; values come from the table):
- List / Detail → `accent` (the brand amber, replacing inline Cyan `view.rs:51,136`).
- Wizard → `success`-adjacent is wrong; use `accent` for border + `success` for the active-step bracket (replacing inline Green `view.rs:264`).
- ConfirmDestroy → `danger` (replacing inline Red `view.rs:361`).
- Progress → `accent` border + per-row status via badge styles (replacing inline Magenta `view.rs:379`).
- DoctorPanel → `warning` border (replacing inline Yellow `view.rs:457`).

---

## 4. Voice / strings decision

**DECISION: Centralize user-facing copy into `src/tui/strings.rs` as `const &str` / small `fn`s.** Rationale:
- The "show don't tell" rule (maintainer memory: `voice-show-dont-tell.md`) is a VOICE constraint; voice should live in ONE reviewable place, not be smeared across `update.rs` (status copy at `update.rs:658,670,688,707,730,753,771`) and `view.rs` (empty/loading/help). Centralizing makes the voice auditable in one diff and unit-testable (AC-COPY-1).
- Keep it minimal: a flat module of `pub const` strings + a few `pub fn` formatters for the parameterized lines (e.g. `loaded(n)`, `created(name)`). NOT an i18n framework.

**Surface (`strings.rs`):**
```
// Brand
pub const WORDMARK: &str = "cardboard-box";
pub const TAGLINE:  &str = "Your Linux environments, unboxed.";   // existing brand anchor (README/assets)
pub const LOGO_GLYPH: &str = "▣";        // compact box glyph for the header strip (single cell)

// Empty / loading / error
pub const EMPTY_LIST: &str = "Nothing boxed up yet.  Press  c  to pack your first one.";
pub const EMPTY_DETAIL: &str = "No box selected.";
pub const LOADING_LIST: &str = "Unpacking your boxes…";
pub const LOADING_DETAIL: &str = "Opening the box…";
pub const LOADING_DOCTOR: &str = "Running a check-up…";
pub const PROGRESS_RUNNING: &str = "Working…";
pub const PROGRESS_DONE: &str = "All set.  Enter or Esc to head back.";
pub const ERROR_PREFIX: &str = "✗ ";     // glyph carries the error semantics in no-color

// Help line (status bar)
pub const HELP: &str = "↑↓ move · enter open · c create · s stop · d destroy · a apply · e edit · ? doctor · q quit";

// Parameterized
pub fn loaded(n: usize) -> String { ... "n box(es) packed and ready" }
pub fn created(name) / removed(list) / stopped(list) / applied(name, ran, skipped, failed) ...
```

**Rewritten copy (concrete, "show don't tell" — character via wording + glyph, never adjectives):**

| Location | Old | New |
|---|---|---|
| Empty list (`view.rs:54`) | `No boxes yet. Press 'c' to create your first one.` | `Nothing boxed up yet.  Press  c  to pack your first one.` |
| List loaded (`update.rs:658`) | `{n} box(es) loaded` | `{n} box(es) packed and ready` |
| Backend unreachable (`update.rs:670`) | `Can't reach backend — running doctor…` | `Backend's gone quiet — running a check-up…` |
| Detail loading (`view.rs:140`) | `{spinner} Loading…` | `{spinner} Opening the box…` |
| Doctor loading (`view.rs:464`) | `{spinner} Running doctor…` | `{spinner} Running a check-up…` |
| Progress running (`view.rs:399`) | `{spinner} Running…` | `{spinner} Working…` |
| Progress done (`view.rs:409`) | `Done. Press Enter or Esc to return.` | `All set.  Enter or Esc to head back.` |
| Created (`update.rs:707`) | `Created "{name}"` | `Packed "{name}".` |
| Removed (`update.rs:730`) | `Removed: {list}` | `Cleared out: {list}` |
| Stopped (`update.rs:753`) | `Stopped: {list}` | `Sealed up: {list}` |
| Detail empty (`view.rs:149`) | `No detail loaded.` | `No box selected.` |

**Constraint check:** none of the new strings advertise a quality (no cozy/friendly/beautiful/delightful). They demonstrate the cardboard-box metaphor through verbs (pack / unpack / seal / clear out / box up). Compliant with `voice-show-dont-tell.md`.

> NOTE for coder: status copy currently lives inline in `update.rs`. Moving it means replacing the `format!(...)` literals at the cited lines with calls into `strings::*`. This is the ONLY reducer-file edit and it changes no logic — only the string source. Existing tests assert StatusLine *variants* (`matches!(model.status, StatusLine::Ok(_))`, `tui_update.rs:116,585`), NOT exact text, so they stay green.

---

## 5. Per-screen restyle spec

All styling references TOKENS (§3.3), never raw colors. Heavy box-drawing: use `BorderType::Thick` (or `Double` where a screen wants extra weight — see per-screen note) on `Block` borders, replacing the default light `Borders::ALL`.

### 5.0 Brand header (NEW component — `render_brand_header`)
- **Placement:** `view()` (`view.rs:19-43`) gains a 3-region vertical split when `model.screen == Screen::List`: `[header(Length 1), body(Min 3), status(Length 1)]`. For all OTHER screens, keep the existing 2-region split (`view.rs:23-26`) — the header is List-only so it never crowds modal/wizard/progress screens. (Spec note: header is "persistent" relative to the home/list experience, which is the launch surface; it is intentionally absent on transient sub-screens.)
- **Wide layout (width ≥ `HEADER_COLLAPSE_WIDTH = 60`):** single line:
  `▣ cardboard-box · Your Linux environments, unboxed.`
  - `▣` = `theme.brand_logo`; `cardboard-box` = `theme.brand_name`; `·` separator = `theme.muted`; tagline = `theme.brand_tagline`.
- **Narrow layout (width < 60):** collapse to logo + wordmark only, drop the tagline:
  `▣ cardboard-box`
- **Collapse helper (PURE, testable):** `header_should_collapse(width: u16) -> bool { width < HEADER_COLLAPSE_WIDTH }`. This is the unit under AC-HEADER-1. The decision is computed from `area.width` of the per-frame `Rect`, NOT from resize events.
- **No splash.** Header is one row, drawn straight above the list.

### 5.1 List / home (`view.rs:47-128`)
- Border: `BorderType::Thick`, `theme.border` (replace inline `Color::Cyan` `view.rs:51`).
- Title: ` your boxes ` styled `theme.title` (the wordmark now lives in the header, so drop the redundant `cbox —` prefix from `view.rs:49`).
- Header row cells: `theme.header_cell` (replace `Modifier::BOLD` + `Color::Yellow` `view.rs:63-70`).
- Selection: `theme.selection` (replace inline `bg Blue/fg White/BOLD` `view.rs:80-84`).
- STATUS column: render via the **badge component** (§5.7), not the inline green/gray ternary at `view.rs:87-93`.
- Empty state: `strings::EMPTY_LIST`, centered, `theme.muted` for the sentence with `c` highlighted in `theme.accent` (two-span line).
- Loading state (NEW — `model.busy && boxes.is_empty()`): show `{spinner} strings::LOADING_LIST` centered in `theme.accent` instead of an empty bordered box.

### 5.2 Detail (`view.rs:132-229`)
- Border: `BorderType::Thick`, `theme.border`; title ` box detail ` in `theme.title` (replace `view.rs:133-136`).
- Field labels (`Name:`, `Status:`…): `theme.header_cell` (replace inline `BOLD` `view.rs:160-207`).
- Status value: badge component (§5.7) (replace inline green/gray `view.rs:165-172`).
- Mounts arrow `→` (`view.rs:219`): keep glyph, color the arrow `theme.muted`.
- Loading: `{spinner} strings::LOADING_DETAIL`, `theme.accent` (replace `view.rs:140`).
- Empty: `strings::EMPTY_DETAIL`, `theme.muted` (replace `view.rs:149`).

### 5.3 Wizard (`view.rs:233-323`)
- Border: `BorderType::Thick`, `theme.accent` border; title ` pack a box — {steps} ` in `theme.title` (replace inline Green `view.rs:261-264`).
- Step indicator (`view.rs:248-259`): active step bracket `[Name]` styled `theme.success`; inactive steps `theme.muted`; `›` separator `theme.muted` (keep glyph `view.rs:259`).
- Field label `theme.header_cell`; value default fg (replace `Color::White` `view.rs:316-317`).
- Hint line (`view.rs:320-322`): `theme.muted` (replace inline `DarkGray`).
- DockerMode active option bracket `[host]` → `theme.accent` (replace plain `view.rs:289-294`).

### 5.4 ConfirmDestroy (`view.rs:327-366`)
- Modal border: `BorderType::Double`, `theme.danger` (replace inline Red `view.rs:361`); title ` confirm destroy ` in `theme.danger` + `BOLD`.
- `[y]es`/`[n]o`: `y` in `theme.danger`, `n` in `theme.success` (two-span emphasis).
- `[x] h: also remove $HOME` indicator (`view.rs:347`): checked box in `theme.warning`.
- Keep the centered-modal geometry (`view.rs:333-343`) unchanged.

### 5.5 Progress (`view.rs:370-449`)
- Border: `BorderType::Thick`, `theme.accent` (replace inline Magenta `view.rs:379`); title ` {progress.title} ` in `theme.title`.
- Step STATUS column (`view.rs:425-430`): map via a small status→style table that reuses badge styles — `ran|copied → theme.success`, `skipped → theme.muted`, `failed → theme.danger`, else default. (This is the existing logic at `view.rs:425-430` re-expressed against tokens; can share the badge mapper's style lookup.)
- Header cells: `theme.header_cell` (replace inline `BOLD` `view.rs:415-418`).
- Running: `{spinner} strings::PROGRESS_RUNNING`, `theme.accent` (replace `view.rs:399`).
- Done: `strings::PROGRESS_DONE`, `theme.muted` (replace `view.rs:409`).
- Recreate confirm (`view.rs:382-393`): `[y]es`/`[n]o` two-span like §5.4; message in default fg.

### 5.6 DoctorPanel (`view.rs:453-532`)
- Border: `BorderType::Thick`, `theme.warning` (replace inline Yellow `view.rs:457`); title ` doctor ` in `theme.title`.
- `✓`/`✗` marks (`view.rs:481`): keep glyphs; color `✓` `theme.success`, `✗` `theme.danger` (currently uncolored). Build a tiny `ok_glyph(b: bool) -> Span` helper that pairs glyph+style — this is the badge idea for booleans.
- Warnings header (`view.rs:515-516`): `theme.warning`; each `! {w}` line: `!` in `theme.warning`.
- Loading: `{spinner} strings::LOADING_DOCTOR`, `theme.warning` (replace `view.rs:464`).
- "Press Esc or q to return." (`view.rs:525-528`): `theme.muted`.

### 5.7 State badge component (NEW — reusable, PURE-mapped)
A function that turns a raw status string into a styled badge. The mapping logic must be a PURE classifier so it is unit-testable independent of ratatui:

```
pub enum BadgeKind { Running, Stopped, Error, Unknown }

// PURE — unit-testable (AC-BADGE-1). Input is the raw distrobox/podman status string.
pub fn classify_status(raw: &str) -> BadgeKind {
    let s = raw.to_lowercase();
    if s.contains("running") || s.contains("up") { BadgeKind::Running }
    else if s.contains("exit") || s.contains("stopped") || s.contains("created") { BadgeKind::Stopped }
    else if s.contains("error") || s.contains("dead") { BadgeKind::Error }
    else { BadgeKind::Unknown }
}

// glyph + label per kind (playful-but-clear; glyph carries meaning in no-color):
//   Running → "● up"        style theme.badge_running
//   Stopped → "○ sealed"    style theme.badge_stopped
//   Error   → "✗ trouble"   style theme.badge_error
//   Unknown → "· unknown"   style theme.badge_unknown
// badge_span(raw, &theme) -> Span<'static>   (#[cfg(feature="tui")])
```

- Preserves the EXISTING running/up → green, else dim semantics (`view.rs:87-93`, `view.rs:167`) but adds explicit stopped/error/unknown tiers and a glyph that survives no-color.
- The label words ("up" / "sealed" / "trouble") demonstrate character without advertising it.
- Glyphs `●`/`○`/`✗`/`·` are single-cell and degrade cleanly.

### 5.8 Status bar (`view.rs:536-558`)
- `Idle` → `strings::HELP` in `theme.muted` (replace inline `DarkGray` `view.rs:541`).
- `Busy(msg)` → `{spinner} {msg}` in `theme.accent` (replace inline `Yellow` `view.rs:543-547`).
- `Ok(msg)` → `{msg} · {help}` in `theme.success` (replace inline Green `view.rs:549-552`; swap the `|` separator for `·` to match the retro voice).
- `Error(msg)` → `{strings::ERROR_PREFIX}{msg}` in `theme.danger` (replace inline Red `view.rs:553`; the `✗ ` prefix carries error semantics in no-color).

---

## 6. Acceptance Criteria (GIVEN / WHEN / THEN — testable)

> The view frame is NOT golden-rendered. Every AC below targets a PURE helper or an existing assertable variant/state. New tests go in `tests/tui_theme.rs` (new file), gated `#![cfg(feature = "tui")]` like `tui_update.rs:4`.

**AC-THEME-1 (mode resolution):** GIVEN `ColorMode::detect` with `no_color_flag=true` WHEN called THEN returns `ColorMode::NoColor`. AND GIVEN `NO_COLOR` env set THEN `NoColor`. AND GIVEN a non-tty stdout THEN `NoColor`. (Tested via injecting the three inputs; env + tty paths may need a thin testable seam, e.g. `detect_from(no_color_flag, no_color_env, is_tty, term, colorterm)` pure core + a thin `detect()` wrapper. SPEC RECOMMENDS the pure-core split so this AC needs no env mutation.)

**AC-THEME-2 (tier mapping):** GIVEN `Theme::resolve(ColorMode::TrueColor)` THEN `theme.accent` carries `Color::Rgb(214,158,92)`. AND GIVEN `Ansi16` THEN `theme.accent` carries `Color::Yellow`. (Assert `theme.accent.fg == Some(Color::Rgb(...))` etc.)

**AC-THEME-3 (no-color invariant, P0):** GIVEN `Theme::resolve(ColorMode::NoColor)` WHEN inspecting EVERY style field THEN none has a `.fg`/`.bg` that is a color (all are `None` or default), and differentiation is only via `Modifier`. (Iterate the documented style fields; assert `fg.is_none() && bg.is_none()` for each — this is the central no-ANSI-color guarantee.)

**AC-BADGE-1 (classifier):** GIVEN `classify_status("running")` → `Running`; `"Up 3 minutes"` → `Running`; `"exited (0)"` → `Stopped`; `"created"` → `Stopped`; `"dead"` → `Error`; `"weird"` → `Unknown`. (Pure, exact.)

**AC-BADGE-2 (glyph/label/style):** GIVEN `BadgeKind::Running` THEN its glyph is `●`, label `up`, style == `theme.badge_running`. (One assertion per kind; if `badge_span` returns `Span`, assert `.content` and `.style`.)

**AC-HEADER-1 (collapse threshold):** GIVEN `header_should_collapse(59)` → `true`; `header_should_collapse(60)` → `false`; `header_should_collapse(120)` → `false`. (Boundary at `HEADER_COLLAPSE_WIDTH=60`.)

**AC-COPY-1 (voice compliance + non-empty):** GIVEN every public copy const in `strings.rs` THEN it is non-empty AND contains none of the banned substrings (case-insensitive) `cozy`, `beautiful`, `friendly`, `delightful`, `cute`, `lovely`. (A single table-driven test enforces the maintainer voice rule mechanically.)

**AC-COPY-2 (formatter shapes):** GIVEN `strings::loaded(2)` THEN result contains `"2"` and is non-empty; `created("web")` contains `"web"`. (Guards the parameterized lines.)

**AC-REGRESSION (existing suites green):** GIVEN the existing `tests/tui_update.rs` + `tests/tui_effects.rs` WHEN run THEN ALL pass unchanged — they assert StatusLine VARIANTS and Model state, not rendered strings/colors (`tui_update.rs:116,196,585`), so the copy/theme changes must not alter variants, effects, or state transitions. (This is the safety net for the `update.rs` copy move.)

---

## 7. Decomposition + Sequencing (ordered by dependency)

| Task | Agent | Title | Timebox | Depends | Verifier |
|---|---|---|---|---|---|
| **T1** | Vivi | `theme.rs` — `ColorMode`, `detect_from`/`detect`, `Theme`, `Theme::resolve`, token table (§3.1-3.4) | ≤2d | — | `make build` + new AC-THEME tests via `make test` |
| **T2** | Vivi | Badge component — `BadgeKind`, `classify_status`, `badge_span`, `ok_glyph` (§5.7, §5.6) | ≤1d | T1 | `make test` (AC-BADGE-1/2) |
| **T3** | Kupo | `strings.rs` module — consts + formatters (§4); add `mod strings;` to `tui/mod.rs` | 1d | — | `make build` + AC-COPY tests |
| **T4** | Vivi | Thread theme + color_mode: add `color_mode` field to `Model`, compute in `app.rs` (`app.rs:320-329`), build `Theme` in `view()` and pass `&theme` to all render fns (§2) | ≤1d | T1 | `make build` (compiles) + AC-REGRESSION |
| **T5** | Vivi | Brand header component + List 3-region split + collapse helper (§5.0); restyle List + Detail (§5.1-5.2) | ≤2d | T1,T2,T3,T4 | `make build` + AC-HEADER-1; manual smoke |
| **T6** | Vivi | Restyle Wizard, ConfirmDestroy, Progress, DoctorPanel, status bar (§5.3-5.6, §5.8); wire `--no-color`/global flag into TUI entry if plumbed (§3.2) | ≤2d | T1,T2,T3,T4 | `make build`; manual smoke; no-color/16-color smoke (§9) |
| **T7** | Kupo | Move status-string copy from `update.rs` (lines 658,670,688,707,730,753,771) into `strings::*` calls (§4) | 1d | T3 | `make test` — AC-REGRESSION MUST stay green |
| **T8** | Vivi | New `tests/tui_theme.rs` — implement AC-THEME-1/2/3, AC-BADGE-1/2, AC-HEADER-1, AC-COPY-1/2 | ≤1d | T1,T2,T3 | `make test` (all new + existing green) |

**Notes:**
- T1, T3 are independent and can start in parallel (T3 is a Kupo micro-task — flat const module, no logic).
- T7 is a Kupo micro-task: pure mechanical string-source swap, no logic change; its risk is entirely covered by AC-REGRESSION.
- T5/T6 are the only frame-rendering tasks — verify by `make build` + manual smoke (the frame is not golden-tested by design).
- Final gate after all tasks: `make check` (fmt-check + lint + lint-lean + build + release + test).

---

## 8. Rejected Alternatives

- **H1 — Global `OnceCell<Theme>`:** less per-fn plumbing, but hides the dependency, complicates isolated unit tests of render helpers, and a render-path singleton is a smell. Score 0.71. Rejected vs H2's by-ref explicitness.
- **H3 — Inline-style refactor only (no `strings.rs`):** satisfies the theme goal but leaves voice scattered in `update.rs`+`view.rs`, fails the "show don't tell" single-source goal, makes copy untestable (no AC-COPY). Score 0.64. Rejected as incomplete vs the locked scope item (d)/(voice).
- **H4 — `Resize`-driven responsive layout:** would make the header truly responsive via reducer state, but `Resize` is a no-op today (`update.rs:38`), it's explicitly OUT of scope, touches the reducer, and raises blast radius + regression risk on the existing 38-test suite. Score 0.58. Deferred to future work; the per-frame-width collapse in §5.0 covers the locked "collapses on narrow width" requirement without it.
- **Theme stored as raw `Color`s, styling assembled at call sites:** more flexible but re-scatters Modifier decisions into `view.rs`, defeating the "named styles replace ALL inline styling" goal. Rejected; `Theme` holds fully-assembled `Style`s.

---

## 9. Risk Register + Validation Gates

| ID | Risk | Likelihood | Impact | Mitigation / Gate |
|---|---|---|---|---|
| R1 | Copy move breaks a test that secretly asserts exact text | Low | Med | AC-REGRESSION confirms existing tests assert VARIANTS not text (`tui_update.rs:116,196,585`). Run `make test` after T7 specifically. |
| R2 | No-color mode still leaks a color (P0 voice/accessibility) | Med | High | AC-THEME-3 mechanically asserts every style field has `fg.is_none() && bg.is_none()` in `NoColor`. Plus manual `NO_COLOR=1` smoke (§ below). |
| R3 | 16-color terminals get unreadable RGB downgrades | Low | Med | Explicit `Ansi16` named-color column (§3.3); manual smoke with `TERM=xterm COLORTERM=` . |
| R4 | Header crowds the list on small terminals | Low | Low | Header is ONE row + List-only; collapse helper (AC-HEADER-1) drops tagline < 60 cols. |
| R5 | New `color_mode` field on `Model` breaks `Model::new` callers / tests | Med | Med | `Model::new(backend)` keeps its signature; `color_mode` defaults to `TrueColor` (or detect) inside `new`, set explicitly in `app.rs`. Existing `Model::new(Backend::Podman)` in tests (`tui_update.rs:16`) compiles unchanged. **Spec requires: add the field with a default in `Model::new`, do NOT change the constructor signature.** |
| R6 | Heavy box-drawing chars render as tofu on minimal fonts | Low | Low | Use standard Unicode box-drawing (`BorderType::Thick`/`Double` are ratatui built-ins) + single-cell badge glyphs already proven in-repo (`✓`/`✗`/`→`/`›` `view.rs:219,259,481`). |
| R7 | Containerized-build assumption violated (host cargo run) | Low | Med | Handoff header + every task verifier names a `make` target, never `cargo`. |

**Blast radius:** Primary = `src/tui/view.rs` (pure, untested — safe). New files `src/tui/theme.rs`, `src/tui/strings.rs`, `tests/tui_theme.rs`. Surgical edits: `src/tui/model.rs` (+1 field, defaulted), `src/tui/app.rs` (compute + set `color_mode`), `src/tui/update.rs` (copy-source swap only), `src/tui/mod.rs` (+2 `mod` decls). NO change to `effect.rs`, `message.rs`, reducer logic, keybindings, or the worker thread.

**Containerized verification (in order):**
1. `make fmt-check` — formatting.
2. `make lint` + `make lint-lean` — clippy.
3. `make build` — compiles with `tui` (default feature).
4. `make test` — all suites incl. new `tui_theme.rs` + unchanged `tui_update.rs`/`tui_effects.rs`.
5. `make check` — full chain (the release gate).

**No-color / 16-color verification approach:**
- Automated: AC-THEME-3 (no color anywhere in `NoColor`) + AC-THEME-2 (Ansi16 maps to named colors). These run in CI via `make test` with NO terminal needed.
- Manual smoke (operator, not CI): launch the TUI under each mode:
  - `NO_COLOR=1 cbox` → expect bold/dim/glyph-only chrome, zero color.
  - `TERM=xterm COLORTERM= cbox` → expect the 16-color named palette.
  - default modern terminal → expect the kraft RGB palette.
  - Verify the header tagline disappears below ~60 columns.

---

## 10. Confidence Report

| Factor (25% each) | Score | Note |
|---|---|---|
| Pattern match | 0.85 | ADAPT from cbox's own `OutputCtx` color-gate (`cli/output.rs:14-27`); 2 prior cbox specs in memory establish the Vivi/Kupo + `make`-gate conventions. |
| Requirement clarity | 0.95 | Direction LOCKED by human; all 6 deliverables explicitly enumerated. |
| Decomposition stability | 0.85 | 3 alt decompositions (by-layer / by-screen / by-token) converge on theme-first → components → screens → fallback → tests. |
| Constraint compliance | 0.85 | Read-only spec; containerized-build + voice rules honored; no-color is a P0 testable invariant. |
| **Weighted** | **0.87** | **AUTO_PROCEED** |

**Open flags for the human (none block proceed):**
1. `--no-color` is NOT wired into the TUI launch path today; `NO_COLOR` env + TTY gate cover the requirement. T6 wires the explicit flag IF a global `no_color` is plumbed to the TUI entry; otherwise it stays env-driven. Confirm whether you want the explicit `--no-color` flag threaded into `cbox` (the TUI subcommand) this pass.
2. Exact kraft RGB values (§3.3) are a defensible starting palette; tweak-friendly since they live in ONE table.
