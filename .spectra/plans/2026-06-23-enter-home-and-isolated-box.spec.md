# Spec — Enter-redirect to box home + Fully-isolated box

**Date:** 2026-06-23
**Status:** decision-ready (authored by orchestrator from ATLAS scout + source re-verification; the SPECTRA subagent was unavailable due to a classifier outage)
**Reports:** (R1) entering a box from outside its creation dir leaves you at the host CWD — users expect to land in the box home. (R2) users want a box fully isolated from host shells (apps/zsh config bleed through the shared `$HOME`).

## Locked product decisions
- **DECISION-A:** redirect-to-box-home on `enter` applies to **all boxes, on by default**. Escape hatch: `cbox enter --no-home`. Explicit-command enter (`cbox enter NAME -- <cmd>`) is unchanged.
- **DECISION-B:** isolated box is configured via **both** a Boxfile field (`[box] isolated`) and a CLI flag (`--isolated`), mirroring how `--home` / `[box] home` are dual-exposed.
- **DECISION-C:** an isolated box's private home defaults to **`$XDG_DATA_HOME/cbox/homes/<name>`** (fallback `~/.local/share/cbox/homes/<name>`).

---

## FEATURE-1 — Land in box `$HOME` on enter

### Mechanism (resolves GAP-1)
`distrobox enter` preserves the host CWD inside the container (that is the reported root cause — `build_enter_argv` injects no working directory, `src/dbox/argv.rs:151-164`, and `Invocation` has no `cwd` field, `src/dbox/runner.rs:20-51`). Rather than depend on unverified distrobox CWD-override behavior, redirect **in-shell**: when no explicit `-- <cmd>` is given and `--no-home` is absent, build

```
distrobox enter --name NAME [--root] [--clean-path] -- sh -lc 'cd "$HOME"; exec "${SHELL:-/bin/sh}" -l'
```

Rationale: works regardless of distrobox's CWD handling; `$HOME` is whatever distrobox set for that box (the host home for shared boxes, the private path for isolated boxes — so this fix and FEATURE-2 compose); `exec` replaces the process so the user gets a normal interactive login shell with correct exit-code propagation; `${SHELL:-/bin/sh}` avoids hardcoding bash. We use a bootstrap `sh -lc` (POSIX, always present) and `exec` the user's `$SHELL` so we don't assume the box's login shell.

**Alternatives considered & rejected:** (a) adding a real `cwd` field to `Invocation`/the runner sets the *host-side* child CWD, which distrobox does not reliably translate into the container — unverifiable and fragile; (b) distrobox-native flags — no portable "enter at home" flag exists at the 1.6.0 floor.

### Changes
- `EnterArgs` (`src/cli/enter.rs:8-25`): add `#[arg(long)] pub no_home: bool` (`--no-home`, "Stay in the current directory instead of the box home").
- `EnterSpec` (`src/core/spec.rs:124-133`): add `pub home_landing: bool`.
- `cli/enter.rs::run` (27-65): set `home_landing: !args.no_home && args.cmd.is_empty()` — explicit command always wins, so an explicit `-- cmd` disables landing automatically.
- `build_enter_argv` (`src/dbox/argv.rs:151-164`): when `spec.cmd.is_empty() && spec.home_landing`, append `["--", "sh", "-lc", "cd \"$HOME\"; exec \"${SHELL:-/bin/sh}\" -l"]`. Keep the existing explicit-cmd branch (`!spec.cmd.is_empty()`) exactly as is. The two branches are mutually exclusive.

### Edge cases
- `--no-home` → no injection, old behavior (preserve CWD).
- explicit `-- cmd` → `home_landing` is false, unchanged.
- `--clean-path` still emitted before `--`; `sh -lc` is reachable on a clean PATH (absolute fallback `/bin/sh`).
- `--root` enter: redirect targets root's `$HOME` inside the box — consistent and expected.

---

## FEATURE-2 — Fully-isolated box

### What `isolated=true` expands to
1. **Private home (DECISION-C):** synthesize `home = <XDG_DATA_HOME or ~/.local/share>/cbox/homes/<name>` when the user has **not** set an explicit home. This is the core of the fix — a distinct `$HOME` means host dotfiles/zsh/installed apps no longer bleed in. Combined with FEATURE-1, enter then lands in this private home.
2. **Namespace hardening:** default unshare set = **`process` + `ipc`** (i.e. `--unshare-process --unshare-ipc`). **Keep netns shared** so networking, DNS, and X11/Wayland display continue to work out of the box — full `--unshare-all`/netns isolation routinely breaks GUI apps and network access, defeating distrobox's purpose for most users. `--unshare-process` implies `--init`, so set `init = true` when isolated (matches `build_create_argv` line 64 / `force_init` semantics). Users who want more can still set `[sandbox] unshare = "all"` explicitly.

