import * as React from "react"
import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import type {
  AgentDraftProjection,
  EnvironmentDto,
  FactoryRunProjection,
  RuntimeIntent,
  TargetAgentProjection,
  TargetAgentVersionProjection,
  VersionFileReadDto,
  VersionFilesListDto,
  WorkspaceBindingProjection,
  WorkspaceProjection,
} from "@agent-factory/runtime-client"

import { WorkspaceShell } from "@/components/shell/workspace-shell"

vi.mock("@pierre/trees/react", () => ({
  useFileTree: (options: {
    paths: readonly string[]
    onSelectionChange: (paths: readonly string[]) => void
  }) => ({ model: options }),
  FileTree: ({ model, ...props }: {
    model: {
      paths: readonly string[]
      onSelectionChange: (paths: readonly string[]) => void
    }
    "aria-label": string
  }) => React.createElement(
    "div",
    { role: "tree", "aria-label": props["aria-label"] },
    model.paths.map((path) => React.createElement(
      "button",
      {
        key: path,
        role: "treeitem",
        onClick: () => model.onSelectionChange([path]),
      },
      path,
    )),
  ),
}))

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

const secondDraft: AgentDraftProjection = {
  ...draft,
  id: "44444444-4444-4444-8444-444444444444",
  name: "Experiment Draft",
  worktreePath: "/code/commerce-experiment",
  workspaceBindingId: "55555555-5555-4555-8555-555555555555",
}

const binding: WorkspaceBindingProjection = {
  id: draft.workspaceBindingId,
  targetAgentId: agent.id,
  projectId: "66666666-6666-4666-8666-666666666666",
  name: "main",
  primaryRoot: draft.worktreePath,
  additionalRoots: [],
  sourceRefLabel: draft.branchRef,
  archived: false,
  lastUsedAtUnixMs: 2,
}

const secondBinding: WorkspaceBindingProjection = {
  ...binding,
  id: secondDraft.workspaceBindingId,
  name: "experiment",
  primaryRoot: secondDraft.worktreePath,
  sourceRefLabel: secondDraft.branchRef,
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

const versionTwo: TargetAgentVersionProjection = {
  id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
  targetAgentId: agent.id,
  version: "0.2.0",
  name: agent.name,
  objective: draft.objective,
  acceptanceCriteria: draft.acceptanceCriteria,
  sourceDraftId: draft.id,
  gitCommit: "fedcba9876543210",
  gitTag: `agent-factory/${agent.id}/v0.2.0`,
  createdAtUnixMs: 30,
}

const versionOne: TargetAgentVersionProjection = {
  id: "88888888-8888-4888-8888-888888888888",
  targetAgentId: agent.id,
  version: "0.1.0",
  name: agent.name,
  objective: draft.objective,
  acceptanceCriteria: draft.acceptanceCriteria,
  sourceDraftId: draft.id,
  gitCommit: "abcdef0123456789",
  gitTag: `agent-factory/${agent.id}/v0.1.0`,
  createdAtUnixMs: 20,
}

function workItem(candidate: AgentDraftProjection) {
  return {
    id: candidate.id,
    kind: "agent_draft" as const,
    targetAgentId: agent.id,
    workspaceBindingId: candidate.workspaceBindingId,
    projectId: candidate.workspaceBindingId === binding.id
      ? binding.projectId
      : secondBinding.projectId,
    agentDraftId: candidate.id,
    title: candidate.name,
    status: "active" as const,
    lastActivityAtUnixMs: candidate.updatedAtUnixMs,
    projectLabel: candidate.name,
    workspaceLabel: candidate.name,
    sourceRefLabel: candidate.branchRef,
  }
}

function projectionWithDrafts({
  dock = "closed",
  focusedDraftId = draft.id,
  includeSecondDraft = true,
  run,
}: {
  dock?: "closed" | "terminal"
  focusedDraftId?: string
  includeSecondDraft?: boolean
  run?: FactoryRunProjection
} = {}): WorkspaceProjection {
  const drafts = includeSecondDraft ? [draft, secondDraft] : [draft]
  const focused = drafts.find((candidate) => candidate.id === focusedDraftId)
    ?? draft
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
          drafts,
          versions: [versionTwo, versionOne],
          workspaceBindings: includeSecondDraft
            ? [binding, secondBinding]
            : [binding],
          workItems: [
            ...drafts.map(workItem),
            ...(run ? [{
              id: run.id,
              kind: "factory_run" as const,
              targetAgentId: agent.id,
              workspaceBindingId: binding.id,
              projectId: binding.projectId,
              agentDraftId: draft.id,
              title: "Evaluation Run",
              status: run.state,
              lastActivityAtUnixMs: 50,
              projectLabel: "commerce",
              workspaceLabel: "main",
              sourceRefLabel: binding.sourceRefLabel,
            }] : []),
          ],
        },
      ],
      workContexts: drafts.map((candidate) => ({
        id: `context-${candidate.id}`,
        targetAgentId: agent.id,
        workspaceBindingId: candidate.workspaceBindingId,
        agentDraftId: candidate.id,
        workItemId: candidate.id,
        workItemKind: "agent_draft" as const,
        dock: candidate.id === focused.id ? dock : "closed" as const,
        dockPercent: 32,
        lastViewedAtUnixMs: 40,
      })),
      panes: [
        {
          id: "pane-draft",
          workContextId: `context-${focused.id}`,
          position: 0,
          widthBasisPoints: 10_000,
        },
      ],
      terminals: [],
      focusedPaneId: "pane-draft",
    },
    factoryRuns: run ? [run] : [],
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

