import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import type { WorkspaceProjection } from "@agent-factory/runtime-client"

import { WorkspaceShell } from "@/components/shell/workspace-shell"

const emptyProjection: WorkspaceProjection = {
  revision: 1,
  connection: "ready",
  projects: [],
  herdr: { connected: true, freshness: "live", issues: [] },
  harnesses: [],
  sessions: [],
  liveAgents: [],
  targetWorkspace: {
    targetGroups: [],
    workContexts: [],
    panes: [],
    terminals: [],
  },
  factoryRuns: [],
  terminals: [],
  files: { state: "idle", entries: [] },
  environments: [],
  llmProviders: [],
  secrets: [],
  pluginRegistries: [],
  pluginCatalogs: [],
  plugins: { installed: [], localMcpServers: [] },
  settings: {
    theme: "system",
    nativeNotifications: true,
    layout: { inspectorPercent: 28, terminalPercent: 24 },
  },
}

function renderShell(projection: WorkspaceProjection = emptyProjection) {
  return render(
    <WorkspaceShell
      projection={projection}
      emitIntent={async () => {}}
      createTargetAgent={async () => true}
      startDraftRun={async () => {}}
      listVersionFiles={async () => ({
        entries: [],
        gitCommit: "test-commit",
        versionId: "test-version",
      })}
      readVersionFile={async () => ({
        path: "",
        kind: "text",
        content: "",
        size: 0,
        gitCommit: "test-commit",
        versionId: "test-version",
      })}
    />,
  )
}

describe("WorkspaceShell agent creation", () => {
  it("hides the sidebar when opening Define your agent and restores it on close", () => {
    renderShell()

    expect(screen.getByRole("button", { name: "Hide sidebar" })).toBeTruthy()
    const createAgent = screen
      .getAllByRole("button", { name: "Create Agent" })
      .find((button) => button.textContent?.includes("Create Agent"))
    expect(createAgent).toBeTruthy()
    fireEvent.click(createAgent!)

    expect(
      screen.getByRole("region", { name: "Define your agent" }),
    ).toBeTruthy()
    expect(screen.getByRole("button", { name: "Show sidebar" })).toBeTruthy()
    expect(screen.queryByRole("button", { name: "Hide sidebar" })).toBeNull()

    fireEvent.click(screen.getByRole("button", { name: "Close" }))

    expect(
      screen.queryByRole("region", { name: "Define your agent" }),
    ).toBeNull()
    expect(screen.getByRole("button", { name: "Hide sidebar" })).toBeTruthy()
  })
})
