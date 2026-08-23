# Intent: <short title>

Author: <name> (<team / role)> · Status: draft · Date: YYYY-MM-DD
External record: <Jira/Linear ID or —>

## Problem

What cannot be done today, who is affected, and the cost of not fixing it.
1–3 sentences in the originator's own words.

## Proposed outcome

What better looks like when this is done. Observable, not implementation.

## Affected users and systems

Users, teams, surfaces, APIs, workspaces. E.g. claims handlers, portal team,
Herdr Workspace `target-agent`, claims-core API, `apps/web-ui` shell.

## Constraints

Non-negotiables the solution must respect. E.g. no new PII in portal session,
existing auth only, Herdr owns agent lifecycle, Rust owns durable ledger.

## Out of scope

Explicitly not doing, to prevent scope creep.

## Success criteria

How the product owner will know this succeeded (measurable where possible).

## Open questions

- Do third-party loss adjusters need access too?
- …

## Constraints from policy

List applicable skills that will bound the spec: e.g. `herdr-authority`,
`rust-ledger`, `agent-factory-architecture`, brand / security / UX skills.

---

*Template version: 1. Review with product owner before committing. Accepted
intent triggers Stage 2 — Design (`spec.md`).*
