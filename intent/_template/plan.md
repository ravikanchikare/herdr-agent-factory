# Plan: <slug> (from intent.md YYYY-MM-DD)

Author: <engineer> · Status: draft · Spec: `../spec.md` · CLAUDE.md: <commit SHA>

## Files that change

- `apps/web-ui/...` (new / edit)
- `crates/...` (…)
- `services/runtime/src/...` (…)
- Tests: `…`

## Order of work

1. …
2. …
3. …

## Risks

Most risky step, blast radius, what could break (e.g. claims-core rate-limit
50 rps → must cache). Note protected paths that hooks will block.

## Proof

How the change is verified before review:

- `make test` / `pnpm test` — which suites, which files
- Screenshot diff vs approved mock (for UI)
- Endpoint returns 200 with new field
- Extent of `plan.md` ↔ diff fidelity required

## Alternatives considered

Other options Claude chose not to do and why.

---

*Commit this approved plan as `plan.md` alongside `intent.md`/`spec.md`.
PR review checks the eventual diff against it. Update this file in the same
commit when implementation departs from the plan.*
