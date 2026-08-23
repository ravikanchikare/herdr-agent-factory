import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import type {
  AgentDraftProjection,
  TargetAgentProjection,
  TargetAgentVersionProjection,
  WorkspaceBindingProjection,
  WorkspaceProjection,
} from "@agent-factory/runtime-client"

import { WorkspaceShell } from "@/components/shell/workspace-shell"

const agent: TargetAgentProjection = {
  id: "11111111-1111-4111-8111-111111111111",
  name: "IPL Expert",
  repositoryRoot: "/code/ipl",
  archived: false,
  lastActivityAtUnixMs: 2,
}

const agent2: TargetAgentProjection = {
  id: "22222222-2222-4222-8222-222222222222",
  name: "Docs Agent",
  repositoryRoot: "/code/docs",
  archived: false,
  lastActivityAtUnixMs: 1,
}

const binding: WorkspaceBindingProjection = {
  id: "33333333-3333-4333-8333-333333333333",
  targetAgentId: agent.id,
  projectId: "44444444-4444-4444-8444-444444444444",
  name: "match-analysis",
  primaryRoot: "/code/ipl-main",
  additionalRoots: [],
  sourceRefLabel: "agent-factory/agent/drafts/d",
  archived: false,
  lastUsedAtUnixMs: 2,
}

const binding2: WorkspaceBindingProjection = {
  id: "55555555-5555-4555-8555-555555555555",
  targetAgentId: agent2.id,
  projectId: "66666666-6666-4666-8666-666666666666",
  name: "main",
  primaryRoot: "/code/docs-main",
  additionalRoots: [],
  sourceRefLabel: "agent-factory/agent/drafts/docs",
  archived: false,
  lastUsedAtUnixMs: 1,
}

const draft: AgentDraftProjection = {
  id: "77777777-7777-4777-8777-777777777777",
  targetAgentId: agent.id,
  workspaceBindingId: binding.id,
  name: "Match Analysis",
  objective: "Analyze matches",
  acceptanceCriteria: ["Cites evidence"],
  baseVersion: "0.3.0",
  branchRef: "agent-factory/agent/drafts/match",
  worktreePath: "/code/ipl-match",
  gitHead: "abc123",
  lifecycle: "active",
  cleanupGuidance: null,
  createdAtUnixMs: 1,
  updatedAtUnixMs: 2,
}

const version: TargetAgentVersionProjection = {
  id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
  targetAgentId: agent.id,
  version: "0.3.0",
  name: agent.name,
  objective: "Analyze matches",
  acceptanceCriteria: ["Cites evidence"],
  sourceDraftId: "99999999-9999-4999-8999-999999999999",
  gitCommit: "fedcba9876543210",
  gitTag: "agent-factory/agent/v0.3.0",
  createdAtUnixMs: 30,
}

function projectionWithNoPanes(
  groups: Array<{
    targetAgent: TargetAgentProjection
    drafts: AgentDraftProjection[]
    versions: TargetAgentVersionProjection[]
    bindings: WorkspaceBindingProjection[]
  }>,
): WorkspaceProjection {
  return {
    revision: 1,
    connection: "ready",
    projects: groups.map((g) => ({
      id: g.bindings[0]!.projectId,
      name: "project",
      root: g.bindings[0]!.primaryRoot,
      trusted: true,
    })),
    herdr: { connected: true, freshness: "live", issues: [] },
    harnesses: [],
    sessions: [],
    liveAgents: [],
    targetWorkspace: {
      targetGroups: groups.map((g) => ({
        targetAgent: g.targetAgent,
        drafts: g.drafts,
        versions: g.versions,
        workspaceBindings: g.bindings,
        workItems: g.drafts.map((d) => ({
          id: d.id,
          kind: "agent_draft" as const,
          targetAgentId: g.targetAgent.id,
          workspaceBindingId: d.workspaceBindingId,
          projectId: g.bindings[0]!.projectId,
          agentDraftId: d.id,
          title: d.name,
          status: "active",
          lastActivityAtUnixMs: d.updatedAtUnixMs,
          projectLabel: "project",
          workspaceLabel: g.bindings[0]!.name,
          sourceRefLabel: d.branchRef,
        })),
      })),
      workContexts: [],
      panes: [],
      terminals: [],
      focusedPaneId: undefined,
    },
    factoryRuns: [],
    terminals: [],
    files: { state: "idle", entries: [] },
    environments: [{
      id: "env-a",
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

function renderShell(projection: WorkspaceProjection) {
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

describe("WorkspaceShell Recent Agents", () => {
  it("shows the Recent Agents heading when no panes are open", () => {
    renderShell(
      projectionWithNoPanes([
        {
          targetAgent: agent,
          drafts: [draft],
          versions: [version],
          bindings: [binding],
        },
      ]),
    )

    expect(screen.getByText("Recent Agents")).toBeTruthy()
  })

  it("lists each agent as a card with its drafts", () => {
    renderShell(
      projectionWithNoPanes([
        {
          targetAgent: agent,
          drafts: [draft],
          versions: [version],
          bindings: [binding],
        },
        {
          targetAgent: agent2,
          drafts: [],
          versions: [],
          bindings: [binding2],
        },
      ]),
    )

    expect(screen.getAllByText("IPL Expert").length).toBeGreaterThanOrEqual(1)
    expect(screen.getAllByText("match-analysis").length).toBeGreaterThanOrEqual(1)
    expect(screen.getAllByText("Docs Agent").length).toBeGreaterThanOrEqual(1)
    expect(screen.getByText("No drafts")).toBeTruthy()
  })

  it("shows a version badge for drafts based on a version", () => {
    renderShell(
      projectionWithNoPanes([
        {
          targetAgent: agent,
          drafts: [draft],
          versions: [version],
          bindings: [binding],
        },
      ]),
    )

    expect(screen.getByText("v0.3.0")).toBeTruthy()
  })

  it("opens a draft when its row is clicked", () => {
    const emitIntent = vi.fn(async () => {})
    render(
      <WorkspaceShell
        projection={projectionWithNoPanes([
          {
            targetAgent: agent,
            drafts: [draft],
            versions: [version],
            bindings: [binding],
          },
        ])}
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
      />,
    )

    const draftRow = screen.getAllByText("match-analysis")[0]!.closest("button")!
    fireEvent.click(draftRow)
    expect(emitIntent).toHaveBeenCalledWith({
      type: "workspacePane.openPrimary",
      targetAgentId: agent.id,
      workspaceBindingId: binding.id,
      workItemId: draft.id,
      workItemKind: "agent_draft",
    })
  })

  it("offers Create Draft for every agent including those with no drafts", () => {
    renderShell(
      projectionWithNoPanes([
        {
          targetAgent: agent,
          drafts: [draft],
          versions: [version],
          bindings: [binding],
        },
        {
          targetAgent: agent2,
          drafts: [],
          versions: [],
          bindings: [binding2],
        },
      ]),
    )

    const createButtons = screen.getAllByRole("button", {
      name: "Create Draft",
    })
    expect(createButtons).toHaveLength(2)
  })
})