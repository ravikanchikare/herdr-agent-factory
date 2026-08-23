import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it } from "vitest"

import type {
  AgentDraftProjection,
  EnvironmentDto,
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
  branchRef: "agent-factory/agent/drafts/d",
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

function draftProjection(): WorkspaceProjection {
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
        drafts: [draft],
        versions: [],
        workspaceBindings: [binding],
        workItems: [{
          id: draft.id,
          kind: "agent_draft",
          targetAgentId: agent.id,
          workspaceBindingId: binding.id,
          projectId: binding.projectId,
          agentDraftId: draft.id,
          title: draft.name,
          status: "active",
          lastActivityAtUnixMs: 2,
          projectLabel: "commerce",
          workspaceLabel: binding.name,
          sourceRefLabel: binding.sourceRefLabel,
        }],
      }],
      workContexts: [{
        id: "context-draft",
        targetAgentId: agent.id,
        workspaceBindingId: binding.id,
        agentDraftId: draft.id,
        workItemId: draft.id,
        workItemKind: "agent_draft",
        dock: "closed",
        dockPercent: 32,
        lastViewedAtUnixMs: 40,
      }],
      panes: [{
        id: "pane-draft",
        workContextId: "context-draft",
        position: 0,
        widthBasisPoints: 10_000,
      }],
      terminals: [],
      focusedPaneId: "pane-draft",
    },
    factoryRuns: [],
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

