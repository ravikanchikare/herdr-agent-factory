# Evals

Evals check that agent configuration still meets the standard when
`CLAUDE.md`, skills, hooks, or agents change. They are ordinary
regression tests for those files, not a substitute for `pnpm test`.

## Running locally

```bash
for eval in evals/*.json; do
  case "$eval" in
    */_template.json) continue ;;
  esac
  claude -p "$(jq -r '.prompt' "$eval")" --allowedTools "Read,Edit,Bash" \
    --output-format json > result.json
  ./evals/check.sh "$eval" result.json
done
```

CI runs the same suite (`.github/workflows/agent-evals.yml`) on:

- every PR that touches `CLAUDE.md`, `.claude/**`, or `evals/**`
- nightly (`0 2 * * *`)
- manual dispatch

A change that drops the pass rate should be reviewed before merge.
Each production miss should add a permanent eval.

## Adding an eval

1. Take a real task from recent work.
2. Write `evals/<slug>.json` with `prompt` and `checks`.
3. Add the expected outcome or a fixture.

See `evals/example.json` for shape. Skip `_template.json` when
running the suite.
