---
name: spectra-discover
description: "Open-ended requirements & stakeholder/goal elicitation BEFORE CLARIFY. Use when the GOAL itself is underspecified (IDEA/STRATEGIC intent) — distinct from CLARIFY, which disambiguates an already-known goal. Read-only, bounded; produces an elicitation summary, never a plan."
metadata:
  methodology: SPECTRA
---

# SPECTRA — DISCOVER Skill (pre-CLARIFY elicitation)

Use this skill when the request's **objective itself is unknown or latent** — not
merely when the spec details are ambiguous. DISCOVER runs BEFORE CLARIFY and hands
CLARIFY a structured elicitation summary.

## When to run DISCOVER (vs CLARIFY)

| | DISCOVER | CLARIFY |
|---|---|---|
| **Precondition** | Goal is unknown / latent | Goal is known; spec details ambiguous |
| **Intent types** | `IDEA`, `STRATEGIC`, any under-GOALED request | `REQUEST`, `CHANGE`, `BUG_SPEC` |
| **Question style** | Open-ended discovery of goals, stakeholders, success | ≤3 plan-shape disambiguation questions |
| **Output** | Elicitation summary → handed to CLARIFY | WHO/WHAT/WHY/CONSTRAINTS parse → Scope |

**Boundary rule:** DISCOVER = "what are we even trying to achieve, and for whom?"
CLARIFY = "the goal is clear; which of these plan shapes do you want?" Do NOT run
DISCOVER on a well-GOALED request — go straight to CLARIFY.

Why this exists: specification/system-design is the dominant multi-agent failure
category (MAST — ~43.8% of failures), and multi-agent systems collapse toward ~30%
accuracy when latent stakeholder knowledge is never actively elicited (HiddenBench).
CLARIFY alone cannot close this — its ≤3 plan-shape contract assumes the goal is
already known. See DESIGN-RATIONALE.md DR-10.

## Elicitation protocol (bounded, read-only)

Surface, do not assume. For each axis, record what is known and emit a `[GAP]`
marker for each unknown rather than inventing an answer:

1. **Stakeholders** — Who requests this? Who is affected? Who approves/reviews?
   Map the approval chain. `[GAP]` for each unidentified party.
2. **Latent goals** — What is the underlying outcome (not the surface ask)?
   What is the "job to be done"? Distinguish the stated request from the real goal.
3. **Success metrics** — How will we know it worked? Target metric + current baseline
   (e.g. "p95 < 200ms, currently 800ms"). `[GAP]` when no measurable criterion exists.
4. **Hard constraints** — Budget, deadline, tech-stack lock-in, compliance, platform.
5. **Non-goals** — What is explicitly OUT of scope? Surface these early to prevent
   scope creep downstream.

## Bound (D5 — no unbounded interview loop)

DISCOVER is elicitation + synthesis, NOT an interactive multi-turn interview agent.
- Produce ONE elicitation summary in a single pass from available context + the host
  conversation. Do not loop.
- If coverage is low (e.g. ≥2 of the 5 axes are `[GAP]` with no resolution path),
  **escalate to the human** with the gap list — do not fabricate goals to proceed.
- DISCOVER NEVER produces a plan and NEVER writes code (D2). Its sole output is the
  elicitation summary handed to CLARIFY.

## Output: elicitation summary

A compact structured block (lives under `.spectra/` like all SPECTRA output):

```
## DISCOVER — Elicitation Summary
- Stakeholders: <list | [GAP]>
- Latent goal: <restated outcome | [GAP]>
- Success metrics: <metric + baseline | [GAP]>
- Hard constraints: <list>
- Non-goals: <list>
- Open gaps: [GAP] <each unknown>
- Coverage: <n/5 axes resolved>  → handing to CLARIFY | ESCALATE to human
```

## Hard constraints (P0)

1. READ-ONLY. No code, no file edits, no mutations (D2).
2. Bounded — single-pass synthesis, `[GAP]`-and-escalate, never an interview loop (D5).
3. Emit `[GAP]` for every unknown; never assume latent intent (D7).
4. DISCOVER hands its summary to CLARIFY; it does not itself produce a plan.
5. Every output path lives under `.spectra/` (see SPEC.md Output Discipline).

See `SPEC.md` "## DISCOVER (pre-CLARIFY, open-ended elicitation)" for the full
methodology section.

---

*SPECTRA — DISCOVER sub-mode*