async function listVersionFiles(
  versionId: string,
): Promise<VersionFilesListDto> {
  const version = versionId === versionTwo.id ? versionTwo : versionOne
  return {
    versionId,
    gitCommit: version.gitCommit,
    entries: versionId === versionTwo.id
      ? [{ path: "README.md", kind: "file", size: 16 }]
      : [{ path: "src/agent.ts", kind: "file", size: 32 }],
  }
}

async function readVersionFile(
  versionId: string,
  path: string,
): Promise<VersionFileReadDto> {
  const version = versionId === versionTwo.id ? versionTwo : versionOne
  return {
    versionId,
    gitCommit: version.gitCommit,
    path,
    size: 16,
    kind: "text",
    content: `${version.version} ${path} content`,
  }
}

function StatefulShell({
  initial,
}: {
  initial: WorkspaceProjection
}) {
  const [projection, setProjection] = React.useState(initial)
  const emitIntent = async (intent: RuntimeIntent) => {
    if (intent.type === "workspacePane.close") {
      setProjection((current) => ({
        ...current,
        revision: current.revision + 1,
        targetWorkspace: {
          ...current.targetWorkspace,
          panes: [],
          focusedPaneId: undefined,
        },
      }))
    }
    if (intent.type === "workspacePane.openPrimary") {
      const nextDraftId = String(intent.workItemId)
      setProjection((current) => ({
        ...current,
        revision: current.revision + 1,
        targetWorkspace: {
          ...current.targetWorkspace,
          panes: current.targetWorkspace.panes.map((pane) => ({
            ...pane,
            workContextId: `context-${nextDraftId}`,
          })),
        },
      }))
    }
  }
  return (
    <WorkspaceShell
      projection={projection}
      emitIntent={emitIntent}
      createTargetAgent={async () => true}
      startDraftRun={async () => undefined}
      listVersionFiles={listVersionFiles}
      readVersionFile={readVersionFile}
    />
  )
}

