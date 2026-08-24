# Review instructions

Every PR gets the same passes. Findings never approve or block a PR
on their own — branch protection requires a human code owner's
approval. The author of a diff cannot approve it.

Check the diff against `intent/<slug>/spec.md` and `plan.md` when
those files exist for the change.

## Passes

Tag each finding with its pass:

- **Bugs** — logic errors, broken edge cases, subtle regressions,
  authority boundary violations (e.g. copying Herdr/Git live state
  into the Rust ledger, React owning a domain state machine).
- **Security** — injection, auth gaps, PII/secrets in logs or diffs,
  filesystem traversal/escaping symlinks, credential leakage
  (Keychain vs SQLite), loopback gateway bypass, plugin transport
  violations (HTTPS/size/digest/signature, unsafe redirects).
- **Compliance** — change matches `intent/<slug>/` artifacts;
  `crates/runtime-contract` is sole authority (no hand-edited
  `packages/shared/runtime-client`); static UI constraints (no
  Next.js server/API routes); Conventional Commit usage;
  `AGENTS.md` / `CLAUDE.md` invariants.

## Severity

- **Important** — would break behavior, leak data, or breach an
  authority invariant. Must be resolved before merge.
- **Nit** — style, naming, minor duplication. Report at most **five
  nits** per review; summarize the rest as a count.

## What to skip

- Generated files under `packages/shared/runtime-client/`,
  `target/`, `apps/web-ui/.next/`, `crates/runtime-contract/generated`
- Anything CI already enforces (`pnpm validate`,
  `cargo clippy -D warnings`, `contracts:check`)

## Human threshold

Findings are advisory. A platform engineer may gate merges on
severity counts via a check run, but the required approval is always
a human code owner through branch protection.

A review comment that asks the review agent to address a finding
should leave the request and the follow-up commit on the PR thread.

## Feedback into CLAUDE.md

When a review flags the same mistake a second time, the correction
goes into `CLAUDE.md` as part of that review. Review also flags when
`CLAUDE.md` is stale.

## Governance

Separation of duties: the agent that wrote the code cannot approve
it. Findings, fixes, and approvals are logged in PR history — the PR
is the audit record.
