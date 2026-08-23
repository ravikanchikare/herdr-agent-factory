#!/bin/bash
# Minimal eval checker — compares eval expectations to result.json
# Usage: ./evals/check.sh evals/example.json result.json
set -euo pipefail
eval_file=${1:?eval json required}
result_file=${2:?result json required}

echo "Checking $eval_file against $result_file"

# 1. Ensure tests/lint checks are represented in result (caller should have run them)
# This stub validates the eval shape; extend to parse result.json's tool outputs.
if ! jq -e '.prompt and .checks' "$eval_file" >/dev/null; then
  echo "FAIL: eval missing prompt/checks" >&2
  exit 1
fi

# 2. Guard generated paths
if git diff --name-only 2>/dev/null | grep -q 'packages/shared/runtime-client'; then
  echo "FAIL: generated bindings were edited — run pnpm contracts:generate instead" >&2
  exit 1
fi

echo "PASS: $eval_file shape ok (extend this script to assert tests_pass/lint_clean from result.json)"
