import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { SidebarProvider } from "@agent-factory/ui/components/sidebar"

import type {
  AgentDraftProjection,
  TargetAgentProjection,
  TargetAgentVersionProjection,
  WorkspaceBindingProjection,
  WorkspaceProjection,
} from "@agent-factory/runtime-client"

import { AgentFactorySidebar } from "@/components/shell/agent-factory-sidebar"

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

const draft: AgentDraftProjection = {
  id: "22222222-2222-4222-8222-222222222222",
  targetAgentId: agent.id,
  workspaceBindingId: binding.id,
  name: agent.name,
  objective: "Ship answers",
  acceptanceCriteria: ["Cites evidence"],
  baseVersion: "0.1.0",
  branchRef: "agent-factory/agent/drafts/d",
  worktreePath: binding.primaryRoot,
  gitHead: "abc",
  lifecycle: "active",
  cleanupGuidance: null,
  createdAtUnixMs: 1,
  updatedAtUnixMs: 2,
}

const version: TargetAgentVersionProjection = {
  id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
  targetAgentId: agent.id,
  version: "0.2.0",
  name: agent.name,
  objective: draft.objective,
  acceptanceCriteria: draft.acceptanceCriteria,
  sourceDraftId: draft.id,
  gitCommit: "fedcba9876543210",
  gitTag: "agent-factory/agent/v0.2.0",
  createdAtUnixMs: 30,
}

function projection(
  drafts: readonly AgentDraftProjection[],
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
        drafts: [...drafts],
        versions: [...versions],
        workspaceBindings: [binding],
        workItems: drafts.map((item) => ({
          id: item.id,
          kind: "agent_draft" as const,
          targetAgentId: agent.id,
          workspaceBindingId: binding.id,
          projectId: binding.projectId,
          agentDraftId: item.id,
          title: item.name,
          status: "active",
          lastActivityAtUnixMs: item.updatedAtUnixMs,
          projectLabel: "commerce",
          workspaceLabel: binding.name,
          sourceRefLabel: item.branchRef,
        })),
      }],
      workContexts: [],
      panes: [],
      terminals: [],
      focusedPaneId: undefined,
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
}

describe("AgentFactorySidebar", () => {
  it("aligns Draft and Agent names while styling the Draft as subordinate", () => {
    render(
      <SidebarProvider>
        <AgentFactorySidebar
          projection={projection([draft])}
          onAddTarget={() => undefined}
          onOpenSettings={() => undefined}
          onOpenDraft={() => undefined}
          onCreateDraft={() => undefined}
          onRemoveAgent={() => undefined}
        />
      </SidebarProvider>,
    )

    const draftRow = screen.getByRole("button", { name: binding.name })
    expect(draftRow).toHaveClass("pl-8")
    expect(draftRow).toHaveClass("text-muted-foreground")
    expect(screen.getByRole("button", { name: agent.name })).toHaveClass(
      "font-medium",
    )
  })

  it("uses the Agent folder only as a disclosure control", () => {
    const onOpenDraft = vi.fn()
    render(
      <SidebarProvider>
      <AgentFactorySidebar
        projection={projection([draft])}
        onAddTarget={() => undefined}
        onOpenSettings={() => undefined}
        onOpenDraft={onOpenDraft}
        onCreateDraft={() => undefined}
        onRemoveAgent={() => undefined}
      />
      </SidebarProvider>,
    )

    const folder = screen.getByRole("button", { name: agent.name })
    expect(folder).toHaveAttribute("aria-expanded", "true")
    fireEvent.click(folder)
    expect(folder).toHaveAttribute("aria-expanded", "false")
    expect(onOpenDraft).not.toHaveBeenCalled()
  })

  it("offers Create Draft on the Agent folder context menu", () => {
    const onCreateDraft = vi.fn()
    render(
      <SidebarProvider>
      <AgentFactorySidebar
        projection={projection([], [version])}
        onAddTarget={() => undefined}
        onOpenSettings={() => undefined}
        onOpenDraft={() => undefined}
        onCreateDraft={onCreateDraft}
        onRemoveAgent={() => undefined}
      />
      </SidebarProvider>,
    )

    fireEvent.contextMenu(screen.getByRole("button", { name: agent.name }))
    fireEvent.click(screen.getByRole("menuitem", { name: "Create Draft" }))
    expect(onCreateDraft).toHaveBeenCalledWith(agent, version)
  })

  it("offers Remove and Open in new window on the Draft row", () => {
    const onRemoveAgent = vi.fn()
    render(
      <SidebarProvider>
      <AgentFactorySidebar
        projection={projection([draft])}
        onAddTarget={() => undefined}
        onOpenSettings={() => undefined}
        onOpenDraft={() => undefined}
        onCreateDraft={() => undefined}
        onRemoveAgent={onRemoveAgent}
      />
      </SidebarProvider>,
    )

    fireEvent.contextMenu(screen.getByRole("button", { name: "main" }))
    expect(screen.getByRole("menuitem", { name: "Open in new window" }))
      .toBeTruthy()
    expect(screen.queryByRole("menuitem", { name: "Delete" })).toBeNull()
    fireEvent.click(screen.getByRole("menuitem", { name: "Remove" }))
    expect(onRemoveAgent).toHaveBeenCalledWith(agent.id)
  })
})
