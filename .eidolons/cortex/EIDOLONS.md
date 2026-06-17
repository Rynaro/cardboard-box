<!-- eidolon:cortex start -->
# EIDOLONS.md — Routing Cortex

> Always-loaded routing cortex for the Eidolons nexus.
> A host LLM reading this file once at session start can route any
> free-form prompt to the correct Eidolon(s), at the correct tier,
> in the correct chain. No vendor model names appear here — capability
> classes and tier ladder only (`light < standard < deep`). The concrete
> model per tier is set by the active profile in `roster/model-profiles.yaml`;
> use `eidolons model` to inspect or change it. See §DEEP for extended
> tables loaded on demand.

---

## Roster Index (always-loaded)

| Name | Capability class | Trigger verbs | Refuses | Hands off to |
|------|-----------------|---------------|---------|--------------|
| **ATLAS** | scout | map, trace, find where, who calls, build call graph, list entrypoints, audit (read-only) | implement, fix, edit, write, commit | SPECTRA, Vivi, IDG |
| **SPECTRA** | planner | spec, plan, decompose, clarify requirements, GIVEN/WHEN/THEN, decision-ready | implement code, modify files | Vivi, IDG |
| **Vivi** | coder (default) | implement, build, fix, extend, wire up, make tests pass — loop-native: drives `eidolons sandbox loop`, host-adaptive (iterate/fanout) | design from scratch, novel architecture | IDG |
| **APIVR-Δ** | coder (opt-in fallback) | named dispatch only ("APIVR-Δ, …"); conservative non-loop posture (`eidolons add apivr`) | design from scratch, novel architecture | IDG |
| **IDG** | scriber | document, ADR, runbook, chronicle, synthesize, record decisions | explore repo, find calls, retrieve | (terminal) |
| **FORGE** | reasoner | trade-off, which approach, ambiguous, counterfactual, deliberate | implement, retrieve, synthesize prose | (lateral consultant) |
| **VIGIL** | debugger | root cause, flaky, heisenbug, regression after X, post-mortem, why does this fail | build new feature, plan from scratch | (lateral specialist) |
| **Kupo** | executor | rename, import/path fix, lockfile bump, lint autofix, one-line fix, grep-replace (localized, verifier-backed micro-tasks) | design, plan, cross-cutting refactor (>2 files), loop-native campaign | (replies to delegating parent) |

> **Delegation target.** Any Eidolon may `DELEGATE` a localized, verifier-backed micro-task to **Kupo** (the low-effort executor) to keep its own session lean. Kupo patches an ephemeral sandbox, proves it with an external verifier, and PROPOSEs a verified patch back to the delegating parent — it never commits and never routes work onward (worker, never router).

---

## Dispatch Protocol (always-loaded)

**Default operating mode — delegate by default.** When this cortex is wired into a host, routing through the Eidolons pipeline is the **default**, not an opt-in. Every non-trivial request runs through Steps 1–5 below; the orchestrator delegates to the Eidolon role(s) and does **not** implement, spec, or scout directly. Answer directly **only** when the prompt is trivial, purely conversational, or a single-fact lookup. This makes *delegation* the default — the **tier** default is still `standard`; TRANCE remains gated (see Step 4), never automatic.

**Step 1 — Classify.** Extract verbs from the prompt. Match against trigger columns above. Score each Eidolon 0–1.

**Step 2 — Gate.**
- Score ≥ 0.8 for one Eidolon and ≤ 1 verb class: dispatch that Eidolon, standard tier.
- Score ≥ 0.6 for ≥ 2 Eidolons OR prompt spans ≥ 2 capability classes: build a chain (see Chain Templates below).
- No Eidolon scores ≥ 0.6: emit `clarification_request` with 1–3 targeted questions. Do not dispatch.

**Step 3 — Refusal check.** If the top-scored Eidolon would refuse the prompt's intent (see Refuses column), set `refusal_rerouting: true`, select the capable peer, emit `[DECISION]` explaining the override.

**Step 4 — Tier.** Default is `standard`. Escalate to `trance` only when **both** hold: (a) complexity flags fire AND (b) user supplies explicit `TRANCE` token or upstream Eidolon flags high-stakes. See TRANCE Activation Gates below.

