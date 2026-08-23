import type { ReactNode } from "react"
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import type {
  AgentDraftProjection,
  EnvironmentDto,
  FactoryRunProjection,
  HerdrStatusDto,
  LiveAgentDto,
  RuntimeIntent,
  SessionProjection,
  TargetAgentProjection,
  WorkspaceBindingProjection,
} from "@agent-factory/runtime-client"

import {
  AgentDraftWorkspace,
  DraftOverview,
} from "@/components/agents/agent-draft-workspace"

const agent: TargetAgentProjection = {
  id: "11111111-1111-4111-8111-111111111111",
  name: "IPL Expert",
  repositoryRoot: "/code/ipl",
  archived: false,
  lastActivityAtUnixMs: 2,
}

const draft: AgentDraftProjection = {
  id: "22222222-2222-4222-8222-222222222222",
  targetAgentId: agent.id,
  workspaceBindingId: "33333333-3333-4333-8333-333333333333",
  name: "IPL Expert",
  objective: "Answer IPL questions accurately",
  acceptanceCriteria: ["Answers cite match evidence"],
  baseVersion: "0.1.0",
  branchRef: `agent-factory/${agent.id}/drafts/22222222-2222-4222-8222-222222222222`,
  worktreePath: "/code/ipl-ipl-expert-main-22222222",
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

function makeRun(
  state: FactoryRunProjection["state"],
  overrides: Partial<FactoryRunProjection> = {},
): FactoryRunProjection {
  return {
    id: `run-${state}`,
    targetAgentId: agent.id,
    agentDraftId: draft.id,
    workspaceBindingId: binding.id,
    projectId: binding.projectId,
    environmentId: environment.id,
    objective: draft.objective,
    state,
    acceptanceCriteria: draft.acceptanceCriteria,
    startingGitHead: draft.gitHead,
    changedFiles: [],
    testEvidence: [],
    ...overrides,
  }
}

function makeSession(
  purpose: SessionProjection["purpose"],
  overrides: Partial<SessionProjection> = {},
): SessionProjection {
  return {
    id: `${purpose}-session`,
    projectId: binding.projectId,
    environmentId: environment.id,
    targetAgentId: agent.id,
    workspaceBindingId: binding.id,
    title: `${purpose} session`,
    purpose,
    lifecycle: "working",
    harnessId: "claude",
    herdrAgentName: `${purpose}-agent`,
    availability: "live",
    attention: [],
    briefDelivered: true,
    createdAtUnixMs: 1,
    lastActivityAtUnixMs: 1,
    ...overrides,
  }
}

function renderWorkspace({
  runs = [],
  sessions = [],
  liveAgents = [],
  herdr,
  selectedRunId,
  environments = [environment],
  activeDraft = draft,
  emitIntent = vi.fn(async () => undefined),
  startDraftRun = vi.fn(async () => undefined),
  onOpenCodeChanges = vi.fn(),
  showOverview = true,
  versionSurface,
  nativeTerminalVisible = false,
}: {
  runs?: readonly FactoryRunProjection[]
  sessions?: readonly SessionProjection[]
  liveAgents?: readonly LiveAgentDto[]
  herdr?: HerdrStatusDto
  selectedRunId?: string
  environments?: readonly EnvironmentDto[]
  activeDraft?: AgentDraftProjection
  emitIntent?: (intent: RuntimeIntent) => Promise<void>
  startDraftRun?: (
    runId: string,
    draftId: string,
    environmentId: string,
  ) => Promise<void>
  onOpenCodeChanges?: (run: FactoryRunProjection) => void
  showOverview?: boolean
  versionSurface?: ReactNode
  nativeTerminalVisible?: boolean
} = {}) {
  const onDraftWorkflowChange = vi.fn()
  const activeRunId = selectedRunId ??
    runs.find((run) => !["passed", "failed", "needs_review", "cancelled"]
      .includes(run.state))?.id
  const projectedSessions = sessions.map((session) =>
    session.factoryRunId || !activeRunId
      ? session
      : { ...session, factoryRunId: activeRunId })
  render(
    <>
      <AgentDraftWorkspace
        agent={agent}
        draft={activeDraft}
        project={{
          id: binding.projectId,
          name: "IPL Expert main",
          root: draft.worktreePath,
          trusted: true,
        }}
        runs={runs}
        sessions={projectedSessions}
        liveAgents={liveAgents}
        herdr={herdr}
        selectedRunId={selectedRunId}
        environments={environments}
        emitIntent={emitIntent}
        startDraftRun={startDraftRun}
        onDraftWorkflowChange={onDraftWorkflowChange}
        versionSurface={versionSurface}
        nativeTerminalVisible={nativeTerminalVisible}
      />
      {showOverview ? (
        <aside aria-label="Draft Overview">
          <DraftOverview
            agent={agent}
            draft={activeDraft}
            versions={[]}
            environments={environments}
            selectedRun={runs.find((run) => run.id === selectedRunId)}
            emitIntent={emitIntent}
            onOpenCodeChanges={onOpenCodeChanges}
          />
        </aside>
      ) : null}
    </>,
  )
  return {
    emitIntent,
    startDraftRun,
    onOpenCodeChanges,
    onDraftWorkflowChange,
  }
}

function latestWorkflow(
  onDraftWorkflowChange: ReturnType<typeof vi.fn>,
) {
  for (let index = onDraftWorkflowChange.mock.calls.length - 1; index >= 0; index -= 1) {
    const chrome = onDraftWorkflowChange.mock.calls[index]?.[0]
    if (chrome) return chrome
  }
  return undefined
}

async function openEditForm(
  onDraftWorkflowChange: ReturnType<typeof vi.fn>,
) {
  await waitFor(() => {
    expect(latestWorkflow(onDraftWorkflowChange)?.canEdit).toBe(true)
  })
  latestWorkflow(onDraftWorkflowChange)?.onEdit()
  await waitFor(() => {
    expect(screen.getByRole("heading", { name: "Edit draft" })).toBeVisible()
  })
}

describe("AgentDraftWorkspace", () => {
  it("shows a simplified floating overview without Draft badge or trust", () => {
    renderWorkspace()

    expect(screen.getByRole("heading", { name: draft.name })).toBeVisible()
    expect(screen.queryByText("Draft", { selector: ".bg-secondary, [data-slot=badge]" })).toBeNull()
    expect(screen.getByText("Derived from v0.1.0")).toBeVisible()
    // Objective / Success criteria are compact rows without secondary summaries.
    expect(screen.getByRole("button", { name: "Objective" })).toBeVisible()
    expect(screen.queryByText(draft.objective, { exact: true })).toBeNull()
    expect(screen.getByRole("button", { name: "Success criteria" })).toBeVisible()
    expect(screen.queryByText("1 criterion")).toBeNull()
    expect(screen.getByText("Environment A")).toBeVisible()
    expect(screen.getByText("Code changes")).toBeVisible()
    expect(screen.getByText("0 files")).toBeVisible()
    expect(screen.getByText("../ipl-ipl-expert-main-22222222")).toBeVisible()
    expect(screen.queryByText("Trust workspace")).toBeNull()
    expect(screen.getByRole("heading", { name: "Session History" }))
      .toBeVisible()
    expect(screen.getByText("No session history yet")).toBeVisible()
    expect(screen.getByText(
      "Completed Orchestrator, Coding, and Evaluation sessions appear here under the Run that created them.",
    )).toBeVisible()
  })

  it("registers Edit workflow while no Runs exist and hides Edit after a Run", async () => {
    const onDraftWorkflowChange = vi.fn()
    const { rerender } = render(
      <AgentDraftWorkspace
        agent={agent}
        draft={draft}
        project={{
          id: binding.projectId,
          name: "IPL Expert main",
          root: draft.worktreePath,
          trusted: true,
        }}
        runs={[]}
        sessions={[]}
        environments={[environment]}
        emitIntent={async () => undefined}
        startDraftRun={async () => undefined}
        onDraftWorkflowChange={onDraftWorkflowChange}
      />,
    )

    await waitFor(() => {
      expect(onDraftWorkflowChange).toHaveBeenCalled()
    })
    const chrome = onDraftWorkflowChange.mock.calls.at(-1)?.[0]
    expect(chrome?.canEdit).toBe(true)

    rerender(
      <AgentDraftWorkspace
        agent={agent}
        draft={draft}
        project={{
          id: binding.projectId,
          name: "IPL Expert main",
          root: draft.worktreePath,
          trusted: true,
        }}
        runs={[makeRun("coding")]}
        sessions={[]}
        environments={[environment]}
        emitIntent={async () => undefined}
        startDraftRun={async () => undefined}
        onDraftWorkflowChange={onDraftWorkflowChange}
      />,
    )
    await waitFor(() => {
      const next = onDraftWorkflowChange.mock.calls.at(-1)?.[0]
      expect(next?.canEdit).toBe(false)
    })
  })

  it("opens the shared Define your agent form for Edit before the first Run", async () => {
    const { emitIntent, onDraftWorkflowChange } = renderWorkspace()

    await openEditForm(onDraftWorkflowChange)
    fireEvent.change(screen.getByLabelText("Objective"), {
      target: { value: "Answer every IPL question with evidence" },
    })
    fireEvent.click(screen.getByRole("button", { name: "Save" }))
    await waitFor(() => {
      expect(emitIntent).toHaveBeenCalledWith(expect.objectContaining({
        type: "agentDraft.update",
        agentDraftId: draft.id,
        objective: "Answer every IPL question with evidence",
      }))
    })
  })

  it("keeps Draft Overview out of the workspace page layout", () => {
    renderWorkspace({ showOverview: false })

    expect(screen.queryByRole("complementary", { name: "Draft Overview" }))
      .toBeNull()
    expect(screen.getByRole("heading", { name: "Session History" }))
      .toBeVisible()
  })

  it("renders Draft Overview independently from the workspace region", () => {
    renderWorkspace()

    const workspace = screen.getByRole("region", { name: "IPL Expert Draft" })
    const overview = screen.getByRole("complementary", {
      name: "Draft Overview",
    })
    expect(workspace.contains(overview)).toBe(false)
    expect(screen.getByRole("heading", { name: draft.name })).toBeVisible()
  })

  it("elevates Version as a reserved floating surface", () => {
    renderWorkspace({
      versionSurface: (
        <div role="region" aria-label="Versions">files</div>
      ),
    })

    const overview = screen.getByRole("complementary", {
      name: "Draft Overview",
    })
    const versionCard = screen.getByRole("region", { name: "Versions" })
      .parentElement
    expect(versionCard?.hasAttribute("data-context-surface")).toBe(true)
    expect(versionCard?.className).toContain("rounded-lg")
    expect(versionCard?.className).toContain("shadow-md")
    expect(versionCard?.className).not.toContain("absolute")
    expect(overview.contains(versionCard)).toBe(false)
    const draftSeam = screen.getByRole("separator", {
      name: "Resize Draft and context",
    })
    expect(draftSeam.getAttribute("data-reveal")).toBe("hover")
    expect(draftSeam.className).toContain("bg-transparent")
    expect(draftSeam.className).not.toContain("bg-border")
    expect(draftSeam.className).toContain("w-3")
  })

  it("offers one Run-level Open action instead of embedded agent terminals", () => {
    const orchestrator = makeSession("orchestration", {
      paneId: "orch-pane",
      lifecycle: "working",
    })
    const run = makeRun("orchestrating")
    renderWorkspace({
      runs: [run],
      sessions: [orchestrator],
      selectedRunId: run.id,
    })
    expect(screen.getByRole("button", { name: "Open Terminal" })).toBeDisabled()
    expect(screen.queryByRole("region", { name: "Orchestrator terminal" }))
      .toBeNull()
  })

  it("does not add a Herdr terminal webview beside Draft", () => {
    const coding = makeSession("coding", {
      paneId: "coding-pane",
      lifecycle: "working",
    })
    const run = makeRun("coding")
    renderWorkspace({
      versionSurface: (
        <div role="region" aria-label="Versions">files</div>
      ),
      runs: [run],
      sessions: [coding],
      selectedRunId: run.id,
    })

    const overview = screen.getByRole("complementary", {
      name: "Draft Overview",
    })
    expect(screen.queryByRole("region", { name: "Coding Agent terminal" }))
      .toBeNull()
    expect(overview).toBeVisible()
  })

  it("starts a new Run with the selected ready Environment", () => {
    const { startDraftRun } = renderWorkspace()

    fireEvent.click(screen.getByRole("button", { name: "Start Run" }))
    expect(startDraftRun).toHaveBeenCalledWith(
      expect.any(String),
      draft.id,
      environment.id,
    )
  })

  it("shows the native terminal action before a Run exists", () => {
    const emitIntent = vi.fn(async () => undefined)
    renderWorkspace({
      herdr: { connected: true, freshness: "live", issues: [] },
      emitIntent,
    })

    fireEvent.click(screen.getByRole("button", { name: "Open Terminal" }))
    expect(emitIntent).toHaveBeenCalledWith({
      type: "agentDraft.toggleWorkspace",
      agentDraftId: draft.id,
    })
  })

  it("shows a running spinner while Start Run is being accepted", async () => {
    let finishStart: (() => void) | undefined
    const startDraftRun = vi.fn<(
      runId: string,
      draftId: string,
      environmentId: string,
    ) => Promise<void>>(() => new Promise<void>((resolve) => {
      finishStart = resolve
    }))
    const emitIntent = vi.fn(async () => undefined)
    renderWorkspace({ startDraftRun, emitIntent })

    fireEvent.click(screen.getByRole("button", { name: "Start Run" }))
    expect(screen.getByRole("button", { name: "Starting Run…" }))
      .toBeDisabled()
    const runId = startDraftRun.mock.calls[0]?.[0]
    fireEvent.click(screen.getByRole("button", { name: "Cancel Run" }))
    expect(emitIntent).toHaveBeenCalledWith({ type: "run.cancel", runId })

    finishStart?.()
    await waitFor(() => expect(
      screen.getByRole("button", { name: "Start Run" }),
    ).toBeVisible())
  })

  it("opens the Draft's Herdr workspace while a Run is active", () => {
    const run = makeRun("orchestrating")
    const emitIntent = vi.fn(async () => undefined)
    renderWorkspace({
      runs: [run],
      selectedRunId: run.id,
      herdr: { connected: true, freshness: "live", issues: [] },
      emitIntent,
    })

    fireEvent.click(screen.getByRole("button", { name: "Open Terminal" }))
    expect(emitIntent).toHaveBeenCalledWith({
      type: "agentDraft.toggleWorkspace",
      agentDraftId: draft.id,
    })
  })

  it("routes the close action through the same Run workspace intent", () => {
    const run = makeRun("orchestrating")
    const emitIntent = vi.fn(async () => undefined)
    renderWorkspace({
      runs: [run],
      selectedRunId: run.id,
      herdr: { connected: true, freshness: "live", issues: [] },
      emitIntent,
      nativeTerminalVisible: true,
    })

    fireEvent.click(screen.getByRole("button", { name: "Close Terminal" }))
    expect(emitIntent).toHaveBeenCalledWith({
      type: "agentDraft.toggleWorkspace",
      agentDraftId: draft.id,
    })
  })

  it("exposes relative worktree display with full path on copy", async () => {
    const writeText = vi.fn(async () => undefined)
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    })
    renderWorkspace()

    fireEvent.click(screen.getByRole("button", { name: "Copy Worktree path" }))
    expect(writeText).toHaveBeenCalledWith(draft.worktreePath)
    expect(screen.getByText("../ipl-ipl-expert-main-22222222")).toBeVisible()
  })

  it("records the Environment chosen in the shared Edit form on the Draft", async () => {
    const environmentB = { ...environment, id: "environment-b", name: "Environment B" }
    const { emitIntent, onDraftWorkflowChange } = renderWorkspace({
      environments: [environment, environmentB],
    })

    await openEditForm(onDraftWorkflowChange)
    // Base UI Select selects on pointerdown, not click alone.
    fireEvent.pointerDown(screen.getByRole("combobox", { name: "Environment" }))
    fireEvent.click(screen.getByRole("combobox", { name: "Environment" }))
    const option = await screen.findByRole("option", { name: "Environment B" })
    fireEvent.pointerDown(option)
    fireEvent.click(option)
    fireEvent.click(screen.getByRole("button", { name: "Save" }))

    await waitFor(() => {
      expect(emitIntent).toHaveBeenCalledWith({
        type: "agentDraft.environment.set",
        agentDraftId: draft.id,
        environmentId: environmentB.id,
      })
    })
  })

  it("starts a Run in the Environment the Draft remembers", () => {
    const environmentB = { ...environment, id: "environment-b", name: "Environment B" }
    const { startDraftRun } = renderWorkspace({
      environments: [environment, environmentB],
      activeDraft: { ...draft, environmentId: environmentB.id },
    })

    fireEvent.click(screen.getByRole("button", { name: "Start Run" }))

    expect(startDraftRun).toHaveBeenCalledWith(
      expect.any(String),
      draft.id,
      environmentB.id,
    )
  })

  // A remembered Environment is a preference, not a promise: one that has been
  // removed or stopped being ready must not leave the Draft unable to start.
  it("falls back to a ready Environment when the remembered one is gone", () => {
    const { startDraftRun } = renderWorkspace({
      environments: [environment],
      activeDraft: { ...draft, environmentId: "environment-deleted" },
    })

    fireEvent.click(screen.getByRole("button", { name: "Start Run" }))

    expect(startDraftRun).toHaveBeenCalledWith(
      expect.any(String),
      draft.id,
      environment.id,
    )
  })

  it("picks the Draft's Environment beside Start Run when more than one is ready", async () => {
    const environmentB = { ...environment, id: "environment-b", name: "Environment B" }
    const { emitIntent } = renderWorkspace({
      environments: [environment, environmentB],
    })

    const picker = screen.getByRole("combobox", {
      name: "Environment for this Draft",
    })
    // A person picks by name, so the trigger shows the name rather than the id
    // the value carries.
    expect(picker).toHaveTextContent(environment.name)
    fireEvent.pointerDown(picker)
    fireEvent.click(picker)
    const option = await screen.findByRole("option", { name: "Environment B" })
    fireEvent.pointerDown(option)
    fireEvent.click(option)

    await waitFor(() => {
      expect(emitIntent).toHaveBeenCalledWith({
        type: "agentDraft.environment.set",
        agentDraftId: draft.id,
        environmentId: environmentB.id,
      })
    })
  })

  // One Environment is no choice, so the picker would be an affordance for
  // something the user cannot do.
  it("offers no Environment picker when only one is ready", () => {
    renderWorkspace({ environments: [environment] })

    expect(
      screen.queryByRole("combobox", { name: "Environment for this Draft" }),
    ).toBeNull()
  })

  // The Orchestrator advances its own Run from inside its pane, so the only
  // lifecycle action a person takes on a live Run is stopping it.
  it.each([
    "draft",
    "orchestrating",
    "coding",
    "evaluating",
    "escalated",
  ] as const)("offers only Cancel Run while a %s Run is live", (state) => {
    const run = makeRun(state)
    const { emitIntent } = renderWorkspace({
      runs: [run],
      selectedRunId: run.id,
    })

    expect(screen.queryByRole("button", { name: "Start Coding" })).toBeNull()
    expect(screen.queryByRole("button", { name: "Iterate" })).toBeNull()
    expect(
      screen.queryByRole("button", { name: "Ready for Evaluation" }),
    ).toBeNull()

    fireEvent.click(screen.getByRole("button", { name: "Cancel Run" }))
    expect(emitIntent).toHaveBeenCalledWith({
      type: "run.cancel",
      runId: run.id,
    })
  })

  it.each(["passed", "failed", "needs_review", "cancelled"] as const)(
    "resets to Start after a %s Run",
    (state) => {
      const run = makeRun(state)
      renderWorkspace({ runs: [run], selectedRunId: run.id })

      expect(screen.getByRole("button", { name: "Start Run" })).toBeVisible()
      expect(screen.getByRole("button", { name: "Open Terminal" })).toBeVisible()
      expect(screen.queryByRole("button", { name: "Iterate" })).toBeNull()
      expect(screen.queryByRole("button", { name: "Cancel Run" })).toBeNull()
    },
  )

  it("renders projected Coding and Evaluation sessions without per-agent Open actions", () => {
    const orchestrator = makeSession("orchestration", {
      id: "orchestrator-session",
      lifecycle: "idle",
    })
    const coding = makeSession("coding", {
      paneId: "coding-pane",
      lifecycle: "working",
      parentSessionId: orchestrator.id,
    })
    const evaluation = makeSession("evaluation", {
      paneId: "evaluation-pane",
      lifecycle: "blocked",
      attention: ["Approval required"],
      parentSessionId: orchestrator.id,
    })
    const run = makeRun("evaluating")
    renderWorkspace({
      runs: [run],
      sessions: [orchestrator, coding, evaluation],
      selectedRunId: run.id,
    })

    expect(screen.getAllByText("Orchestrator").length).toBeGreaterThan(0)
    expect(screen.getAllByText("Coding Agent").length).toBeGreaterThan(0)
    expect(screen.getAllByText("Evaluation Agent").length).toBeGreaterThan(0)
    expect(screen.getAllByText("Working").length).toBeGreaterThan(0)
    expect(screen.getAllByText("Needs attention").length).toBeGreaterThan(0)
    expect(screen.getAllByText("Approval required").length).toBeGreaterThan(0)
    expect(screen.queryByText("Coding has not started.")).toBeNull()
    expect(screen.getAllByText(
      "The agent is working in Herdr. Open the Run workspace to view it.",
    ).length).toBeGreaterThan(0)
    expect(screen.queryByRole("button", { name: "Open Coding Agent" }))
      .toBeNull()
    expect(screen.queryByRole("button", { name: "Open Evaluation Agent" }))
      .toBeNull()
  })

  it("labels stale authority and keeps unassociated Herdr agents visible", () => {
    const run = makeRun("coding")
    const liveAgent: LiveAgentDto = {
      workspaceBindingId: binding.id,
      managedSessionId: null,
      factoryRunId: null,
      purpose: null,
      agentName: "manual-helper",
      displayAgent: "Claude Code",
      agentKind: "claude",
      lifecycle: "working",
      attention: [],
      placement: {
        workspaceId: "workspace-1",
        tabId: "tab-2",
        paneId: "pane-2",
        agentName: "manual-helper",
      },
      revision: 4,
      observedAtUnixMs: 5,
    }
    renderWorkspace({
      runs: [run],
      selectedRunId: run.id,
      liveAgents: [liveAgent],
      herdr: {
        connected: false,
        freshness: "last_observed",
        observedAtUnixMs: 5,
        issues: ["Herdr is reconnecting."],
      },
    })

    expect(screen.getByText("Last observed")).toBeVisible()
    expect(screen.getByRole("heading", {
      name: "Other runtime activity",
    })).toBeVisible()
    expect(screen.getByText("manual-helper")).toBeVisible()
    expect(screen.getByText("Claude Code")).toBeVisible()
  })

  it("groups every observed managed session by Run and parentage", () => {
    const selectedRun = makeRun("coding", { id: "selected-run" })
    const previousRun = makeRun("passed", { id: "previous-run" })
    const orchestrator = makeSession("orchestration", {
      id: "orchestrator-session",
      factoryRunId: selectedRun.id,
      lifecycle: "idle",
    })
    const coding = makeSession("coding", {
      id: "coding-child",
      factoryRunId: selectedRun.id,
      parentSessionId: orchestrator.id,
    })
    const previousEvaluation = makeSession("evaluation", {
      id: "previous-evaluation",
      factoryRunId: previousRun.id,
      availability: "last_observed",
      lifecycle: "done",
    })
    renderWorkspace({
      runs: [previousRun, selectedRun],
      sessions: [previousEvaluation, coding, orchestrator],
      selectedRunId: selectedRun.id,
    })

    const selectedGroup = screen.getByRole("region", {
      name: "Selected Run managed agents",
    })
    const previousGroup = screen.getByRole("region", {
      name: "Run managed agents",
    })
    expect(selectedGroup).toHaveTextContent("Orchestrator")
    expect(selectedGroup).toHaveTextContent("Coding Agent")
    expect(previousGroup).toHaveTextContent("Evaluation Agent")
    expect(previousGroup).toHaveTextContent("Last observed")
  })

  it("shows the selected Run Environment and derives code-change totals", () => {
    const run = makeRun("passed", {
      changedFiles: [{
        path: "src/index.ts",
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
    })
    const onOpenCodeChanges = vi.fn()
    renderWorkspace({
      runs: [run],
      selectedRunId: run.id,
      onOpenCodeChanges,
    })

    expect(screen.getByText("Frozen for this Run")).toBeVisible()
    expect(screen.queryByLabelText("Run Environment")).toBeNull()
    expect(screen.getByText("1 files")).toBeVisible()
    expect(screen.getByLabelText("1 additions")).toHaveTextContent("+1")
    expect(screen.getByLabelText("1 deletions")).toHaveTextContent("−1")
    fireEvent.click(screen.getByRole("button", {
      name: "Inspect Code changes",
    }))
    expect(onOpenCodeChanges).toHaveBeenCalledWith(run)
  })

  it("cancels the shared Edit form without saving", async () => {
    const { onDraftWorkflowChange } = renderWorkspace()

    await openEditForm(onDraftWorkflowChange)
    fireEvent.change(screen.getByLabelText("Name"), {
      target: { value: "Changed name" },
    })
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }))

    await waitFor(() => {
      expect(screen.queryByRole("heading", { name: "Edit draft" })).toBeNull()
    })
    expect(screen.getByRole("heading", { name: draft.name })).toBeVisible()
  })

  it("exposes Create Version through registered workflow chrome", async () => {
    const { onDraftWorkflowChange } = renderWorkspace()

    await waitFor(() => {
      expect(latestWorkflow(onDraftWorkflowChange)).toBeTruthy()
    })
    latestWorkflow(onDraftWorkflowChange)?.onCreateVersion()
    await waitFor(() => {
      expect(screen.getByRole("dialog", {
        name: "Create immutable Version",
      })).toBeVisible()
    })
  })

  it("lists Orchestrator, Coding, and Evaluation sessions in Session History", () => {
    const coding = makeSession("coding", {
      lifecycle: undefined,
      availability: "historical",
      outcome: { kind: "completed", summary: "Coding completed", recordedAtUnixMs: 2 },
      initialPrompt: "Implement the objective",
    })
    const evaluation = makeSession("evaluation", {
      lifecycle: undefined,
      availability: "historical",
      outcome: { kind: "completed", summary: "Evaluation completed", recordedAtUnixMs: 3 },
      initialPrompt: "Evaluate the workspace",
    })
    const run = makeRun("passed", {
      evaluation: {
        verdict: "pass",
        summary: "Acceptance criteria are met",
        findings: [],
        protocolValid: true,
      },
    })
    renderWorkspace({
      runs: [run],
      sessions: [coding, evaluation],
      selectedRunId: run.id,
    })

    expect(screen.getByRole("heading", { name: "Session History" }))
      .toBeVisible()
    expect(screen.getAllByText("Orchestrator").length).toBeGreaterThan(0)
    expect(screen.getAllByText("Coding").length).toBeGreaterThan(0)
    expect(screen.getAllByText("Evaluation").length).toBeGreaterThan(0)
  })

  it("groups persisted session history under the Run that created it", () => {
    const firstRun = makeRun("cancelled", {
      id: "first-run",
      objective: "Build the first iteration",
    })
    const secondRun = makeRun("passed", {
      id: "second-run",
      objective: "Build the second iteration",
    })
    const sessions = [
      makeSession("orchestration", {
        id: "first-orchestrator",
        factoryRunId: firstRun.id,
        availability: "historical",
        lifecycle: undefined,
      }),
      makeSession("coding", {
        id: "first-coding",
        factoryRunId: firstRun.id,
        parentSessionId: "first-orchestrator",
        availability: "historical",
        lifecycle: undefined,
      }),
      makeSession("orchestration", {
        id: "second-orchestrator",
        factoryRunId: secondRun.id,
        availability: "historical",
        lifecycle: undefined,
      }),
      makeSession("evaluation", {
        id: "second-evaluation",
        factoryRunId: secondRun.id,
        parentSessionId: "second-orchestrator",
        availability: "historical",
        lifecycle: undefined,
      }),
    ]
    renderWorkspace({ runs: [firstRun, secondRun], sessions })

    const first = screen.getByRole("region", {
      name: "Run session history: Build the first iteration",
    })
    const second = screen.getByRole("region", {
      name: "Run session history: Build the second iteration",
    })
    expect(within(first).getByText("Cancelled")).toBeVisible()
    expect(within(first).getByRole("button", {
      name: "Coding session, Historical",
    })).toBeVisible()
    expect(within(second).getByText("Passed")).toBeVisible()
    expect(within(second).getByRole("button", {
      name: "Evaluation session, Historical",
    })).toBeVisible()
  })

  it("keeps a live Orchestrator out of historical session rows", () => {
    const orchestrator = makeSession("orchestration", { lifecycle: "idle" })
    const run = makeRun("orchestrating")
    renderWorkspace({
      runs: [run],
      sessions: [orchestrator],
      selectedRunId: run.id,
    })

    expect(screen.queryByRole("button", {
      name: "Orchestrator session, Idle",
    })).toBeNull()
    expect(screen.queryByRole("button", {
      name: "Coding session, Idle",
    })).toBeNull()
    expect(screen.getByText("No session history yet")).toBeVisible()
    expect(screen.getByText("The agent is ready for input.")).toBeVisible()
  })

  it("expands session history to show initial input and completed output", async () => {
    const coding = makeSession("coding", {
      lifecycle: undefined,
      availability: "historical",
      outcome: { kind: "completed", summary: "Coding completed", recordedAtUnixMs: 2 },
      initialPrompt: "Implement the objective",
    })
    const run = makeRun("passed", {
      evaluation: {
        verdict: "pass",
        summary: "Acceptance criteria are met",
        findings: [],
        protocolValid: true,
      },
    })
    const readAgentTranscript = vi.fn(async () => ({
      agentSessionId: coding.id,
      capturedAtUnixMs: 1,
      revision: 1,
      text: "coding finished the turn",
      truncated: false,
    }))
    render(
      <AgentDraftWorkspace
        agent={agent}
        draft={draft}
        project={{
          id: binding.projectId,
          name: "IPL Expert main",
          root: draft.worktreePath,
          trusted: true,
        }}
        runs={[run]}
        sessions={[{ ...coding, factoryRunId: run.id }]}
        selectedRunId={run.id}
        environments={[environment]}
        emitIntent={vi.fn(async () => undefined)}
        startDraftRun={vi.fn(async () => undefined)}
        readAgentTranscript={readAgentTranscript}
      />,
    )

    fireEvent.click(screen.getByRole("button", {
      name: "Orchestrator session, Passed",
    }))
    expect(screen.getByText(/Objective:/)).toBeVisible()
    expect(screen.getByText(/Verdict: pass/)).toBeVisible()
    expect(screen.getByText(/Acceptance criteria are met/)).toBeVisible()

    fireEvent.click(screen.getByRole("button", {
      name: "Coding session, Historical",
    }))
    expect(screen.getByText("Implement the objective")).toBeVisible()
    await waitFor(() => {
      expect(screen.getAllByText("coding finished the turn").length)
        .toBeGreaterThan(0)
    })
    expect(readAgentTranscript).toHaveBeenCalledWith(coding.id)
  })

  it("keeps session history read-only without terminal Open actions", () => {
    const coding = makeSession("coding", {
      paneId: "coding-pane",
      lifecycle: undefined,
      availability: "historical",
    })
    const evaluation = makeSession("evaluation", {
      paneId: "evaluation-pane",
      lifecycle: undefined,
      availability: "historical",
    })
    const run = makeRun("passed")
    renderWorkspace({
      runs: [run],
      sessions: [coding, evaluation],
      selectedRunId: run.id,
    })

    expect(screen.queryByRole("button", { name: "Open Orchestrator" }))
      .toBeNull()
    expect(screen.queryByRole("button", { name: "Open Coding terminal" }))
      .toBeNull()
    expect(screen.queryByRole("button", { name: "Open Evaluation terminal" }))
      .toBeNull()
  })

  it("shows the no-active-Draft state without inline Version history", () => {
    render(
      <AgentDraftWorkspace
        agent={agent}
        runs={[]}
        sessions={[]}
        environments={[]}
        emitIntent={vi.fn(async () => undefined)}
        startDraftRun={vi.fn(async () => undefined)}
      />,
    )

    expect(screen.queryByRole("complementary", { name: "Draft Overview" }))
      .toBeNull()
    expect(screen.queryByRole("heading", { name: "Version history" })).toBeNull()
  })
})
