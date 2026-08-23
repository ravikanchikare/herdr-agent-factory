import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import type {
  AgentDraftProjection,
  EnvironmentDto,
  FactoryRunProjection,
  RuntimeIntent,
  TargetAgentProjection,
  WorkspaceBindingProjection,
  WorkspaceProjection,
} from "@agent-factory/runtime-client"

import { WorkspaceShell } from "@/components/shell/workspace-shell"

const agent: TargetAgentProjection = {
  id: "11111111-1111-4111-8111-111111111111",
  name: "Commerce Copilot",
  repositoryRoot: "/code/commerce",
  archived: false,
  lastActivityAtUnixMs: 2,
}

const draft: AgentDraftProjection = {
  id: "22222222-2222-4222-8222-222222222222",
  targetAgentId: agent.id,
  workspaceBindingId: "33333333-3333-4333-8333-333333333333",
  name: "Commerce Copilot",
  objective: "Ship commerce answers",
  acceptanceCriteria: ["Cites evidence"],
  baseVersion: "0.1.0",
  branchRef: `agent-factory/${agent.id}/drafts/22222222-2222-4222-8222-222222222222`,
  worktreePath: "/code/commerce-main",
  gitHead: "0123456789abcdef",
  lifecycle: "active",
  cleanupGuidance: null,
  createdAtUnixMs: 1,
  updatedAtUnixMs: 2,
}

const binding: WorkspaceBindingProjection = {
  id: draft.workspaceBindingId,
  targetAgentId: agent.id,
  projectId: "44444444-4444-4444-8444-444444444444",
  name: "main",
  primaryRoot: draft.worktreePath,
  additionalRoots: [],
  sourceRefLabel: draft.branchRef,
  archived: false,
  lastUsedAtUnixMs: 2,
}

const environment: EnvironmentDto = {
  id: "environment-a",
  name: "Environment A",
  codingHarnessId: "claude",
  evaluationHarnessId: "claude",
  plugins: [],
  permissions: {
    trustedRead: "allow",
    trustedWrite: "ask",
    terminal: "ask",
  },
  registryIds: [],
  environmentVariables: [],
  llm: null,
  resolvedLlm: null,
  llmNeedsSetup: false,
  readiness: { state: "ready", issues: [] },
}

const run: FactoryRunProjection = {
  id: "55555555-5555-4555-8555-555555555555",
  projectId: binding.projectId,
  environmentId: environment.id,
  targetAgentId: agent.id,
  agentDraftId: draft.id,
  workspaceBindingId: binding.id,
  objective: draft.objective,
  acceptanceCriteria: draft.acceptanceCriteria,
  state: "orchestrating",
  startingGitHead: draft.gitHead,
  changedFiles: [],
  testEvidence: [],
}

function projectionWithDraft(): WorkspaceProjection {
  return {
    revision: 1,
    connection: "ready",
    projects: [
      {
        id: binding.projectId,
        name: "commerce",
        root: draft.worktreePath,
        trusted: true,
      },
    ],
    herdr: { connected: true, freshness: "live", issues: [] },
    harnesses: [],
    sessions: [],
    liveAgents: [],
    targetWorkspace: {
      targetGroups: [
        {
          targetAgent: agent,
          drafts: [draft],
          versions: [],
          workspaceBindings: [binding],
          workItems: [
            {
              id: draft.id,
              kind: "agent_draft",
              targetAgentId: agent.id,
              workspaceBindingId: binding.id,
              projectId: binding.projectId,
              agentDraftId: draft.id,
              title: draft.name,
              status: "active",
              lastActivityAtUnixMs: draft.updatedAtUnixMs,
              projectLabel: "commerce",
              workspaceLabel: binding.name,
              sourceRefLabel: binding.sourceRefLabel,
            },
          ],
        },
      ],
      workContexts: [
        {
          id: "context-draft",
          targetAgentId: agent.id,
          workspaceBindingId: binding.id,
          agentDraftId: draft.id,
          workItemId: draft.id,
          workItemKind: "agent_draft",
          dock: "closed",
          dockPercent: 32,
          lastViewedAtUnixMs: 40,
        },
      ],
      panes: [
        {
          id: "pane-draft",
          workContextId: "context-draft",
          position: 0,
          widthBasisPoints: 10_000,
        },
      ],
      terminals: [],
      focusedPaneId: "pane-draft",
    },
    factoryRuns: [run],
    terminals: [],
    files: { state: "idle", entries: [] },
    environments: [environment],
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
}

function renderShell(
  projection: WorkspaceProjection,
  nativeTerminalVisible = false,
  emitIntent: (intent: RuntimeIntent) => Promise<void> = async () => {},
) {
  return render(
    <WorkspaceShell
      projection={projection}
      emitIntent={emitIntent}
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
      nativeTerminalVisible={nativeTerminalVisible}
    />,
  )
}

describe("WorkspaceShell terminal toggle", () => {
  it("uses the Lucide Terminal icon for the panel toggle", () => {
    renderShell(projectionWithDraft())

    const toggles = screen.getAllByRole("button", { name: "Open Terminal" })
    expect(toggles).toHaveLength(1)
    for (const toggle of toggles) {
      expect(toggle.querySelector("svg.lucide-terminal")).not.toBeNull()
      expect(toggle.querySelector("svg.lucide-panel-bottom")).toBeNull()
    }
  })

  it("keeps the Terminal icon when the panel is open", () => {
    renderShell(projectionWithDraft(), true)

    const toggles = screen.getAllByRole("button", { name: "Close Terminal" })
    expect(toggles).toHaveLength(1)
    for (const toggle of toggles) {
      expect(toggle.querySelector("svg.lucide-terminal")).not.toBeNull()
      expect(toggle.querySelector("svg.lucide-panel-bottom")).toBeNull()
    }
  })

  it("hides the web sidebar while the native terminal is visible", () => {
    renderShell(projectionWithDraft(), true)

    const sidebar = document.querySelector('[data-slot="sidebar"]')
    expect(sidebar).toHaveAttribute("data-state", "collapsed")
    expect(screen.getByRole("region", {
      name: "Agent Factory workspace",
    })).toBeTruthy()
  })

  it("can reopen the web sidebar while the native terminal remains visible", () => {
    renderShell(projectionWithDraft(), true)

    fireEvent.click(screen.getByRole("button", { name: "Show sidebar" }))

    const sidebar = document.querySelector('[data-slot="sidebar"]')
    expect(sidebar).toHaveAttribute("data-state", "expanded")
    expect(screen.getByRole("button", { name: "Hide sidebar" })).toBeTruthy()
  })

  it("routes every entry point to the current Draft's native terminal", () => {
    const intents: RuntimeIntent[] = []
    const emitIntent = async (intent: RuntimeIntent) => {
      intents.push(intent)
    }
    const { unmount } = renderShell(
      projectionWithDraft(),
      false,
      emitIntent,
    )

    for (const button of screen.getAllByRole("button", {
      name: "Open Terminal",
    })) {
      fireEvent.click(button)
    }

    unmount()
    renderShell(projectionWithDraft(), true, emitIntent)
    for (const button of screen.getAllByRole("button", {
      name: "Close Terminal",
    })) {
      fireEvent.click(button)
    }
    expect(intents).toEqual(Array.from({ length: 2 }, () => ({
      type: "agentDraft.toggleWorkspace",
      agentDraftId: draft.id,
    })))
  })
})
