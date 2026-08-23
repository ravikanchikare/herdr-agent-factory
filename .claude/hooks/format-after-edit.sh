#!/bin/bash
# Run formatter/linter after file edits so drift never accumulates (Stage 3 guardrail).
# Must be fast and scoped to the changed file; heavier checks belong at commit/PR.
set -euo pipefail
input=$(cat)
file=$(echo "$input" | jq -r '.tool_input.file_path // .tool_input.path // empty' 2>/dev/null || echo "")

# Only act on TypeScript/Rust files we can format cheaply
if [[ "$file" == *.ts || "$file" == *.tsx ]]; then
  # Best-effort: run eslint --fix on the single file if available; never fail the hook
  npx --yes eslint --fix "$file" 2>/dev/null || true
fi
if [[ "$file" == *.rs ]]; then
  rustfmt "$file" 2>/dev/null || true
fi
exit 0
