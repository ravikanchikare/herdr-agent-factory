# Review instructions — AI in the PR review loop (Stage 5 · Deploy)

All PRs get an identical set of review passes, findings ranked by severity. Human attention moves to intent/risk; agents handle mechanical policy checks. Findings never approve or block a PR on their own — branch protection requires a human code owner's approval.

## Passes

Run three passes and tag each finding with its pass:

- **Bugs** — logic errors, broken edge cases, subtle regressions, authority boundary violations (e.g. copying Herdr/Git live state into Rust ledger, React owning domain state machine).
- **Security** — injection, auth gaps, PII/secrets in logs or diffs, filesystem traversal/escaping symlinks, credential leakage (Keychain vs SQLite), loopback gateway bypass, plugin transport violations (HTTPS/size/digest/signature, unsafe redirects).
- **Compliance** — change matches `intent/<slug>/intent.md` + `spec.md` + `plan.md`; `crates/runtime-contract` is sole authority (no hand-edited `packages/shared/runtime-client`); static UI constraints (no Next.js server/API routes); Conventional Commit usage; `AGENTS.md`/`CLAUDE.md` invariants.

## Severity

- **Important** — would break behavior, leak data, breach an authority invariant or policy. Must be resolved before merge.
- **Nit** — style, naming, minor duplication. Report at most **five nits** per review; summarize the rest as a count.

## What to skip

- Generated files under `packages/shared/runtime-client/`, `target/`, `apps/web-ui/.next/`, `crates/runtime-contract/generated`
- Anything CI already enforces (`pnpm validate`, `cargo clippy -D warnings`, `contracts:check`)

## Human threshold

- Findings are advisory. A platform engineer may gate merges on severity counts via the check run's machine-readable tally, but the required approval is always a human code owner through branch protection.
- Tagging `@claude` on a review comment asks Claude to address the comment and push the fix; the PR thread records request + change. For PRs Claude opened, a slash command may sweep unresolved comments + failing checks until only code-owner approval remains.

## Feedback into CLAUDE.md

When a review flags the same mistake a second time, the correction goes into `CLAUDE.md` as part of that review. Review also flags when `CLAUDE.md` is stale.

## Governance

Separation of duties: the agent that wrote the code cannot approve it. Findings, fixes, ratings, and approvals are logged in PR history — the PR is the audit record.
