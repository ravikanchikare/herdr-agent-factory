#!/bin/bash
# Block edits to generated or frozen paths. Fast, file-scoped (Stage 3 guardrail).
set -euo pipefail
input=$(cat)
file=$(echo "$input" | jq -r '.tool_input.file_path // .tool_input.path // empty' 2>/dev/null || echo "")

blocked_patterns=(
  "packages/shared/runtime-client/"
  "crates/runtime-contract/generated"
  "apps/web-ui/.next/"
  "target/"
)

for pat in "${blocked_patterns[@]}"; do
  if [[ "$file" == *"$pat"* ]]; then
    echo "Blocked: $file matches protected path '$pat'. Edit the source (crates/runtime-contract) and run pnpm contracts:generate instead." >&2
    exit 2
  fi
done
exit 0
