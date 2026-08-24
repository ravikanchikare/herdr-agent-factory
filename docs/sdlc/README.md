# How we record and ship work

Agent Factory is built as a loop of committed artifacts. Each accepted
artifact is what the next step reads. Humans review at the gates;
agents draft the files in between.

```
intent.md ──► spec.md ──► plan.md ──► diff + tests ──► review + CI
   ▲                                                          │
   └──────── production failure writes the next intent.md ◄───┘
```

The repo is the source of truth. Markdown, diffs, and PR history are
the audit trail.

## Artifacts

| Artifact | What it records | Who accepts |
|---|---|---|
| `intent/<slug>/intent.md` | Problem, outcome, constraints, success | Product owner |
| `intent/<slug>/spec.md` | Requirements, design, flagged concerns | Product owner; policy owners on flags |
| `intent/<slug>/plan.md` | Files, order, risks, proof | Engineer; tech lead if high-risk |
| Diff + tests | Implementation | Code owner via branch protection |
| PR findings | Bugs, security, compliance (`REVIEW.md`) | Advisory; humans merge |
| New `intent.md` | A production miss that must not recur | Product owner |

Templates live in `intent/_template/`. The current product is
`intent/2026-08-24-herdr-native-control-plane/`.

## What agents read every session

- `AGENTS.md` — product direction and authority boundaries.
- `CLAUDE.md` — commands, conventions, and recurring mistakes. Keep
  it under a page. When a mistake happens twice, put the correction
  here. Code owners approve changes.
- `.claude/skills/` — institutional policy (`agent-factory-architecture`,
  `herdr-authority`, `rust-ledger`, plus vendor skills). Advisory;
  hooks and review enforce.
- `.claude/hooks/` — fail-closed guardrails (generated bindings,
  secrets, format, production gate).
- `.claude/agents/` — scoped helpers (researcher, simplifier,
  verifier).

## Proof while building

- `pnpm validate`, `pnpm test:web`, `cargo test --workspace`.
- `evals/` when `CLAUDE.md` or `.claude/**` changes
  (`.github/workflows/agent-evals.yml`).
- `pnpm smoke:web` for static UI; `pnpm smoke` when native behavior
  changed.
- UI work is compared to an agreed screenshot, not only typechecked.

## Review and merge

`REVIEW.md` defines three passes (bugs, security, compliance) and
caps nits. Findings do not approve a PR. Branch protection requires
a human code owner. The author of a diff cannot approve it.

## Measuring the loop

Use git timestamps and CI, not a slide:

- time from first conversation to committed `intent.md`;
- time from `intent.md` to `spec.md` to `plan.md` to merge;
- share of intents accepted vs closed;
- rework after spec or plan is committed;
- first-pass CI; Important vs Nit ratio;
- eval pass rate; time from an incident to a permanent eval.
