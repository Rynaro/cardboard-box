# cbox TUI Bundle 2 "Living Box" — SPECTRA Spec

- **Spec ID:** `2026-06-20-tui-bundle2-living-box`
- **Intent:** REQUEST / CHANGE (feature set + GAP resolutions LOCKED by human after an ATLAS feasibility delta; this spec makes them precise + buildable — decisions are NOT re-opened).
- **Coder:** Vivi (feature work) / Kupo (one flagged micro-task)
- **Complexity:** 10/12 — extended depth, standard tier, single-pass cycle. The driver is GAP-1: a timeout on the shared Capture path touched by the WHOLE CLI, plus a concurrency-gated background poll on a single-thread worker. That blast radius pushes this above Bundle 1's 9.
- **Confidence:** 87% → AUTO_PROCEED (factor breakdown §10)
- **Release target:** next release-please increment — `feat` → minor bump (0.9.0 → 0.10.0). One umbrella `feat(tui): living box …` or per-feature `feat(tui): …` commits; either lands a minor bump.
- **Build constraint (pass to coder):** Builds are CONTAINERIZED. NEVER run host `cargo`. Use `make` targets only — verified present: `make build`, `make release`, `make test`, `make fmt`, `make fmt-check`, `make lint`, `make lint-lean`, `make check` (`Makefile` targets confirmed). `tui` is default-on (`Cargo.toml:17-18`), so plain `make build`/`make test` exercises this code. `make lint-lean` builds WITHOUT the `tui` feature — every new pure helper landing in an always-compiled module must keep that build clean (the timeout seam in `real.rs`/`runner.rs` and any new `core::stats` are always-compiled; the `nucleo-matcher` and ratatui-typed code stay `tui`-gated).
- **Voice rule (LOCKED, do not relitigate):** "show don't tell." Never advertise qualities (no cozy/beautiful/friendly/delightful/cute/lovely) in user-facing copy. Demonstrate character via layout, glyphs, cardboard-metaphor verbs (pack/unpack/seal/clear out). `AC-COPY-1` (`tests/tui_theme.rs:351-395`, `BANNED` list at `:351`) mechanically asserts the banned-substring list against EVERY public `strings` const — every new const this bundle adds MUST be added to that test's coverage and stay compliant. New `Action` labels and palette strings are user-facing too: extend the voice check to them (`AC-VOICE-1`, §8).
- **State of the tree:** Bundle 1 "Retro Cockpit" has LANDED on `main` (v0.9.0). `Overlay` enum, `keymap.rs`, `filter.rs`, `cmdlog.rs`, `LoggingRunner`, toasts, skins all present and verified. Bundle 2 builds directly on them.

---

## 0. Locked decisions (do NOT relitigate)

**Feature set (exactly these four, nothing more):**

1. **Live CPU/mem sparklines** on a running box (Detail screen) — periodic per-box `stats` poll, ratatui `Sparkline` fed from a bounded history buffer.
2. **Live list auto-refresh** (~2s) — a box flips stopped→running without manual `r`.
3. **`:` command palette** — fuzzy-searchable overlay mapping labels to actions.
4. **Bulk operations** — ALL FOUR, each with a confirm listing affected boxes: (a) prune all stopped, (b) stop all running, (c) destroy all cbox-managed, (d) destroy all NON-cbox-managed (DANGEROUS — strongest confirm, typed/extra, not a single keypress).

**GAP resolutions (honored verbatim; design detail in the cited sections):**

- **GAP-1** → durable Capture-timeout fix on the runner + a *silent* periodic poll that never sets `busy` and is coalesced when an effect is in-flight. Testable with a hung `MockRunner`. (§2.1, §3.1, §8 AC-TIMEOUT/AC-POLL)
- **GAP-2** → bulk ops are FILTERS over the live `model.boxes` (using `BoxRow.cbox_managed`, `spec.rs:183`, and `BoxRow.status`), fanned out over the EXISTING `Effect::Rm`/`Effect::Stop` which already batch via `names: Vec<String>` (`spec.rs:103-122`). NOT a Boxfile reconcile. (§3.4, §6)
- **GAP-3** → a first-class `Action` enum single-sources keymap + palette + cheatsheet, with a defined reducer dispatch. (§3.3, §4)

### Out of scope (note as future; do NOT spec or build)

- Stats history persistence across sessions; stats on the List screen; per-process breakdown.
- A configurable poll interval / config-file knob (poll cadence is a compile-time const this bundle).
- Network/disk-IO sparklines (CPU + mem only).
- Palette command history, aliases, or argument prompts (labels → fixed actions only).
- Bulk ops scoped to a filter/selection subset (bulk ops act on the WHOLE current `model.boxes`, partitioned by the four predicates — not the fuzzy-filtered view).
- A fleet Boxfile / manifest reconcile (explicitly rejected per GAP-2).
- Per-box stop/destroy *progress* fan-out UI beyond the existing single Progress screen reused for the batch.

### Deferred / explicitly NOT changed

- The worker thread COUNT stays ONE (`app.rs:124`). GAP-1's fix is a per-call timeout, NOT a thread pool. (§2.1 rationale.)
- `sync_channel::<Effect>(4)` bound (`app.rs:122`) and `sync_channel::<Message>(32)` (`app.rs:237`) — UNCHANGED. The poll reuses the existing transport (§3.1).
- All existing `StatusLine` semantics that `tests/tui_update.rs` asserts; all existing screen-dispatch shape.

---

## 1. Scope

### In scope

The four locked features (§0) + three cross-cutting enablers that are prerequisites, not new features:

- **E-0 Capture timeout** (`run_with_timeout` on the runner path) — the load-bearing GAP-1 fix; everything periodic depends on it.
- **E-1 Silent-poll gating** — a poll-counter + in-flight guard on `Model` so the background poll never sets `busy` and never piles up.
- **E-2 `Action` enum** (GAP-3) — single source for keymap/palette/cheatsheet, with reducer dispatch.

### Assumptions (risk-if-wrong)

| # | Assumption | Risk if wrong | Mitigation |
|---|-----------|---------------|------------|
| A-1 | `podman stats <id> --no-stream --format json` and `docker stats <id> --no-stream --format json` both emit a parseable per-container CPU%/mem object/array. | Sparklines show no data on one engine. | Parse defensively; missing fields → empty history → graceful "no stats" render (§5.4). Verified pattern: inspect already calls the engine directly (`mod.rs:373`). |
| A-2 | A single in-flight guard (`poll_in_flight: bool`) suffices because there is ONE worker thread draining serially (`app.rs:124-131`). | Concurrent polls could pile in the 4-deep channel. | The guard is set when a silent effect is dispatched and cleared on its completion Message; the serial worker guarantees ordering (§3.1). |
| A-3 | `std::process::Command` child can be killed on timeout from a watcher thread without leaking the worker. | Hung child zombifies / worker blocks. | Spawn the child, poll `try_wait` with a deadline, `child.kill()` on expiry, return a typed timeout error — the worker thread stays free (§2.1). |
| A-4 | Bulk ops act on `model.boxes` as last loaded; a stale row (just-destroyed elsewhere) is tolerable. | Bulk op targets a vanished box → backend error on that name. | Fan-out tolerates per-name backend errors the same way single ops do; the post-op LoadList reconciles. |

---

## 2. Concurrency / safety design (GAP-1 — the load-bearing section)

### 2.1 The Capture timeout seam (exact signatures + behavior)

**Problem (verified):** `RealRunner::run` uses `Command::output()` (`src/dbox/real.rs:26-29`) which blocks indefinitely; `run_interactive` uses `.status()` (`real.rs:52-76`). The worker is a SINGLE thread draining a bounded channel (`app.rs:122-131`); any hung Capture call freezes the entire TUI because no further effect can be drained and no completion Message can be posted.

**Design — add a timeout to the Capture path at the `RealRunner` level, surfaced through the trait via a new defaulted method. NO change to the trait's existing two methods, so MockRunner and every existing caller keep working unchanged.**

Add to `DistroboxRunner` (`src/dbox/runner.rs:89-96`) a THIRD method with a DEFAULT impl that delegates to `run` (so no impl breaks):

```
pub trait DistroboxRunner: Send + Sync {
    fn run(&self, inv: Invocation) -> Result<CmdOutput, RunnerError>;
    fn run_interactive(&self, inv: Invocation) -> Result<i32, RunnerError>;

    /// Capture mode with a wall-clock deadline. Default: ignore the deadline and
    /// delegate to `run` (preserves behavior for runners that can't time out, e.g.
    /// the simplest mocks). RealRunner OVERRIDES this with a real watchdog.
    fn run_with_timeout(&self, inv: Invocation, timeout: Duration)
        -> Result<CmdOutput, RunnerError>
    {
        let _ = timeout;
        self.run(inv)
    }
}
```