### Precedence (isolated + explicit home)
**Explicit home wins for the path; isolation hardening still applies.** If `isolated=true` AND (`[box] home` non-empty OR `--home` given), use the explicit home path but still apply the unshare/init hardening. This is the least-surprising rule (explicit user input is honored) and avoids a hard error on a reasonable combination. Emit an info hint: `"isolated: using your explicit home <path>"`.

### Schema / CLI / reconciliation
- `BoxConfig` (`src/boxfile/model.rs:180-189`): add `#[serde(default)] pub isolated: bool`. (Required because `deny_unknown_fields` is on; `#[serde(default)]` keeps backward compat — older Boxfiles parse with `isolated=false`.)
- `CreateArgs` (`src/cli/create.rs:16-65`): add `#[arg(long)] pub isolated: bool` (`--isolated`).
- `CreateSpec` (`src/core/spec.rs:43-99`): no new field strictly needed — isolation is *expanded* into existing `home`/`unshare`/`init` before/inside spec construction. (Optional: carry `isolated: bool` for output/labels; not required for behavior.)
- **Synthesis point:** in `spec_from_boxfile_model` (`create.rs:215-286`), after computing `home`, if `bf.box_config.isolated` and home is `None`, set `home = Some(synth_isolated_home(&bf.name))`; merge the default unshare set into `unshare` (union with any explicit `[sandbox] unshare`); set `init = true`.
- **CLI override** in `run_with_store` (145-172): `if args.isolated { /* apply same synthesis using spec.name, respecting an explicit spec.home */ }`. Factor the synthesis into one helper (`fn apply_isolation(spec: &mut CreateSpec)`) called from both paths so Boxfile and flag stay identical.
- `synth_isolated_home(name)`: `env XDG_DATA_HOME` if set & non-empty else `${HOME}/.local/share`, then `/cbox/homes/<name>`.

### Diff / convergence (critical — the DIFF GOTCHA)
`diff_boxfile_vs_live` (`src/core/diff.rs:12-171`) classifies a home change as **Recreate** by comparing the **raw** `bf.box_config.home` (line 99, empty→skipped) against the live `HOME` env. It never sees the synthesized CreateSpec home. So without a fix, toggling `isolated` on an existing shared-home box would skip the home check and **fail to recreate**.

**Fix:** compute an *effective Boxfile home* inside the diff: `effective = if bf.box_config.home non-empty { home } else if bf.box_config.isolated { synth_isolated_home(&bf.name) } else { "" }`, and use `effective` in place of `bf_home` at diff.rs:99-127. Then toggling isolated ↔ shared yields a correct `Recreate` DiffField. (Reuse the same `synth_isolated_home` helper so synthesis and diff can never drift.)

**unshare caveat:** `sandbox.unshare` is **not** recovered from live (`diff.rs:159-162`, "assume unchanged"). So the unshare portion of isolation won't itself produce a diff — acceptable because the **home change already forces Recreate**, which re-applies the unshare set. Document this; do not add live-unshare recovery in this change.

---

## Backward-compat / migration
- DECISION-A changes default `enter` behavior at v0.11.0. Document the `--no-home` escape hatch prominently. Commit the enter change as a documented behavior change — `feat(enter)` with a `BREAKING CHANGE:` footer (or `feat(enter)!`) so release-please surfaces it; isolated box as `feat(create): add isolated box flavor`.
- README command reference: update `cbox enter` (note default home landing + `--no-home`) and `cbox create` / Boxfile reference (`[box] isolated`, `--isolated`, the XDG home path, the default unshare set). Per the project "show don't tell" voice memo, demonstrate isolation with a concrete before/after rather than adjectives.

---