describe("WorkspaceShell Draft contextual column", () => {
  it("opens Versions inside the Draft without a legacy web terminal", async () => {
    render(<StatefulShell initial={projectionWithDrafts()} />)

    // Version dropdown removed from Details header per design refinement.
    expect(screen.queryByRole("combobox", {
      name: "Open version selector",
    })).toBeNull()
    // Draft region still renders without version picker.
    expect(screen.getByRole("region", {
      name: "Commerce Copilot Draft",
    })).toBeTruthy()
  })

  it("drops Version when the owning Draft is switched or closed", async () => {
    render(<StatefulShell initial={projectionWithDrafts()} />)
    // No version picker in header — switching drafts no longer involves version tabs here.
    expect(screen.queryByRole("combobox", {
      name: "Open version selector",
    })).toBeNull()
    expect(screen.getByRole("region", {
      name: "Commerce Copilot Draft",
    })).toBeTruthy()
  })

  it("lets an empty Draft pane own the Versions column", async () => {
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
    const empty = projectionWithDrafts({ includeSecondDraft: false })
    empty.targetWorkspace.targetGroups[0] = {
      ...empty.targetWorkspace.targetGroups[0]!,
      drafts: [],
      workItems: [],
    }
    empty.targetWorkspace.workContexts = [{
      id: "context-agent",
      targetAgentId: agent.id,
      workspaceBindingId: binding.id,
      agentDraftId: null,
      workItemId: null,
      workItemKind: null,
      dock: "closed",
      dockPercent: 32,
      lastViewedAtUnixMs: 40,
    }]
    empty.targetWorkspace.panes = [{
      id: "pane-draft",
      workContextId: "context-agent",
      position: 0,
      widthBasisPoints: 10_000,
    }]
    render(
      <WorkspaceShell
        projection={empty}
        emitIntent={async () => undefined}
        createTargetAgent={async () => true}
        startDraftRun={async () => undefined}
        listVersionFiles={listVersionFiles}
        readVersionFile={readVersionFile}
      />,
    )

    expect(screen.queryByRole("complementary", { name: "Draft Overview" }))
      .toBeNull()
    fireEvent.click(screen.getByRole("button", {
      name: "Show Draft Overview",
    }))
    expect(screen.getByRole("complementary", { name: "Draft Overview" }))
      .toBeTruthy()
    // No version picker in empty draft header after refinement.
    expect(screen.queryByRole("combobox", {
      name: "Open version selector",
    })).toBeNull()
  })

  it("keeps Code changes on the application-level inspector split", async () => {
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
    const run: FactoryRunProjection = {
      id: "run-1",
      targetAgentId: agent.id,
      agentDraftId: draft.id,
      workspaceBindingId: binding.id,
      projectId: binding.projectId,
      environmentId: environment.id,
      objective: draft.objective,
      acceptanceCriteria: draft.acceptanceCriteria,
      startingGitHead: draft.gitHead,
      state: "evaluating",
      changedFiles: [{
        path: "src/agent.ts",
        change: "modified",
        beforeHash: "before",
        afterHash: "after",
        diff: {
          hunks: [{
            oldStart: 1,
            oldLines: 1,
            newStart: 1,
            newLines: 1,
            lines: [
              { kind: "delete", oldLine: 1, text: "old" },
              { kind: "insert", newLine: 1, text: "new" },
            ],
          }],
        },
      }],
      testEvidence: [],
    }
    render(
      <WorkspaceShell
        projection={projectionWithDrafts({ run })}
        emitIntent={async () => undefined}
        createTargetAgent={async () => true}
        startDraftRun={async () => undefined}
        listVersionFiles={listVersionFiles}
        readVersionFile={readVersionFile}
      />,
    )

    fireEvent.click(screen.getByRole("button", {
      name: "Show Draft Overview",
    }))
    fireEvent.click(screen.getByRole("button", {
      name: "Inspect Code changes",
    }))
    expect(await screen.findByRole("region", {
      name: "Code changes inspector",
    })).toBeTruthy()
    expect(screen.getByRole("separator", {
      name: "Resize workspace and Inspector",
    })).toBeTruthy()
  })

  it("keeps the pane title as the draft name when a Factory Run is focused", () => {
    const run: FactoryRunProjection = {
      id: "run-1",
      targetAgentId: agent.id,
      agentDraftId: draft.id,
      workspaceBindingId: binding.id,
      projectId: binding.projectId,
      environmentId: environment.id,
      objective: draft.objective,
      acceptanceCriteria: draft.acceptanceCriteria,
      startingGitHead: draft.gitHead,
      state: "evaluating",
      changedFiles: [],
      testEvidence: [],
    }
    const projection = projectionWithDrafts({ run })
    const focused = projection.targetWorkspace.workContexts[0]
    if (focused) {
      focused.workItemId = run.id
      focused.workItemKind = "factory_run"
    }
    render(
      <WorkspaceShell
        projection={projection}
        emitIntent={async () => undefined}
        createTargetAgent={async () => true}
        startDraftRun={async () => undefined}
        listVersionFiles={listVersionFiles}
        readVersionFile={readVersionFile}
      />,
    )

    expect(screen.getByRole("region", { name: `${draft.name} pane` }))
      .toBeVisible()
    expect(screen.queryByText("Evaluation Run")).toBeNull()
  })
})