Add a timeout variant to `RunnerError` (`runner.rs:59-75`):

```
#[error("{program} timed out after {seconds}s")]
Timeout { program: String, seconds: u64 },
```
`exit_code()` (`runner.rs:77-84`) maps `Timeout` → `exit::TEMPFAIL` (75) — consistent with "backend not responding right now" (matches the `tempfail` semantics already used in `backend.rs:65`). This keeps the CLI's exit taxonomy coherent because `run_with_timeout` is only called from the TUI poll paths in Bundle 2 (the rest of the CLI keeps calling `run`), so no existing CLI exit code shifts. (R-1.)

**`RealRunner::run_with_timeout` watchdog (the only place real spawning gains a deadline):**

```
fn run_with_timeout(&self, inv: Invocation, timeout: Duration) -> Result<CmdOutput, RunnerError> {
    // DryRun short-circuits exactly as run() does (no spawn).
    if inv.mode == RunMode::DryRun { return self.run(inv); }

    let mut child = Command::new(&inv.program)
        .args(&inv.args)
        /* env, piped stdout/stderr — same setup as run() */
        .spawn()
        .map_err(/* BinaryNotFound / Io exactly as run() */)?;

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => break,                 // finished in time → collect output
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();               // SIGKILL the hung child
                    let _ = child.wait();               // reap to avoid a zombie
                    return Err(RunnerError::Timeout {
                        program: inv.program.clone(),
                        seconds: timeout.as_secs(),
                    });
                }
                std::thread::sleep(Duration::from_millis(POLL_GRAIN_MS)); // e.g. 25ms
            }
            Err(e) => return Err(RunnerError::Io { program: inv.program.clone(), source: e }),
        }
    }
    // collect piped stdout/stderr via child.wait_with_output() and build CmdOutput
    // (same fields as run(): status, stdout, stderr, argv).
}
```

- **Why poll-`try_wait` and not a second thread per call:** the watchdog runs INLINE on the worker thread (the worker is already the only thread that should block on this call). The loop's `sleep(25ms)` grain bounds the kill latency to ~one grain past the deadline; total CPU cost is negligible. No extra thread, no channel, no shared state. This is the minimal seam.
- **Default duration:** `STATS_TIMEOUT = Duration::from_secs(3)` and `LIST_TIMEOUT = Duration::from_secs(5)` (constants in the effect module). `stats` is the most likely to hang (a wedged engine socket), hence the tighter bound. Both well above a healthy call's latency.
- **Behavior on timeout = an ERROR Message, NOT a hang.** The effect handler maps `Err(RunnerError::Timeout{..})` → `CboxError` → the relevant completion Message carrying the error. The reducer's silent path swallows it quietly (no toast, no busy clear of a busy that was never set); the stats path renders graceful degradation (§5.4). The TUI keeps drawing every frame.

