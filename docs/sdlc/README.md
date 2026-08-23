# AI-native SDLC — Agent Factory adaptation

Source playbook: <https://claude.com/blog/the-ai-native-sdlc-playbook> (Aug 21 2026, Louis Claxton).

This document maps each play to how this repository runs it. The traditional linear SDLC becomes a loop where each accepted artifact fires the next gate; human attention concentrates at the gates, reviewing what agents flagged rather than restarting each stage.

```
intent.md ──► spec.md ──► plan.md ──► diff + tests ──► PR + review findings ──► pipeline ──► production signal
   ▲                                                                                                   │
   └─────────────────────── breached control writes next intent.md ◄───────────────────────────────────┘
```

## Shift table (this repo)

| Stage | Traditional (before) | AI-native (this repo) |
|---|---|---|
| **Plan** | Backlog grooming, story points, refinement meetings | Originator brainstorms with Claude → versioned `intent/<slug>/intent.md` |
| **Design** | Analysts → designers, lost context | One session: Claude turns `intent.md` into `spec.md` bounded by skills, versioned in git |
| **Build** | Handwritten code, docs after | Claude Code plan mode → `plan.md`; knowledge in `CLAUDE.md` + `.claude/skills/*`; hooks guard |
| **Test** | QA gates at boundaries | Continuous evals + session feedback loop (`make test`, build, screenshot) |
| **Deploy** | Humans review every line, inconsistent | Agentic review passes per `REVIEW.md`; hooks as approval gates; humans approve regulated code |
| **Maintain** | Humans watch prod | Agents monitor; breached control writes next `intent.md` |

## Plays and adoption order

The plays are modular. Adoption order ≠ stage order — start with any clay play (no incoming dependencies). For other plays, adopt prerequisites first.

```
clay (no deps):  intent.md  ·  CLAUDE.md  ·  feedback loop
        ↓              ↓              ↓
   spec.md      skills (institutional knowledge)
        ↓              ↓
     plan mode ──► hooks (guardrails) ──► parallel sessions / subagents ──► auto mode
        ↓              ↓
   continuous evals   PR review loop (REVIEW.md)
                           ↓
                    hooks as approval gates ──► CI/CD (pipeline)
                           ↓
                      maintain / loop close
```

Recommended order for this repo (already partially in place):

1. `intent/` home + templates (Stage 1) ✅ this commit
2. Refresh `CLAUDE.md` (Stage 3) ✅
3. Institutional skills (Stage 3) ✅
4. Hooks as guardrails (Stage 3) ✅
5. Feedback loop encoding (Stage 4) — via `CLAUDE.md` Verifying block + hook ✅
6. `REVIEW.md` + AI review wiring (Stage 5) ✅
7. `evals/` + `agent-evals.yml` (Stage 4) ✅
8. Hooks as approval gates / managed settings (Stage 5) ✅
9. Parallel sessions & subagents (Stage 3) ✅
10. Maintain loop — incident → `intent.md` (Stage 6) — process, not code

## Artifacts and audit trail

Each stage ends by committing one artifact the next stage reads:

- `intent/<slug>/intent.md` — what is wanted, why, constraints (Stage 1)
- `intent/<slug>/spec.md` — requirements + design, flagged concerns (Stage 2)
- `intent/<slug>/plan.md` — files, order, risks, proof (Stage 3)
- diff + tests — implementation (Stage 3/4)
- PR + review findings — (Stage 5)
- incident record → next `intent.md` — (Stage 6)

The chain of commits is the audit trail: who asked, what the agent produced, who approved, which skill/hook versions were in force.

## Repository specifics

- **Intent home:** `intent/` in this repo (simplest for single product). Monorepo alternative would be `intent/` directory; multi-repo alternative would be a dedicated intent repo.
- **Legacy tracker linkage:** If Linear/Jira is used, every `intent.md` carries `External record:` and every tracker ticket links the intent commit SHA. Repo is source of truth.
- **CLAUDE.md:** Root file, <1 page, read at session start. Encodes commands, conventions, architecture pointers, verification, and recurring mistakes. Changes reviewed like code; code owners approve.
- **Skills:** `.claude/skills/<name>/SKILL.md` with frontmatter `name` + `description` (trigger). Advisory controls — deterministic enforcement comes from hooks + review.
- **Hooks:** `.claude/settings.json` + `.claude/hooks/*.sh`. Fast, file-scoped for build; heavier checks at commit/PR. Approval hooks belong to Stage 5 (not build) so they don't block parallel sessions.
- **Subagents:** `.claude/agents/*.md` — scoped helpers (verifier, researcher, simplifier) with own context + tool limits.
- **Review:** `REVIEW.md` at root defines passes (bugs, security, compliance vs `spec.md`/`plan.md`), severity (Important vs Nit), and caps.
- **Evals:** `evals/*.json` + `.github/workflows/agent-evals.yml`. Run on schedule and on `CLAUDE.md` / `.claude/**` changes; block skill/CLAUDE changes that drop pass rate.

## Governance per play

See each artifact's dedicated doc for who approves and where evidence lives. Summary: `intent.md` — product owner; `spec.md` — product owner + policy owners for flagged concerns; `plan.md` — engineer (tech lead for high-risk); PR — human code owner via branch protection, informed by agentic findings.

## How we measure it

Read from git/CI/PR history, not manual reports.

- Time to `intent.md`, `intent→spec`, `plan→merge` (git timestamps)
- Survival rate, rework after `plan.md`, first-pass CI success
- Review time to first review, Important vs Nit ratio, defects caught pre-merge vs escaped
- Eval pass rate over time, time to turn an incident into a permanent eval
