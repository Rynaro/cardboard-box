# cbox TUI — Bundle 3 "Glass Cockpit" (FOCUSED)

**Spec ID:** `2026-06-20-tui-bundle3-glass-cockpit`
**Intent:** REQUEST / CHANGE (extend existing TUI; decisions LOCKED by human after ATLAS scout)
**Release target:** next release-please increment — `feat` → minor bump → **v0.11.0**
**Methodology:** SPECTRA 4.9.1 (single-pass standard tier)
**Author agent:** spectra
**Stack:** Rust + ratatui 0.28 + crossterm 0.28, The Elm Architecture (Model / Message / update / Effect / view), single background worker thread
**Builds:** containerized only — Vivi uses `make` targets, never host `cargo` (per project memory).

> SPECTRA produces the spec; **Vivi** (Builder/speed-class) implements. This document does **not** re-open any locked decision or GAP resolution. It makes them precise and buildable.

---

## 1. CLARIFY (intake)

| Axis | Resolution |
|---|---|
| **WHO** | Maintainer (Rynaro) is requester/approver. Vivi implements. End users are `cbox` TUI users. |
| **WHAT** | Three features: (1) live container-log streaming modal, (2) scroll-wheel mouse, (3) cross-session redacted action history. |
| **WHY** | Operational visibility ("glass cockpit") on top of Bundles 1 & 2 without introducing a persistent multi-pane dashboard. |
| **CONSTRAINTS** | No focus/pane model. No layout Rects on Model. Scroll-only mouse (no click-to-select). Container logs only. Secrets redacted before disk. Voice rule "show don't tell" extended. Existing suites stay green. Single worker thread is sacred; `logs -f` must NOT run on it. |

**CLARIFY questions skipped** — justified: the feature set, all product flags, and the six GAP resolutions are explicitly LOCKED in the brief ("Do NOT re-open the decisions"). DISCOVER skipped: goal is fully specified, intent is CHANGE not IDEA.

**Conventions file:** `.spectra/setup/spectra-conventions.md` absent → generic SPECTRA defaults, enriched by the verified codebase vocabulary and the Bundle 1 / Bundle 2 specs surfaced from CRYSTALIUM memory.

**Memory pre-flight:** CRYSTALIUM recall returned prior cbox specs (v1.0 CLI lifecycle, v2.0 provisioning). No prior streaming/mouse/history spec exists → this is net-new (GENERATE, with the TEA shell + CmdLog/StatsHistory ring + Overlay/fuzzy patterns as proven skeletons).

---

## 2. SCOPE

### 2.1 Complexity score (4-dimension, 4–12)

| Dimension | Score | Reason |
|---|---|---|
| Technical depth | 3/3 | New dedicated OS thread + child lifecycle/cancel; 4th `RunMode`; streaming coalescer; mouse capture interacting with terminal restore; secrets-on-disk redaction. |
| Scope breadth | 3/3 | Touches `dbox/{runner,real,argv}`, `core/mod`, `tui/{app,model,message,effect,update,view,strings,filter}`, new `tui/history` + new host persist store, new tests. |
| Integration risk | 3/3 | Thread/child leak; mouse capture not disabled on panic; secrets leaking to disk; wedge under log throughput. |
| Uncertainty | 1/3 | Low — all design decisions LOCKED; proven local patterns to mirror. |
| **Total** | **10/12** | **Extended-thinking tier; human-in-the-loop review recommended (already gated: human decided the feature set).** |

### 2.2 Boundaries

**In scope (LOCKED — these three, nothing more):**
1. Live log streaming pane — **full-screen MODAL** (a new `Screen::Logs`, see §4.1 decision), tailing `<backend> logs -f <id>` for the selected box, scrollable bounded ring, autoscroll + wrap toggles, opens on a key, closes on Esc/q.
2. Scroll-wheel mouse — **scroll-only**, no click-to-select. Wheel scrolls lists, log pane, command-log overlay, history overlay.
3. Cross-session action history — searchable overlay (atuin-style) persisted under `$XDG_STATE_HOME/cbox/history.jsonl`, secret-bearing args **redacted** before disk write, bounded read + retention cap, reuses `fuzzy_rank` + an `Overlay::History` variant.

**Out of scope / DEFERRED (note as future work, do NOT spec):**
- Persistent multi-pane dashboard + focus model.
- Click-to-select mouse (hit-testing, layout Rects on Model).
- Streaming apply/provisioning output (different source).

**Assumptions (with risk-if-wrong):**

| # | Assumption | Risk if wrong |
|---|---|---|
| A1 | `<backend> logs -f <id>` (podman/docker) streams to stdout line-buffered and exits only on container stop / kill. | If the engine buffers heavily, lines arrive in bursts; the coalescer (GAP-1) absorbs this — low risk. |
| A2 | The result channel `sync_channel::<Message>(32)` (app.rs:237) tolerates one extra producer (the stream thread) cloning its `SyncSender`. | `SyncSender` is `Clone + Send`; the worker already clones it (app.rs:238). No risk. |
| A3 | crossterm 0.28 `EnableMouseCapture` + `Event::Mouse` with `MouseEventKind::ScrollUp/ScrollDown` is available (already imported `event::{...}` at app.rs:22). | crossterm 0.28 supports these. No risk. |
| A4 | Secret VALUES never appear in argv today (`cbox secret set` reads value from TTY/stdin — secret.rs:86,124-147; persist=true env is name-only `--env KEY` — argv.rs:91-96). The only inline-value vector is plaintext `[env]` (argv.rs:98-102) and provision `run` strings. | If a future command puts a secret in argv, the allow-list redaction (§6.2) would miss it. Mitigated: redaction uses a **deny-pattern scrubber** over the whole argv string, not just known flags. |

---

## 3. PATTERN

Memory + codebase ranking (MMR): all three features ADAPT proven in-repo patterns rather than GENERATE from scratch.