**Step 5 — Emit routing artifact.**
```
selected: [<eidolon>, ...]
tier: standard | trance
chain: [{eidolon, role, hand_off_artifact_path, edge_origin}, ...]
model_tier_per_step: [light | standard | deep, ...]   # suggested tier per step
confidence: 0..1
assumptions: [...]       # [GAP]/[DISPUTED] when routing is ambiguous
clarification_request: <string?>
refusal_rerouting: <bool>
```

---

## Chain Templates (always-loaded)

| Template | Steps | When |
|----------|-------|------|
| **plan-before-build** | ATLAS → SPECTRA → Vivi → IDG | Unfamiliar code + multi-component change |
| **audit-without-touching** | ATLAS → IDG | "Audit", "explain", "review" with no write intent |
| **ship-fast** | SPECTRA → Vivi | Known terrain, scoped feature |
| **direct-implementation-bypass** | ATLAS → Vivi (skip SPECTRA) | Complexity < 7/12 AND small surface AND unambiguous reqs; emit `[DECISION]` |
| **decide-then-implement** | FORGE → SPECTRA → Vivi | "Should we use X or Y, then build it" |
| **forensic-then-fix** | VIGIL → Vivi | Bug with reproduction + verified patch suggestion |
| **failed-attempt-recovery** | (prior coder failure) → VIGIL → Vivi | Conversation shows prior coder Reflect-exhaustion |
| **decision-only** | FORGE | No code touching; deliberation emitting verdict + assumptions |

---

## TRANCE Activation Gates (always-loaded)

TRANCE grants: parallel fan-out (max 5 branches), worktree isolation per branch, verifier-cascade wrapping, evaluator-optimizer loop (cap 3 iterations), model-tier upgrade (lead = deep, workers = light).

TRANCE is **never** the default. Auto-trigger requires **both** a complexity flag AND a stakes flag. Cost warning emitted at ≥ 5× standard-tier budget.

| Gate | Eidolon | Condition |
|------|---------|-----------|
| G1 — Discovery scatter | ATLAS | Surface > 25 files OR > 5 modules → scatter sub-agents per module, aggregate via Abstract phase |
| G2 — Hard-decision consistency | FORGE | ≥ 3 plausible alternatives AND (high-stakes flag OR explicit TRANCE token) → N=3 reasoning traces, majority-vote |
| G3 — Spec evaluator-optimizer | SPECTRA | Complexity ≥ 7/12 AND (high-stakes OR ambiguous reqs) → generator + evaluator, max 3 iterations |
| G4 — Parallel implementation | Vivi | SPECTRA emitted > 1 independent story AND budget bounded → one Vivi per track, worktree isolation |
| G5 — Doc parallel synthesis | IDG | Large source artifact set AND topological order allows parallelism → per-section parallel, CHT per section, one-revision cap preserved |
| G6 — Forensic counterfactuals | VIGIL | ≥ 2 plausible root-cause hypotheses AND bisect surface allows independent testing → parallel hypothesis tests on isolated bisects |

**TRANCE refusals (immutable):** A refused capability does not become available at TRANCE. ATLAS still does not write. SPECTRA still does not implement. IDG still does not retrieve. FORGE still does not tool-call. VIGIL still does not auto-apply patches. Per-Eidolon retry budgets remain enforced inside TRANCE.

---

## Confidence Signals (always-loaded)

| Signal | Effect | Target |
|--------|--------|--------|
| Stack trace, panic, "still failing after retry" | +0.3 | VIGIL |
| Surface > 25 files or 5 modules | +0.2 | ATLAS-TRANCE |
| "Greenfield", "from scratch", "novel" | −0.3 | the coder class (Vivi and APIVR-Δ refuse greenfield) |
| "I don't have a spec yet" | +0.2 | SPECTRA |
| Prior failed coder attempt in conversation | +0.4 | VIGIL |
| Eidolon named explicitly in prompt | +0.5 | that Eidolon (still check refusal table) |
| Multiple SDLC phases ("scout and spec and build") | chain trigger | (see Chain Templates) |

---

## Memory protocol — CRYSTALIUM MCP (always-loaded)

When `crystalium` is installed (`grants_to_eidolons: all`), every dispatched Eidolon runs a mandatory pre-flight + post-flight against the harness. Direct file writes to crystalium's data dir are forbidden; **all reads and writes funnel through MCP tool calls** so the chokepoint can enforce tier × layer × operation.

**Wiring:** crystalium is `wiring_mode: allowlist/direct` — `mcp__crystalium__*` tools ARE injected into each Eidolon's agent `tools:` allowlist (same as atlas-aci; opposite of junction's transport-only wiring).