**Which paths use the timeout (and which DON'T):** ONLY the two new silent/poll effects call `run_with_timeout` — `Effect::SilentLoadList` (list auto-refresh) and `Effect::StatsPoll` (sparklines). Every existing effect (`LoadList`, `Create`, `Rm`, `Stop`, `Apply`, `Up`, `Doctor`, inspect) keeps calling `run`/`run_interactive` UNCHANGED. This containment is deliberate: it shrinks GAP-1's blast radius to the two new periodic paths and leaves the manual, user-initiated operations exactly as they are (a user pressing `c`/`d` accepts a blocking op with a spinner). The trait addition is additive and defaulted, so it compiles against every existing impl.

### 2.2 Silent-poll gating (never set busy; coalesce in-flight)

**The busy trap (verified):** manual `r` sets `model.busy = true` (`update.rs:362-365`); `model.busy` blocks ALL keys (`update.rs:78-80`). A background poll MUST NOT set `busy`, or every ~2s the UI would freeze input.

**Design:**

- New `Model` fields: `last_poll_tick: usize` and `poll_in_flight: bool` (§3.1).
- The `Message::Tick` handler (`update.rs:35-46`) gains a poll decision AFTER advancing the spinner and expiring toasts:
  ```
  // pure helper — testable without a clock
  pub fn should_poll(model: &Model) -> Option<Effect> { ... }
  ```
  Returns `Some(silent effect)` IFF ALL of:
  1. `model.spinner_tick.wrapping_sub(model.last_poll_tick) >= POLL_INTERVAL_TICKS` (≈40 ticks ≈2s, `POLL_INTERVAL_TICKS` const), AND
  2. `!model.busy` (a user op is running — defer; we don't want to race a manual LoadList), AND
  3. `!model.poll_in_flight` (the previous silent effect hasn't completed — coalesce, don't pile).
  4. The chosen effect depends on screen: on `Detail` with a running box → `Effect::StatsPoll{...}`; otherwise → `Effect::SilentLoadList`. (On Detail we still want list freshness too, but stats is the higher-value poll there; the list refreshes when the user returns to List. Spec keeps ONE silent effect per poll tick to respect the in-flight guard — see §3.1 note.)
- When `should_poll` returns `Some(eff)`, the Tick handler:
  - sets `model.last_poll_tick = model.spinner_tick` (reset the counter), and
  - sets `model.poll_in_flight = true` (NOT `busy`), and
  - returns `vec![eff]`.
- The completion Messages for silent effects (`Message::SilentListLoaded`, `Message::StatsLoaded`) clear `model.poll_in_flight = false` and DO NOT touch `model.busy`. They never set a `StatusLine::Busy`, never push a toast (a silent refresh is invisible unless state actually changed). (§3.2)

**Why a counter and not wall-clock:** the reducer is pure over (Model, Message) with no clock (`update.rs` invariant). `spinner_tick` IS the clock (advances on every Tick, `update.rs:36`). Tying the poll to a tick delta keeps the reducer pure and the gate unit-testable by feeding N Ticks (AC-POLL-1/2/3).

**The no-freeze guarantee, stated precisely:** A background poll can never freeze the TUI because (a) it never sets `busy` (so keys are never blocked by it), and (b) its worst-case latency is bounded by `run_with_timeout` (≤ `STATS_TIMEOUT`/`LIST_TIMEOUT`), after which the worker returns an error Message and is free again. Even a permanently-wedged engine yields a steady stream of timeout errors every poll cycle, not a hang. (G-NOFREEZE, §9.)

---

## 3. Data-model design

All new types live in `src/tui/model.rs` (pure data) unless noted; ratatui-typed parts stay `#[cfg(feature = "tui")]`-gated as today.

### 3.1 Poll gating fields

```
// added to struct Model (model.rs:210-247)
pub last_poll_tick: usize,   // spinner_tick value at the last poll dispatch; init 0
pub poll_in_flight: bool,    // a silent effect is dispatched and not yet completed; init false
```
`Model::new` (`model.rs:250-279`) initializes both (`0`, `false`). Constants (in `src/tui/effect.rs` beside the timeouts, or a new `src/tui/poll.rs` always-compiled helper module):
```
pub const POLL_INTERVAL_TICKS: usize = 40;   // ~2s at POLL_MS=50 (app.rs:41)
pub const STATS_TIMEOUT_SECS: u64 = 3;
pub const LIST_TIMEOUT_SECS:  u64 = 5;
```
**`should_poll` lives in a pure, always-compiled module** (`src/tui/poll.rs`, NEW, no `tui` gate, no ratatui) so it compiles in the lean build and is unit-testable. It takes `&Model` and returns `Option<PollKind>` (a tiny pure enum: `PollKind::List | PollKind::Stats{ id, backend }`), NOT an `Effect` directly — keeps `poll.rs` free of the `Effect` type and lean-clean. The reducer maps `PollKind` → `Effect`. (Decision: returning a small pure enum avoids dragging `Effect`/spec types into the lean module; R-7.)

> **In-flight + dual-poll note:** because there is ONE in-flight guard and the worker is serial, only ONE silent effect is outstanding at a time. On Detail-with-running-box, `should_poll` prefers `Stats`; the list still gets refreshed whenever the user is on List (or returns to it and the next poll fires). This keeps the guard simple and avoids two silent effects racing the 4-deep channel. (Accepted limitation, documented; A-2.)

### 3.2 New Effects + Messages

**New `Effect` variants** (`src/tui/effect.rs:44-69`), all worker-handled, all using `run_with_timeout`:

```
/// Silent list refresh — like LoadList but the completion does NOT set busy
/// or status; only updates model.boxes if changed. Uses LIST_TIMEOUT.
SilentLoadList,
/// Per-box stats poll for the Detail screen. Uses STATS_TIMEOUT.
StatsPoll(StatsSpec),       // StatsSpec { id: String, backend: Backend }
/// Bulk fan-out is NOT a new Effect — see §3.4: it reuses Effect::Rm / Effect::Stop
/// with multi-name specs.
```
`execute_effect` (`effect.rs:77-131`) gains two arms:
- `Effect::SilentLoadList` → `core::list_all_with_timeout(backends, runner, LIST_TIMEOUT)` (a thin wrapper that calls `run_with_timeout`; see §3.5) → `Some(Message::SilentListLoaded(result))`.
- `Effect::StatsPoll(spec)` → `core::stats(&spec, runner, STATS_TIMEOUT)` → `Some(Message::StatsLoaded(result))`.

**New `Message` variants** (`src/tui/message.rs:49-68`):

```
SilentListLoaded(Result<Vec<BoxRow>, CboxError>),
StatsLoaded(Result<StatsSample, CboxError>),   // StatsSample { cpu_pct: f64, mem_used: u64, mem_limit: u64 }
```
Reducer handlers (`update.rs`):
- `handle_silent_list_loaded`: set `poll_in_flight = false`; on `Ok(rows)` update `model.boxes` ONLY (clamp selection exactly like `handle_list_loaded` `update.rs:837-849`; recompute filter if open `update.rs:851-854`). Do NOT set `model.status`, do NOT set `busy`, do NOT push a toast. On `Err` (incl. timeout): set `poll_in_flight = false` and return `vec![]` (swallow quietly — a transient refresh failure is invisible; the next poll retries). (AC-POLL-4.)
- `handle_stats_loaded`: set `poll_in_flight = false`; on `Ok(sample)` push the sample into the bounded history buffers (§3.6); on `Err` push NOTHING (history goes stale → renders as "no recent stats", §5.4). Never busy, never toast.

### 3.3 The `Action` enum (GAP-3 — single source for keymap/palette/cheatsheet)

**Problem (verified):** `KeyBinding.action` is a display-only `&'static str` (`keymap.rs:18-20`) with no link to reducer behavior. The palette needs a label→behavior map; the cheatsheet shows labels; the keymap binds keys. Three surfaces, no shared truth.

**Design — introduce `Action` in a new always-compiled module `src/tui/action.rs`** (no `tui` gate; pure; lean-clean):

```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    // Navigation / single-box (mirror existing List/Detail keys)
    MoveUp, MoveDown, Open, Inspect, Create, Stop, Destroy, Apply, Edit, Refresh,
    // Overlays / global
    Filter, Cheatsheet, Doctor, CycleSkin, CommandLog, Palette, Quit,
    // Bulk (Feature 4) — the four predicates
    BulkPruneStopped, BulkStopRunning, BulkDestroyManaged, BulkDestroyUnmanaged,
}

impl Action {
    /// Stable label shown in palette + cheatsheet (voice-compliant; AC-VOICE-1).
    pub fn label(&self) -> &'static str { ... }       // e.g. "filter", "stop all running"
    /// Whether this action is offered in the `:` palette (some keys, e.g. MoveUp,
    /// are not palette-worthy). Bulk + overlay + single-box ops are.
    pub fn in_palette(&self) -> bool { ... }
    /// The display key (if a direct keybinding exists), for the cheatsheet column.
    pub fn default_key(&self) -> Option<&'static str> { ... }   // e.g. Filter -> Some("/")
}

/// The ordered list of palette-offered actions (the palette's command source).
pub fn palette_actions() -> &'static [Action] { ... }   // every Action where in_palette()
```

**`KeyBinding` gains an `action: Action` link** (`keymap.rs:15-20`). The existing display string is DERIVED from `Action::label()` so the keymap, palette, and cheatsheet cannot drift:
```
pub struct KeyBinding {
    pub key: &'static str,
    pub action: Action,          // was &'static str — now the enum (single source)
}
```
The cheatsheet renders `kb.key` + `kb.action.label()`. `help_line` (`keymap.rs:241-247`) uses `kb.action.label()`. The palette renders `palette_actions()` mapped through `label()`.

> **Migration of `keymap.rs`:** every `KeyBinding{ key, action: "filter" }` becomes `KeyBinding{ key, action: Action::Filter }`. `AC-MAP-VOICE` (the existing voice assertion over action strings) is rewritten to iterate `Action::label()` across all variants (AC-VOICE-1). The status-bar `help_line` output is unchanged textually because `Action::label()` returns the same verb strings that are in the table today (e.g. `Action::Filter.label() == "filter"`). This keeps any string-shape assertion in `tests/tui_keymap.rs` green if the labels match the prior `action` strings — VERIFY each label equals the prior string (R-3).

**Reducer dispatch — `dispatch_action(model, action) -> Vec<Effect>`** (in `update.rs`). This is the single place an `Action` becomes behavior; both the palette (on Enter over a command) and (optionally) the keymap can route through it:

```
fn dispatch_action(model: &mut Model, action: Action) -> Vec<Effect> {
    match action {
        Action::Filter        => { open_filter(model); vec![] }
        Action::Cheatsheet    => { model.overlay = Overlay::Cheatsheet; vec![] }
        Action::CommandLog    => { model.overlay = Overlay::CommandLog{scroll:0}; vec![] }
        Action::CycleSkin     => { cycle_skin(model); vec![] }
        Action::Doctor        => start_doctor(model),
        Action::Refresh       => start_manual_refresh(model),   // sets busy=true, Effect::LoadList
        Action::Create        => { open_wizard(model); vec![] }
        Action::Stop          => stop_selected(model),
        Action::Destroy       => confirm_destroy_selected(model),
        Action::Apply         => apply_selected(model),
        Action::Edit          => edit_selected(model),
        Action::Inspect       => inspect_selected(model),
        Action::Open          => open_selected(model),
        Action::MoveUp        => { model.move_up();   vec![] }
        Action::MoveDown      => { model.move_down(); vec![] }
        Action::Palette       => { open_palette(model); vec![] }
        Action::Quit          => { model.should_quit = true; vec![Effect::Quit] }
        // Bulk: open the bulk-confirm modal pre-loaded with the filtered target set.
        Action::BulkPruneStopped     => open_bulk_confirm(model, BulkOp::PruneStopped),
        Action::BulkStopRunning      => open_bulk_confirm(model, BulkOp::StopRunning),
        Action::BulkDestroyManaged   => open_bulk_confirm(model, BulkOp::DestroyManaged),
        Action::BulkDestroyUnmanaged => open_bulk_confirm(model, BulkOp::DestroyUnmanaged),
    }
}
```

**Refactor strategy (keeps existing tests green):** the existing `handle_key_list` arms (`update.rs:253-402`) are the canonical behaviors. Extract each arm's body into a small named helper (`stop_selected`, `confirm_destroy_selected`, …) and have BOTH the key arm and `dispatch_action` call the helper. This is a mechanical extract-method refactor — the key path behavior is byte-identical, so all of `tests/tui_update.rs` stays green. `dispatch_action` is the NEW caller (palette). (R-2; the palette doesn't re-implement behavior — it routes through the same helpers.)

### 3.4 Bulk operations (GAP-2 — filters over `model.boxes`)

**New pure types** (`src/tui/model.rs`, pure; predicate logic in an always-compiled helper for testability):

```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulkOp { PruneStopped, StopRunning, DestroyManaged, DestroyUnmanaged }

pub struct BulkConfirmState {
    pub op: BulkOp,
    pub targets: Vec<String>,          // box NAMES selected by the predicate (display + fan-out)
    pub typed_confirm: String,         // buffer for the typed phrase (DestroyUnmanaged only)
}
```
New `Model` field: `pub bulk_confirm: Option<BulkConfirmState>` (`None` = no bulk modal). It is its own modal state (parallel to `confirm: Option<ConfirmState>`, `model.rs:218`), shown on a reused-screen modal (§6).

**The four predicates — a pure function over `&[BoxRow]`** (in a new always-compiled `src/tui/bulk.rs`, lean-clean, unit-testable without ratatui):

```
pub fn bulk_targets(op: BulkOp, boxes: &[BoxRow]) -> Vec<usize> {
    boxes.iter().enumerate().filter(|(_, b)| match op {
        BulkOp::PruneStopped     => !is_running(&b.status),
        BulkOp::StopRunning      =>  is_running(&b.status),
        BulkOp::DestroyManaged   =>  b.cbox_managed,            // spec.rs:183
        BulkOp::DestroyUnmanaged => !b.cbox_managed,
    }).map(|(i, _)| i).collect()
}
// is_running mirrors the existing reducer test (status contains "running"/"up",
// update.rs:265-266) — extract it as a shared pure helper so List nav, Detail,
// and bulk all agree.
```
Predicate sources are VERIFIED: `BoxRow.cbox_managed` from the `cbox.managed=true` label (`core/mod.rs:231-233`); `BoxRow.status` (`spec.rs:180`); `BoxRow.backend` per-row (`spec.rs:186`) so the fan-out can pin the right engine.

**Fan-out (reuses existing batch Effects — NO new effect):** on bulk confirm, the reducer builds ONE multi-name spec per backend present in the target set and emits the existing Effect. `Effect::Rm`/`Effect::Stop` already accept `names: Vec<String>` (`spec.rs:104,118`; `core::stop`/`core::rm` iterate the spec, `mod.rs:308,331`):

- `PruneStopped`  → `Effect::Rm(RmSpec{ names, force:true, rm_home:false, all:false, yes:true, backend })` per backend group.
- `StopRunning`   → `Effect::Stop(StopSpec{ names, all:false, backend })` per backend group.
- `DestroyManaged`/`DestroyUnmanaged` → `Effect::Rm(...)` per backend group.

Because boxes can span podman+docker, group `targets` by `BoxRow.backend` and emit one Effect per group (the reducer returns a `Vec<Effect>` already; the worker drains them serially). Reuse the existing Progress screen for the in-flight state (set `busy=true`, `Screen::Progress`), and the existing `RmDone`/`StopDone` completion handlers (`update.rs:921-962`) which already refresh via `LoadList`. (R-5: if the fan-out exceeds the 4-deep channel, see §9 — group-by-backend caps it at ≤2 effects in practice.)

### 3.5 Stats core (greenfield) + list-with-timeout wrapper

**`build_stats_argv`** beside `build_list_argv`/`build_inspect_argv` (`src/dbox/argv.rs:166-186`, same `Vec<String>` pattern):
```
/// `<backend> stats <id> --no-stream --format json` — an ENGINE call, not distrobox.
pub fn build_stats_argv(id: &str) -> Vec<String> {
    vec![
        "stats".to_string(),
        id.to_string(),
        "--no-stream".to_string(),
        "--format".to_string(),
        "json".to_string(),
    ]
}
```
**`core::stats`** (NEW, in `src/core/mod.rs`, beside `inspect` `mod.rs:368`; calls the ENGINE like inspect does, `Invocation::new(spec.backend.as_str(), …)` `mod.rs:373`):
```
pub fn stats(spec: &StatsSpec, runner: &dyn DistroboxRunner, timeout: Duration)
    -> Result<StatsSample, CboxError>
{
    let args = build_stats_argv(&spec.id);
    let inv = Invocation::new(spec.backend.as_str(), args, RunMode::Capture);
    let out = runner.run_with_timeout(inv, timeout)?;     // TIMEOUT-BOUNDED
    if out.status != 0 {
        return Err(CboxError::backend_error(out.status, &out.stderr, &out.argv));
    }
    parse_stats_json(&out.stdout)        // serde_json (already used, mod.rs:211)
}
```
`StatsSpec { id: String, backend: Backend }` and `StatsSample { cpu_pct: f64, mem_used: u64, mem_limit: u64 }` live in `src/core/spec.rs` (beside `InspectSpec`/`InspectResult`, `spec.rs:136-167`).

**`parse_stats_json` (fields + defensive parsing):** podman `stats --format json` emits an array of objects with `CPU` (e.g. `"1.23%"`) and `MemUsage` (e.g. `"12.3MB / 1.9GB"`) or numeric `mem_usage`/`mem_limit`; docker emits NDJSON / a JSON object with `CPUPerc` (`"1.23%"`) and `MemUsage` (`"12.3MiB / 1.9GiB"`). Parse defensively, mirroring `extract_str`/the NDJSON fallback (`mod.rs:202-247`):
- CPU: read `CPUPerc`/`CPU`/`cpu_percent`, strip a trailing `%`, parse `f64`; on failure → `0.0`.
- Mem: read `MemUsage`/`mem_usage` (used) and limit (`mem_limit` or the "used / limit" string's second half); parse a human size ("12.3MiB", "1.9GiB", "512MB") into bytes via a small pure parser; on failure → `0`.
- **Empty/`null`/`[]`/malformed → `Err(CboxError::backend_error(...))` OR `Ok(StatsSample::default())`** — spec chooses: malformed stdout that is non-empty but unparseable → `Err` (logged-silent, history goes stale); EMPTY/`null`/`[]` (a stopped box returns no rows) → `Err(CboxError::usage("no stats for box"))` so the reducer simply skips the push. EITHER way the reducer's `handle_stats_loaded` pushes nothing on `Err` (§3.2). No panic, no freeze. (AC-STATS-PARSE-1/2/3.)

**`core::list_all_with_timeout`** — a thin wrapper: identical to `list_all` (`mod.rs:116-137`) but `list_machine` uses `run_with_timeout(inv, timeout)` instead of `run(inv)`. Simplest impl: add a `timeout: Option<Duration>` param to an internal `list_machine_inner` and have both `list_all` (None → `run`) and `list_all_with_timeout` (Some → `run_with_timeout`) delegate. (Keeps the manual `r` path on the un-timed `run`; only the silent poll is timed. R-1 containment.)

### 3.6 Stats history buffers (bounded; keyed by the box being viewed)

```
// on Model (model.rs) — ratatui-free pure data
pub const STATS_HISTORY_CAP: usize = 60;   // ~2 min at one sample / ~2s poll
pub struct StatsHistory {
    pub box_id: String,             // which box these samples belong to
    pub cpu: VecDeque<u64>,         // CPU% × 100 or rounded — Sparkline takes u64
    pub mem_used: VecDeque<u64>,    // bytes (or MiB) — Sparkline takes u64
    pub mem_limit: u64,             // latest limit (for the gauge/label)
}
pub stats_history: Option<StatsHistory>,    // None until first sample / non-Detail
```
- **Update cadence = the poll cadence** (§3.1): each `Message::StatsLoaded(Ok(sample))` pushes onto `cpu`/`mem_used`, popping the front past `STATS_HISTORY_CAP` (same bounded-ring discipline as `CmdLog::push`, `cmdlog.rs:51-59`, and `push_toast`, `model.rs:358-375`). A pure `push_sample(&mut StatsHistory, sample)` helper (in `model.rs` or `bulk.rs`'s sibling) is unit-testable (AC-HIST-1).
- **Keyed by box:** when the user opens a NEW box's Detail (a different `box_id`), reset `stats_history` to a fresh buffer for that id (clear stale samples). When leaving Detail → `stats_history = None`. Keying prevents one box's sparkline bleeding into another's. (AC-HIST-2.)
- `Sparkline` consumes `&[u64]`; convert `VecDeque` → slice with `make_contiguous()` at render time.

---

## 4. Keymap / palette / cheatsheet

### 4.1 Updated List keymap (additions only; existing rows unchanged)

Existing List bindings (`keymap.rs:44-105`) stay. Add:

| key | `Action` | reducer site | conflict / note |
|-----|----------|--------------|-----------------|
| `:` | `Action::Palette` | NEW global (step 5, §4.3) | opens `Overlay::Palette`. `:` is UNBOUND everywhere — verified no `Char(':')` arm in `update.rs`. Mnemonic: vim-style command line. |
| `b` | (bulk entry) | NEW List arm | opens a small **bulk menu** (sub-overlay) listing the four bulk ops, OR is itself routed through the palette. **Spec decision (§4.4):** `b` opens `Overlay::Palette` PRE-FILTERED to the four bulk actions (query seeded `"bulk"` is fragile; instead seed the palette in a "bulk-only" mode). `b` is UNBOUND on List — verified no `Char('b')` arm. |

The four bulk ops are ALSO reachable from the plain `:` palette (they are `palette_actions()` entries). `b` is a fast-path that opens the palette scoped to bulk. **D-1 (low stakes):** if the maintainer prefers four direct keys over a menu, that's a one-line-per-key change — flagged; spec recommends the palette/`b` approach to avoid burning four scarce single-letter keys and to demonstrate the palette.

### 4.2 Palette overlay state (`Overlay::Palette`)

Extend the existing `Overlay` enum (`model.rs:167-175`) — it already holds a stateful variant (`CommandLog{scroll}`):
```
pub enum Overlay {
    None,
    Cheatsheet,
    CommandLog { scroll: usize },
    Palette {
        query: String,
        matches: Vec<usize>,    // indices into the palette's action source, best-rank first
        cursor: usize,          // selection within matches
        bulk_only: bool,        // true when opened via `b` (scopes the source to bulk actions)
    },
}
```
- The palette's command SOURCE is `action::palette_actions()` (or the bulk subset when `bulk_only`). The fuzzy ranking REUSES `fuzzy_rank(query, &labels)` (`filter.rs:26`) over `Action::label()` strings — the exact same matcher as the box filter, no new dep. `FilterState` (`model.rs:137-145`) is the working template for query+matches+cursor (the palette mirrors it).
- Open: `model.overlay = Overlay::Palette{ query:"", matches:(0..n).collect(), cursor:0, bulk_only }`.
- Type/Backspace: push/pop into `query`, recompute `matches = fuzzy_rank(query, &labels)`, clamp `cursor` — identical mechanics to `recompute_filter` (`update.rs:193-224`).
- Enter: take `actions[matches[cursor]]`, set `overlay = None`, then `dispatch_action(model, action)` (§3.3) and return its effects.
- Up/Down: move `cursor` within `matches`. Esc: `overlay = None` (cancel, no action).

### 4.3 Key-handling precedence (reducer — the additions)

`handle_key` (`update.rs:70-127`) already has the overlay pre-check (step 4) and global keys (step 5). Additions:
- **Step 4 (overlay pre-check)** gains a `Overlay::Palette{..}` arm → `handle_key_palette(model, key, …)` (intercepts ALL keys for palette input/nav, exactly like the filter handler `update.rs:131-189`). Esc/Enter close per §4.2.
- **Step 5 (global keys)** gains `Key::Char(':')` → `dispatch_action(model, Action::Palette)` (open palette, `bulk_only:false`). `:` is global (reachable on any non-busy, non-overlay screen) like `?`/`t`.
- **List arm** (`handle_key_list`, `update.rs:253`) gains `Key::Char('b')` → open palette with `bulk_only:true`.
- **`bulk_confirm` pre-check:** when `model.bulk_confirm.is_some()`, intercept keys in a dedicated `handle_key_bulk_confirm` BEFORE screen dispatch (parallel to the filter pre-check) so the typed-confirm input and y/n/Esc are captured (§6). This pre-check sits alongside the filter check (step 3) — i.e. `if model.bulk_confirm.is_some() { return handle_key_bulk_confirm(...) }`.

### 4.4 Conflicts resolved (explicit log)

- `:` — UNBOUND everywhere (no `Char(':')` in `update.rs`); global palette open. No conflict.
- `b` — UNBOUND on List (no `Char('b')` arm in `handle_key_list`, `update.rs:253-402`); opens bulk-scoped palette. No conflict.
- Palette `j`/`k` vs typed query: SAME resolution as the box filter (R-8 in Bundle 1) — inside the palette, `j`/`k` are TEXT typed into the query; nav within matches is `↑`/`↓` ONLY. Consistent with `handle_key_filter` (`update.rs:177-186`).
- Bulk-confirm typed phrase: while `bulk_confirm` is the DestroyUnmanaged op, ALL chars feed the `typed_confirm` buffer; only an exact phrase match + Enter executes (§6). `y` is NOT a shortcut for the dangerous op (it would be a single keypress — forbidden by the lock). For the other three bulk ops, `y`/Enter confirms and `n`/Esc cancels (single-keypress is acceptable for non-dangerous ops).

### 4.5 Cheatsheet reflects new actions

The cheatsheet renders `keymap_for(ctx)` (`keymap.rs:224`). Because `:`, `b` are added to `KEYMAP_LIST` with `Action::Palette`/(bulk entry) and the cheatsheet shows `kb.action.label()`, they appear automatically. The palette overlay also gets a `KeyContext::Palette` row set (type/↑↓/enter/esc) added to `keymap_for` and the `KeyContext` enum (`keymap.rs:27-40`), mirroring `FilterInput`. (AC-CHEAT-3.)

---

## 5. Sparklines (Detail screen)

### 5.1 Wiring

- When the user opens Detail on a RUNNING box (`handle_key_list` Enter/`i` path that sets `Screen::Detail`, `update.rs:283-302`, and the Detail Enter `update.rs:431-450`), the reducer ALSO initializes `model.stats_history = Some(StatsHistory::new(box_id))` for that box. (Stopped boxes get NO history — §5.4.)
- The periodic `should_poll` (§3.1), while on Detail with a running box, emits `Effect::StatsPoll(StatsSpec{ id, backend })` using `BoxRow.id` + `BoxRow.backend` (`spec.rs:184,186`). `StatsLoaded` pushes the sample.

### 5.2 `core::stats` + `build_stats_argv` — see §3.5 (verified engine-call pattern).

### 5.3 Detail-screen layout split

`render_detail` (`view.rs:260-354`) currently builds `Vec<Line>` → ONE `Paragraph` on the full `area` (`view.rs:350-353`). Split `area` with a `Layout` (already imported, per ATLAS `view.rs:8`):
- Top region: the existing detail `Paragraph` (name/status/image/… unchanged).
- Bottom region (only when `stats_history.is_some()` and it has ≥1 sample): two stacked `Sparkline`s — one CPU (`theme.accent` or a success-tinted style), one mem — each with a one-line label `CPU 12%` / `mem 240MiB / 1.9GiB`. `Sparkline::default().data(history.cpu.make_contiguous()).max(10000)` (CPU%×100 scale) and similar for mem (`.max(mem_limit)`).
- The split is conditional: if no stats region (stopped box or no samples yet), the detail `Paragraph` keeps the full `area` exactly as today (zero visual change for stopped boxes).

### 5.4 Graceful degradation (no crash, no freeze)

| Situation | Behavior |
|-----------|----------|
| Stopped box opened in Detail | `stats_history = None`; NO `StatsPoll` emitted; render = today's detail-only layout. |
| Engine returns empty/`null`/`[]` (no stats) | `core::stats` → `Err` → `handle_stats_loaded` pushes nothing; history stays as-is (or empty). Render shows the detail; if history empty → no sparkline region (no panic). |
| Malformed stats JSON | `parse_stats_json` → `Err`; same as above (push nothing). |
| `stats` call hangs | `run_with_timeout` kills it at `STATS_TIMEOUT` (3s) → timeout `Err` → push nothing, `poll_in_flight` cleared, TUI never froze. |
| First sample not yet arrived | `cpu`/`mem_used` empty → render detail-only (sparkline appears once ≥1 sample lands). |

---

## 6. Bulk ops — confirm UX + the dangerous guard

### 6.1 Opening a bulk op

`dispatch_action` (§3.3) for any `Action::Bulk*` calls `open_bulk_confirm(model, op)`:
1. `targets = bulk_targets(op, &model.boxes)` (§3.4) → collect the NAMES.
2. If `targets` empty → push an Info toast (`"Nothing to <verb>."`, voice-compliant) and do NOT open the modal (`bulk_confirm = None`). (AC-BULK-EMPTY.)
3. Else `model.bulk_confirm = Some(BulkConfirmState{ op, targets, typed_confirm:"" })` and show the modal (reuse the ConfirmDestroy centered-modal render pattern, `view.rs` `render_*` confirm; the bulk modal is its own render fn keyed off `model.bulk_confirm`, drawn over the current screen with `Clear` + centered `Rect`).

### 6.2 The confirm modal (shows the target list)

- Title per op: ` prune stopped `, ` stop all running `, ` destroy cbox-managed `, ` destroy NON-managed `.
- Body: the EXPLICIT list of affected box names (the `targets`), so the user sees exactly what's hit. Count line: `N boxes`.
- Footer for the THREE non-dangerous ops: `y / enter  confirm   ·   n / esc  cancel`.

### 6.3 The dangerous op (DestroyUnmanaged) — strongest confirm

`BulkOp::DestroyUnmanaged` destroys boxes cbox did NOT create (other people's containers). Per the lock, it MUST require a typed/extra confirmation, NOT a single keypress:
- The modal shows the target list PLUS a typed-phrase prompt: `Type DESTROY UNMANAGED to confirm:` and an input line echoing `typed_confirm`.
- `handle_key_bulk_confirm`: while `op == DestroyUnmanaged`, ALL `Char(c)`/`Backspace` feed `typed_confirm`; `Enter` executes ONLY IF `typed_confirm == "DESTROY UNMANAGED"` (exact, case-sensitive — chosen for unmistakable intent). A wrong phrase + Enter does nothing (stays in the modal). `Esc` cancels. (AC-BULK-DANGER-1/2.)
- The phrase is a `strings` const (`strings::BULK_UNMANAGED_PHRASE = "DESTROY UNMANAGED"`) so the test asserts the same value the UI shows.

### 6.4 Execute (fan-out) — §3.4

On confirm: build the multi-name spec(s) grouped by `BoxRow.backend`, set `Screen::Progress` + `busy=true` + a Busy status (`"Clearing out N boxes…"` etc.), `model.bulk_confirm = None`, return the `Vec<Effect>` (one Rm/Stop per backend group). Existing `RmDone`/`StopDone` handlers refresh the list. (AC-BULK-FANOUT-1.)

### 6.5 Copy (show don't tell, honest)

New `strings` consts (all added to `AC-COPY-1` + `AC-VOICE-1`), e.g.:
- `BULK_PRUNE_TITLE = " prune stopped "`, `BULK_STOP_TITLE = " stop all running "`, `BULK_DESTROY_MANAGED_TITLE = " destroy cbox-managed "`, `BULK_DESTROY_UNMANAGED_TITLE = " destroy NON-managed "`.
- `BULK_UNMANAGED_WARN = "These boxes were not packed by cbox. Destroying them is permanent."` (states the truth; no adjectives).
- `BULK_UNMANAGED_PHRASE = "DESTROY UNMANAGED"`.
- `BULK_EMPTY = "Nothing to do — no boxes match."`
Verbs only, no banned substrings.

---

## 7. Decomposition + sequencing (Vivi-sized, dependency-ordered)

ATLAS's recommended order is honored: (i) timeout+gating foundation → (ii) list auto-refresh → (iii) palette+Action → (iv) bulk → (v) sparklines. All tasks containerized; verifier is a `make` target. `T` = Vivi; `K` = Kupo.

| id | title | files | depends | timebox | verifier |
|----|-------|-------|---------|---------|----------|
| **T1** | **Capture timeout seam (GAP-1 core).** Add `run_with_timeout` (defaulted) to `DistroboxRunner`; implement the `try_wait` watchdog in `RealRunner`; add `RunnerError::Timeout` + `exit_code` mapping; `core::list_all_with_timeout` + thread the timeout through `list_machine_inner`. Timeouts as consts. | `src/dbox/runner.rs`, `src/dbox/real.rs`, `src/core/mod.rs`, `tests/tui_bundle2.rs` (new) | — | ≤3d | `make build; make lint-lean; make test` (hung-MockRunner timeout test) |
| **T2** | **Silent-poll gating (GAP-1 gate).** `poll.rs` (`PollKind`, `should_poll` pure); `Model.last_poll_tick`/`poll_in_flight`; `Effect::SilentLoadList`; `Message::SilentListLoaded`; Tick-handler poll dispatch + completion handler (never busy, never toast). | `src/tui/poll.rs` (new), `src/tui/model.rs`, `src/tui/effect.rs`, `src/tui/message.rs`, `src/tui/update.rs`, `tests/tui_bundle2.rs` | T1 | ≤3d | `make build; make lint-lean; make test` (AC-POLL-1..4) |
| **T3** | **`Action` enum (GAP-3) + keymap migration + reducer dispatch.** `action.rs` (`Action`, `label`, `in_palette`, `default_key`, `palette_actions`); migrate `KeyBinding.action` to `Action`; extract `handle_key_list` arm bodies into helpers; add `dispatch_action`. | `src/tui/action.rs` (new), `src/tui/keymap.rs`, `src/tui/update.rs`, `src/tui/mod.rs`, `tests/tui_keymap.rs`, `tests/tui_bundle2.rs` | — (independent of T1/T2) | ≤3d | `make build; make lint-lean; make test` (AC-ACTION-1..3, all `tui_update` green) |
| **T4** | **`:` command palette.** `Overlay::Palette{..}` variant; `KeyContext::Palette` + keymap rows; `handle_key_palette` (type/nav/enter→dispatch_action/esc); `:` global + `b` bulk-scoped open; `render_palette` overlay; voice-compliant labels. | `src/tui/model.rs`, `src/tui/keymap.rs`, `src/tui/update.rs`, `src/tui/view.rs`, `src/tui/strings.rs`, `tests/tui_bundle2.rs` | T3 | ≤3d | `make build; make test` (AC-PALETTE-1..4) |
| **T5** | **Bulk operations.** `bulk.rs` (`BulkOp`, `bulk_targets`, shared `is_running`); `BulkConfirmState` + `Model.bulk_confirm`; `open_bulk_confirm`; `handle_key_bulk_confirm` (typed-phrase guard for unmanaged); fan-out grouped by backend over existing Rm/Stop; `render_bulk_confirm`; copy consts. Wire the four `Action::Bulk*` into `dispatch_action`. | `src/tui/bulk.rs` (new), `src/tui/model.rs`, `src/tui/update.rs`, `src/tui/view.rs`, `src/tui/strings.rs`, `tests/tui_bundle2.rs` | T3, T4 | ≤3d | `make build; make lint-lean; make test` (AC-BULK-*) |
| **T6** | **Live CPU/mem sparklines.** `build_stats_argv`; `core::stats` + `StatsSpec`/`StatsSample` + `parse_stats_json` (defensive); `Effect::StatsPoll` + `Message::StatsLoaded`; `StatsHistory` bounded buffers + `push_sample`; Detail-on-running init + leave-Detail reset; `should_poll` Stats branch; `render_detail` Layout split + two `Sparkline`s; graceful degradation. | `src/dbox/argv.rs`, `src/core/mod.rs`, `src/core/spec.rs`, `src/tui/model.rs`, `src/tui/poll.rs`, `src/tui/effect.rs`, `src/tui/message.rs`, `src/tui/update.rs`, `src/tui/view.rs`, `tests/tui_bundle2.rs` | T1, T2 | ≤5d | `make build; make lint-lean; make test` (AC-STATS-*, AC-HIST-*) |
| **K1** | **Add new `strings` consts to the `AC-COPY-1` coverage** (the bulk + palette copy) and add `AC-VOICE-1` over `Action::label()`. Mechanical ≤2-file test edit once the consts exist. | `tests/tui_theme.rs` (or `tests/tui_bundle2.rs`), `src/tui/strings.rs` | T4, T5 | 1d | `make test` |
| **FINAL** | full gate | — | all | — | `make check` (fmt-check + lint + lint-lean + build + release + test) |

- **Kupo micro-task (flagged):** **K1** is a localized, mechanical ≤2-file change (extend the banned-substring coverage to the new consts + add the `Action::label()` voice loop). No design. Ideal Kupo scope; lands AFTER the consts/labels exist (T4/T5).
- **Sequencing rationale:** T1 (timeout) is the foundation everything periodic needs — lands first. T2 (gating) depends on T1 (the silent effect uses the timeout). T3 (`Action`) is independent (no timeout dependency) and can run in parallel with T1/T2; it precedes T4 (palette dispatches actions) and T5 (bulk actions). T4 precedes T5 only because the bulk fast-path opens the palette; if `b` opened a direct menu instead (D-1), T5 would not depend on T4. T6 (sparklines, highest new infra) depends on BOTH the timeout (T1) and the gating (T2). Three self-consistency passes (by-GAP / by-file / by-dependency) converge on this shape; the only interchange is T3↔T1/T2 ordering (siblings).

---

## 8. Acceptance criteria (GIVEN/WHEN/THEN — pure helpers, no terminal)

New file **`tests/tui_bundle2.rs`**; extend **`tests/tui_keymap.rs`** (Action), **`tests/tui_update.rs`** (dispatch + poll handlers stay green), **`tests/tui_theme.rs`** (copy/voice). Existing `tui_update`, `tui_effects`, `tui_theme`, `tui_bundle1`, `tui_keymap` and the CLI suites MUST stay green.

### Timeout (GAP-1 — load-bearing)

- **AC-TIMEOUT-1 (hung mock → error within the deadline):** GIVEN a `SlowMockRunner` whose `run`/`run_with_timeout` sleeps longer than the timeout (e.g. via a programmable delay), wrapped so `run_with_timeout(inv, Duration::from_millis(200))` is called, WHEN invoked THEN it returns `Err(RunnerError::Timeout{..})` in ≤ ~deadline + one poll grain (assert it completes and the elapsed time < a generous bound, e.g. 2s, proving it did NOT hang). *Test seam:* either (a) a new mock type implementing `DistroboxRunner` whose `run` blocks on a `recv` that never fires, and a `run_with_timeout` that spawns it under a watchdog, OR (b) drive `RealRunner::run_with_timeout` against a real `sleep`/`cat` style hung command in the container (CI has a shell). Spec recommends (a) a pure `HangingRunner` so the test needs no real process and runs in the lean build. The `HangingRunner` lives in the test file.
- **AC-TIMEOUT-2 (default delegates):** GIVEN a plain `MockRunner` (no override) WHEN `run_with_timeout(inv, t)` is called THEN it returns the same `Ok(CmdOutput)` as `run(inv)` (default impl delegates) — proving existing mocks are unaffected.
- **AC-TIMEOUT-3 (exit mapping):** `RunnerError::Timeout{..}.exit_code() == exit::TEMPFAIL`.

### Poll gating (GAP-1 — never busy, coalesced)

- **AC-POLL-1 (fires at the interval):** GIVEN a fresh model on List, `busy=false`, `poll_in_flight=false`, `spinner_tick=0`, `last_poll_tick=0` WHEN `POLL_INTERVAL_TICKS` Tick messages are applied THEN the last Tick returns `vec![Effect::SilentLoadList]`, `model.poll_in_flight==true`, `model.busy==false` (NEVER set), `model.last_poll_tick` reset to the current tick.
- **AC-POLL-2 (skips while busy):** GIVEN `model.busy=true` WHEN `2*POLL_INTERVAL_TICKS` Ticks applied THEN `should_poll` returns `None` every time (no silent effect) — a manual op is never raced.
- **AC-POLL-3 (coalesces in-flight):** GIVEN `model.poll_in_flight=true` WHEN the interval elapses THEN `should_poll` returns `None` (no second silent effect piles into the channel).
- **AC-POLL-4 (silent completion is invisible):** GIVEN `poll_in_flight=true`, a non-Busy `status` WHEN `Message::SilentListLoaded(Ok(rows))` THEN `model.boxes==rows`, `model.poll_in_flight==false`, `model.busy` UNCHANGED (false), `model.status` UNCHANGED (NOT set to Ok), `model.toasts` UNCHANGED (no toast). WHEN `Message::SilentListLoaded(Err(timeout))` THEN `poll_in_flight==false` and nothing else changes.
- **AC-POLL-5 (selection survives silent refresh):** GIVEN a filter open / a selection WHEN `SilentListLoaded(Ok(new_rows))` THEN selection is clamped and the filter recomputed (no out-of-range `selected`, no panic) — mirrors `handle_list_loaded` clamping.

### `Action` (GAP-3)

- **AC-ACTION-1 (single source — keymap references Action):** every `KeyBinding` in every `keymap_for(ctx)` carries an `Action` whose `label()` equals the displayed verb; `KEYMAP_LIST` contains `Action::Filter`, `Cheatsheet`, `Doctor`, `CycleSkin`, `CommandLog`, `Palette`, `Create`, `Stop`, `Destroy`, `Apply`, `Edit`, `Refresh`, `Quit`.
- **AC-ACTION-2 (palette source):** `palette_actions()` contains all four `Bulk*` actions and the overlay/single-box actions, and does NOT contain `MoveUp`/`MoveDown` (per `in_palette()`).
- **AC-ACTION-3 (dispatch maps to expected effect/behavior):** for a representative set: `dispatch_action(m, Action::Refresh)` sets `busy=true` and returns `[Effect::LoadList]`; `dispatch_action(m, Action::Cheatsheet)` sets `overlay==Cheatsheet` and `[]`; `dispatch_action(m, Action::Quit)` sets `should_quit` and returns `[Effect::Quit]`; `dispatch_action(m, Action::BulkStopRunning)` with a known box set opens `bulk_confirm` with the running-box targets.

### Palette

- **AC-PALETTE-1 (open/close):** GIVEN List WHEN `Key::Char(':')` THEN `overlay == Overlay::Palette{ bulk_only:false, .. }`. WHEN `Esc` THEN `overlay == None` and NO action ran.
- **AC-PALETTE-2 (fuzzy order over labels):** GIVEN the palette open WHEN query `"stop"` typed THEN `matches` (mapped through labels) contains `Action::Stop`/`Action::BulkStopRunning` ranked ahead of unrelated actions; reuse `fuzzy_rank` semantics (assert membership + relative order, like AC-FILTER-1).
- **AC-PALETTE-3 (enter dispatches):** GIVEN the palette with `cursor` on `Action::Cheatsheet` WHEN `Enter` THEN `overlay` becomes `Cheatsheet` (the action ran via `dispatch_action`) — proves the palette routes through the single dispatch.
- **AC-PALETTE-4 (`b` opens bulk-scoped):** GIVEN List WHEN `Key::Char('b')` THEN `overlay == Overlay::Palette{ bulk_only:true, .. }` and `matches` only index the four bulk actions.

### Bulk filters + confirm

- **AC-BULK-FILTER-1 (predicates select the right subset):** GIVEN boxes `[ {a, running, managed}, {b, stopped, managed}, {c, running, unmanaged}, {d, stopped, unmanaged} ]`: `bulk_targets(PruneStopped, …)` → `{b,d}`; `bulk_targets(StopRunning, …)` → `{a,c}`; `bulk_targets(DestroyManaged, …)` → `{a,b}`; `bulk_targets(DestroyUnmanaged, …)` → `{c,d}`.
- **AC-BULK-EMPTY (no targets → no modal):** GIVEN a box set with zero stopped boxes WHEN `Action::BulkPruneStopped` THEN `bulk_confirm == None` AND an Info toast pushed.
- **AC-BULK-FANOUT-1 (reuses Rm/Stop, grouped by backend):** GIVEN targets spanning podman+docker WHEN the bulk confirm fires THEN the returned effects are `Effect::Rm`/`Effect::Stop` with `names` = the target names, one effect per backend group; `busy==true`, `screen==Progress`.
- **AC-BULK-DANGER-1 (typed phrase required):** GIVEN `bulk_confirm` for `DestroyUnmanaged` WHEN `Key::Char('y')` or `Enter` with `typed_confirm != "DESTROY UNMANAGED"` THEN NO effect emitted, modal stays open. WHEN the exact phrase is typed and Enter pressed THEN the fan-out effects are returned and `bulk_confirm == None`.
- **AC-BULK-DANGER-2 (single keypress never destroys unmanaged):** GIVEN the unmanaged modal WHEN any single `Char` is pressed THEN it only appends to `typed_confirm` (no destroy) — proves no single-keypress path exists for the dangerous op.
- **AC-BULK-CONFIRM-NONDANGEROUS:** GIVEN `bulk_confirm` for `StopRunning` WHEN `Key::Char('y')`/`Enter` THEN fan-out runs; WHEN `n`/`Esc` THEN `bulk_confirm==None`, no effect.

### Sparklines / stats

- **AC-STATS-ARGV-1 (per backend):** `build_stats_argv("abc123") == ["stats","abc123","--no-stream","--format","json"]`; `core::stats` builds the invocation with `program == backend.as_str()` ("podman"/"docker") — assert via a recording MockRunner's `calls()`.
- **AC-STATS-PARSE-1 (valid):** GIVEN a podman-shaped stats JSON with `CPU:"12.5%"`, `MemUsage:"240MiB / 1.9GiB"` WHEN `parse_stats_json` THEN `cpu_pct≈12.5`, `mem_used≈240*1024*1024` (within tolerance), `mem_limit≈1.9*1024^3`.
- **AC-STATS-PARSE-2 (docker-shaped):** GIVEN `CPUPerc:"3.1%"`, `MemUsage:"12MiB / 512MiB"` THEN parses analogously.
- **AC-STATS-PARSE-3 (malformed/empty → graceful Err, no panic):** `parse_stats_json("")`, `("null")`, `("not json")`, `("[]")` each return `Err` (no panic); `handle_stats_loaded(Err)` pushes nothing and clears `poll_in_flight`.
- **AC-HIST-1 (bounded ring drops oldest):** GIVEN `StatsHistory::new("x")` with `STATS_HISTORY_CAP` WHEN `cap+5` samples pushed THEN `cpu.len()==cap` and the newest sample is at the back.
- **AC-HIST-2 (keyed by box / reset on box change):** GIVEN history for box "x" WHEN Detail opens for box "y" THEN history resets to `box_id=="y"` empty buffers (no stale "x" samples). WHEN leaving Detail THEN `stats_history == None`.
- **AC-STATS-STOPPED (no poll for stopped):** GIVEN Detail on a stopped box WHEN `should_poll` evaluated THEN it does NOT return a `Stats` poll (returns `List` or `None` per gating) — no `StatsPoll` for a stopped box.

### Copy / voice

- **AC-COPY-1 (extended):** every NEW public `strings` const (bulk titles, warn, phrase, empty) is non-empty and free of the `BANNED` adjectives (`tests/tui_theme.rs:351`).
- **AC-VOICE-1 (Action labels):** every `Action::label()` across all variants is non-empty and free of the BANNED adjectives.

---

## 9. Risk register + validation gates

### 9.1 Risk register

| id | risk | blast radius | severity | mitigation / gate |
|----|------|--------------|----------|-------------------|
| **R-1** | The timeout change touches the shared Capture path used by the WHOLE CLI → an existing CLI op regresses or a real op gets killed prematurely. | `runner.rs`, `real.rs`, `core/mod.rs`; every CLI subcommand. | **P0** | `run_with_timeout` is ADDITIVE + DEFAULTED; ONLY the two new silent effects call it. Every existing effect/CLI path keeps calling `run`/`run_interactive` UNCHANGED. `RunnerError::Timeout` maps to TEMPFAIL but is never produced on the un-timed paths. Gate: `make test` (full CLI suites green) + `make lint-lean` + AC-TIMEOUT-1/2/3. |
| **R-2** | The `Action` refactor (extract-method on `handle_key_list`) changes key behavior → `tests/tui_update.rs` regresses. | `update.rs`; `tests/tui_update.rs`. | **P0** | Pure extract-method: key arms call the SAME extracted helpers `dispatch_action` calls; no behavior change. Gate: every existing `tui_update` assertion green after T3 + AC-ACTION-3. |
| **R-3** | Migrating `KeyBinding.action` from `&str` to `Action` breaks `tests/tui_keymap.rs` (which may assert action strings). | `keymap.rs`; `tests/tui_keymap.rs`. | **P1** | `Action::label()` returns the SAME verb strings the table holds today; assertions read `.action.label()`. VERIFY each label == prior string. Gate: `tui_keymap` green + AC-ACTION-1. |
| **R-4** | Silent refresh races user input / overlays / filter / selection → flicker, lost selection, or a desynced filter. | `update.rs` poll + completion handlers. | **P1** | Poll is skipped while `busy` (AC-POLL-2) and coalesced while in-flight (AC-POLL-3); the completion clamps selection + recomputes filter exactly like `handle_list_loaded` (AC-POLL-5). The poll never opens/closes an overlay. Gate: AC-POLL-4/5 + `tui_update` green. |
| **R-5** | Bulk fan-out overflows the 4-deep effect channel (`try_send` silently DROPS, `app.rs:316,122`) → some boxes not acted on. | `app.rs` channel; bulk reducer. | **P1** | Group targets BY BACKEND → ≤2 effects (podman, docker) per bulk op, well under the 4-deep bound. (The reducer returns a `Vec<Effect>`; the shell routes each via `try_send` — 2 ≤ 4.) Gate: AC-BULK-FANOUT-1 (asserts one effect per backend group). |
| **R-6** | The dangerous bulk op (DestroyUnmanaged) is reachable via a single keypress → catastrophic data loss on other people's containers. | `update.rs` bulk handler; the user's non-cbox containers. | **P0** | Typed-phrase guard: ONLY `typed_confirm == BULK_UNMANAGED_PHRASE` + Enter executes; `y`/single keys only append to the buffer (AC-BULK-DANGER-1/2). The phrase is a const the test asserts. Gate: AC-BULK-DANGER-1/2. |
| **R-7** | `nucleo-matcher`, ratatui, `Effect`, or spec types leak into a lean (`make lint-lean`) build via the new always-compiled modules (`poll.rs`, `action.rs`, `bulk.rs`). | `Cargo.toml`, the new modules; lean build. | **P1** | `poll.rs` returns a pure `PollKind` enum (no `Effect`); `action.rs`/`bulk.rs` import only pure spec types (`BoxRow`, `Backend`). `fuzzy_rank` (palette) stays `tui`-gated (the palette is tui-only). Gate: `make lint-lean` MUST pass in T1/T2/T3/T5/T6. |
| **R-8** | Stats poll fires for a box on the wrong backend / wrong id → empty sparkline silently. | `update.rs` Detail init; `core::stats`. | **P2** | Use `BoxRow.id` + `BoxRow.backend` from the selected/detail row (`spec.rs:184,186`); `core::stats` pins `program=backend.as_str()`. AC-STATS-ARGV-1 asserts the program+argv. Graceful (no crash) if wrong. |
| **R-9** | `child.kill()` on timeout leaves a zombie or the worker still blocks collecting output. | `real.rs` watchdog. | **P1** | After `kill()`, call `child.wait()` to reap; the watchdog loop owns the child entirely on the worker thread; `wait_with_output` only on the success path. Gate: AC-TIMEOUT-1 (proves return, not hang) + `make build`. |
| **R-10** | `:` / `b` collide with a future per-box action or a typed query. | `update.rs` precedence. | **P2** | `:`/`b` verified UNBOUND (no `Char(':')`/`Char('b')` arms in `update.rs`); inside the palette they're text (consistent with the filter's R-8). Gate: AC-PALETTE-1/4. |
| **R-11** | Stats poll vs list poll contend for the single in-flight slot → list goes stale on Detail (or vice versa). | poll gating. | **P2** | Accepted, documented (§3.1 note): one silent effect at a time; Detail prefers Stats, List refreshes on List. The user sees fresh stats on Detail and a fresh list on List. No correctness issue. Gate: AC-STATS-STOPPED + AC-POLL-1. |

### 9.2 Validation gates (containerized)

- **G-BUILD:** `make build` (tui on) — all new modules + the trait method + watchdog compile.
- **G-LEAN:** `make lint-lean` — the no-tui build stays clean; `poll.rs`/`action.rs`/`bulk.rs`/the timeout seam are lean-clean; `nucleo-matcher`/ratatui stay tui-gated (R-7).
- **G-FMT:** `make fmt-check`. **G-LINT:** `make lint` (clippy).
- **G-TEST:** `make test` — ALL existing suites green (`tui_update`, `tui_effects`, `tui_theme`, `tui_bundle1`, `tui_keymap`, CLI suites) PLUS new `tui_bundle2` and extended `tui_keymap`/`tui_theme`/`tui_update`.
- **G-NOFREEZE (the no-freeze guarantee):** AC-TIMEOUT-1 (a hung command returns an error within the deadline, proving the worker frees itself) + AC-POLL-1/2/3 (the poll never sets `busy`, never piles). Together these are the automated proof the background work can't freeze the TUI. Optional containerized smoke: run the binary, open a running box's Detail, and confirm input stays live while stats poll runs (and stays live even if the engine socket is stalled).
- **G-DANGER (the dangerous-bulk guard):** AC-BULK-DANGER-1/2 — automated proof the typed phrase is mandatory and no single keypress destroys unmanaged boxes.
- **G-FINAL:** `make check` (fmt-check + lint + lint-lean + build + release + test) — the release gate.

### 9.3 No-freeze verification — explicit procedure

1. Automated (authoritative): AC-TIMEOUT-1 drives a `HangingRunner` through `run_with_timeout` and asserts it RETURNS a `Timeout` error in bounded wall-clock time (not a hang). AC-POLL-1/2/3 assert the gate never sets `busy` and never emits a second in-flight effect.
2. Smoke (optional, containerized): launch cbox, enter a running box, observe input responsiveness during stats polling; (if reproducible) stall the engine socket and confirm the UI keeps drawing and accepting keys while timeout errors recur silently.

---

## 10. Confidence report

- **Pattern match (25%):** 92% — every new piece ADAPTs a proven seam: the runner-trait method mirrors the existing `run`/`run_interactive` shape; `should_poll` mirrors the pure-helper testing style; the palette mirrors `FilterState`+`fuzzy_rank`; bulk reuses the multi-name `RmSpec`/`StopSpec` + the ConfirmDestroy modal; `core::stats` mirrors `core::inspect`'s engine-call. Bundle 1's pure-helper test approach is the validated prior.
- **Requirement clarity (25%):** 90% — feature set + GAP resolutions LOCKED; one low-stakes flagged decision (D-1 bulk entry: palette/`b` vs four direct keys), defaulted with a recommendation, non-blocking.
- **Decomposition stability (25%):** 84% — three self-consistency passes (by-GAP / by-file / by-dependency) converge on T1→T2→{T3}→T4→T5, T6 off {T1,T2}; the only variance is T3's position (independent sibling) and whether T5 depends on T4 (only via the `b` fast-path).
- **Constraint compliance (25%):** 86% — containerized `make` verifiers throughout; lean build gated (R-7); the timeout containment keeps GAP-1's blast radius to two new paths (R-1); the dangerous-op guard and no-freeze guarantee are pinned by dedicated gates. Residuals: the single in-flight slot's stats-vs-list contention (R-11, accepted P2) and the human-size mem parser's tolerance (AC-STATS-PARSE within tolerance).

**Aggregate: 87% → AUTO_PROCEED.** Deliver to Vivi; K1 routable to Kupo.

---

## 11. Flagged decision for the human (defaulted; does not block)

- **D-1 (bulk entry UX):** recommend the `:` palette + a `b` bulk-scoped fast-path (the four bulk ops are `palette_actions()` entries) rather than burning four scarce single-letter List keys. If you prefer four direct keys, that's a one-line-per-key change and removes T5's dependency on T4. Sign-off optional; spec proceeds with the palette/`b` approach.

---

*SPECTRA — Strategic Specification through Deliberate Reasoning. Plan only; execution is Vivi's phase. Cite file:line anchors verified against the working tree at HEAD (Bundle 1 landed; v0.9.0).*
