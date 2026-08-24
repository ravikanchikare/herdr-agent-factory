# Plan: <slug> (from intent.md YYYY-MM-DD)

Author: <engineer> · Status: draft · Spec: `../spec.md`

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

Most risky step, blast radius, what could break. Note protected
paths that hooks will block.

## Proof

How the change is verified before review:

- `pnpm test` / `cargo test --workspace` — which suites, which files
- Screenshot vs agreed mock (for UI)
- Extent of `plan.md` ↔ diff fidelity required

## Alternatives considered

Options considered and why they were not taken.

---

Commit the accepted plan beside `intent.md` / `spec.md`. PR review
checks the eventual diff against it. Update this file in the same
commit when implementation departs from the plan.