| Feature | Pattern source (verified file:line) | Strategy |
|---|---|---|
| Bounded log ring | `CmdLog` VecDeque-with-cap + drop-oldest (`cmdlog.rs:34-75`, push 51-59); `StatsHistory.push_sample` (`model.rs:236-250`) | USE_TEMPLATE |
| Coalesce + bounded channel | Worker `sync_channel` + `try_send` drop-on-full (`app.rs:122,237,316`); silent-poll coalescing in `should_poll` (`poll.rs:71-92`) | ADAPT |
| Child + deadline + kill+reap | `run_with_timeout` spawn + `try_wait` + `kill`+`wait` (`real.rs:57-124`, kill/reap 93-97) | ADAPT |
| Cancel via AtomicBool flag | `TERMINAL_RESTORED` static `AtomicBool` + `compare_exchange` idempotence (`app.rs:47,69-77`) | ADAPT |
| 4th RunMode | `RunMode` enum `Capture\|Interactive\|DryRun` (`runner.rs:7-15`); trait default-method extension (`run_with_timeout` default `113-120`) | ADAPT |
| Modal screen | `Screen` enum (`model.rs:19-27`) + key dispatch `match model.screen` (`update.rs:157-164`); per-screen handler (`handle_key_doctor` 710-718) | USE_TEMPLATE |
| Search overlay (fuzzy) | `Overlay::Palette{query,matches,cursor,..}` (`model.rs:179-189`) + `fuzzy_rank` (`filter.rs:26-49`) + `handle_key_palette` dispatch (`update.rs:124-134`) | USE_TEMPLATE |
| Host XDG path | inline `$XDG_CONFIG_HOME` fallback (`update.rs:1299-1303`) → mirror for `$XDG_STATE_HOME` | ADAPT |
| Capture chokepoint for history | `LoggingRunner` write-hook (`cmdlog.rs:108-131`, DryRun-skip 111) | ADAPT (the natural capture point) |
| Mouse enable/disable | `TerminalGuard::new` / `restore_terminal_once` (`app.rs:49-77`); event loop match (`app.rs:270-273`) | ADAPT |
| Voice rule | `BANNED` adjective scan over copy consts (`tui_theme.rs:351-407`) | EXTEND |

**Failure patterns to avoid (catalogued from the architecture):**
- **Thread/child leak** — a `logs -f` child runs forever; if the stream thread isn't joined and the child isn't killed on every exit path, processes leak across sessions. (P0 — §6.)
- **Wedged UI loop** — blocking the worker on a never-exiting child (GAP-2) or an unbounded buffer growing without backpressure (GAP-1).
- **Secret leak** — writing un-redacted argv to a host file that survives the session.
- **Terminal corruption** — mouse capture left enabled after panic.

---

## 4. EXPLORE (key design decisions)

Three load-bearing decisions are open within the LOCKED constraints; each was explored and resolved.

### 4.1 Log pane: `Overlay` variant vs new `Screen::Logs` → **`Screen::Logs`** [DECISION]

| Option | Score (Align/Correct/Maint/Simpl/Risk) | Verdict |
|---|---|---|
| `Overlay::Logs` | Reuses overlay pre-check, but overlays are *additive over a screen* and dismissed by "any key" (Cheatsheet) — a live stream with its own scroll, two toggles, and a continuously-mutating ring is a stateful full-screen MODE, not a transient veil. | Reject |
| **`Screen::Logs`** (new) | Full-screen, stateful, has its own key handler `handle_key_logs` mirroring `handle_key_doctor` (`update.rs:710-718`); enters from List, returns to List on Esc/q. The brief explicitly calls it "full-screen MODAL — NOT a persistent split"; a `Screen` is exactly that. | **Select** |

Rationale: brief says "modal/full-screen and stateful with a live stream + scroll + toggles" and ATLAS recommends evaluating; `Screen` matches the lifecycle (open → live → close) and avoids overloading the overlay-dismiss semantics. **This is NOT a focus/pane model** — it is a single full-screen screen, exactly like `Screen::DoctorPanel`. No layout Rects on Model.

### 4.2 "What is an action" for history → **each real spawn captured via `LoggingRunner`** [DECISION]

| Option | Verdict |
|---|---|
| Each `dispatch_action` | Captures intent, but misses the actual argv, fires for no-op/nav actions, and double-counts (one action can fan out to many spawns in bulk ops). | Reject |
| **Each real spawn via `LoggingRunner`** | The `LoggingRunner` write-hook (`cmdlog.rs:108-131`) is already the single chokepoint where every real distrobox/podman argv + exit code is known, DryRun is already skipped (line 111), and bulk fan-out is naturally captured as N entries. History is "what actually ran," symmetric with the in-memory command log. | **Select** |

Rationale: ATLAS names the `LoggingRunner` write-hook as "the natural capture point." History becomes the persistent, redacted, cross-session sibling of the in-memory `CmdLog`. One concept, two sinks (RAM ring + disk JSONL).

### 4.3 Stream/child handles location: Model vs shell → **shell-local (`app.rs`), beside `Worker`** [DECISION]

The `Child`, the stop-`AtomicBool` (`Arc<AtomicBool>`), and the `JoinHandle` are **impure OS resources** — they must NOT live on the pure `Model` (which has "no I/O" by contract, `model.rs:1-3`). They live in a shell-local struct `LogStream` held by `run_loop`, beside `Worker` (`app.rs:113`). The Model holds only the **pure** stream state: the bounded ring, toggles, scroll, and a `logs_target: Option<(id, backend)>` flag. The shell reacts to `Effect::StreamLogs` / `Effect::StopLogs` to spawn/cancel the thread. This mirrors how `SuspendAndEnter`/`SuspendAndEdit` are shell-handled (`app.rs:291-304`) while the Model stays pure.

**Rejected alternatives recorded** so replanning never re-explores: `Overlay::Logs` (4.1), `dispatch_action`-as-action (4.2), child handles on Model (4.3).

---

## 5. CONSTRUCT — architecture & stories

### 5.0 Architecture: streaming (the load-bearing design)

#### 5.0.1 New `RunMode::Stream` + `run_stream` (runner seam)

Extend `RunMode` (`runner.rs:7-15`) with a 4th variant and add a streaming method to the `DistroboxRunner` trait (`runner.rs:96-121`) with a **default impl that returns "unsupported"** so mocks don't have to implement it (mirrors `run_with_timeout`'s default `113-120`).

```
// runner.rs — additive, no change to Capture/Interactive/DryRun semantics
enum RunMode { Capture, Interactive, DryRun, Stream }

// Trait method (default-impl so mocks are unaffected):
//   fn run_stream(
//       &self,
//       inv: Invocation,
//       on_line: &mut dyn FnMut(String),   // called per stdout line
//       stop: &AtomicBool,                  // checked between reads; true => stop+kill
//   ) -> Result<i32, RunnerError>;          // returns child exit code (or kill outcome)
// Default impl: Err(RunnerError::Io{...}) "streaming unsupported by this runner".
```

`RealRunner::run_stream` (`real.rs`, beside `run_with_timeout` 57-124):
- `Command::new(program).args(...).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()` (stderr dropped — we tail stdout logs only).
- Wrap child stdout in `BufReader`; loop `read_line`:
  - On each non-empty line → `on_line(line)`.
  - **Between reads**, check `stop.load(Acquire)`; if set → `child.kill()` + `child.wait()` (kill+reap idiom, `real.rs:93-97`) → return.
  - On EOF (`read_line` returns Ok(0)) → the process exited (container stopped); `child.wait()` to reap → return exit code. **No crash, no wedge** — the reader thread simply ends and the model shows the stream as ended.
- All error paths still reap the child (no zombie).

