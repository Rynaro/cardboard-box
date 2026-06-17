# SPECTRA Output Templates

Phase-specific output formats. Load on-demand during Construct and Assemble phases.

---

## S — Scope Output

```markdown
## 🎯 SCOPE ANALYSIS

**Intent Type:** [IDEA/REQUEST/CHANGE/BUG_SPEC/STRATEGIC]
**Complexity Score:** [N]/12
**Thinking Budget:** [Standard/Extended/Collaborative]

**WHO:** [Actor/User type]
**WHAT:** [Capability needed]
**WHY:** [Business value / strategic alignment]
**CONSTRAINTS:** [Non-functional requirements, deadlines]

**Boundaries:**
| In Scope | Out of Scope | Deferred |
|----------|--------------|----------|
| [items] | [items] | [items] |

**Assumptions:**
1. [Assumption] — Risk if wrong: [impact]
```

---

## P — Pattern Output

```markdown
## 📚 PATTERN ANALYSIS

**Query:** "[terms]"
**Matches:** [N] patterns

| ID | Pattern | Similarity | Decision |
|----|---------|------------|----------|
| P1 | [name] | 87% | USE_TEMPLATE |
| P2 | [name] | 72% | ADAPT |
| P3 | [name] | 45% | CONTEXT_ONLY |

**Strategy:** [USE_TEMPLATE/ADAPT/GENERATE]
**Adaptations:** [if adapting]
```

---

## E — Exploration Output

```markdown
## 🌳 EXPLORATION SUMMARY

**Hypotheses:** [N] generated, top 2 expanded

| # | Name | Feas | Value | Risk | Pattern | Timebox | Total |
|---|------|------|-------|------|---------|---------|-------|
| 1 | [name] | 3 | 3 | 2 | 3 | 2 | 13 |
| 2 | [name] | 2 | 3 | 3 | 2 | 2 | 12 |

**Selected:** H[X] — [Name]
**Rationale:** [Why this over alternatives]
**Rejected:** [H2 reason], [H3 reason]
```

---

## C — Story Format (Action Plan)

```markdown
#### 📋 STORY: [ID] [Title]

> 🔵 [Info] | 🟡 [Warning] | 🔴 [Critical]

**Description:** As a [ACTOR], I want [CAPABILITY] so that [VALUE]
**Timebox:** [1d/≤2d/≤3d/≤5d/≤8d]
**Risk:** [P0/P1/P2]

## Action Plan:
1. **[Verb]:** [Step detail]
2. **[Verb]:** [Step detail]

## Acceptance Criteria:
- [ ] GIVEN [context] WHEN [action] THEN [outcome]

## Technical Context:
- **Pattern:** [name of pattern used in your stack]
- **Files:** [paths]
- **Dependencies:** [story IDs this depends on]

## Agent Hints:
- **Class:** [builder/reasoner/debugger/reviewer]
- **Context:** [exemplar paths]
- **Gates:** P0 checked, tests cover success+failure
```

---

## T — Test Output

```markdown
## ✅ VERIFICATION REPORT

| Layer | Check | Status |
|-------|-------|--------|
| Structural | Hierarchy intact, stories independent | ✓/✗ |
| Self-Consistency | N% overlap across 3 decompositions | ✓/✗ |
| Dependency | All affected files identified | ✓/✗ |
| Constraint | NFRs met, timeboxes realistic | ✓/✗ |
| Process Reward | Steps reduce risk progressively | ✓/✗ |
| Adversarial | Failure modes addressed | ✓/✗ |

**Self-Consistency:** [N]% overlap
**Constraints:** [N]/[N] passed
**Gate:** [PASS/REFINE/ESCALATE]
```

---

## R — Refine Output

```markdown
## 🔄 REFINEMENT LOG

### Cycle [N]
| Dimension | Before | After | Change |
|-----------|--------|-------|--------|
| Clarity | [N] | [N] | [what improved] |
| Completeness | [N] | [N] | [what improved] |
| Actionability | [N] | [N] | [what improved] |
| Efficiency | [N] | [N] | [what improved] |
| Testability | [N] | [N] | [what improved] |

**Diagnosis:** [What failed and why]
**Prescription:** [Specific fix applied]
**Exit:** [All ≥4 / Max cycles / Diminishing returns]
```

