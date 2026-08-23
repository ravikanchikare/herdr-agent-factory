# Agent Factory native host

This directory is the Native-SDK desktop shell. It owns window lifecycle,
platform integration, packaging, the Rust runtime process, and opaque byte
transport. It contains no protocol or application behavior.

The web frontend is built in `../web-ui`. Development uses the fixed loopback
origin `http://127.0.0.1:3000/`; production packages `../web-ui/out` as
`frontend/out` under the application resources directory.

The application-defined bridge commands are `runtime.invoke`,
`desktop.window.startDrag.v1`, `desktop.terminal.show.v1`, and
`desktop.terminal.hide.v1`. `runtime.invoke` carries one versioned Rust IPC
request frame. Native forwards its length-prefixed JSON over the runtime
sidecar's stdin/stdout and returns the matching response frame. Window dragging
and terminal visibility are host platform behavior: they never enter Rust and
are not exposed as stock Native-SDK builtins.

The window-drag contract is synchronous so AppKit can consume the mouse-down
event that triggered it:

```js
void window.zero
  .invoke("desktop.window.startDrag.v1", null)
  .catch(() => undefined) // resolves to { version: 1, windowId: number }
```

The host accepts the command only from `zero://app` and
`http://127.0.0.1:3000`, requires the `window` permission, and passes the
invoking WebView's source window ID to Native-SDK platform services. AppKit owns
the actual movement and the user's titlebar double-click preference. The web UI
owns only DOM hit-testing and the typed intent.

`desktop.terminal.show.v1` receives the Rust-resolved Herdr client executable,
session arguments, Workspace ID, and label, then reveals the retained Native-SDK
`<terminal pty={key}>` surface. `desktop.terminal.hide.v1` hides that same
surface without closing the Herdr Workspace. Both return `{ version: 1,
visible: boolean }`.

Stock Native-SDK bridge commands are allowlisted for the directory picker used
to choose an existing project folder, notifications, and dedicated Draft-window
create/focus/close/list operations. Unrelated stock Native-SDK commands remain
outside the builtin allowlist.

The picker contract is:

```js
const selected = await window.zero.invoke("native-sdk.dialog.openFile", {
  title: "Open Project",
  allowDirectories: true,
  allowMultiple: false,
})
const absolutePath = selected?.[0] ?? null
```

The result is `string[] | null`; with `allowMultiple: false`, the first entry
is the only selected absolute directory path. Cancellation returns `null`.
Rust remains responsible for validating the path and creating the project.

In development, the runtime is resolved in this order:

1. `AGENT_FACTORY_RUNTIME_PATH`
2. `../../target/debug/agent-factory-runtime`
3. `agent-factory-runtime` from `PATH`

A packaged application ignores all overrides and `PATH`; it launches only
`Contents/Resources/agent-factory-runtime` from its signed bundle.

Run from this directory:

```sh
pnpm --dir ../.. exec native validate apps/native-host/app.zon
pnpm --dir ../.. exec native doctor --manifest apps/native-host/app.zon --strict
pnpm --dir ../.. native:build
pnpm --dir ../.. test:native
```

Repository Native SDK commands use `scripts/native-sdk-command.mjs`. It puts
the managed Zig 0.16.0 directory on `PATH` before launching the SDK so
Ghostty's nested `zig env` configure command resolves the same pinned
toolchain. `NATIVE_SDK_ZIG` and `NATIVE_SDK_HOME` remain supported overrides.

Local macOS packages are ad-hoc signed and contain a Rust runtime built for
the same architecture as the Zig host. Create a notarized release with:

```sh
RELEASE_ARCH=arm64 \
APPLE_DEVELOPER_ID_APPLICATION="Developer ID Application: Example (TEAMID)" \
APPLE_NOTARY_KEYCHAIN_PROFILE=agent-factory-notary \
  ./scripts/release-macos.sh
```

The notary credential is read only from the named Keychain profile. The script
signs the inner runtime before the outer app, notarizes and staples the bundle,
then writes CycloneDX/SPDX SBOMs and SHA-256 checksums. It requires `syft`.
It uses Native-SDK's managed Zig 0.16.0; `NATIVE_SDK_ZIG` may override that
path, but the script rejects any other Zig version.

Native automation validates runtime and bridge integration. WebView DOM and
pixel behavior remains the responsibility of the frontend Playwright suite.
