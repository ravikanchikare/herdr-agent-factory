# Continuous evals — AI-native SDLC Stage 4 · Test

Evals are the AI-native equivalent of stage-gate QA: a live suite that runs whenever the agent's configuration (`CLAUDE.md`, `.claude/skills/**`, `.claude/hooks/**`, `.claude/agents/**`) changes. When a model or prompt is swapped, evals say whether work still meets the standard.

## Running locally

```bash
for eval in evals/*.json; do
  claude -p "$(jq -r '.prompt' "$eval")" --allowedTools "Read,Edit,Bash" --output-format json > result.json
  ./evals/check.sh "$eval" result.json
done
```

In CI the same suite runs non-interactively (see `.github/workflows/agent-evals.yml`) on:

- every PR that touches `CLAUDE.md` or `.claude/**`
- nightly schedule (`0 2 * * *`)
- manual dispatch

Gate `CLAUDE.md`/skill/hook changes on the pass-rate threshold; a change that drops the rate gets reviewed before merge. Each production incident gets a permanent eval as a regression test (owner = incident team).

## Adding an eval

1. Collect a real task from recent work (20–50 to start).
2. Write `evals/<slug>.json` with `prompt` and `checks` (tests pass, lint clean, behavior unchanged, policy followed).
3. Add the expected outcome or reference fixture.

See `evals/example.json` for shape.