---

## A — Confidence Report

```markdown
## 📊 CONFIDENCE ASSESSMENT

| Factor | Score |
|--------|-------|
| Pattern Match | [N]/3 |
| Requirement Clarity | [N]/3 |
| Decomposition Stability | [N]/3 |
| Constraint Compliance | [N]/3 |

**Weighted Confidence:** [N]%
**Decision:** [AUTO_PROCEED/VALIDATE/COLLABORATE/ESCALATE]

**Gaps:** [if any]
```

---

## Agent Handoff (YAML)

```yaml
metadata:
  spec_id: "SPEC-YYYY-MM-DD-XXX"
  confidence: [N]
  complexity: [N]
  spectra_version: "4.9.1"

projects:
  - id: "P-1"
    name: "[Project Name]"
    features:
      - id: "F-1"
        name: "[Feature Name]"
        stories:
          - id: "S-1"
            title: "[Story Title]"
            timebox: "≤2d"
            risk: "P1"
            action_plan:
              - verb: "Create"
                target: "[specific target]"
              - verb: "Test"
                target: "[test scope]"
            acceptance_criteria:
              - given: "[context]"
                when: "[action]"
                then: "[outcome]"
            agent_hints:
              recommended_class: "builder"
              context_files: ["paths"]
              validation_gates:
                p0: "checked"
                coverage: "≥85%"

execution_plan:
  phases:
    - name: "[Phase Name]"
      stories: ["S-1"]
      agent_class: "builder"
```

---

## State Machine (JSON)

```json
{
  "session_id": "uuid",
  "spec_id": "SPEC-YYYY-MM-DD-XXX",
  "goal": "string",
  "spectra_version": "4.9.1",
  "steps": [
    {
      "id": 1,
      "story_id": "S-1",
      "title": "string",
      "status": "pending|in_progress|completed|blocked|failed",
      "dependencies": [],
      "files_affected": ["path/to/file"],
      "verification_command": "string",
      "estimated_timebox": "≤2d",
      "replanning_notes": null
    }
  ],
  "current_step": 0,
  "completed_steps": [],
  "replanning_history": []
}
```

---

## Convention Map (Installation Output)

Generated once during SPECTRA installation (see [RETROFIT.md](../research/RETROFIT.md) for brownfield projects). Saved at `.spectra/setup/spectra-conventions.md` in the consumer project — never in the project root, never duplicated into vendor-specific folders. Consumed by CLARIFY (step 4) and Pattern (step 2) as structural context.

```markdown
## 🗺️ CONVENTION MAP

**Project:** [name]
**Generated:** [date]
**Sources:** [list of convention files found + inference method]

### Quick Reference (Top 5 Conventions)
1. [Most important convention]
2. [Second most important]
3. [Third]
4. [Fourth]
5. [Fifth]

### Convention Mapping (SPECTRA → Project)
| SPECTRA Concept | This Project | Path Pattern | Exemplar |
|----------------|-------------|--------------|----------|
| Service / Business Logic | [convention] | [path] | [file] |
| Data Access | [convention] | [path] | [file] |
| Validation | [convention] | [path] | [file] |
| API Endpoint | [convention] | [path] | [file] |
| Test File | [convention] | [path] | [file] |

### Action Verb Mapping
| SPECTRA Verb | In This Project | Example |
|-------------|----------------|---------|
| Create | [convention] | [example] |
| Extend | [convention] | [example] |
| Modify | [convention] | [example] |
| Test | [convention] | [example] |

### Validation Gates (Default)
- P0: [critical gate]
- P1: [test gate]
- P2: [lint gate]

### Architectural Boundaries
[project-specific constraints that every SPECTRA spec must respect]

### Sources
| Convention | Source | Confidence |
|-----------|--------|------------|
| [convention] | [file + line or inference method] | HIGH/MEDIUM/LOW |
```

---

*SPECTRA — Output Templates*