> **Why a blocking `read_line` is safe here:** it runs on the **dedicated stream thread** (GAP-2), never the worker and never the UI thread. The stop-flag is checked between line reads; for a truly idle stream, the cancel still works because `child.kill()` closes the pipe and unblocks `read_line` with EOF. (Vivi: set the flag THEN kill, so the post-kill EOF is interpreted as a clean stop.)

#### 5.0.2 Coalescing flush → `Message::LogChunk(Vec<String>)` (GAP-1)

The `on_line` closure passed by the **shell** does NOT send one Message per line (that would flood the 32-slot channel). Instead it batches into a pure coalescer:

- Pure helper `LogCoalescer` (new, in a lean module e.g. `tui/logstream.rs`, no ratatui dep) with:
  - `push(line)` → buffers; returns `true` when a flush is due.
  - **Flush trigger:** every `FLUSH_LINES` lines **OR** when `FLUSH_INTERVAL` (~50ms) has elapsed since the last flush, whichever first.
  - `drain() -> Vec<String>` → empties the buffer.
- The shell's `on_line` closure: `coalescer.push(line); if due { let chunk = coalescer.drain(); result_tx.try_send(Message::LogChunk(chunk)); }`. A periodic time-based flush is driven by the reader checking elapsed time on each line; for a slow stream, a trailing partial batch flushes on the next line or on stream end (the shell drains the coalescer once more when the thread joins).
- **Backpressure:** `try_send` (consistent with `app.rs:316`) — if the channel is full, the chunk is **dropped** (logs degrade to "some lines missed," never a wedged UI). This is the documented GAP-1 degradation. Constants live in `tui/logstream.rs` (`FLUSH_LINES`, `FLUSH_INTERVAL_MS`, `LOG_RING_CAP`).

#### 5.0.3 Bounded Model ring (GAP-1, Model side)

New pure type `LogBuffer` on Model (mirrors `CmdLog`/`StatsHistory` exactly):

```
// model.rs — pure, VecDeque-with-cap, drop-oldest past LOG_RING_CAP
struct LogBuffer { lines: VecDeque<String>, cap: usize }
//   push_chunk(&mut self, chunk: Vec<String>)  -> extend + pop_front while len>cap
//   lines(&self) -> iterator (oldest first; newest at bottom for autoscroll)
```

`Message::LogChunk(Vec<String>)` handler in `update.rs`: `model.log_buffer.push_chunk(chunk)`; if `autoscroll` → snap scroll to bottom; else hold position. Overrun silently scrolls oldest off — never a loop. `LOG_RING_CAP` recommend **2000** lines.

#### 5.0.4 Shell wiring: dedicated thread, Child + stop-flag + JoinHandle (GAP-2)

Shell-local struct in `app.rs` (beside `Worker`, line 113):

```
struct LogStream {
    stop: Arc<AtomicBool>,       // set true to request cancel
    join: thread::JoinHandle<()>,// joined on stop
    target: (String, String),    // (id, backend) currently streaming — to detect "same box"
}
// run_loop holds:  let mut log_stream: Option<LogStream> = None;
```

**Start (`Effect::StreamLogs { id, backend }`):**
1. If a stream is already running for a DIFFERENT target → run the StopLogs teardown first (kill old, join). If for the SAME target → no-op (idempotent re-open).
2. `let stop = Arc::new(AtomicBool::new(false));` clone for the thread.
3. Spawn a **NEW dedicated thread** (NOT the worker): builds the `logs -f` Invocation (`RunMode::Stream`), constructs a `LogCoalescer`, and calls `runner.run_stream(inv, &mut on_line, &stop)`. `on_line` `try_send`s `Message::LogChunk` via a cloned `result_tx`. On return (EOF or kill), drain the coalescer once more and `try_send` a final chunk + `Message::LogStreamEnded(exit_code)`.
4. Store `LogStream { stop, join, target }`.

