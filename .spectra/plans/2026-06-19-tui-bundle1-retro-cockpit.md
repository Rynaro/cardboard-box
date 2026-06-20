# cbox TUI Bundle 1 "Retro Cockpit" — SPECTRA Spec

- **Spec ID:** `2026-06-19-tui-bundle1-retro-cockpit`
- **Intent:** REQUEST / CHANGE (feature set + product flags LOCKED by human; this spec makes them precise + buildable)
- **Coder:** Vivi (feature work) / Kupo (one flagged micro-task)
- **Complexity:** 9/12 (extended depth, standard tier, single-pass cycle)
- **Confidence:** 88% → AUTO_PROCEED
- **Release target:** next release-please increment — `feat` → minor bump (0.8.0 → 0.9.0). One umbrella `feat(tui): retro cockpit ...` or per-feature `feat(tui): ...` commits; either lands a minor bump.
- **Build constraint (pass to coder):** Builds are CONTAINERIZED. NEVER run host `cargo`. Use `make` targets only (`make build`, `make release`, `make test`, `make fmt`, `make lint`, `make lint-lean`, `make check`). The `tui` feature is default-on (`Cargo.toml:16-18`), so plain `make build`/`make test` exercises this code. `make lint-lean` builds WITHOUT the `tui` feature — every new pure helper that lands in an always-compiled module must keep that build clean.
- **Voice rule (LOCKED, do not relitigate):** "show don't tell." Never advertise qualities (no cozy/beautiful/friendly/delightful/cute/lovely) in user-facing copy. Demonstrate character via layout, glyphs, cardboard-metaphor verbs (pack/unpack/seal/clear out). `AC-COPY-1` (`tests/tui_theme.rs:246-283`) mechanically asserts the banned-substring list against every public `strings` const — every new const this bundle adds MUST be added to that test's list and stay compliant.
- **Locked flags (do NOT relitigate):** `?` → cheatsheet, Doctor → `D`; theme switcher is echo-of-styling only; command-log is **echo-only, NO undo**; fuzzy substrate is the crate later bundles reuse.

---

## 1. Scope

### In scope (these five features, nothing more)

1. **Fuzzy filter overlay** on the box List (`/` opens; type to narrow live; Esc/Enter close).
2. **Keybinding cheatsheet overlay** (`?` opens; any key/Esc dismisses; content generated from the keymap).
3. **Runtime theme/skin switcher** (a key cycles named skins; re-resolves immediately; NO_COLOR invariant preserved per skin).
4. **Toast / transient notifications** (stacked, TTL-expiring queue layered over the existing StatusLine).
5. **Command-log echo** (bounded ring buffer of the REAL `distrobox`/`podman` argv strings cbox runs; viewable in an overlay; echo-only, no undo).

### Cross-cutting enabler (prerequisite, not a 6th feature)

0. **Keymap-as-data** — a single static `KEYMAP` table that the status bar, the cheatsheet, and (documentation of) the reducer all read. This is the single source of truth that makes #2 testable and keeps the help line and cheatsheet from drifting. Lands first (see §7).

### Out of scope (note as future work; do NOT spec or build)

- Filtering on any screen other than List.
- Fuzzy match on fields other than box **name** (later bundles may widen to image/status; Bundle 1 matches name only).
- Persisting the chosen skin across sessions (no config write; skin resets to default `kraft` each launch).
- Command-log persistence to disk, copy-to-clipboard, or re-run.
- Undo / rollback of any kind (explicitly rejected).
- Per-box / per-screen *dynamic* keymaps beyond the static screen→keys table defined here.
- Reflow / responsive layout beyond the existing header-collapse rule.

### Deferred / explicitly NOT changed

- `app.rs` worker thread model, channel bounds (`sync_channel(4)`/`(32)`, `app.rs:121,236`) — UNCHANGED. Bundle 1 adds no long-running ops.
- `effect.rs` Effect set — UNCHANGED except the command-log capture path (§6), which is a **runner-decorator** seam, NOT a new Effect.
- The reducer's existing screen-dispatch shape and all existing StatusLine semantics that `tests/tui_update.rs` asserts.

---

## 2. Approach + Rationale

**Selected strategy: H2 — "Boolean-overlay flags on Model + keymap-as-data + runner-decorator capture" (ADAPT from the existing ConfirmDestroy modal pattern and the `Theme::resolve` pure-resolution seam).**

Four angles drove this:

- **Lowest blast radius via the proven overlay pattern.** The codebase already renders a modal as `render_list(...) + Clear + centered Rect` while keeping `Screen::ConfirmDestroy` (`view.rs:51-54,409-468`). Filter, cheatsheet, and command-log are all *transient overlays on top of the List screen*, not new top-level screens. Modeling them as **boolean/Option overlay flags on `Model`** (not new `Screen` variants) keeps the existing screen-dispatch (`update.rs:72-79`, `view.rs:47-57`) intact and avoids touching the six-variant `Screen` enum and its exhaustive matches.
- **Keymap-as-data makes the cheatsheet testable.** The view frame is not unit-testable, but a `keymap_for(screen, ctx) -> &[KeyBinding]` pure function is. Generating the cheatsheet AND the status-bar help line from one table kills drift and gives us `AC-MAP-*` assertions without a terminal.
- **Theme already centralizes styling behind one pure `resolve`.** `Theme::resolve(ColorMode)` is the single resolution point (`theme.rs:129-135`) and `view()` rebuilds the theme per frame (`view.rs:24`). Adding a `Skin` axis is a second dimension on that pure function — `Theme::resolve(skin, mode)` — and a `skin` field on Model. The NoColor invariant test (`tests/tui_theme.rs:88-124`) generalizes to "for every skin."
- **Command-log capture belongs at the runner boundary, decorated — not at each effect.** Every spawn already funnels through `DistroboxRunner::run` / `run_interactive` (`runner.rs:89-96`), and `CmdOutput` already carries `argv` + `status` (`runner.rs:49-56`). Wrapping the injected runner in a `LoggingRunner` decorator captures the real argv + outcome at the single chokepoint, with ZERO change to `effect.rs`, `real.rs`, or `core::*`. The reducer learns about new log lines by draining the shared buffer on `Tick` (the existing clock).

### Threading model (decided)

- **Overlays are Model state, not screens.** New `Model` fields: `filter: Option<FilterState>`, `overlay: Overlay` (an enum: `None | Cheatsheet | CommandLog`), `skin: Skin`, `toasts: Vec<Toast>`, plus a shared command-log handle. The active `Screen` stays `List` while an overlay is up; the reducer checks overlay state FIRST in `handle_key`, before screen dispatch.
- **Tick is the clock.** Toast TTL expiry and command-log drain both run inside `Message::Tick` (`update.rs:30-36`), which already fires every ~50ms when idle and returns `vec![]`. No new timer, no new thread.
- **Filter never mutates `model.boxes`.** Selection still indexes into `model.boxes` via `move_up/move_down` (`model.rs:197-218`). The filter computes a *view* — an ordered `Vec<usize>` of matching indices into `model.boxes` — and navigation/selection operate over that view when a filter is active. This is the load-bearing index-mapping decision (§3.2).

