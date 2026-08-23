#!/bin/bash
# Block credentials from entering diffs (Stage 3 guardrail). Keep credentials out of SQLite/logs/IPC.
set -euo pipefail
input=$(cat)
file=$(echo "$input" | jq -r '.tool_input.file_path // .tool_input.path // empty' 2>/dev/null || echo "")
content=$(echo "$input" | jq -r '.tool_input.content // .tool_input.text // empty' 2>/dev/null || echo "")

# Block obvious secret patterns in edited content
if echo "$content" | grep -qiE '(sk-ant-|ghp_|AKIA|BEGIN (RSA )?PRIVATE KEY|api[_-]?key\s*[:=])'; then
  echo "Blocked: edit appears to contain a credential. Raw credentials must live in the platform credential store (Keychain), never in SQLite, descriptors, logs, or diffs." >&2
  exit 2
fi

# Block writing .env files with secrets
if [[ "$file" == *".env"* ]] && [[ -n "$content" ]]; then
  if echo "$content" | grep -qiE '(KEY|SECRET|TOKEN|PASSWORD)'; then
    echo "Blocked: .env writes with credential-like keys are not allowed. Use platform-secrets / Keychain via Rust." >&2
    exit 2
  fi
fi
exit 0
