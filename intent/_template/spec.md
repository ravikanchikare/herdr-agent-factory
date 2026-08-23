# Spec: <same slug as intent>

Derived from: `intent.md` (YYYY-MM-DD) · Status: draft · Author: <product owner>
Skills applied: <list skill names + versions at time of writing>
Prompt: <paste the prompt used to generate this spec>

## Requirements

Numbered, testable requirements distilled from the intent.

1. …
2. …

## Design

Architecture, UX, API, and data decisions constrained by the listed skills.
Include mock or Figma link for frontend work (Claude Design export if used).

### UX

…

### API / Contracts

… (`crates/runtime-contract` is authoritative; no hand-edited bindings)

### Data / Persistence

… (what is durable Factory ledger vs live Herdr/Git observation)

## Flagged concerns

Areas where policies conflict or cannot be fully satisfied. Each concern names
its policy owner and resolution status.

- [ ] <concern> — owner: <name> — status: open / resolved

## Out of scope

## Acceptance criteria

Maps 1-to-1 to intent success criteria.

## Risks & mitigations

---

*Accepted spec triggers Stage 3 — Build (plan mode, `plan.md`). Flagged
concerns must be resolved with policy owners before engineering starts.*