**Cancel (`Effect::StopLogs`) — ALL triggers:**
- Pane close (Esc/q in `Screen::Logs`).
- Selection change while streaming (handled as kill-old-then-start-new: the reducer emits `StopLogs` then `StreamLogs` for the new box, OR the shell detects target mismatch on `StreamLogs` and swaps — choose the shell-swap path in §5.0.4 step 1 for atomicity).
- Quit (Ctrl-C/D/q): `run_loop` must tear down any live `log_stream` before returning (add to the `Effect::Quit` arm AND as a final teardown after the loop, so an abrupt quit can't leak).

Teardown sequence (single helper `stop_log_stream(&mut Option<LogStream>)`):
```
if let Some(ls) = log_stream.take() {
    ls.stop.store(true, Ordering::SeqCst);  // request stop
    // run_stream sees the flag and kills+reaps the child; kill also unblocks read_line via EOF
    let _ = ls.join.join();                  // wait for the thread to finish (bounded: child is killed)
}
```

> **Thread/child-leak prevention (explicit):** the only spawn point is `Effect::StreamLogs`; the only teardown is `stop_log_stream`, called on (a) pane close, (b) target swap, (c) `Effect::Quit`, AND (d) unconditionally after the `run_loop` returns. Setting the flag THEN joining guarantees the child is killed+reaped before the handle is dropped. A dropped `JoinHandle` without join would detach the thread — so we ALWAYS join. (P0 gate G-LOGLEAK, §6.)

#### 5.0.5 `build_logs_argv` + streaming `core::logs` (GAP-5)

- `build_logs_argv(id: &str) -> Vec<String>` beside `build_stats_argv` (`argv.rs:194-205`): `vec!["logs", "-f", id]`. (Container logs via the ENGINE — `<backend> logs -f <id>` — NOT a distrobox subcommand. Mirrors `build_stats_argv` which is also an engine call.)
- `core::logs(...)` beside `core::stats` (`mod.rs:191-212`): builds the Invocation with `spec.backend.as_str()` as the program (pattern: `mod.rs:204`) + `RunMode::Stream`, and delegates to `runner.run_stream(...)`. Signature takes the `on_line` callback + `stop` flag (the shell owns the thread; `core::logs` is the pure-ish builder/driver, kept thin). Do NOT wire apply/provisioning output (out of scope, GAP-5).

#### 5.0.6 Edge cases (no crash / no wedge)

| Case | Behavior |
|---|---|
| Stopped box (no logs / immediate exit) | `read_line` hits EOF quickly → thread ends → `LogStreamEnded` → pane shows "stream ended" line; user can Esc/q. |
| Empty logs | Ring stays empty; view shows `LOGS_EMPTY` copy (voice-compliant). |
| Stream ends mid-session (container stopped externally) | EOF → `LogStreamEnded` → no further chunks; ring frozen; Esc/q closes. |
| High throughput | Coalescer batches; `try_send` drops overrun chunks; ring drops oldest. UI never blocks (G-NOWEDGE). |
| Selection change | Shell swaps: kill old, join, start new (§5.0.4 step 1). |
| Quit during stream | `stop_log_stream` on Quit arm + post-loop. No leak. |

### 5.1 Mouse (scroll-only)

- `TerminalGuard::new` (`app.rs:52-60`): after `EnterAlternateScreen`, `execute!(stdout, EnableMouseCapture)`.
- `restore_terminal_once` (`app.rs:69-77`): inside the `compare_exchange` guard, before/with `LeaveAlternateScreen`, `execute!(io::stdout(), DisableMouseCapture)`. **Because the panic hook calls `restore_terminal_once` (`app.rs:85`), the panic path also disables mouse capture — no extra code needed.** Add `EnableMouseCapture`/`DisableMouseCapture` to the crossterm `terminal`/`event` import (`app.rs:22-25`).
- `suspend_and_edit` / `suspend_and_enter` (`app.rs:144-198`, `203-224`): these leave/re-enter alt-screen manually; add `DisableMouseCapture` on leave and `EnableMouseCapture` on re-enter so `$EDITOR`/`enter` get a clean terminal and the TUI regains capture on return.
- Event loop match (`app.rs:270-273`): add `Event::Mouse(me) => Some(Message::Mouse(normalize_mouse(me)))`. New `normalize_mouse` maps `MouseEventKind::ScrollUp → Mouse::ScrollUp`, `ScrollDown → Mouse::ScrollDown`, all else → `Mouse::Other` (ignored). Define a small internal `Mouse` enum in `message.rs` (mirrors `Key`, decouples from crossterm) and `Message::Mouse(Mouse)`.
- **Scroll routing (pure)** — a helper `scroll_delta(kind) -> i32` (+1 down / -1 up) and a reducer dispatch by current context:
  - `Screen::List` (and Detail list) → move selection by ±1 (reuse `move_up`/`move_down`).
  - `Screen::Logs` → adjust `log_scroll` by ±SCROLL_STEP, clamped; disables autoscroll on manual scroll-up, re-enables on scroll-to-bottom (recommend SCROLL_STEP = 3).
  - `Overlay::CommandLog{scroll}` → adjust `scroll` by ±SCROLL_STEP (reuse `handle_key_cmdlog` semantics, `update.rs:120-122`).
  - `Overlay::History{...}` → move the history cursor by ±1.
  - **No hit-testing, no Rects** — routing is by current screen/overlay only.

### 5.2 Action history (cross-session, redacted)

- **Capture point:** `LoggingRunner` write-hook (`cmdlog.rs:108-131`). After the existing `log.push(argv, status)`, also append a redacted entry to a host store. To keep `LoggingRunner` lean and the write off the hot path/UI, the decorator holds an `Arc<Mutex<HistoryStore>>` (constructed in `app::run` beside the `cmdlog`, `app.rs:333-338`). DryRun is already skipped (line 111) — history excludes dry-runs too.
- **Redaction (§6.2 detail):** before writing, run the joined argv through `redact_argv(&str) -> String`:
  - Scrub the value following any known secret-bearing flag (`--env KEY=VALUE` → `--env KEY=<redacted>`; the persist=true form `--env KEY` is already value-less — argv.rs:91-96 — keep as-is). Scrub `--label cbox.*` only if it could carry user data? No — labels are non-secret metadata; keep. Plaintext `[env]` inline values (argv.rs:98-102) are the real inline-value vector → redact the `=VALUE` half.
  - Scrub any token matching a secret-shaped deny pattern (e.g. `*token*=`, `*secret*=`, `*password*=`, `*passwd*=`, `*apikey*=`, `*api_key*=`, case-insensitive) → keep key, mask value. This is a **deny-pattern scrubber over the whole argv**, robust to A4.
  - `cbox secret set` never reaches the runner with a value in argv (value is read from TTY/stdin — secret.rs:86,124-147), so it is inherently safe; the scrubber is defense-in-depth.
- **On-disk format:** JSONL (`history.jsonl`), one `HistoryEntry` per line, serialized with `serde_json` (`Cargo.toml:27`). `HistoryEntry { argv: String /* already redacted */, status: Option<i32>, ts: i64 /* unix secs */ }`. Append-only writes (open with append); no rewrite on every entry.
- **Location:** `$XDG_STATE_HOME/cbox/history.jsonl`; default `$XDG_STATE_HOME` → `~/.local/state` (mirror the `$XDG_CONFIG_HOME` fallback at `update.rs:1299-1303`). Create the dir if absent (`create_dir_all`, best-effort).
- **Retention cap:** keep the most recent `HISTORY_CAP` entries (recommend **1000**). Strategy: load is bounded (read at most last N lines / parse + truncate to cap); on store open, if the file exceeds ~2× cap, rewrite-compact to cap (cheap amortized). Append normally otherwise.
- **Loader (bounded, graceful):** `HistoryStore::load()` reads the file, parses line-by-line; **skips** any line that fails to parse (corrupt/partial-write tolerant), caps to the last `HISTORY_CAP` valid entries. **Missing file → empty history. Corrupt file → empty (or partial-valid) history, NEVER a panic** (G-HISTSAFE).
- **Search overlay:** new `Overlay::History { query, matches, cursor, entries: Vec<HistoryEntry> }` (mirror `Overlay::Palette`, `model.rs:179-189`). Opened by a new key (recommend `H`, since `l`/`L` is command-log; pick a free key and add to keymap + cheatsheet + `Action::History`). Typing updates `matches` via `fuzzy_rank(query, &argv_strs)` (`filter.rs:26-49`). Up/Down move cursor; Esc closes. (Enter is a no-op for now — history is read-only review; do NOT re-run, out of scope.) Add `Action::History` to the `Action` enum (`action.rs:17-48`), `label()` (voice-compliant verb), `default_key`, `ALL_ACTIONS`, `palette_actions` source, and `dispatch_action` (`update.rs:726`).

### 5.3 Story hierarchy

```
PROJECT: cbox TUI Bundle 3 "Glass Cockpit" (FOCUSED) → v0.11.0
├── FEATURE F1: Scroll-wheel mouse (scroll-only)
│   └── STORY S1: Mouse capture + Event::Mouse arm + scroll routing
├── FEATURE F2: Cross-session action history (redacted)
│   ├── STORY S2: Host HistoryStore (XDG path, JSONL, cap, redaction, graceful load)
│   ├── STORY S3: LoggingRunner write-hook → history append (redacted)
│   └── STORY S4: Overlay::History search UX (fuzzy) + Action::History wiring
├── FEATURE F3: Live container-log streaming modal
│   ├── STORY S5: RunMode::Stream + run_stream + build_logs_argv + core::logs
│   ├── STORY S6: LogCoalescer + LogBuffer ring + Message::LogChunk/LogStreamEnded
│   ├── STORY S7: Shell dedicated thread + Child/stop-flag/JoinHandle + cancel triggers
│   └── STORY S8: Screen::Logs modal + handle_key_logs + view (toggles/scroll) + strings
└── STORY K1: Voice-rule extension (AC-COPY-1) over new copy + Action::History label
```

#### STORY S1 — Mouse capture + scroll routing  `≤2d` `P1`
> As a TUI user, I want my scroll wheel to scroll lists/panes so that navigation feels native.

- **Action plan:** Extend `TerminalGuard::new` (`app.rs:52-60`) + `restore_terminal_once` (`app.rs:69-77`) with `EnableMouseCapture`/`DisableMouseCapture`; patch `suspend_and_edit`/`suspend_and_enter` (`app.rs:144-198,203-224`). Add `Mouse` enum + `Message::Mouse` (`message.rs`), `normalize_mouse` (`app.rs`), `Event::Mouse` arm (`app.rs:270-273`). Add pure `scroll_delta` + reducer routing in `update.rs`.
- **AC:** AC-MOUSE-1, AC-MOUSE-2, AC-MOUSE-3 (§7).
- **Technical context:** crossterm 0.28 `MouseEventKind::ScrollUp/ScrollDown`. No Rects. Panic-restore disables capture for free (panic hook → `restore_terminal_once`).
- **Agent hints:** Builder (Vivi). Context: `app.rs`, `message.rs`, `update.rs`. Gates: `make build`, `make lint`, `make lint-lean`, `make test`.
- **Risk:** P1 — capture-not-disabled-on-panic (mitigated by shared restore path).

#### STORY S2 — Host HistoryStore  `≤2d` `P0`
> As a user, I want my action history saved across sessions without leaking secrets.

- **Action plan:** New module `tui/history.rs` (lean, no ratatui): `HistoryEntry` (serde), `HistoryStore` (XDG path resolution mirroring `update.rs:1299-1303` but `$XDG_STATE_HOME`→`~/.local/state`), `redact_argv`, `append`, `load` (bounded + corrupt-tolerant), compaction to `HISTORY_CAP`. Constants `HISTORY_CAP=1000`, file `history.jsonl`.
- **AC:** AC-HIST-REDACT-1/2, AC-HIST-ROUNDTRIP-1, AC-HIST-CAP-1, AC-HIST-CORRUPT-1, AC-HIST-PATH-1 (§7).
- **Technical context:** `serde_json` (`Cargo.toml:27`). Pure-testable: redaction + load over a temp file (no TTY).
- **Agent hints:** Builder. Context: `update.rs:1299-1303` (XDG pattern), `cmdlog.rs` (entry shape). Gates: `make build`, `make lint-lean`, `make test`.
- **Risk:** P0 — secrets-on-disk.

#### STORY S3 — LoggingRunner → history append  `1d` `P1`
> As the system, I want every real spawn appended (redacted) to history at the existing chokepoint.

- **Action plan:** Add `Arc<Mutex<HistoryStore>>` to `LoggingRunner` (`cmdlog.rs:97-131`); after `log.push` in `run`/`run_interactive`, call `history.append(redact_argv(&argv), status)`. Construct + inject in `app::run` (`app.rs:333-338`). DryRun already skipped (line 111).
- **AC:** AC-HIST-CAPTURE-1 (§7).
- **Technical context:** Keep the write best-effort (`if let Ok(mut h) = ...lock()`), never panic the runner.
- **Agent hints:** Builder. Context: `cmdlog.rs:108-131`, `app.rs:333-338`. Gates: `make build`, `make lint-lean`, `make test`. **Kupo-eligible** (≤2 files, mechanical).
- **Risk:** P1.

#### STORY S4 — Overlay::History search UX  `≤2d` `P2`
> As a user, I want to fuzzy-search my past actions in an atuin-style overlay.

- **Action plan:** Add `Overlay::History{query,matches,cursor,entries}` (`model.rs:171-190`), `handle_key_history` (mirror `handle_key_palette` `update.rs:124-134`), overlay pre-check arm (`update.rs:113-135`), `Action::History` (`action.rs`), `dispatch_action` arm (`update.rs:726`), keymap key, view rendering, `strings.rs` copy (HISTORY_TITLE/EMPTY/HINT). Load entries on open via `HistoryStore::load()`.
- **AC:** AC-HIST-FUZZY-1, AC-HIST-OPEN-1 (§7).
- **Technical context:** `fuzzy_rank` (`filter.rs:26-49`); `Overlay::Palette` template. Enter is a no-op (read-only review).
- **Agent hints:** Builder. Context: `model.rs:179-189`, `update.rs:124-134`, `filter.rs`. Gates: `make build`, `make test`.
- **Risk:** P2.

#### STORY S5 — RunMode::Stream + run_stream + logs argv/core  `≤2d` `P0`
> As the system, I want a streaming runner seam so logs can be tailed off the worker.

- **Action plan:** Add `RunMode::Stream` (`runner.rs:7-15`); add `run_stream` to the trait with a default "unsupported" impl (`runner.rs:96-121`, mirroring `run_with_timeout` default 113-120); implement `RealRunner::run_stream` (`real.rs`, beside 57-124) with `BufReader` line loop + stop-flag check + kill+reap (93-97) + EOF→reap+exit-code. Add `build_logs_argv(id)` (`argv.rs:194-205`) and `core::logs` (`mod.rs:191-212`, program = `spec.backend.as_str()`).
- **AC:** AC-LOGARGV-1, AC-STREAM-STOP-1, AC-STREAM-EOF-1 (§7).
- **Technical context:** stderr → `Stdio::null()` (tail stdout only). Default-impl keeps every existing Mock green.
- **Agent hints:** Builder/Reasoner. Context: `real.rs:57-124`, `runner.rs`, `argv.rs:194-205`, `mod.rs:191-212`. Gates: `make build`, `make lint-lean`, `make test`.
- **Risk:** P0 — child lifecycle.

#### STORY S6 — Coalescer + LogBuffer ring + messages  `≤2d` `P0`
> As the system, I want streamed lines coalesced and bounded so the UI never wedges.

- **Action plan:** New lean module `tui/logstream.rs`: `LogCoalescer` (`push`/`due`/`drain`, `FLUSH_LINES`, `FLUSH_INTERVAL_MS`) — pure, time-injectable for tests (pass elapsed in, or expose a `due_at(now)`). `LogBuffer` on Model (`model.rs`, VecDeque-with-cap, `push_chunk`, `LOG_RING_CAP=2000`). `Message::LogChunk(Vec<String>)` + `Message::LogStreamEnded(Option<i32>)` (`message.rs:51-76`). Handlers in `update.rs`: push_chunk + autoscroll snap; ended → mark stream ended.
- **AC:** AC-COALESCE-1, AC-COALESCE-2, AC-LOGRING-1 (§7).
- **Technical context:** mirror `StatsHistory.push_sample` (`model.rs:236-250`) drop-oldest. Coalescer must be deterministically unit-testable without a clock (inject elapsed).
- **Agent hints:** Builder. Context: `model.rs:236-250`, `cmdlog.rs:51-59`, `poll.rs` (pure-helper style). Gates: `make build`, `make lint-lean`, `make test`.
- **Risk:** P0.

#### STORY S7 — Shell dedicated thread + cancel  `≤3d` `P0`
> As the system, I want a dedicated stream thread with leak-proof teardown.

- **Action plan:** `LogStream` struct + `Option<LogStream>` in `run_loop` (`app.rs:228-313`, beside `Worker` 113). `Effect::StreamLogs{id,backend}` / `Effect::StopLogs` (`effect.rs:44-81`) — **shell-handled** (like `SuspendAndEnter`, NOT in `execute_effect`; add to the `match eff` in the loop `app.rs:286-308`, and return `None` for them in `execute_effect`). `stop_log_stream` helper (set flag → join). Spawn dedicated thread that drives `runner.run_stream` with a cloned `result_tx` + `LogCoalescer`. Teardown on pane-close StopLogs, target-swap (kill-old-start-new in the StreamLogs handler), Quit arm, AND post-loop unconditional teardown.
- **AC:** AC-STREAM-SWAP-1, AC-STREAM-QUIT-1, AC-NOWEDGE-1 (§7, via the cancel seam — see §7 test design).
- **Technical context:** `Arc<AtomicBool>` (precedent `app.rs:47`), `JoinHandle`. NEVER drop the handle without join. Model stays pure (handles in shell only — §4.3).
- **Agent hints:** Reasoner (highest-risk story). Context: `app.rs:113-134,228-317`, `real.rs:93-97`. Gates: `make build`, `make lint`, `make lint-lean`, `make test`.
- **Risk:** P0 — thread/child leak (G-LOGLEAK).

#### STORY S8 — Screen::Logs modal + view  `≤2d` `P1`
> As a user, I want a full-screen log modal with autoscroll/wrap toggles.

- **Action plan:** Add `Screen::Logs` (`model.rs:19-27`); Model log-pane state (`log_buffer`, `log_scroll`, `log_autoscroll: bool`, `log_wrap: bool`, `log_target`, `log_ended: bool`). Open key on List (recommend `L`) → set `Screen::Logs` + emit `Effect::StreamLogs` for the selected box. `handle_key_logs` (mirror `handle_key_doctor` `update.rs:710-718`): Esc/q → `Screen::List` + `Effect::StopLogs`; toggles (e.g. `a` autoscroll, `w` wrap); Up/Down/PgUp/PgDn scroll. View renders the ring (wrap on/off), a header with toggle state, and `LOGS_EMPTY`/"stream ended" lines. `strings.rs`: LOGS_TITLE/EMPTY/ENDED/HINT.
- **AC:** AC-LOGSCREEN-1, AC-LOGTOGGLE-1, AC-LOGOPEN-1 (§7).
- **Technical context:** `Screen` dispatch (`update.rs:157-164`). Selection change while on List re-opening Logs → swap (S7).
- **Agent hints:** Builder. Context: `model.rs:19-27`, `update.rs:157-164,710-718`, `view.rs`. Gates: `make build`, `make test`.
- **Risk:** P1.

#### STORY K1 — Voice-rule extension  `1d` `P1`  **Kupo-eligible**
> As the maintainer, I want all new copy to pass the "show don't tell" voice rule.

- **Action plan:** Extend AC-COPY-1 const list (`tui_theme.rs:362-398`) with every new `strings.rs` const (LOGS_*, HISTORY_*). Extend AC-VOICE-1 to cover `Action::History.label()`. Add new consts to `strings.rs` (voice-compliant).
- **AC:** AC-COPY-1 (extended), AC-VOICE-1 (extended) (§7).
- **Agent hints:** Kupo (≤2 files, mechanical: `tests/tui_theme.rs`, `src/tui/strings.rs`). Gate: `make test`.
- **Risk:** P1.

---

## 6. RISK REGISTER + VALIDATION GATES

### 6.1 Risk register

| ID | Risk | Blast radius | Likelihood | Severity | Mitigation | Gate |
|---|---|---|---|---|---|---|
| R1 | **Thread/child leak** — `logs -f` child or stream thread survives pane-close/quit. | New thread + child PID; leaks compound across sessions. | Med | **P0** | Single spawn point (`Effect::StreamLogs`); single teardown `stop_log_stream` called on pane-close, swap, Quit arm, AND post-loop. Always set-flag-then-join (never detach). kill+reap (`real.rs:93-97`). | G-LOGLEAK |
| R2 | **Wedged UI** under high log throughput or never-exiting child on the wrong thread. | Whole TUI freezes. | Med | **P0** | GAP-2: stream on a DEDICATED thread, never the worker. GAP-1: coalesce + `try_send` drop-on-full + bounded ring drop-oldest. | G-NOWEDGE |
| R3 | **Secrets on disk** — un-redacted argv persisted to `history.jsonl`. | Host file survives sessions. | Med | **P0** | `redact_argv` deny-pattern scrubber over whole argv (env values, token/secret/password/apikey patterns); `secret set` value never in argv anyway (secret.rs:124-147). | G-REDACT |
| R4 | **Mouse capture left on after panic** — terminal emits escape codes after crash. | User's terminal corrupted. | Low | **P1** | `DisableMouseCapture` inside the shared `restore_terminal_once`, which the panic hook already calls (`app.rs:85`). | G-MOUSE-RESTORE |
| R5 | **Corrupt/missing history file** crashes the TUI at startup/overlay-open. | TUI won't launch / overlay panics. | Low | **P1** | `load()` skips unparseable lines; missing → empty; never panic. | G-HISTSAFE |
| R6 | **Mock breakage** — adding `run_stream`/`RunMode::Stream` forces every existing Mock to implement it. | All existing suites. | Low | **P1** | `run_stream` trait **default impl** ("unsupported"); `RunMode::Stream` is additive (no exhaustive match elsewhere breaks if `_ =>` arms exist — verify). | G-COMPAT |
| R7 | **Channel contention** — stream thread + worker both produce to the 32-slot result channel; coalescing reduces but doesn't eliminate pressure. | Dropped log chunks (degradation, not failure). | Low | **P2** | Documented GAP-1 degradation; coalescing keeps chunk rate low; `try_send` never blocks. | G-NOWEDGE |
| R8 | **Scope creep** toward the deferred dashboard/focus model. | Architecture drift. | Low | **P1** | `Screen::Logs` is a single full-screen screen (NOT a pane); no Rects on Model; scroll-only mouse. Reviewer checks no focus model introduced. | Review |

### 6.2 Redaction rule (precise)

`redact_argv(argv: &str) -> String`:
1. Tokenize on spaces (argv is already space-joined — `cmdlog.rs:110`).
2. For any token of shape `KEY=VALUE` where `KEY` (case-insensitive) contains any of: `token`, `secret`, `password`, `passwd`, `apikey`, `api_key`, `api-key`, `pass`, `key` → replace with `KEY=<redacted>`. (Tune the deny-list; `key` may be too broad — recommend the explicit list above without bare `key`.)
3. For `--env KEY=VALUE` (plaintext env, argv.rs:98-102) → replace VALUE with `<redacted>` unconditionally (env values are user-supplied and may be sensitive). The persist=true `--env KEY` form (argv.rs:91-96) is value-less → unchanged.
4. Everything else (subcommands, box names, image refs, `--label cbox.*`, flags) passes through verbatim.
5. The result is what gets written to disk AND shown in the History overlay (so the overlay never reveals a secret either).

### 6.3 Validation gates (containerized — `make` targets)

| Gate | Command | Asserts |
|---|---|---|
| G-BUILD | `make build` | tui feature compiles. |
| G-BUILD-LEAN | `make lint-lean` (`cargo clippy --no-default-features`) | lean (no-tui) build still compiles — pure modules (`logstream`, `history`, `runner`) stay feature-clean. |
| G-LINT | `make lint` (`clippy --all-features -D warnings`) | no warnings. |
| G-TEST | `make test` (`cargo test`) | all suites incl. new `tests/tui_bundle3.rs` green; existing tui_update, tui_effects, tui_theme, tui_bundle1, tui_bundle2, tui_keymap, tui_smoke, CLI suites green. |
| G-FINAL | `make check` (fmt-check + lint + lint-lean + build + release + test) | full release gate before the v0.11.0 PR. |
| G-LOGLEAK | unit (S7 cancel seam) + manual smoke | stop flag set → kill invoked → thread joined; no orphan after quit. |
| G-NOWEDGE | unit (coalescer/ring) + manual high-throughput smoke | UI responsive under flood; chunks/lines degrade gracefully. |
| G-REDACT | unit (AC-HIST-REDACT) | secret-bearing argv scrubbed before write. |
| G-HISTSAFE | unit (AC-HIST-CORRUPT) | corrupt/missing file → empty, no panic. |
| G-MOUSE-RESTORE | code review + smoke | DisableMouseCapture in restore path (covers panic). |
| G-COMPAT | `make test` | existing Mocks unchanged (default `run_stream`). |

---

## 7. ACCEPTANCE CRITERIA (GIVEN / WHEN / THEN)

All ACs are testable against **PURE helpers** (mirroring Bundles 1–2 — no real TTY, no real distrobox). New test file: **`tests/tui_bundle3.rs`** (compiled under `cfg(feature = "tui")` for reducer/model parts; pure-helper tests — coalescer, log ring, argv, redaction, history I/O over a temp dir — run unconditionally, matching the bundle2 file split).

### Streaming
- **AC-LOGARGV-1** — GIVEN id `"abc123"`; WHEN `build_logs_argv("abc123")`; THEN `["logs","-f","abc123"]`. (Program = backend, verified separately via `core::logs` building the Invocation with `spec.backend.as_str()`.)
- **AC-COALESCE-1** — GIVEN a fresh `LogCoalescer`; WHEN `FLUSH_LINES` lines pushed; THEN `due()` is true and `drain()` returns exactly those lines in order, buffer empty after.
- **AC-COALESCE-2** — GIVEN fewer than `FLUSH_LINES` lines but elapsed ≥ `FLUSH_INTERVAL_MS` (injected elapsed); WHEN checked; THEN flush is due (time-based trigger). With <lines AND <interval → not due.
- **AC-LOGRING-1** — GIVEN a `LogBuffer` cap N; WHEN `push_chunk` drives total > N; THEN `lines()` yields exactly the most recent N (oldest scrolled off), order preserved.
- **AC-STREAM-STOP-1** — GIVEN a `MockStreamRunner` whose `run_stream` loops emitting lines and checks the `stop` flag; WHEN the test sets `stop=true`; THEN the loop exits, the mock records a `kill()` call, and `run_stream` returns. (The seam: `run_stream(inv, on_line, stop)` takes `&AtomicBool`, so cancel is unit-testable with NO real child.)
- **AC-STREAM-EOF-1** — GIVEN a `MockStreamRunner` whose source ends after K lines (EOF); WHEN driven; THEN `on_line` fires K times and `run_stream` returns the exit code without panicking; downstream `LogStreamEnded` is producible.
- **AC-STREAM-SWAP-1** — GIVEN reducer with `log_target = box A` (streaming); WHEN selection changes to box B and Logs is (re)opened; THEN reducer emits `StopLogs` then `StreamLogs{B}` (or the shell-swap contract: StreamLogs{B} with a different target triggers teardown-then-start). Assert the effect sequence.
- **AC-STREAM-QUIT-1** — GIVEN `Screen::Logs` active with a target; WHEN Quit (Ctrl-C/q); THEN effects include `Effect::StopLogs` (and shell teardown runs post-loop). Assert `StopLogs` present in the Quit effect set.

### Mouse
- **AC-MOUSE-1** — GIVEN `scroll_delta(Mouse::ScrollDown)` / `ScrollUp`; THEN `+1` / `-1` (and `Mouse::Other → 0`, no-op).
- **AC-MOUSE-2** — GIVEN `Screen::List`, 3 boxes, selected=0; WHEN `Message::Mouse(ScrollDown)`; THEN selection becomes 1 (reuses `move_down`); `ScrollUp` at 0 stays 0.
- **AC-MOUSE-3** — GIVEN `Screen::Logs` with autoscroll on and scrollback available; WHEN `Message::Mouse(ScrollUp)`; THEN `log_scroll` decreases by SCROLL_STEP (clamped) and autoscroll turns OFF; scrolling back to bottom re-enables autoscroll. GIVEN `Overlay::CommandLog{scroll}` / `Overlay::History` → scroll/cursor moves by the documented step.

### Action history
- **AC-HIST-REDACT-1** — GIVEN argv `"podman create --additional-flags --env DB_PASSWORD=hunter2 ..."` (or `"... --env API_TOKEN=abc ..."`); WHEN `redact_argv`; THEN the value is `<redacted>`, the key and the rest of the argv are intact.
- **AC-HIST-REDACT-2** — GIVEN argv with NO secret-shaped token (e.g. `"distrobox create --name web"`); WHEN `redact_argv`; THEN output equals input (no false redaction).
- **AC-HIST-ROUNDTRIP-1** — GIVEN a temp `$XDG_STATE_HOME`; WHEN `append` 3 redacted entries then `load`; THEN the 3 entries reload in order with matching argv/status/ts.
- **AC-HIST-CAP-1** — GIVEN `HISTORY_CAP` = N and N+5 appends; WHEN `load`; THEN exactly N most-recent entries returned (oldest dropped).
- **AC-HIST-CORRUPT-1** — GIVEN a `history.jsonl` containing a garbage / truncated line among valid lines; WHEN `load`; THEN the valid entries return, the bad line is skipped, NO panic. GIVEN a missing file → empty `Vec`, no panic.
- **AC-HIST-PATH-1** — GIVEN `XDG_STATE_HOME` set → path is `$XDG_STATE_HOME/cbox/history.jsonl`; GIVEN unset → `~/.local/state/cbox/history.jsonl` (mirrors `update.rs:1299-1303`).
- **AC-HIST-CAPTURE-1** — GIVEN a `LoggingRunner` wrapping a Mock + an in-memory/temp `HistoryStore`; WHEN a non-DryRun `run` executes; THEN one redacted entry is appended; WHEN a DryRun `run` executes; THEN NO entry is appended (parity with the in-memory CmdLog skip, `cmdlog.rs:111`).
- **AC-HIST-FUZZY-1** — GIVEN history argvs `["distrobox create web","podman ps","distrobox rm web"]` and query `"web"`; WHEN `fuzzy_rank`; THEN the two `web` entries rank above `podman ps` (ordering assertion, mirrors AC-FILTER).
- **AC-HIST-OPEN-1** — GIVEN List screen; WHEN the history key (e.g. `H`) is pressed; THEN `model.overlay == Overlay::History{..}` with entries loaded; Esc → `Overlay::None`.

### Log screen
- **AC-LOGSCREEN-1** — GIVEN List with a selected box; WHEN the logs key (e.g. `L`); THEN `model.screen == Screen::Logs`, `log_target == Some((id,backend))`, and `Effect::StreamLogs{..}` is emitted for that box.
- **AC-LOGTOGGLE-1** — GIVEN `Screen::Logs`; WHEN the autoscroll toggle key; THEN `log_autoscroll` flips; WHEN the wrap toggle key; THEN `log_wrap` flips. (Pure reducer assertions.)
- **AC-LOGOPEN-1** — GIVEN List with NO box selected; WHEN the logs key; THEN no `Screen::Logs` transition / no `StreamLogs` (graceful no-op).

### Voice
- **AC-COPY-1 (extended)** — every new `strings.rs` const (LOGS_TITLE/EMPTY/ENDED/HINT, HISTORY_TITLE/EMPTY/HINT) is non-empty and free of `BANNED` adjectives (`tui_theme.rs:351-358`).
- **AC-VOICE-1 (extended)** — `Action::History.label()` (and all `Action::label()`) contains no banned adjective; verbs only.

### Regression
- **AC-REGRESSION-1** — `tui_update`, `tui_effects`, `tui_theme`, `tui_bundle1`, `tui_bundle2`, `tui_keymap`, `tui_smoke`, and all CLI suites pass unchanged (`make test`). `make lint-lean` proves the no-tui build still compiles (new pure modules feature-clean).

---

## 8. DECOMPOSITION & SEQUENCING

ATLAS-recommended order, dependency-ordered (cheap/self-contained → reuse → A-XL):

```
Phase 1 (mouse — cheap, self-contained):        S1
Phase 2 (history — reuses overlay+fuzzy):       S2 → S3 → S4
Phase 3 (streaming — A-XL):                     S5 → S6 → S7 → S8
Phase 4 (voice + gate):                         K1 → FINAL
```

**Execution order:** `S1, S2, S3, S4, S5, S6, S7, S8, K1, FINAL`
**Critical path:** `S5 → S6 → S7 → S8` (streaming) — the highest-risk chain; S7 to Reasoner.
**Parallelizable:** S1 (mouse) and S2 (history store) are independent and may proceed concurrently if capacity allows; S5+S6 are independent of the history chain.

**Kupo-eligible (localized ≤2-file mechanical):** S3 (`cmdlog.rs` + `app.rs`), K1 (`tui_theme.rs` + `strings.rs`).

**Per-task `make` verifier:** each story names its gate(s) in §5.3; FINAL = `make check`.

---

## 9. TEST — 6-layer verification (self-check)

| Layer | Result |
|---|---|
| Structural | Hierarchy PROJECT→FEATURE→STORY→TASK intact; 9 stories, no orphans; all INVEST-compliant; all timeboxes ≤3d. |
| Self-consistency | Three decompositions (by-feature / by-risk / by-file-touch) converge ≥80% on the S1→S2..S4→S5..S8 ordering. Stable. |
| Dependency | Every file:line anchor verified against current main @ v0.10.0 (see §3 + inline cites). New files: `tui/logstream.rs`, `tui/history.rs`, `tests/tui_bundle3.rs`. Capture point, XDG pattern, ring pattern, kill+reap, restore path all confirmed. |
| Constraint | No focus/pane model; no Rects on Model; scroll-only; container logs only; secrets redacted; voice rule extended; single worker thread untouched (stream on dedicated thread). All GAP resolutions honored verbatim. |
| Process reward | Ordering reduces risk monotonically (cheap mouse → reusing history → A-XL streaming last); each story has its own gate. |
| Adversarial | Checked: under-spec (cancel seam made unit-testable), dependency blindness (default `run_stream` for Mocks; `RunMode::Stream` additive), assumption drift (A4 secret-in-argv → deny-pattern scrubber), scope creep (Screen vs pane explicitly bounded), stale context (all anchors re-read). |

---

## 10. CONFIDENCE

| Factor (25% each) | Score | Note |
|---|---|---|
| Pattern match | 0.92 | Every feature ADAPTs a verified in-repo pattern (CmdLog ring, run_with_timeout child, Overlay/fuzzy, AtomicBool restore). |
| Requirement clarity | 0.95 | Feature set + 6 GAP resolutions LOCKED by human; zero open product decisions. |
| Decomposition stability | 0.85 | ≥80% self-consistency across three decompositions. |
| Constraint compliance | 0.90 | All deferrals/constraints encoded as gates + adversarial checks. |
| **Weighted confidence** | **0.905** | **≥85% → AUTO_PROCEED.** |

**Decision: AUTO_PROCEED.** Hand to Vivi (Builder) with S7 to a Reasoner-class agent and S3/K1 to Kupo.

---

## 11. FUTURE WORK (deferred — do NOT implement now)

- Persistent multi-pane dashboard + focus model (would introduce focus/pane state and layout Rects on Model).
- Click-to-select mouse (hit-testing against rendered Rects).
- Streaming apply/provisioning output into the log pane (different source).
- Re-run a history entry from the History overlay (currently read-only review).

---

*SPECTRA 4.9.1 — Bundle 3 "Glass Cockpit" (FOCUSED). Plan only; no code produced. All paths absolute under `.spectra/plans/`.*