### Why not the alternatives (full scoring in §9)

- **H1 (new `Screen` variants per overlay):** forces edits to every exhaustive `match model.screen` and the screen-dispatch in both `update.rs` and `view.rs`, and conflates transient overlays with navigable screens. Higher blast radius, worse fit with the existing ConfirmDestroy precedent. Rejected.
- **H3 (filter by mutating `model.boxes` + keeping a backup):** simplest selection story but corrupts the source of truth, breaks `ListLoaded` clamping (`update.rs:647-658`), and risks losing rows on refresh. Rejected.
- **H4 (command-log captured per-effect inside `execute_effect`):** would touch ~8 effect arms, miss the interactive `enter` path, and duplicate argv assembly. The decorator captures everything at one seam. Rejected.

---

## 3. Data-model design (all five features)

All new types live in `src/tui/model.rs` unless noted. Pure-data types (no ratatui) so `tests/` can import them without the `tui` feature where practical; ratatui-typed parts stay `#[cfg(feature = "tui")]`-gated as `theme.rs`/`view.rs` already do.

### 3.0 Keymap-as-data (enabler — `src/tui/keymap.rs`, NEW module)

Single source of truth for #2 and the status bar. Pure, always-compiled (no `tui` feature gate), so both the cheatsheet test and a future lean build see it.

```
// src/tui/keymap.rs   (NOT feature-gated; pure data + pure fns)

pub struct KeyBinding {
    pub key: &'static str,    // display form, e.g. "?", "enter", "↑↓"
    pub action: &'static str, // verb phrase, e.g. "cheatsheet", "open", "move"
}

/// Logical context the keymap is resolved for. Mirrors the reducer's
/// screen-dispatch, plus the box-state nuance the List screen has.
pub enum KeyContext {
    List,        // box list, no overlay
    Detail,
    Wizard,
    ConfirmDestroy,
    Progress,
    DoctorPanel,
    FilterInput, // filter overlay active (keys: type, esc/enter)
    Cheatsheet,  // cheatsheet overlay active (any key dismisses)
    CommandLog,  // command-log overlay active (scroll, esc)
}

/// The ONE table. Cheatsheet renders it; status bar derives its help line from
/// the List slice; AC-MAP-* assert against it.
pub fn keymap_for(ctx: KeyContext) -> &'static [KeyBinding] { ... }

/// The compact one-line help string for the status bar, derived from
/// keymap_for(List) (replaces the hand-written strings::HELP).
pub fn help_line(ctx: KeyContext) -> String { ... }
```

**Decision — keymap IS the single source of truth.** `strings::HELP` (`strings.rs:37-38`) is REPLACED by `keymap::help_line(KeyContext::List)`. The cheatsheet renders `keymap_for(ctx)`. `AC-COPY-1` must still pass — either keep `HELP` as a thin `const` delegating semantics OR remove it from the `AC-COPY-1` const list and add the keymap's action strings to a new voice-compliance assertion (`AC-MAP-VOICE`, §6). **Flagged decision D-1** (low stakes): recommend removing `strings::HELP` and asserting voice on the keymap action strings instead. If Vivi prefers minimal churn, keep `HELP` as `help_line(List)` materialized at first use — but then it cannot be a `const`. Spec recommends the keymap-derived `help_line`.

### 3.1 Fuzzy filter (`FilterState` on Model)

```
pub struct FilterState {
    pub query: String,          // raw user input
    pub matches: Vec<usize>,    // indices into model.boxes, best-rank first
    pub cursor: usize,          // selection index *within* matches (0-based)
}
```

- New `Model` field: `pub filter: Option<FilterState>` (None = filter closed). `Model::new` initializes `None`.
- **Crate choice — `nucleo` (recommend `nucleo-matcher`, the matcher-only sub-crate).** Justification:
  - It is the matcher Helix and other modern Rust TUIs standardized on; it is fast, Unicode-correct, and exposes a pure `Matcher` + `Pattern` API that returns an `Option<u32>` score — ideal for ranking a small `Vec<BoxRow>` without async or a worker.
  - `nucleo-matcher` (the lower crate) has a small dependency footprint and no async runtime, fitting cbox's "no extra runtime" posture (cf. the keyring async-vs-sync care in `Cargo.toml`).
  - It is the substrate later bundles reuse (multi-field, incremental, large lists) without a crate swap — `nucleo` (the high-level crate) layers an incremental/threaded API on the SAME matcher, so Bundle 1's `nucleo-matcher` choice forward-compatibly upgrades.
  - vs. `fuzzy-matcher` (SkimMatcherV2): smaller and zero-dep, but unmaintained-leaning and not the forward path; choosing it now forces a later swap. Rejected for the substrate role.
  - **Add to `Cargo.toml` `[dependencies]`:** `nucleo-matcher = "0.3"` (pin to the latest 0.3.x; verify exact version at build). It is used only under the `tui` feature path of the reducer, BUT the pure ranking helper (below) should compile in lean builds too if placed in an always-compiled module; gate the import accordingly. **Flagged decision D-2** (low stakes): if the maintainer wants zero new deps in the lean build, gate the `nucleo-matcher` dep behind the `tui` feature (`nucleo-matcher = { version = "0.3", optional = true }`, added to the `tui = [...]` feature list). Spec recommends gating it under `tui` since filtering only exists in the TUI.
- **Pure ranking helper** (testable, `AC-FILTER-1/2`):
  ```
  // returns indices into `names`, best-match first; empty query → all indices in order
  pub fn fuzzy_rank(query: &str, names: &[&str]) -> Vec<usize>
  ```
  Lives in `src/tui/filter.rs` (NEW) or in `keymap.rs`'s sibling `filter.rs`. It owns the `nucleo_matcher::Matcher` + `Pattern::parse` call so the reducer stays thin. Empty/whitespace query returns `(0..names.len()).collect()` (identity — filter open but matching everything).

### 3.2 Selection ↔ filter index-mapping (load-bearing)

The hand-rolled cursor (`move_up/move_down`, `model.rs:197-218`) indexes directly into `model.boxes`. Under an active filter we must NOT mutate `model.boxes`. Design:

