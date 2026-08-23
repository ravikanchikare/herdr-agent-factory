#!/bin/bash
# Approval gate hook (Stage 5 - Deploy). Pauses actions until release authorization exists.
# Unlike build guardrails, this hook may ask for human approval and belongs in Deploy,
# not in the build critical path of parallel sessions.
set -euo pipefail
input=$(cat)
cmd=$(echo "$input" | jq -r '.tool_input.command // empty' 2>/dev/null || echo "")

# Production deploys require release authorization
if [[ "$cmd" == *"deploy"* && "$cmd" == *"production"* ]]; then
  if [ -z "${RELEASE_APPROVAL:-}" ]; then
    echo "Blocked: production deploys need a release authorization (RELEASE_APPROVAL env or approved change ticket). Route to release manager per docs/sdlc/README.md." >&2
    exit 2
  fi
fi

# Block edits to migrations/infra without a change ticket during build
if echo "$cmd" | grep -qE '(migration|infra/|terraform|crates/app-core/migrations)'; then
  if [ -z "${CHANGE_TICKET:-}" ]; then
    echo "Blocked: edits to migrations/infra require CHANGE_TICKET. Set CHANGE_TICKET=<id> or request via change management." >&2
    exit 2
  fi
fi
exit 0
