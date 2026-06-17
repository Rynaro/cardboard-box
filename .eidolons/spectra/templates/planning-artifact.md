---
artifact: planning-artifact
version: 4.9.1
---

# Planning Artifact — SPECTRA Output Template

SPECTRA produces dual-format output: human-readable Markdown + agent-executable
structured data (YAML/JSON). Plans are never code.

## Output Contract

Every SPECTRA Assemble phase produces:
1. **Markdown spec** — human-readable, reviewer-friendly
2. **YAML/JSON block** — agent-executable structured data

## Full Template Library

The complete template library (one template per phase artifact) is at:

[`docs/spectra-methodology/templates.md`](../docs/spectra-methodology/templates.md)

Templates include:
- Scope artifact (intent classification, complexity score, boundaries)
- Pattern catalog (existing patterns, anti-patterns)
- Hypothesis table (3–5 hypotheses × 7-dimension rubric)
- Feature Story (GIVEN/WHEN/THEN acceptance criteria)
- Test verification checklist (6-layer)
- Refinement log
- Final assembly artifact (dual-format)

---

## ECL Envelope Sidecar

When `ECL_VERSION` is present in the install root, every Assemble output includes a fourth file alongside the Markdown + YAML + state.json triple:

**File location:** `<payload>.envelope.json` — sibling of the Markdown spec at `.spectra/plans/{date}-{feature}.envelope.json`.

**Required fields (per ECL v1.0 §1.1):**

| Field | Value |
|-------|-------|
| `envelope_version` | `"1.0"` |
| `message_id` | UUIDv7 (unique per emission) |
| `thread_id` | UUIDv7 (same for all envelopes in a mission) |
| `parent_id` | `null` (SPECTRA is the thread initiator on this edge) |
| `from.eidolon` | `"spectra"` |
| `from.version` | SemVer of the installed SPECTRA |
| `to.eidolon` | `"apivr"` |
| `performative` | `"PROPOSE"` |
| `artifact.kind` | `"spec"` |
| `artifact.sha256` | sha256 hex digest of the Markdown payload bytes |
| `integrity.method` | `"sha256"` |
| `integrity.value` | MUST equal `artifact.sha256` |
| `trace.ts` | RFC 3339 UTC timestamp at emit time |
| `trace.host` | Host environment slug (e.g. `claude-code`) |
| `trace.model` | Model identifier (e.g. `claude-sonnet-4-6`) |
| `trace.tier` | `"standard"` (or `"trance"` for TRANCE-tier sessions) |

**sha256 anchor:** The integrity check is the hex digest of the Markdown file bytes at the moment of emission. APIVR-Δ verifies this before acting on the spec: `shasum -a 256 <payload>.md | awk '{print $1}'` MUST match `integrity.value`.

**When emitted:** Only when `ECL_VERSION` is present in the install root. Non-ECL consumers ignore the file entirely.

**Template:** Use `templates/spec.envelope.json` as the skeleton — fill every `<placeholder>` before emitting. Validate against `schemas/ecl-envelope.v1.json` before handing off.

---

*SPECTRA — Planning Artifact Template*