function renderShell(projection: WorkspaceProjection) {
  return render(
    <WorkspaceShell
      projection={projection}
      emitIntent={async () => undefined}
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

// A ResizeObserver that records observations so tests can simulate workspace
// resizes after faking element widths.
type Observation = {
  callback: ResizeObserverCallback
  element: Element
}
let observations: Observation[] = []

class MockResizeObserver {
  constructor(private readonly callback: ResizeObserverCallback) {}
  observe(element: Element) {
    observations.push({ callback: this.callback, element })
  }
  unobserve(element: Element) {
    observations = observations.filter(
      (entry) => entry.callback !== this.callback ||
        entry.element !== element,
    )
  }
  disconnect() {
    observations = observations.filter(
      (entry) => entry.callback !== this.callback,
    )
  }
}

// The global setup defines a writable no-op ResizeObserver; swap in the
// recording mock directly because the property is not configurable.
const OriginalResizeObserver = window.ResizeObserver

beforeEach(() => {
  observations = []
  window.ResizeObserver = MockResizeObserver as unknown as typeof ResizeObserver
})

afterEach(() => {
  window.ResizeObserver = OriginalResizeObserver
})

function resizeWorkspaceTo(width: number) {
  Element.prototype.getBoundingClientRect = function getBoundingClientRect() {
    return {
      width,
      height: 800,
      top: 0,
      left: 0,
      bottom: 800,
      right: width,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    } as DOMRect
  }
  act(() => {
    const fired = new Set<ResizeObserverCallback>()
    for (const { callback, element } of [...observations]) {
      if (fired.has(callback)) continue
      fired.add(callback)
      const rect = element.getBoundingClientRect()
      callback(
        [{
          target: element,
          contentRect: rect,
          borderBoxSize: [{ inlineSize: rect.width, blockSize: rect.height }],
          contentBoxSize: [{ inlineSize: rect.width, blockSize: rect.height }],
        }] as unknown as ResizeObserverEntry[],
        {} as ResizeObserver,
      )
    }
  })
}

function draftOverview() {
  return screen.queryByRole("complementary", { name: "Draft Overview" })
}

describe("Draft Overview responsive surface", () => {
  it("shows Draft Overview inline as a second column when the workspace is wide", () => {
    resizeWorkspaceTo(1200)
    renderShell(draftProjection())

    const pane = screen.getByRole("region", { name: "Commerce Copilot pane" })
    expect(draftOverview()).toBeNull()

    fireEvent.click(screen.getByRole("button", {
      name: "Show Draft Overview",
    }))

    const overview = draftOverview()
    expect(overview).toBeTruthy()
    expect(overview?.tagName).toBe("ASIDE")
    // The column docks inside the workspace pane, not in a popover.
    expect(pane.contains(overview)).toBe(true)
    expect(overview?.closest("[data-slot=popover-content]")).toBeNull()
    // The same Draft Overview content renders in both presentations.
    expect(within(overview as HTMLElement).getByRole("heading", {
      name: "Commerce Copilot",
    })).toBeTruthy()
    // The primary Draft view stays visible beside it.
    expect(screen.getByRole("region", { name: "Commerce Copilot Draft" }))
      .toBeTruthy()
    expect(screen.getByRole("button", { name: "Hide Draft Overview" }))
      .toHaveAttribute("aria-expanded", "true")
  })

  it("hides the inline column when the toggle is pressed again", () => {
    resizeWorkspaceTo(1200)
    renderShell(draftProjection())

    fireEvent.click(screen.getByRole("button", {
      name: "Show Draft Overview",
    }))
    expect(draftOverview()).toBeTruthy()

    fireEvent.click(screen.getByRole("button", {
      name: "Hide Draft Overview",
    }))
    expect(draftOverview()).toBeNull()
    expect(screen.getByRole("button", { name: "Show Draft Overview" }))
      .toHaveAttribute("aria-expanded", "false")
  })

  it("opens Draft Overview as a popover when the workspace is narrow", async () => {
    resizeWorkspaceTo(600)
    renderShell(draftProjection())

    const pane = screen.getByRole("region", { name: "Commerce Copilot pane" })
    expect(draftOverview()).toBeNull()

    const trigger = screen.getByRole("button", {
      name: "Show Draft Overview",
    })
    fireEvent.click(trigger)
    const overview = draftOverview()
    expect(overview).toBeTruthy()
    expect(overview?.closest("[data-slot=popover-content]")).not.toBeNull()
    // The popover overlays the Draft view; it is not part of the pane layout.
    expect(pane.contains(overview)).toBe(false)
    expect(within(overview as HTMLElement).getByRole("heading", {
      name: "Commerce Copilot",
    })).toBeTruthy()

    fireEvent.click(trigger)
    // Base UI animates the popover closed; wait for it to leave the DOM.
    await waitFor(() => {
      expect(draftOverview()).toBeNull()
    })
  })

  it("hides the inline column when the workspace narrows, without opening the popover", () => {
    resizeWorkspaceTo(1200)
    renderShell(draftProjection())

    fireEvent.click(screen.getByRole("button", {
      name: "Show Draft Overview",
    }))
    expect(draftOverview()).toBeTruthy()

    resizeWorkspaceTo(600)

    // Narrowing hides the inline column; the popover stays closed and is
    // available through its trigger rather than opening on its own.
    expect(draftOverview()).toBeNull()
    expect(screen.getByRole("button", { name: "Show Draft Overview" }))
      .toHaveAttribute("aria-expanded", "false")
  })

  it("restores the inline column when the workspace widens again", () => {
    resizeWorkspaceTo(1200)
    renderShell(draftProjection())

    fireEvent.click(screen.getByRole("button", {
      name: "Show Draft Overview",
    }))
    expect(draftOverview()?.tagName).toBe("ASIDE")

    resizeWorkspaceTo(600)
    expect(draftOverview()).toBeNull()

    // Widening back restores the inline column on its own.
    resizeWorkspaceTo(1200)
    const overview = draftOverview()
    expect(overview).toBeTruthy()
    expect(overview?.tagName).toBe("ASIDE")
    expect(screen.getByRole("button", { name: "Hide Draft Overview" }))
      .toHaveAttribute("aria-expanded", "true")
  })

  it("restores the inline column after opening the popover then widening", () => {
    resizeWorkspaceTo(600)
    renderShell(draftProjection())

    fireEvent.click(screen.getByRole("button", {
      name: "Show Draft Overview",
    }))
    expect(draftOverview()?.closest("[data-slot=popover-content]"))
      .not.toBeNull()

    // Widening moves the overview back into the layout as an inline column.
    resizeWorkspaceTo(1200)
    const overview = draftOverview()
    expect(overview).toBeTruthy()
    expect(overview?.tagName).toBe("ASIDE")
    expect(screen.getByRole("button", { name: "Hide Draft Overview" }))
      .toHaveAttribute("aria-expanded", "true")
  })

  it("keeps the overview hidden when the user closed it before resizing", () => {
    resizeWorkspaceTo(1200)
    renderShell(draftProjection())

    fireEvent.click(screen.getByRole("button", {
      name: "Show Draft Overview",
    }))
    fireEvent.click(screen.getByRole("button", {
      name: "Hide Draft Overview",
    }))
    expect(draftOverview()).toBeNull()

    // Resizing within and across the range does not reopen what the user
    // deliberately closed.
    resizeWorkspaceTo(600)
    resizeWorkspaceTo(1200)
    expect(draftOverview()).toBeNull()
    expect(screen.getByRole("button", { name: "Show Draft Overview" }))
      .toHaveAttribute("aria-expanded", "false")
  })
})
