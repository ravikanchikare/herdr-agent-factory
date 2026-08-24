# Intent home

This directory is the version-controlled home for every change record.
Each intent is a human-readable, machine-actionable proto-spec. The
commit chain is the audit trail: who asked, what was produced, who
approved.

The **repo is the source of truth**. If a tracker (Jira / Linear) is
used elsewhere, every intent notes that record id and every tracker
item links the intent commit SHA.

## Layout

```
intent/
  README.md            — this file
  _template/
    intent.md          — copy this to start a new intent
    spec.md            — requirements and design
    plan.md            — files, order, risks, proof
  YYYY-MM-DD-<slug>/   — one directory per intent
    intent.md          — required
    spec.md            — after design is accepted
    plan.md            — after the implementation plan is accepted
```

Keep at most one intent directory per change. A merged change for that
intent closes the loop. A production failure writes the next intent.

The current product is recorded in
`intent/2026-08-24-herdr-native-control-plane/`. New work starts a new
slug; it does not rewrite that record unless the product itself
changes.

## Lifecycle

1. Write the problem in ordinary language. Capture who is affected,
   what better looks like, constraints, and how success will be
   judged. Agents may draft; the originator corrects.
2. Commit `intent/<slug>/intent.md` from the template.
3. Product owner accepts or rejects. Accepting triggers a spec pass.
4. Spec applies `AGENTS.md` and the architecture skills, flags
   conflicts, and is accepted before planning.
5. Plan names files, order, risks, and proof. Implementation follows
   the accepted plan. If the diff departs, update `plan.md` in the
   same commit.
6. PR review checks the diff against `spec.md` and `plan.md`
   (`REVIEW.md`). CI is `pnpm validate` and `pnpm test`.
7. A breached production control becomes a new `intent.md`.

Humans accept at each gate. Agents draft artifacts and diffs; they
do not approve them.

## Governance

- Evidence: the committed markdown plus git history.
- Approval: product owner for intent and spec (tech lead on
  high-risk specs); engineer for routine plans (tech lead on
  high-risk plans); human code owner for merge.
- Audit: `git log -- intent/`.

## Metrics

Read from git and CI, not from a separate report.

| Signal | How to read |
|---|---|
| Time to `intent.md` | `git log --diff-filter=A --format=%aI -- intent/<slug>/intent.md` |
| Survival rate | accepted intents / total intents |
| Rework | `intent.md` commits after the first `spec.md` for the same slug |
| Spec rework | `spec.md` commits after the first `plan.md` |
| Plan fidelity | whether the merged diff still matches `plan.md` |

## Creating a new intent

```bash
SLUG=$(date +%Y-%m-%d)-my-idea
mkdir -p intent/$SLUG
cp intent/_template/intent.md intent/$SLUG/intent.md
# edit, then:
# git add intent/$SLUG/intent.md
# git commit -m "feat(intent): add $SLUG"
```
