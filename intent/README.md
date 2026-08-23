# Intent home — AI-native SDLC Stage 1 · Plan

This directory is the version-controlled home for every `intent.md` (play 01 of the [AI-native SDLC playbook](https://claude.com/blog/the-ai-native-sdlc-playbook)). Each intent is a human-readable, machine-actionable proto-spec that the next stage reads. The commit chain is the audit trail: who asked, what was produced, who approved.

## Source-of-truth rule

For this repository the **repo is the source of truth** for intent. If a legacy tracker (Jira / Linear) is used elsewhere, every intent notes the external record ID and every external record links the intent commit SHA. See “Legacy systems and the source of truth” sidebar in the playbook.

## Layout

```
intent/
  README.md            — this file
  _template/
    intent.md          — copy this to start a new intent
    spec.md            — template for Stage 2 output
    plan.md            — template for Stage 3 output
  YYYY-MM-DD-<slug>/   — one directory per intent
    intent.md          — Stage 1 artifact (required)
    spec.md            — Stage 2 artifact (added after design)
    plan.md            — Stage 3 artifact (added after planning)
```

Example: `intent/2026-08-23-workspace-terminals/intent.md`.

Keep at most one intent directory per change. A merged PR for that intent closes the loop; a breached control in production writes the next intent.

## Lifecycle

1. Originator brainstorms with Claude (claude.ai / Cowork / Claude Code) in their own words — no formal language required. Claude asks analyst questions (scope, users, constraints, success criteria).
2. Claude writes `intent/_template/intent.md` → `intent/<slug>/intent.md` using the template.
3. Originator corrects misunderstandings; product owner reviews.
4. Commit `intent.md`. Author + timestamp are in git history.
5. Product owner accepts or rejects via PR merge / close review — that decision triggers Stage 2 (Design). An accepted `intent.md` triggers a spec pass; a rejected intent is closed.

Non-engineers do not need git — a GitHub connector lets Claude commit on their behalf.

## Governance

- Evidence: the committed `intent.md` + git history (author, timestamp, revisions).
- Approval: product owner. High-risk intents also consult a tech lead.
- Audit: `git log -- intent/` gives time from conversation to committed intent; survival rate = share of intents merged into `spec.md` vs closed.

## Metrics

| Signal | How to read |
|---|---|
| Leading — time to `intent.md` | `git log --diff-filter=A --format=%aI -- intent/<slug>/intent.md` |
| Lagging — survival rate | accepted intents / total intents per month (PR merge vs close) |
| Lagging — rework | `intent.md` commits after first `spec.md` for same slug |

## Creating a new intent

```bash
SLUG=$(date +%Y-%m-%d)-my-idea
mkdir -p intent/$SLUG
cp intent/_template/intent.md intent/$SLUG/intent.md
# edit, then: but diff && but commit -b intent/$SLUG -m "feat(intent): add $SLUG" <ids>
```