| Step | Tool call | When |
|------|-----------|------|
| Pre-flight | `mcp__crystalium__recall(scope, query, k=5, layers=[semantic, episodic])` | First action of every dispatch — recall relevant prior context before reasoning |
| Write spine | `mcp__crystalium__ingest(ecl_envelope)` | Primary persist path — ingest the ECL hand-off envelope; derives T1 identity from `from.eidolon`, preserves provenance + MIN-trust |
| Raw notes | `mcp__crystalium__commit(layer=episodic, payload, provenance)` | Direct episodic write — mission notes, intermediate findings |
| Plan gate | `mcp__crystalium__plan_checkpoint / plan_replan` | APIVR-Δ and FORGE execution-layer checkpoints (T0/T1 only) |
| Session end | `mcp__crystalium__session_end()` | Once per host disconnect — enqueues the Dream consolidation worker |

**Trust-tier map:**

| Tier | Callers | Permitted layers |
|------|---------|-----------------|
| T0 | Host / operator (CLI-only: forget, force_promote, review) | All four layers |
| T1 | ATLAS / SPECTRA / APIVR-Δ / IDG / FORGE / VIGIL | episodic, semantic, procedural, execution |
| T3 | Tool-origin artefacts (enter via `ingest` only) | episodic (quarantined); may never write semantic/procedural/execution |

**ECL → ingest mapping:** when a hand-off envelope is available, `ingest(ecl_envelope)` is the primary commit path. The field `from.eidolon` determines tier (T1 for all six Eidolons). `thread_id` scopes to the current chain run. SHA-256 provenance is preserved verbatim from the envelope's `integrity.value`.

**Refusals are mechanical:** if any crystalium tool returns `reason_code: TIER_VIOLATION`, `TIER_CEILING`, or `PROMOTION_GATE`, treat as terminal for that path — do **not** retry with a different tier, and do **not** mutate the data dir directly to bypass. The chokepoint catches what gets caught.

**Deep table:** `methodology/cortex/memory-protocol.md` — full 8-tool surface, layer × tier × operation matrix, Dream consolidation knobs, skill_invoke sandboxing, bi-temporal update rules.

**Append-only:** This section is the wire-time enforcement of crystalium usage. Removing it disconnects the team's memory. Edit only via additive amendments — do not delete columns or rows.

---

## Invariants

- **I-C1** — Marker-bounded sections when embedding into shared host files (`<!-- eidolon:cortex start/end -->`).
- **I-C2** — No `eval` of routing rules; descriptor table is data, dispatch is interpretive.
- **I-C3** — Capability classes + vendor-neutral tiers only (`light < standard < deep`). Never vendor model names.
- **I-C4** — Always-loaded section ≤ 900 tokens; deep tables in `methodology/cortex/`.
- **I-C5** — Refusals are immutable; cortex must never request a refused capability of a target Eidolon.
- **I-C6** — Same prompt + same context + same roster ⇒ same routing decision.
- **I-C7** — `roster/index.yaml` is the source of truth; new Eidolons auto-appear, removed Eidolons disappear. `roster/mcps.yaml` is the closed MCP catalogue; `eidolons mcp list|show|install|refresh|uninstall|upgrade|sync|health|run` is the unified verb set (v1.3+).
- **I-C8** — `[GAP]` and `[DISPUTED]` over silent merge when routing is genuinely ambiguous.
- **I-C9** — Bash 3.2 compatibility for any CLI helper consuming a cortex artifact.
- **I-C10** — Stderr discipline for all tooling logs; stdout reserved for captured values.

---

**ECL — Wire Format.** Inter-Eidolon hand-offs use the **Eidolons Communication Layer (ECL v1.0)** wire format: every emitted artefact carries a JSON sidecar envelope (`ecl-envelope.json`) containing the performative, sender/receiver identities, a SHA-256 integrity tag, and a JSONL trace stream. ECL is opt-in for v1.0; existing Eidolons remain conformant unchanged. Machine-readable hand-off contracts live at [`Rynaro/eidolons-ecl/contracts/`](https://github.com/Rynaro/eidolons-ecl/tree/main/contracts).

*Deep tables (TRANCE matrix, hand-off graph, disambiguation table, validation gates, open questions) load on demand from `methodology/cortex/`. See `methodology/cortex/README.md`.*

<!-- eidolon:cortex end -->
