import { fireEvent, render, screen, within } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import type {
  RuntimeIntent,
  TargetAgentProjection,
  TargetAgentVersionProjection,
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

const binding: WorkspaceBindingProjection = {
  id: "33333333-3333-4333-8333-333333333333",
  targetAgentId: agent.id,
  projectId: "44444444-4444-4444-8444-444444444444",
  name: "main",
  primaryRoot: "/code/commerce-main",
  additionalRoots: [],
  sourceRefLabel: "agent-factory/agent/drafts/d",
  archived: false,
  lastUsedAtUnixMs: 2,
}

const version: TargetAgentVersionProjection = {
  id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
  targetAgentId: agent.id,
  version: "0.2.0",
  name: agent.name,
  objective: "Ship answers",
  acceptanceCriteria: ["Cites evidence"],
  sourceDraftId: "99999999-9999-4999-8999-999999999999",
  gitCommit: "fedcba9876543210",
  gitTag: "agent-factory/agent/v0.2.0",
  createdAtUnixMs: 30,
}

function emptyDraftProjection(
  versions: readonly TargetAgentVersionProjection[] = [],
): WorkspaceProjection {
  return {
    revision: 1,
    connection: "ready",
    projects: [{
      id: binding.projectId,
      name: "commerce",
      root: binding.primaryRoot,
      trusted: true,
    }],
    herdr: { connected: true, freshness: "live", issues: [] },
    harnesses: [],
    sessions: [],
    liveAgents: [],
    targetWorkspace: {
      targetGroups: [{
        targetAgent: agent,
        drafts: [],
        versions: [...versions],
        workspaceBindings: [binding],
        workItems: [],
      }],
      workContexts: [{
        id: "context-agent",
        targetAgentId: agent.id,
        workspaceBindingId: binding.id,
        agentDraftId: null,
        workItemId: null,
        workItemKind: null,
        dock: "closed",
        dockPercent: 32,
        lastViewedAtUnixMs: 40,
      }],
      panes: [{
        id: "pane-draft",
        workContextId: "context-agent",
        position: 0,
        widthBasisPoints: 10_000,
      }],
      terminals: [],
      focusedPaneId: "pane-draft",
    },
    factoryRuns: [],
    terminals: [],
    files: { state: "idle", entries: [] },
    environments: [{
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
    }],
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

function widePane() {
  Element.prototype.getBoundingClientRect = function getBoundingClientRect() {
    return {
      width: 1200,
      height: 800,
      top: 0,
      left: 0,
      bottom: 800,
      right: 1200,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    }
  }
}

function renderShell(
  projection: WorkspaceProjection,
  emitIntent: (intent: RuntimeIntent) => Promise<void> = async () =>
    undefined,
) {
  return render(
    <WorkspaceShell
      projection={projection}
      emitIntent={emitIntent}
      createTargetAgent={async () => true}
      startDraftRun={async () => undefined}
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

function openDraftOverview() {
  fireEvent.click(screen.getByRole("button", { name: "Show Draft Overview" }))
  return screen.getByRole("complementary", { name: "Draft Overview" })
}

describe("WorkspaceShell empty Draft Overview", () => {
  it("shows Draft Overview only after its header toggle opens", () => {
    widePane()
    renderShell(emptyDraftProjection())

    expect(screen.getByRole("region", { name: "Commerce Copilot Draft" }))
      .toBeTruthy()
    expect(screen.queryByRole("complementary", { name: "Draft Overview" }))
      .toBeNull()
    const overview = openDraftOverview()
    expect(overview.querySelector('[data-empty="true"]')).not.toBeNull()
    expect(screen.getByText("No versions yet")).toBeTruthy()
    expect(screen.queryByRole("heading", { name: "Version history" }))
      .toBeNull()
  })

  it("creates the first Draft from Overview when there are no Versions", () => {
    widePane()
    const emitIntent = vi.fn(async () => undefined)
    renderShell(emptyDraftProjection(), emitIntent)

    openDraftOverview()
    fireEvent.click(screen.getByRole("button", { name: "Create Draft" }))
    const dialog = screen.getByRole("dialog", { name: "Create Draft" })
    fireEvent.click(within(dialog).getByRole("button", { name: "Create Draft" }))
    expect(emitIntent).toHaveBeenCalledWith({
      type: "agentDraft.create",
      targetAgentId: agent.id,
      draftName: "Draft",
    })
  })

  it("lists Versions and creates a Draft from a Version row", () => {
    widePane()
    const emitIntent = vi.fn(async () => undefined)
    renderShell(emptyDraftProjection([version]), emitIntent)

    openDraftOverview()
    expect(screen.getByRole("list", { name: "Versions" })).toBeTruthy()
    expect(screen.getByText("v0.2.0")).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "Create Draft" }))
    const dialog = screen.getByRole("dialog", {
      name: "Create Draft from v0.2.0",
    })
    fireEvent.click(within(dialog).getByRole("button", { name: "Create Draft" }))
    expect(emitIntent).toHaveBeenCalledWith({
      type: "agentDraft.create",
      targetAgentId: agent.id,
      baseVersionId: version.id,
      draftName: "v0.2.0 changes",
    })
  })

  it("keeps Remove and Delete off the workflow menu", () => {
    widePane()
    renderShell(emptyDraftProjection())

    fireEvent.click(screen.getByRole("button", {
      name: "Actions for Commerce Copilot",
    }))
    expect(screen.queryByRole("menuitem", { name: "Delete" })).toBeNull()
    expect(screen.queryByRole("menuitem", { name: "Remove" })).toBeNull()
    expect(screen.queryByRole("menuitem", { name: "Open in new window" }))
      .toBeNull()
  })
})

function populatedDraftProjection(): WorkspaceProjection {
  const empty = emptyDraftProjection()
  const draftId = "22222222-2222-4222-8222-222222222222"
  return {
    ...empty,
    targetWorkspace: {
      ...empty.targetWorkspace,
      targetGroups: [{
        ...empty.targetWorkspace.targetGroups[0]!,
        drafts: [{
          id: draftId,
          targetAgentId: agent.id,
          workspaceBindingId: binding.id,
          name: agent.name,
          objective: "Ship answers",
          acceptanceCriteria: ["Cites evidence"],
          baseVersion: "0.1.0",
          branchRef: binding.sourceRefLabel ?? "branch",
          worktreePath: binding.primaryRoot,
          gitHead: "abc",
          lifecycle: "active",
          cleanupGuidance: null,
          createdAtUnixMs: 1,
          updatedAtUnixMs: 2,
        }],
        workItems: [{
          id: draftId,
          kind: "agent_draft",
          targetAgentId: agent.id,
          workspaceBindingId: binding.id,
          projectId: binding.projectId,
          agentDraftId: draftId,
          title: agent.name,
          status: "active",
          lastActivityAtUnixMs: 2,
          projectLabel: "commerce",
          workspaceLabel: binding.name,
          sourceRefLabel: binding.sourceRefLabel,
        }],
      }],
      workContexts: [{
        id: "context-agent",
        targetAgentId: agent.id,
        workspaceBindingId: binding.id,
        agentDraftId: draftId,
        workItemId: draftId,
        workItemKind: "agent_draft",
        dock: "closed",
        dockPercent: 32,
        lastViewedAtUnixMs: 40,
      }],
    },
  }
}

describe("WorkspaceShell populated Draft workflow", () => {
  it("does not offer Open in new window in the workflow menu", () => {
    widePane()
    renderShell(populatedDraftProjection())

    fireEvent.click(screen.getByRole("button", {
      name: "Actions for Commerce Copilot",
    }))
    expect(screen.queryByRole("menuitem", { name: "Open in new window" }))
      .toBeNull()
  })

  it("keeps Remove and Delete off the workflow menu", () => {
    widePane()
    renderShell(populatedDraftProjection())

    fireEvent.click(screen.getByRole("button", {
      name: "Actions for Commerce Copilot",
    }))
    expect(screen.queryByRole("menuitem", { name: "Remove" })).toBeNull()
    expect(screen.queryByRole("menuitem", { name: "Delete" })).toBeNull()
  })
})

describe("WorkspaceShell dedicated Draft window", () => {
  afterEach(() => {
    window.history.replaceState({}, "", "/")
  })

  it("renders the Draft without the factory sidebar", () => {
    widePane()
    const draftId = "22222222-2222-4222-8222-222222222222"
    window.history.replaceState(
      {},
      "",
      `/?draftWindow=1&draftId=${draftId}&targetAgentId=${agent.id}&workspaceBindingId=${binding.id}&title=Commerce%20Copilot`,
    )
    renderShell(populatedDraftProjection())

    expect(screen.getByRole("heading", { level: 1, name: "Commerce Copilot" }))
      .toBeTruthy()
    expect(screen.getByRole("region", { name: "Commerce Copilot Draft" }))
      .toBeTruthy()
    expect(screen.queryByRole("navigation", { name: "Agents" })).toBeNull()
    expect(screen.queryByText("Agent Factory could not render")).toBeNull()
    fireEvent.click(screen.getByRole("button", {
      name: "Actions for Commerce Copilot",
    }))
    expect(screen.queryByRole("menuitem", { name: "Open in new window" }))
      .toBeNull()
    expect(screen.queryByRole("menuitem", { name: "Delete" })).toBeNull()
    expect(screen.queryByRole("menuitem", { name: "Remove" })).toBeNull()
  })
})
