#!/bin/bash

set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
host_dir=$(cd -- "$script_dir/.." && pwd)
repo_dir=$(cd -- "$host_dir/../.." && pwd)
export PATH="$repo_dir/node_modules/.bin:$PATH"

if [[ -z "${NATIVE_SDK_PATH:-}" ]]; then
  installed_sdk="$repo_dir/node_modules/@native-sdk/cli"
  if [[ ! -f "$installed_sdk/src/root.zig" ]]; then
    echo "Native-SDK source is unavailable; run pnpm install first" >&2
    exit 69
  fi
  NATIVE_SDK_PATH=$(cd -- "$installed_sdk" && pwd -P)
  export NATIVE_SDK_PATH
fi

native_sdk_home=${NATIVE_SDK_HOME:-"$HOME/.native"}
native_sdk_zig=${NATIVE_SDK_ZIG:-"$native_sdk_home/toolchains/zig-0.16.0/zig"}
if [[ ! -x "$native_sdk_zig" ]]; then
  echo "Native-SDK Zig 0.16.0 is unavailable at: $native_sdk_zig" >&2
  echo "Run 'pnpm exec native build apps/native-host --yes' or set NATIVE_SDK_ZIG" >&2
  exit 69
fi
if ! zig_version=$("$native_sdk_zig" version); then
  echo "NATIVE_SDK_ZIG could not report its version: $native_sdk_zig" >&2
  exit 69
fi
if [[ "$zig_version" != "0.16.0" ]]; then
  echo "NATIVE_SDK_ZIG must be Zig 0.16.0; found: $zig_version" >&2
  exit 69
fi
export PATH="$(dirname -- "$native_sdk_zig"):$PATH"

release_arch=${RELEASE_ARCH:-arm64}
case "$release_arch" in
  arm64)
    zig_target=aarch64-macos
    ;;
  x86_64)
    zig_target=x86_64-macos
    ;;
  *)
    echo "RELEASE_ARCH must be arm64 or x86_64" >&2
    exit 64
    ;;
esac

: "${APPLE_DEVELOPER_ID_APPLICATION:?Set APPLE_DEVELOPER_ID_APPLICATION to a Developer ID Application identity}"
: "${APPLE_NOTARY_KEYCHAIN_PROFILE:?Set APPLE_NOTARY_KEYCHAIN_PROFILE to an xcrun notarytool keychain profile}"

for command in cargo codesign ditto file lipo native shasum spctl syft xcrun; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "required release command is unavailable: $command" >&2
    exit 69
  fi
done

version=$(
  sed -n 's/^[[:space:]]*\.version = "\([^"]*\)",/\1/p' \
    "$host_dir/app.zon"
)
if [[ -z "$version" ]]; then
  echo "could not read version from app.zon" >&2
  exit 65
fi

artifact_base="agent-factory-$version-macos-$release_arch-ReleaseFast"
app_bundle="$host_dir/zig-out/package/$artifact_base.app"
release_dir="$repo_dir/dist/macos/$version/$release_arch"
submission_zip="$release_dir/$artifact_base-notarization.zip"
release_zip="$release_dir/$artifact_base.zip"

mkdir -p "$release_dir"

(
  cd "$host_dir"
  "$native_sdk_zig" build package -Dtarget="$zig_target"
)

"$script_dir/verify-macos-bundle.sh" "$app_bundle" "$release_arch"

runtime_binary="$app_bundle/Contents/Resources/agent-factory-runtime"
updater_binary="$app_bundle/Contents/Resources/updater-helper"
/usr/bin/codesign \
  --force \
  --timestamp \
  --options runtime \
  --sign "$APPLE_DEVELOPER_ID_APPLICATION" \
  "$runtime_binary"
/usr/bin/codesign \
  --force \
  --timestamp \
  --options runtime \
  --sign "$APPLE_DEVELOPER_ID_APPLICATION" \
  "$updater_binary"
/usr/bin/codesign \
  --force \
  --timestamp \
  --options runtime \
  --sign "$APPLE_DEVELOPER_ID_APPLICATION" \
  "$app_bundle"
/usr/bin/codesign --verify --strict --verbose=2 "$runtime_binary"
/usr/bin/codesign --verify --strict --verbose=2 "$updater_binary"
/usr/bin/codesign --verify --deep --strict --verbose=2 "$app_bundle"

/usr/bin/ditto -c -k --keepParent "$app_bundle" "$submission_zip"
notary_options=(--keychain-profile "$APPLE_NOTARY_KEYCHAIN_PROFILE")
if [[ -n "${APPLE_NOTARY_KEYCHAIN_PATH:-}" ]]; then
  notary_options+=(--keychain "$APPLE_NOTARY_KEYCHAIN_PATH")
fi
xcrun notarytool submit "$submission_zip" "${notary_options[@]}" --wait
xcrun stapler staple "$app_bundle"
xcrun stapler validate "$app_bundle"
/usr/bin/codesign --verify --deep --strict --verbose=2 "$app_bundle"
/usr/sbin/spctl --assess --type execute --verbose=2 "$app_bundle"

/bin/rm -f "$submission_zip" "$release_zip"
/usr/bin/ditto -c -k --keepParent "$app_bundle" "$release_zip"

syft "$app_bundle" -o "cyclonedx-json=$release_dir/$artifact_base.cdx.json"
syft "$app_bundle" -o "spdx-json=$release_dir/$artifact_base.spdx.json"
(
  cd "$release_dir"
  /usr/bin/shasum -a 256 \
    "$artifact_base.zip" \
    "$artifact_base.cdx.json" \
    "$artifact_base.spdx.json" \
    > "$artifact_base.SHA256SUMS"
)

echo "release artifacts: $release_dir"