## Acceptance (GIVEN/WHEN/THEN)
```yaml
acceptance:
  - id: AC-1-default-home
    given: a running box NAME and the host CWD is /tmp
    when: cbox enter NAME   # no -- cmd, no --no-home
    then: build_enter_argv ends with ["--","sh","-lc","cd \"$HOME\"; exec \"${SHELL:-/bin/sh}\" -l"]
    test: unit (src/dbox/argv.rs tests) + tests/enter.rs (mock)
  - id: AC-2-no-home
    given: a running box NAME
    when: cbox enter NAME --no-home
    then: argv == ["enter","--name","NAME"] (no -- block)
    test: unit + tests/enter.rs
  - id: AC-3-explicit-cmd
    given: a running box NAME
    when: cbox enter NAME -- ls -la
    then: argv ends with ["--","ls","-la"] (no home-landing injection)
    test: unit + tests/enter.rs
  - id: AC-4-isolated-boxfile
    given: Boxfile with [box] isolated = true, no home set, name=devbox
    when: cbox create --file Boxfile (dry-run argv)
    then: argv contains --home <XDG>/cbox/homes/devbox, --unshare-process, --unshare-ipc, --init
    test: tests/create.rs (mock/dry-run) + unit argv
  - id: AC-5-isolated-flag
    given: cbox create devbox --isolated (dry-run)
    then: same synthesized home + unshare set as AC-4
    test: tests/create.rs
  - id: AC-6-isolated-explicit-home
    given: Boxfile [box] isolated=true and home="/custom/home"
    when: create
    then: --home /custom/home (explicit wins) AND --unshare-process/--unshare-ipc/--init still present
    test: tests/create.rs + unit
  - id: AC-7-diff-toggle
    given: a live shared-home box and a Boxfile flipping isolated false->true
    when: diff_boxfile_vs_live
    then: DiffResult.class == "Recreate" with a home DiffField (effective home synthesized)
    test: src/core/diff.rs unit tests
  - id: AC-8-smoke-enter-home
    given: a real distrobox box (make dist)
    when: cbox enter NAME -- pwd   # NOTE: smoke must drive default-landing path; assert via a wrapper that captures pwd at login
    then: working directory equals $HOME
    test: tests/smoke.rs #[ignore]
  - id: AC-9-smoke-isolation
    given: a real isolated box
    then: $HOME inside box != host $HOME; host ~/.zshrc not visible inside the box
    test: tests/smoke.rs #[ignore]
```

> AC-8 note for Vivi: a default interactive enter execs a login shell, so the smoke test can't just append `-- pwd` (that disables landing). Drive it by setting `SHELL` to a small script, or assert the *argv* in unit tests and verify *behavior* in smoke via `cbox enter NAME --no-home -- sh -lc 'cd "$HOME"; pwd'` compared against a direct `pwd`. Decide during implementation; the load-bearing guarantee is "default enter lands in $HOME".

---

## Test plan
| Story | Location | Type |
|-------|----------|------|
| AC-1,2,3 | `src/dbox/argv.rs` `#[cfg(test)]` (399-564) + `tests/argv_builder.rs`; `tests/enter.rs` | unit + mock behavior |
| AC-4,5,6 | `tests/create.rs`, `tests/up.rs`; argv unit | mock/dry-run + unit |
| AC-7 | `src/core/diff.rs` unit tests | unit |
| AC-8,9 | `tests/smoke.rs` `#[ignore]` (via `make dist` → `./dist/cbox`) | real distrobox |

---

## Complexity / risk
**Complexity: 6/12.** Mostly localized, well-isolated argv/spec/serde edits with strong existing test seams. **Risks:** (1) GAP-1 enter-landing behavior must be confirmed by smoke (AC-8) — the in-shell mechanism is the hedge; (2) the diff-gotcha is the one cross-cutting subtlety — get the shared `synth_isolated_home` helper right so diff and create never drift; (3) default-behavior change on enter needs clear docs + breaking-change commit.

## Ordered task breakdown (for Vivi)
1. **FEATURE-1 argv + spec** — add `home_landing` to EnterSpec, `--no-home` to EnterArgs, wire in `cli/enter.rs::run`, inject in `build_enter_argv`; unit tests AC-1/2/3. *(Kupo-eligible: argv.rs + enter.rs, ≤2 files, verifier = cargo test argv/enter — but it touches spec.rs too, so 3 files → Vivi.)*
2. **`synth_isolated_home` helper** — single source of truth (place in `core` so both create and diff import it). *(Kupo-eligible micro-task: 1 file + its unit test.)*
3. **FEATURE-2 model + create** — add `isolated` to `BoxConfig`, `--isolated` to `CreateArgs`, `apply_isolation` helper, wire into `spec_from_boxfile_model` + `run_with_store`; tests AC-4/5/6.
4. **FEATURE-2 diff fix** — effective-home in `diff_boxfile_vs_live`; test AC-7.
5. **Smoke tests** — AC-8/9 `#[ignore]` in `tests/smoke.rs`.
6. **Docs** — README enter + create/Boxfile reference; commit messages per release-please.

All builds/tests run containerized (`make build` / `make test` / `make lint`); smoke via `make dist` → `./dist/cbox`. Never run cargo on the host.
