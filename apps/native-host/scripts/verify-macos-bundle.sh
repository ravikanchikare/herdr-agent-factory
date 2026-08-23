#!/bin/bash

set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: $0 <app-bundle> <arm64|x86_64>" >&2
  exit 64
fi

app_bundle=$1
expected_arch=$2
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
host_dir=$(cd -- "$script_dir/.." && pwd)
repo_dir=$(cd -- "$host_dir/../.." && pwd)
host_binary="$app_bundle/Contents/MacOS/agent-factory"
runtime_binary="$app_bundle/Contents/Resources/agent-factory-runtime"
updater_binary="$app_bundle/Contents/Resources/updater-helper"
update_config="$app_bundle/Contents/Resources/update-config-v1.json"
herdr_config="$app_bundle/Contents/Resources/herdr-client.toml"

case "$expected_arch" in
  arm64 | x86_64) ;;
  *)
    echo "unsupported macOS release architecture: $expected_arch" >&2
    exit 64
    ;;
esac

for binary in "$host_binary" "$runtime_binary" "$updater_binary"; do
  if [[ ! -f "$binary" || ! -x "$binary" || -L "$binary" ]]; then
    echo "missing executable bundle member: $binary" >&2
    exit 1
  fi

  actual_arch=$(/usr/bin/lipo -archs "$binary")
  if [[ "$actual_arch" != "$expected_arch" ]]; then
    echo "architecture mismatch: $binary is [$actual_arch], expected [$expected_arch]" >&2
    exit 1
  fi

  /usr/bin/file "$binary"
done

for resource in "$update_config" "$herdr_config"; do
  if [[ ! -f "$resource" || -L "$resource" ]]; then
    echo "missing regular sealed bundle resource: $resource" >&2
    exit 1
  fi
done
for mutable_directory in environments plugins user-environments; do
  if [[ -e "$app_bundle/Contents/Resources/$mutable_directory" || \
        -L "$app_bundle/Contents/Resources/$mutable_directory" ]]; then
    echo "mutable application data must not be packaged: $mutable_directory" >&2
    exit 1
  fi
done

cargo run --quiet --locked \
  --manifest-path "$repo_dir/Cargo.toml" \
  --package update-runtime \
  --bin validate-update-config \
  -- \
  --path "$update_config"