- **When `model.filter` is `Some`:** the *effective* selection is `filter.matches[filter.cursor]` (an index into `model.boxes`). Navigation moves `filter.cursor` within `0..matches.len()`, then sets `model.selected = Some(filter.matches[filter.cursor])` so every existing consumer of `model.selected` / `model.selected_box()` (`model.rs:193-195`, all of `handle_key_list`) keeps working UNCHANGED.
- **When `model.filter` is `None`:** behavior is exactly today's (`move_up/move_down` over `model.boxes`).
- **New pure methods on `Model`** (extend, don't replace, the existing ones):
  ```
  pub fn filtered_indices(&self) -> Vec<usize>   // filter.matches if Some, else (0..boxes.len())
  pub fn move_up(&mut self)    // filter-aware: moves within filtered_indices, updates selected
  pub fn move_down(&mut self)  // filter-aware
  ```
  `move_up/move_down` gain a branch: if `filter.is_some()`, move `cursor` and resync `selected`; else current logic. The existing `tests/tui_update.rs` `ac_nav_1_*` (filter is None) stay green because the `None` branch is the old code verbatim.
- **Re-filter triggers** (recompute `matches`, clamp `cursor`): on each character typed/Backspace into the filter, AND on `ListLoaded` while a filter is open (so a refresh doesn't desync). After recompute, clamp `cursor` to `matches.len().saturating_sub(1)`; if `matches` empty, `selected = None`.
- **Closing the filter:** Esc closes and CLEARS the filter (selection falls back to the previously-selected box if still present, else `Some(0)` / `None`). Enter closes but KEEPS the current selection (commits the highlighted box, filter cleared, full list restored with that box selected). Both set `model.filter = None`.

### 3.3 Cheatsheet + Command-log overlays (`Overlay` enum on Model)

```
pub enum Overlay {
    None,
    Cheatsheet,
    CommandLog { scroll: usize },  // scroll offset into the ring buffer
}
```

- New `Model` field: `pub overlay: Overlay` (default `Overlay::None`).
- **Why an enum, not two bools:** the cheatsheet and command-log are mutually exclusive (only one overlay at a time) and the command-log carries scroll state. An enum makes "at most one overlay" unrepresentable-otherwise and matches the `Option`-state precedent (`confirm`, `progress`).
- The filter overlay is tracked separately by `Option<FilterState>` (it has input semantics distinct from the dismiss-only overlays); filter and `Overlay` are also mutually exclusive in practice (open one closes the other — enforced in the reducer).

### 3.4 Skins (`Skin` enum on Model + skin axis in `theme.rs`)

```
// src/tui/theme.rs — pure, always-compiled like ColorMode
pub enum Skin { Kraft, Carbon, Blueprint }   // Copy + Eq + Debug; default Kraft

impl Skin {
    pub fn next(self) -> Skin { ... }   // cycle Kraft→Carbon→Blueprint→Kraft
    pub fn name(self) -> &'static str { ... } // "kraft" | "carbon" | "blueprint"
}
```

- New `Model` field: `pub skin: Skin` (default `Skin::Kraft` — preserves the just-shipped look). `app::run` does NOT override it (no env/config in Bundle 1).
- **`Theme::resolve` gains the skin axis:** `Theme::resolve(skin: Skin, mode: ColorMode) -> Theme`. The NoColor arm IGNORES skin (all skins collapse to the SAME zero-color modifier-only theme — this is what preserves the invariant). TrueColor/Ansi16 arms branch on skin. See §5 for the token tables.
- **`view()` call site** (`view.rs:24`) changes from `Theme::resolve(model.color_mode)` to `Theme::resolve(model.skin, model.color_mode)`.

### 3.5 Toasts (`Toast` on Model — augments StatusLine, does NOT replace it)

```
pub enum ToastKind { Success, Info, Error }

pub struct Toast {
    pub kind: ToastKind,
    pub text: String,
    pub born_tick: u64,   // model.spinner_tick value at creation
    pub ttl_ticks: u64,   // expire when (now_tick - born_tick) >= ttl_ticks
}
```

- New `Model` field: `pub toasts: Vec<Toast>` (default empty). Bounded to a small MAX (e.g. 3 visible; push drops the OLDEST beyond cap, same discipline as the command-log ring).
- **Tick type note:** `model.spinner_tick` is currently `usize` (`model.rs`). Toast TTL math uses it as the clock. Keep `born_tick`/`ttl_ticks` as `usize` to match, OR widen `spinner_tick` to `u64` — recommend keeping `usize` to avoid touching unrelated code; document the wrap (`wrapping_add`, `update.rs:31`) is benign at TTL scale. **Flagged note** (not a decision): TTL is in *ticks*, and a tick is ~50ms ONLY when idle; during rapid input ticks fire less predictably. This is acceptable for a transient toast (it expires after roughly TTL×poll-interval of idle time). Spec sets default TTL = **60 ticks** (~3s idle) for Success/Info, **120 ticks** (~6s) for Error.
- **Coexistence with StatusLine (the migration that keeps tests green):** `model.status: StatusLine` STAYS exactly as is. The ~15 completion handlers (`update.rs:659,688,707,730,753,...`) that set `model.status = StatusLine::Ok/Error(...)` are UNCHANGED — `tests/tui_update.rs` keeps asserting those variants. **Toasts are ADDITIVE:** each completion handler, AFTER setting `model.status`, ALSO pushes a `Toast` mirroring it. To avoid editing 15 sites by hand error-prone-ly, add ONE helper on `Model`:
  ```
  fn set_status_ok(&mut self, msg: String)    { self.status = StatusLine::Ok(msg.clone());    self.push_toast(ToastKind::Success, msg); }
  fn set_status_error(&mut self, msg: String) { self.status = StatusLine::Error(msg.clone()); self.push_toast(ToastKind::Error, msg); }
  ```
  and refactor the completion handlers to call these. This is mechanical and preserves the `StatusLine` variant each test asserts. `StatusLine::Busy` does NOT spawn a toast (it's a live spinner, not a transient result). See §4 for the exact migration.
- **Expiry on Tick:** in `Message::Tick`, after advancing the spinner, retain only non-expired toasts: `self.toasts.retain(|t| now - t.born_tick < t.ttl_ticks)`. Pure, testable (`AC-TOAST-1`).

### 3.6 Command-log (shared ring buffer + `Overlay::CommandLog`)

```
// src/tui/cmdlog.rs (NEW) — pure ring buffer, always-compiled
pub struct CmdLogEntry {
    pub argv: String,     // "distrobox create --name web ..." (space-joined)
    pub status: Option<i32>, // exit code; None for interactive/unknown
    pub seq: u64,         // monotonic id for stable ordering
}

pub struct CmdLog {
    buf: VecDeque<CmdLogEntry>,
    cap: usize,           // BOUNDED — default 200
    next_seq: u64,
}
impl CmdLog {
    pub fn new(cap: usize) -> Self
    pub fn push(&mut self, argv: String, status: Option<i32>)  // drops oldest past cap
    pub fn entries(&self) -> impl Iterator<Item = &CmdLogEntry> // newest-last or newest-first (decide; spec: oldest→newest, scroll to bottom)
    pub fn len(&self) -> usize
}
```

- **Shared handle:** `Arc<Mutex<CmdLog>>`. Created in `app::run`, cloned into (a) the `LoggingRunner` decorator that wraps the injected runner before it goes to the worker, and (b) the `Model` (`pub cmdlog: Arc<Mutex<CmdLog>>`). The worker thread (writer) and the reducer/view (reader) share it; the `Mutex` is uncontended in practice (worker writes on spawn-completion, main thread reads on `Tick`/render). This is the ONLY place the otherwise-pure model holds a shared handle — justified because the capture point is necessarily on the worker thread.
- **Bound:** `cap = 200` entries (a few KB; generous for a session, cheap). Past cap, `push` pops the front (oldest). `AC-CMDLOG-1` asserts boundedness.
- **Captured data:** the space-joined argv string (from `CmdOutput.argv` / `Invocation.argv()`) + exit `status`. NO stdout/stderr (privacy + size; the log is "what ran," not "what it printed"). Interactive spawns log argv with `status = Some(code)` from `run_interactive`'s return; DryRun is not logged (it never spawns).
- **Capture mechanism — `LoggingRunner` decorator (§6).**

---

## 4. Keymap spec (full updated table + conflict resolution)

The keymap is the data table in §3.0. Below is its content. **Display keys are in-voice and lowercase; the cheatsheet renders `key` + `action`.**

### 4.1 List screen (no overlay)

| key | action (verb) | reducer site | conflict / note |
|-----|---------------|--------------|-----------------|
| `↑↓` / `j` `k` | move | `update.rs:86-93` | unchanged |
| `enter` | open | `update.rs:94-122` | unchanged |
| `i` | inspect | `update.rs:123-137` | unchanged |
| `c` | pack (create) | `update.rs:138-142` | unchanged |
| `s` | seal (stop) | `update.rs:154-167` | unchanged |
| `d` | clear out (destroy) | `update.rs:143-153` | unchanged — NOTE: `d` is destroy; Doctor moves to **`D`** (uppercase), no clash |
| `a` | apply | `update.rs:168-175` | unchanged |
| `e` | edit | `update.rs:186-192` | unchanged |
| `r` | refresh | `update.rs:193-197` | unchanged |
| `/` | **filter** | NEW | opens `FilterState`; no prior binding for `/` |
| `?` | **cheatsheet** | REBIND (was Doctor `update.rs:198-205`) | opens `Overlay::Cheatsheet` |
| `D` | **doctor** | NEW (moved from `?`) | uppercase D; `d` (destroy) is unaffected — crossterm delivers `Char('D')` distinct from `Char('d')` |
| `t` | **skin (theme) cycle** | NEW | `model.skin = model.skin.next()`; `t` is currently UNBOUND on List — no conflict. (Considered `T`; chose lowercase `t` since no List binding uses it. `s`=stop is taken, so not `s`.) |
| `l` | **command-log** | NEW | opens `Overlay::CommandLog`; `l` is UNBOUND on List — no conflict. Mnemonic: **l**og |
| `q` / `esc` | quit | `update.rs:206-209` | unchanged (List only; esc closes overlays first — see precedence) |

### 4.2 Overlay/secondary contexts

| context | keys |
|---------|------|
| **FilterInput** (`/` active) | type to narrow · `backspace` delete · `↑↓`/`j`/`k` move within matches · `enter` keep selection + close · `esc` clear + close |
| **Cheatsheet** (`?` active) | any key or `esc` dismiss |
| **CommandLog** (`l` active) | `↑↓`/`j`/`k` scroll · `esc`/`q`/`l` close |
| **Detail** | unchanged (`esc`/`q` back · `e` edit · `a` apply · `enter` enter-if-running) — cheatsheet `?` and skin `t` SHOULD also work here (see precedence) |
| **Wizard / ConfirmDestroy / Progress / DoctorPanel** | unchanged |

### 4.3 Key-handling precedence (reducer — the critical ordering)

`handle_key` (`update.rs:60-79`) gains an overlay pre-check BEFORE the `match model.screen`:

```
1. Ctrl-C / Ctrl-D → Quit   (unchanged, even when busy — update.rs:62-65)
2. if model.busy → drop      (unchanged — update.rs:68-70)
3. if model.filter.is_some() → handle_key_filter(...)        // intercepts ALL keys
4. match model.overlay:
     Cheatsheet      → any key / esc → overlay = None; return
     CommandLog{..}  → scroll / esc-close; return
     None            → fall through
5. (global, screen-agnostic) Char('t') → cycle skin; return   // available on every screen
6. match model.screen { ... }   (existing dispatch, with `?`→cheatsheet / `D`→doctor / `/`→filter / `l`→cmdlog added to List arm)
```

- **`esc` precedence:** when an overlay/filter is open, `esc` closes the OVERLAY, not the screen. Only when no overlay is open does `esc` reach the screen handler (where on List it still quits). This is why the overlay pre-check runs before screen dispatch.
- **`?` while in Detail/etc.:** because step 5 is screen-agnostic only for `t`, `?` opening the cheatsheet is added to the List arm primarily; OPTIONAL stretch — add `?`→cheatsheet to step 4's fall-through as a global so the cheatsheet is reachable everywhere. **Spec decision:** make `?`→cheatsheet and `t`→skin BOTH global (steps 5), reachable on any non-busy screen, since they're informational/cosmetic and never mutate box state. `/`, `l`, `D` stay List-only.

### 4.4 Conflicts resolved (explicit log)

- **`?` double-duty:** previously Doctor; now cheatsheet. Doctor rebind to `D` resolves it. **Test migration required:** `tests/tui_update.rs:514-520` `doctor_question_mark_opens_panel` asserts `?`→DoctorPanel — this test MUST be updated to assert `D`→DoctorPanel and a NEW test added for `?`→cheatsheet. (Risk register R-1.)
- **`d` vs `D`:** lowercase `d` = destroy (kept), uppercase `D` = doctor (new). crossterm `KeyCode::Char` is case-sensitive and `normalize_key` (`app.rs:91-107`) passes the char through verbatim, so `Char('d')` and `Char('D')` are distinct — no normalization collision. (Risk register R-2: confirm the worker's key path doesn't lowercase.)
- **`t`, `l`, `/`:** all currently UNBOUND on List (verified against `handle_key_list`, `update.rs:84-211`) — no conflict.
- **`j`/`k` reuse:** in FilterInput and CommandLog, `j`/`k` are nav, NOT literal characters typed into the query — EXCEPT in FilterInput where a user might want to type "j" into a box name. **Resolution:** in FilterInput, `j`/`k` are TEXT (typed into query); navigation within matches is `↑`/`↓` ONLY. (Avoids the "can't type j" trap.) In CommandLog (no text input) `j`/`k` are scroll. Documented in the keymap per-context.

---

## 5. Skin design (token tables)

Three named skins, layered on the existing `theme.rs` structure. Each defines the TrueColor and Ansi16 tiers; the NoColor tier is **shared and skin-independent** (the invariant). Names are in-voice (materials/blueprints — cardboard-adjacent, not adjectives).

### 5.1 `Skin::Kraft` (default — the just-shipped retro look, UNCHANGED)

Exactly today's `truecolor()` / `ansi16()` tables (`theme.rs:139-210`). Kraft amber `Rgb(214,158,92)` accent, brown borders, green/red semantics. No change — this is the baseline `AC-THEME-2` already pins (`tests/tui_theme.rs:62-81`).

### 5.2 `Skin::Carbon` (dark slate / monochrome-leaning)

A low-chroma terminal-native skin for users who find amber loud. Token intent (Vivi picks exact RGBs within these intents; spec fixes the *anchors* the tests assert):

| token | TrueColor intent | Ansi16 |
|-------|------------------|--------|
| accent | cool gray `Rgb(160,170,180)` (ANCHOR — `AC-SKIN-1` asserts this exact value) | `Color::White` |
| accent_dim | `Rgb(110,118,128)` | `Color::Gray` |
| success | `Rgb(120,180,140)` | `Color::Green` |
| warning | `Rgb(210,180,110)` | `Color::Yellow` |
| danger | `Rgb(205,100,95)` | `Color::Red` |
| border / border_focus | `Rgb(80,86,94)` / accent | `DarkGray` / `White` |
| selection | bg `Rgb(40,44,50)` fg `Rgb(220,224,230)` BOLD | bg `Blue` fg `White` BOLD |
| brand_* | accent-derived | White/Gray |
| badges | success/danger/dim per running/error/stopped | Green/Red/DarkGray |

### 5.3 `Skin::Blueprint` (cyan-on-dark drafting look)

A blueprint/drafting skin — cyan accent, cool blues. Token intent:

| token | TrueColor intent | Ansi16 |
|-------|------------------|--------|
| accent | blueprint cyan `Rgb(90,170,200)` (ANCHOR — `AC-SKIN-1`) | `Color::Cyan` |
| accent_dim | `Rgb(60,120,150)` | `Color::Blue` |
| success | `Rgb(110,190,160)` | `Color::Green` |
| warning | `Rgb(220,180,90)` | `Color::Yellow` |
| danger | `Rgb(210,95,90)` | `Color::Red` |
| border / border_focus | `Rgb(60,100,130)` / accent | `Blue` / `Cyan` |
| selection | bg `Rgb(20,45,60)` fg `Rgb(210,235,245)` BOLD | bg `Cyan` fg `Black` BOLD |
| brand_* | accent-derived | Cyan |
| badges | success/danger/dim | Green/Red/Blue |

### 5.4 NoColor tier (shared, skin-independent — the invariant)

`Theme::resolve(_, ColorMode::NoColor)` returns ONE table for ALL skins: the existing `nocolor()` (`theme.rs:214-238`) — zero fg/bg, differentiation only via Modifier (BOLD/DIM/REVERSED). The skin argument is ignored in the NoColor arm. This is what makes `AC-SKIN-NOCOLOR` (the generalized invariant) hold for every skin trivially.

**Flagged sub-decision (you may sign off, human):** the two new skins `Carbon` + `Blueprint` are a small product choice. If you want different names or a different second/third palette, that's a one-line `enum` + table change — flag it now if so; otherwise spec proceeds with Kraft/Carbon/Blueprint. (D-3, low stakes.)

### 5.5 Skin cycle copy

Pressing `t` shows a transient toast: `ToastKind::Info`, text `Skin: kraft` / `Skin: carbon` / `Skin: blueprint` (from `Skin::name()`). Honest, concrete, no adjectives — voice-compliant.

---

## 6. Command-log capture (exact mechanism)

### 6.1 The `LoggingRunner` decorator (NEW — `src/tui/cmdlog.rs` or `src/dbox/logging.rs`)

```
pub struct LoggingRunner {
    inner: Arc<dyn DistroboxRunner>,
    log: Arc<Mutex<CmdLog>>,
}
impl DistroboxRunner for LoggingRunner {
    fn run(&self, inv: Invocation) -> Result<CmdOutput, RunnerError> {
        let argv = inv.argv().join(" ");          // capture BEFORE move
        let skip = inv.mode == RunMode::DryRun;   // don't log dry-runs (never spawn)
        let out = self.inner.run(inv);
        if !skip {
            let status = out.as_ref().ok().map(|o| o.status);
            self.log.lock().unwrap().push(argv, status);
        }
        out
    }
    fn run_interactive(&self, inv: Invocation) -> Result<i32, RunnerError> {
        let argv = inv.argv().join(" ");
        let res = self.inner.run_interactive(inv);
        let status = res.as_ref().ok().copied();
        self.log.lock().unwrap().push(argv, status);
        res
    }
}
```

- **Capture point cited:** `DistroboxRunner::run` / `run_interactive` (`src/dbox/runner.rs:89-96`) — the single chokepoint every spawn passes through. `Invocation::argv()` (`runner.rs:41-45`) yields the exact program+args; `CmdOutput.argv`/`.status` (`runner.rs:49-56`) confirm it round-trips. `RealRunner` (`real.rs:8-50`) and `MockRunner` (`mock.rs:159-183`) both implement the trait, so the decorator wraps EITHER transparently — tests drive it with `MockRunner`.
- **Wire-in (`app.rs:320-337` + the worker spawn `app.rs:116-133`):** in `app::run`, build the shared `Arc<Mutex<CmdLog>>`, wrap the injected `runner` in `LoggingRunner`, and pass the WRAPPED runner into `run_loop`/`spawn_worker`. Put the SAME `Arc` on `Model`. ZERO change to `effect.rs`, `real.rs`, `core::*`. (The decorator is `Send + Sync` because `inner` is `Arc<dyn DistroboxRunner>` which the `_assert_runner_send_sync` check, `effect.rs:141-142`, already requires.)
- **Read path (drain on Tick):** the reducer doesn't need to copy entries — the view reads `model.cmdlog.lock()` directly when rendering `Overlay::CommandLog`. No Message round-trip needed (the log is shared state, not a completion event). This keeps the reducer pure-ish: it only reads the mutex during render, and the only mutation is bounded `push` on the worker side. **No new `Message` variant for the command-log.**

### 6.2 Command-log overlay UX + copy (echo-only, honest)

- Rendered like the ConfirmDestroy modal: `Clear` + centered/large `Rect` over the current screen, `Block` with `Borders::ALL`, title ` command log `.
- Body: newest entries at the bottom (terminal-log convention), each line: `{glyph} {argv}` where glyph encodes status (`✓` status 0, `✗` non-zero, `·` unknown/interactive). Scroll with `↑↓`/`j`/`k`; `Overlay::CommandLog{scroll}` holds the offset.
- **Honest copy (new `strings` consts, voice-compliant, added to `AC-COPY-1`):**
  - Title: `" command log "` (or `strings::CMDLOG_TITLE`).
  - Empty state: `strings::CMDLOG_EMPTY = "Nothing has run yet."`
  - Footer hint: `strings::CMDLOG_HINT = "What cbox ran, newest last. This is a record — it can't undo anything."` — explicitly states it's a log, not an undo. **Must NOT** contain banned adjectives; states the no-undo truth plainly.
- This copy is the load-bearing honesty requirement: it tells the user the log cannot reverse a destroy.

---

## 7. Decomposition + sequencing (Vivi-sized tasks, dependency-ordered)

All tasks: containerized, verifier is a `make` target. Ordered so each builds on green predecessors. `T` = Vivi task; `K` = Kupo micro-task.

| id | title | files | depends | timebox | verifier |
|----|-------|-------|---------|---------|----------|
| **K1** | `?`→cheatsheet / Doctor→`D` rebind + test migration | `src/tui/update.rs`, `tests/tui_update.rs` | — | 1d | `make test` (migrate `doctor_question_mark_opens_panel`; add `D`→doctor test) |
| **T1** | keymap-as-data module (`KEYMAP`, `keymap_for`, `help_line`); rewire status bar to `help_line`; voice-assert keymap actions | `src/tui/keymap.rs` (new), `src/tui/mod.rs`, `src/tui/view.rs`, `src/tui/strings.rs`, `tests/tui_keymap.rs` (new) | K1 | ≤2d | `make build; make lint-lean; make test` |
| **T2** | cheatsheet overlay: `Overlay` enum on Model, reducer precedence pre-check, `?`→Cheatsheet, render from `keymap_for` | `src/tui/model.rs`, `src/tui/update.rs`, `src/tui/view.rs`, `tests/tui_bundle1.rs` (new) | T1 | ≤2d | `make build; make test` |
| **T3** | fuzzy filter: `nucleo-matcher` dep, `filter.rs` `fuzzy_rank`, `FilterState`, filter-aware `move_up/down`, `/` open + input handling, `render_list` `.filter()` | `Cargo.toml`, `src/tui/filter.rs` (new), `src/tui/model.rs`, `src/tui/update.rs`, `src/tui/view.rs`, `tests/tui_bundle1.rs` | T2 | ≤3d | `make build; make lint-lean; make test` |
| **T4** | skin switcher: `Skin` enum + `next`/`name`, `Theme::resolve(skin, mode)`, Carbon+Blueprint tables, `skin` field, global `t` cycle, skin toast, generalize NoColor-invariant test for all skins | `src/tui/theme.rs`, `src/tui/model.rs`, `src/tui/update.rs`, `src/tui/view.rs`, `src/tui/strings.rs`, `tests/tui_theme.rs` | T2 | ≤2d | `make build; make test` |
| **T5** | toasts: `Toast`/`ToastKind`, `toasts` field, `set_status_ok/error` helpers, refactor ~15 completion handlers to use them, expire-on-Tick, stacked render region | `src/tui/model.rs`, `src/tui/update.rs`, `src/tui/view.rs`, `tests/tui_update.rs` (keep green), `tests/tui_bundle1.rs` | T2 | ≤3d | `make test` (StatusLine variants stay green) |
| **T6** | command-log: `CmdLog` ring + `CmdLogEntry`, `LoggingRunner` decorator, wire-in at `app::run`/`spawn_worker`, `Arc<Mutex<CmdLog>>` on Model, `Overlay::CommandLog` render + `l` open + scroll, honest copy consts | `src/tui/cmdlog.rs` (new), `src/tui/model.rs`, `src/tui/app.rs`, `src/tui/update.rs`, `src/tui/view.rs`, `src/tui/strings.rs`, `tests/tui_bundle1.rs` | T2 (overlay enum), T1 | ≤3d | `make build; make test` |
| **FINAL** | full gate | — | all | — | `make check` (fmt-check + lint + lint-lean + build + release + test) |

- **Kupo micro-task (flagged):** **K1** is a localized ≤2-file mechanical change (rebind `?`→`D` at `update.rs:198-205`; add the `D` arm; migrate the one test). Self-contained, no design — ideal Kupo scope. It MUST land first because every later cheatsheet/keymap assertion assumes the rebind.
- **Sequencing rationale:** K1 unblocks the keymap (the cheatsheet's source of truth). T1 (keymap-as-data) precedes everything that reads keys. T2 (overlay enum) is the shared substrate for cheatsheet + command-log, so it precedes both T6 and (loosely) the filter. T3/T4/T5 are independent siblings off T2 and can be done in any order or in parallel by one coder. T6 depends on the overlay enum (T2) and the keymap (T1).

---

## 8. Acceptance criteria (GIVEN/WHEN/THEN — testable against pure helpers)

Mirrors the theme work's pure-helper testing style (`tests/tui_theme.rs`). New file **`tests/tui_bundle1.rs`** for filter/cheatsheet/toast/command-log reducer + helper ACs; **`tests/tui_keymap.rs`** for keymap; extend **`tests/tui_theme.rs`** for skins; extend **`tests/tui_update.rs`** for the rebind + toast-coexistence. Existing `tests/tui_update.rs` + `tests/tui_effects.rs` stay green.

### Fuzzy filter

- **AC-FILTER-1 (ranks/filters a known set):** GIVEN names `["web-dev","api","webhook","db"]` WHEN `fuzzy_rank("web", &names)` THEN result indices map to a subset containing `web-dev` and `webhook` and NOT `api`/`db`, with `web-dev` and `webhook` ranked ahead of any non-match. (Assert membership + relative order, not exact scores.)
- **AC-FILTER-2 (empty query = identity):** GIVEN any names WHEN `fuzzy_rank("", &names)` THEN result == `(0..names.len())` (all, in order).
- **AC-FILTER-3 (selection maps under filter):** GIVEN a model with boxes `[A,B,C,D]`, filter open with query matching `[B,D]` (matches = `[1,3]`, cursor 0 → selected = Some(1)) WHEN `move_down` THEN cursor = 1 AND `model.selected == Some(3)` (maps through `matches`, NOT to index 1). WHEN `move_down` again THEN cursor clamps at 1, `selected == Some(3)`.
- **AC-FILTER-4 (close semantics):** GIVEN filter open with a selection WHEN Enter THEN `model.filter == None` AND `model.selected` unchanged (committed). WHEN Esc instead THEN `model.filter == None` AND selection falls back per §3.2.
- **AC-FILTER-5 (refresh under filter):** GIVEN filter open WHEN `ListLoaded(Ok(rows))` THEN matches recompute against new rows AND cursor clamps to valid range (no panic, no out-of-range `selected`).

### Cheatsheet

- **AC-CHEAT-1 (open/dismiss):** GIVEN List, no overlay WHEN `Key::Char('?')` THEN `model.overlay == Overlay::Cheatsheet`. WHEN any subsequent key (e.g. `Char('x')`) or Esc THEN `model.overlay == Overlay::None`.
- **AC-CHEAT-2 (content from keymap):** GIVEN `keymap_for(KeyContext::List)` THEN it contains bindings for `/`, `?`, `D`, `t`, `l`, `c`, `d`, `s`, `a`, `e`, `q` (the full List set) — assert each `key` present. (This is what the cheatsheet renders.)

### Keymap

- **AC-MAP-1 (single source — help line):** `keymap::help_line(KeyContext::List)` contains `move`, `pack`/`create`, `seal`/`stop`, and `cheatsheet` — derived, not hand-written.
- **AC-MAP-2 (per-context entries):** `keymap_for(FilterInput)` advertises type/backspace/enter/esc; `keymap_for(CommandLog)` advertises scroll/esc — assert distinct sets per context.
- **AC-MAP-VOICE (voice compliance):** every `KeyBinding.action` across every context is free of the BANNED adjectives (same list as `AC-COPY-1`). (Replaces `HELP` in the voice check per D-1.)

### Skins

- **AC-SKIN-1 (resolution per skin/tier):** `Theme::resolve(Skin::Kraft, TrueColor).accent.fg == Some(Rgb(214,158,92))` (unchanged baseline); `Theme::resolve(Skin::Carbon, TrueColor).accent.fg == Some(Rgb(160,170,180))`; `Theme::resolve(Skin::Blueprint, TrueColor).accent.fg == Some(Rgb(90,170,200))`. Ansi16 accent maps per §5 (`Kraft→Yellow`, `Carbon→White`, `Blueprint→Cyan`).
- **AC-SKIN-NOCOLOR (invariant per skin — P0):** FOR EACH `skin in [Kraft, Carbon, Blueprint]`: `Theme::resolve(skin, NoColor)` has `fg.is_none() && bg.is_none()` for ALL 18 style fields (generalize `ac_theme_3_nocolor_no_fg_bg_anywhere`, `tests/tui_theme.rs:88-124`, into a loop over skins).
- **AC-SKIN-CYCLE (cycle order):** `Skin::Kraft.next() == Carbon`, `Carbon.next() == Blueprint`, `Blueprint.next() == Kraft`. GIVEN a model WHEN `Key::Char('t')` THEN `model.skin` advances one step AND a `ToastKind::Info` toast with the new skin name is pushed.

### Toasts

- **AC-TOAST-1 (TTL expiry on Tick):** GIVEN a toast with `born_tick=0, ttl_ticks=60` and `model.spinner_tick=0` WHEN 60 `Tick` messages are applied (spinner_tick→60) THEN the toast is no longer in `model.toasts`. WHEN only 59 ticks THEN it is still present.
- **AC-TOAST-2 (completion spawns toast + preserves StatusLine):** GIVEN a busy model WHEN `Message::CreateDone(Ok(...))` THEN `model.status` is `StatusLine::Ok(_)` (UNCHANGED assertion — keeps `tests/tui_update.rs` semantics) AND `model.toasts` contains one `ToastKind::Success` toast. WHEN `Message::StopDone(Err(...))` THEN `StatusLine::Error(_)` AND a `ToastKind::Error` toast.
- **AC-TOAST-3 (bounded queue):** GIVEN MAX visible = 3 WHEN 5 toasts are pushed THEN `model.toasts.len() <= 3` and the 2 oldest are dropped.
- **AC-TOAST-4 (Busy does not toast):** GIVEN any handler that sets `StatusLine::Busy` WHEN applied THEN no toast is pushed (live spinner, not a result).

### Command-log

- **AC-CMDLOG-1 (bounded ring drops oldest):** GIVEN `CmdLog::new(3)` WHEN 5 `push` calls THEN `len() == 3` AND the retained entries are the 3 newest (by `seq`).
- **AC-CMDLOG-2 (decorator captures real argv + status):** GIVEN a `MockRunner` returning status 0 wrapped in `LoggingRunner` with a shared `CmdLog` WHEN `run(Invocation::new("distrobox", vec!["create","--name","web"], Capture))` THEN the log gains one entry with `argv == "distrobox create --name web"` and `status == Some(0)`.
- **AC-CMDLOG-3 (dry-run not logged):** GIVEN a `DryRun`-mode invocation through `LoggingRunner` THEN the log is unchanged (no entry).
- **AC-CMDLOG-4 (interactive logs exit code):** GIVEN `run_interactive` returning `Ok(0)` through the decorator THEN one entry with `status == Some(0)`.
- **AC-CMDLOG-5 (overlay open/scroll):** GIVEN List WHEN `Key::Char('l')` THEN `model.overlay == Overlay::CommandLog{scroll:0}`. WHEN `Key::Down` THEN scroll increments (bounded by entry count). WHEN `Esc` THEN `Overlay::None`.

### Rebind (migration)

- **AC-REBIND-1:** GIVEN List WHEN `Key::Char('D')` THEN `model.screen == Screen::DoctorPanel` AND a `Doctor` effect is emitted. (Replaces the old `?`→doctor test.)
- **AC-REBIND-2:** GIVEN List WHEN `Key::Char('?')` THEN `model.overlay == Overlay::Cheatsheet` AND NO `Doctor` effect (the old binding is gone).

---

## 9. Risk register + validation gates

### 9.1 Risk register

| id | risk | blast radius | severity | mitigation / gate |
|----|------|--------------|----------|-------------------|
| **R-1** | Toast refactor regresses `StatusLine` variant tests | `update.rs` ~15 handlers; `tests/tui_update.rs` | **P0** | `set_status_ok/error` helpers PRESERVE `model.status = StatusLine::Ok/Error` exactly; toasts are additive. Gate: `make test` — every existing `tests/tui_update.rs` assertion green AFTER T5. AC-TOAST-2 pins coexistence. |
| **R-2** | `?`→`D` rebind misses the existing test → stale failing test | `tests/tui_update.rs:514-520` | **P0** | K1 migrates the test in the SAME task as the rebind. AC-REBIND-1/2 replace it. Gate: `make test` after K1. |
| **R-3** | Filter desyncs selection (`selected` points outside `boxes`, panic in `selected_box`) | `model.rs`, `update.rs`, `view.rs` | **P1** | All selection goes through `filter.matches[cursor]`; clamp on every recompute (AC-FILTER-3/5). `selected_box` already `.get()`s safely (`model.rs:193`). Gate: AC-FILTER-3/5 + `make test`. |
| **R-4** | NoColor invariant broken by a new skin (a Carbon/Blueprint token leaks fg/bg into NoColor) | `theme.rs` | **P0** | NoColor arm is SKIN-INDEPENDENT (one shared `nocolor()`); skin ignored there. AC-SKIN-NOCOLOR loops all skins × all 18 fields. Gate: `make test`. |
| **R-5** | `LoggingRunner` mutex contention / `Send+Sync` break on the worker thread | `app.rs`, `cmdlog.rs` | **P1** | `inner: Arc<dyn DistroboxRunner>` keeps `Send+Sync` (compile-checked by `_assert_runner_send_sync`, `effect.rs:141`). Lock held only for the push. Gate: `make build` (compile) + AC-CMDLOG-2/4. |
| **R-6** | `nucleo-matcher` leaks into the lean (`make lint-lean`) build | `Cargo.toml`, filter module | **P1** | Gate the dep behind the `tui` feature (D-2); place `fuzzy_rank` in a `#[cfg(feature="tui")]` path OR a module not compiled lean. Gate: `make lint-lean` MUST pass in T3. |
| **R-7** | New `strings` consts trip `AC-COPY-1` (banned adjective) | `strings.rs`, `tests/tui_theme.rs` | **P1** | Add every new const to the `AC-COPY-1` list AND keep copy verb-driven (pack/seal/log). The command-log no-undo line is honest, no adjectives. Gate: `make test`. |
| **R-8** | `j`/`k` in FilterInput can't be typed into a query | `update.rs` filter handler | **P2** | In FilterInput, `j`/`k` are TEXT; nav is `↑`/`↓` only (§4.4). Gate: AC-FILTER nav uses arrows. |
| **R-9** | Tick-based TTL is wall-clock-inaccurate (ticks only fire ~50ms when idle) | toasts | **P2** | Accepted: transient toasts expire after ~TTL idle ticks; precise wall-clock not required. Documented in §3.5. No gate beyond AC-TOAST-1 (tick-count semantics). |
| **R-10** | Overlay precedence wrong → `esc` quits the app instead of closing an overlay | `update.rs` `handle_key` | **P1** | Overlay/filter pre-check runs BEFORE screen dispatch (§4.3). AC-CHEAT-1, AC-CMDLOG-5, AC-FILTER-4 each assert `esc` closes the overlay, not the screen. Gate: `make test`. |

### 9.2 Validation gates (containerized)

- **G-BUILD:** `make build` (tui feature on, default) — compiles all new modules + decorator.
- **G-LEAN:** `make lint-lean` — the no-tui build stays clean; `nucleo-matcher` gated behind `tui`; pure always-compiled helpers (`keymap`, `cmdlog` ring if always-compiled) lint-clean.
- **G-FMT:** `make fmt-check`.
- **G-LINT:** `make lint` (clippy).
- **G-TEST:** `make test` — ALL existing tests green (`tui_update`, `tui_effects`, `tui_theme`) PLUS new `tui_bundle1`, `tui_keymap`, and extended `tui_theme`/`tui_update`.
- **G-INVARIANT:** AC-SKIN-NOCOLOR (every skin × every field, NoColor) — the P0 visual invariant.
- **G-COEXIST:** AC-TOAST-2 + the unchanged `tests/tui_update.rs` StatusLine assertions — the toast migration didn't regress the asserted surface.
- **G-FINAL:** `make check` (fmt-check + lint + lint-lean + build + release + test) — the release gate.

### 9.3 NoColor invariant verification (per skin) — explicit procedure

1. Generalize `ac_theme_3_nocolor_no_fg_bg_anywhere` (`tests/tui_theme.rs:88-124`) into `for skin in [Kraft, Carbon, Blueprint] { let theme = Theme::resolve(skin, NoColor); /* assert all 18 fields fg/bg None */ }`.
2. Manual smoke (optional, containerized): run the binary with `NO_COLOR=1` and cycle `t` through all three skins — the frame must be visually identical across skins (only modifiers differ). This is a smoke check; the automated loop is authoritative.

---

## 10. Confidence report

- **Pattern match (25%):** 95% — ConfirmDestroy overlay, `Theme::resolve` pure seam, runner-trait decorator, and the existing pure-helper test style are all directly reused. Strong prior (the just-shipped theme spec validated the same testing approach).
- **Requirement clarity (25%):** 90% — feature set + flags LOCKED; three low-stakes flagged sub-decisions (D-1 keymap-vs-HELP, D-2 dep gating, D-3 skin names) are all defaulted with a recommendation and don't block.
- **Decomposition stability (25%):** 85% — three independent self-consistency passes (by-feature / by-file / by-dependency) converge on the K1→T1→T2→{T3,T4,T5,T6} shape; T3–T6 ordering is interchangeable (siblings off T2), which is the only material variance.
- **Constraint compliance (25%):** 90% — containerized `make` verifiers throughout; NoColor invariant pinned per skin; StatusLine-test coexistence designed in; lean build gated. The one residual is the Tick-as-clock imprecision for TTL (R-9, accepted P2).

**Aggregate: 88% → AUTO_PROCEED.** Deliver to Vivi; K1 routable to Kupo.

---

## 11. Flagged decisions for the human (all defaulted; none block)

- **D-1 (keymap vs `strings::HELP`):** recommend REPLACING `strings::HELP` with `keymap::help_line(List)` and voice-asserting the keymap actions (AC-MAP-VOICE) instead of the `HELP` const. Sign-off optional.
- **D-2 (fuzzy dep gating):** recommend `nucleo-matcher = { version = "0.3", optional = true }` gated under the `tui` feature (no new lean-build dep). Sign-off optional.
- **D-3 (skin names/palettes):** recommend `Kraft` (default, unchanged) + `Carbon` (slate) + `Blueprint` (cyan). Two new names + palettes are a small product choice — flag if you want different names/colors; otherwise spec proceeds.
- **D-4 (skin-cycle key `t`, command-log key `l`):** both currently unbound on List; `t`=theme/skin, `l`=log. Sign-off optional — swap is trivial if you prefer different mnemonics.

---

*SPECTRA — Strategic Specification through Deliberate Reasoning. Plan only; execution is Vivi's phase. Cite file:line anchors verified against the working tree at HEAD `0f71b50`.*
