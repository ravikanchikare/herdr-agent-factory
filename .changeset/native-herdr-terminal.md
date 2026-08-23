---
"@agent-factory/runtime-client": minor
"@agent-factory/web-ui": minor
---

Open an active Run in one full-height Native SDK terminal that hosts the Herdr
workspace TUI. The shell starts with the terminal hidden, reveals it edge to
edge at 70 percent width, and hides the React sidebar without mirroring Herdr
panes. Title-bar and Run workspace actions toggle this one native terminal;
the legacy web terminal renderer and its WASM bundle are removed.
Pointer input now follows the terminal application's mouse-reporting mode, so
Herdr receives clicks, drags, right-clicks, and wheel input while Shift keeps
the native selection and Copy/Paste override available.

Cancel Run now waits for Herdr to confirm that every Factory-managed agent and
tab has terminated before publishing the cancelled Run state to the UI.
Recorded tabs are revalidated even if a managed agent is renamed, and a
cancelled Draft can immediately start a fresh Run. The Orchestrator brief now
requires the Herdr skill to manage every Coding, Evaluation, or other agent
created through the Environment-authorized control boundary.

Factory-managed Herdr labels omit the redundant product prefix, and the
embedded client uses an app-owned 32-column sidebar without changing the
user's global Herdr configuration.

Native development, build, test, and package commands now expose the Native
SDK-managed Zig 0.16 toolchain to Ghostty's nested configure commands, so a
clean shell no longer fails at `zig env` when Zig is absent from its original
`PATH`.
