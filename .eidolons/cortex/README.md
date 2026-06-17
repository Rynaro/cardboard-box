# Cortex Deep Tables

Deep reference content for `EIDOLONS.md`. Loaded on demand per the progressive
disclosure principle (spec P3). The always-loaded section of `EIDOLONS.md` stays
under 900 tokens; detail that doesn't need to be resident in every session lives
here.

## Files in this directory

| File | Contents | Load when |
|------|----------|-----------|
| `handoff-graph.md` | Canonical hand-off graph (union of roster + composition.md edges with origin labels), disambiguation table, and routing open questions | Composing a multi-Eidolon chain or auditing edge provenance |
| `trance-matrix.md` | Per-Eidolon TRANCE capability matrix, cost ceiling rules, refusal gates | Evaluating or authorizing an TRANCE escalation |
| `validation-gates.md` | All 14 GIVEN/WHEN/THEN acceptance gates (V1–V14) the cortex must satisfy | Testing cortex behavior, writing new routing rules |
| `memory-protocol.md` | Full 8-tool CRYSTALIUM surface, layer × tier × operation matrix, Dream consolidation knobs, skill_invoke sandboxing, ECL-envelope → ingest mapping | Composing memory-aware chains, auditing tier gates, configuring Dream |

## Token budget note

Each file here is on-demand only. A host that loads `EIDOLONS.md` and then
hits a chain-composition step should load `handoff-graph.md`; a host evaluating
whether to escalate to TRANCE should load `trance-matrix.md`. Neither is needed
for simple single-Eidolon standard-tier dispatch.

The cortex itself (EIDOLONS.md always-loaded section) + one deep table still
stays well inside the ≤3500-token specialist working-set budget
(`methodology/prime-directives.md D1`).
