import AxeBuilder from "@axe-core/playwright"
import { expect, test, type Page } from "@playwright/test"

type WorkspaceMode =
  | "empty"
  | "empty-no-environments"
  | "ready"
  | "run-context"
  | "no-draft"
  | "no-environments"
  | "git-error"

async function installRuntime(page: Page, mode: WorkspaceMode) {
  await page.addInitScript((runtimeMode) => {
    const requests: Array<{ method: string; params: Record<string, unknown> }> = []
    const nativeWindowDragRequests: Array<{ command: string; payload: null }> = []
    const nativeWindowCommands: Array<{ command: string; payload: unknown }> = []
    let nativeTerminalVisible = false
    Object.assign(window, {
      __runtimeRequests: requests,
      __nativeWindowDragRequests: nativeWindowDragRequests,
      __nativeWindowCommands: nativeWindowCommands,
    })

    const hasAgent = !runtimeMode.startsWith("empty")
    const noActiveDraft = runtimeMode === "no-draft"
    const runContext = runtimeMode === "run-context"
    const agentId = "11111111-1111-4111-8111-111111111111"
    const draftOneId = "22222222-2222-4222-8222-222222222222"
    const draftTwoId = "33333333-3333-4333-8333-333333333333"
    const project = {
      id: "44444444-4444-4444-8444-444444444444",
      name: "Commerce Copilot — main",
      root: "/Users/test/code/customer-ai-commerce-copilot-main-22222222",
      trusted: true,
    }
    const secondProject = {
      ...project,
      id: "55555555-5555-4555-8555-555555555555",
      name: "Commerce Copilot — experiment",
      root: "/Users/test/code/customer-ai-commerce-copilot-experiment-33333333",
    }
    const binding = {
      id: "66666666-6666-4666-8666-666666666666",
      targetAgentId: agentId,
      projectId: project.id,
      name: "main",
      primaryRoot: project.root,
      additionalRoots: [],
      sourceRefLabel: `agent-factory/${agentId}/drafts/${draftOneId}`,
      archived: false,
      lastUsedAtUnixMs: 40,
    }
    const secondBinding = {
      ...binding,
      id: "77777777-7777-4777-8777-777777777777",
      projectId: secondProject.id,
      name: "experiment",
      primaryRoot: secondProject.root,
      sourceRefLabel: `agent-factory/${agentId}/drafts/${draftTwoId}`,
      lastUsedAtUnixMs: 30,
    }
    const version = {
      id: "88888888-8888-4888-8888-888888888888",
      targetAgentId: agentId,
      version: "0.1.0",
      name: "Commerce Copilot",
      objective: "Resolve commerce support requests",
      acceptanceCriteria: ["Refund requests are classified correctly"],
      sourceDraftId: "99999999-9999-4999-8999-999999999999",
      gitCommit: "abcdef0123456789",
      gitTag: `agent-factory/${agentId}/v0.1.0`,
      createdAtUnixMs: 20,
    }
    const latestVersion = {
      ...version,
      id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
      version: "0.2.0",
      gitCommit: "fedcba9876543210",
      gitTag: `agent-factory/${agentId}/v0.2.0`,
      createdAtUnixMs: 30,
    }
    const makeDraft = (
      id: string,
      workspaceBindingId: string,
      worktreePath: string,
      updatedAtUnixMs: number,
    ) => ({
      id,
      targetAgentId: agentId,
      workspaceBindingId,
      name: "Commerce Copilot",
      objective: "Resolve commerce support requests",
      acceptanceCriteria: [
        "Refund requests are classified correctly",
        "Escalations include the relevant policy",
      ],
      baseVersion: "0.1.0",
      branchRef: `agent-factory/${agentId}/drafts/${id}`,
      worktreePath,
      gitHead: "0123456789abcdef",
      lifecycle: "active",
      cleanupGuidance: null,
      createdAtUnixMs: 20,
      updatedAtUnixMs,
    })
    const drafts = noActiveDraft ? [] : [
      makeDraft(draftOneId, binding.id, project.root, 40),
      makeDraft(draftTwoId, secondBinding.id, secondProject.root, 30),
    ]
    const run = {
      id: "run-1",
      targetAgentId: agentId,
      agentDraftId: draftOneId,
      workspaceBindingId: binding.id,
      projectId: project.id,
      environmentId: "smoke-environment",
      objective: "Resolve commerce support requests",
      acceptanceCriteria: ["Refund requests are classified correctly"],
      startingGitHead: "0123456789abcdef",
      finalGitHead: null,
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
      evaluation: null,
      escalation: null,
      completedAtUnixMs: null,
      state: "evaluating",
    }
    const workItems = drafts.map((draft) => ({
      id: draft.id,
      kind: "agent_draft",
      targetAgentId: agentId,
      workspaceBindingId: draft.workspaceBindingId,
      projectId: draft.workspaceBindingId === binding.id ? project.id : secondProject.id,
      agentDraftId: draft.id,
      title: draft.name,
      status: "active",
      lastActivityAtUnixMs: draft.updatedAtUnixMs,
      projectLabel: draft.name,
      workspaceLabel: draft.name,
      sourceRefLabel: draft.branchRef,
    })).concat(runContext ? [{
      id: run.id,
      kind: "factory_run",
      targetAgentId: agentId,
      workspaceBindingId: binding.id,
      projectId: project.id,
      agentDraftId: draftOneId,
      title: "Evaluation Run",
      status: run.state,
      lastActivityAtUnixMs: 50,
      projectLabel: project.name,
      workspaceLabel: "main",
      sourceRefLabel: binding.sourceRefLabel,
    }] : [])
    const factoryRuns: Array<Record<string, unknown>> = runContext ? [run] : []
    const workContexts = hasAgent ? [{
      id: "context-draft",
      targetAgentId: agentId,
      workspaceBindingId: binding.id,
      agentDraftId: noActiveDraft ? null : draftOneId,
      workItemId: noActiveDraft ? null : runContext ? run.id : draftOneId,
      workItemKind: noActiveDraft
        ? null
        : runContext ? "factory_run" : "agent_draft",
      dock: "closed",
      dockPercent: 32,
      lastViewedAtUnixMs: 40,
    }] : []
    const snapshot = {
      revision: 1,
      settings: {
        theme: "system",
        nativeNotifications: true,
        layout: { inspectorPercent: 28, terminalPercent: 24 },
      },
      activeProjectId: hasAgent ? project.id : null,
      activeAgentSessionId: null,
      activeRunId: null,
      harnesses: [],
      projects: hasAgent ? [project, secondProject] : [],
      llmProviders: [],
      environments: runtimeMode.endsWith("no-environments") ? [] : [{
        id: "smoke-environment",
        name: "Smoke Environment",
        codingHarnessId: "claude",
        evaluationHarnessId: "claude",
        plugins: [],
        permissions: { trustedRead: "allow", trustedWrite: "ask", terminal: "ask" },
        registryIds: [],
        environmentVariables: [],
        llm: null,
        resolvedLlm: null,
        llmNeedsSetup: false,
        readiness: { state: "ready", issues: [] },
      }],
      agentSessions: runContext ? [{
        id: "orchestrator-session",
        targetAgentId: agentId,
        workspaceBindingId: binding.id,
        projectId: project.id,
        environmentId: "smoke-environment",
        harnessId: "claude",
        purpose: "orchestration",
        factoryRunId: run.id,
        parentSessionId: null,
        herdrAgentName: "orchestrator-agent",
        availability: "live",
        lifecycle: "idle",
        placement: {
          workspaceId: "workspace-1",
          tabId: "tab-1",
          paneId: "orchestrator-pane",
          agentName: "orchestrator-agent",
        },
        title: "Orchestrator",
        createdAtUnixMs: 0,
        lastActivityAtUnixMs: 3,
        attention: [],
        llmProviderSnapshot: null,
        effectiveModel: null,
        initialPrompt: "Coordinate the Run",
        briefDelivered: true,
        outcome: null,
      }, {
        id: "coding-session",
        targetAgentId: agentId,
        workspaceBindingId: binding.id,
        projectId: project.id,
        environmentId: "smoke-environment",
        harnessId: "claude",
        purpose: "coding",
        factoryRunId: run.id,
        parentSessionId: "orchestrator-session",
        herdrAgentName: "coding-agent",
        availability: "live",
        lifecycle: "done",
        placement: {
          workspaceId: "workspace-1",
          tabId: "tab-1",
          paneId: "coding-pane",
          agentName: "coding-agent",
        },
        title: "Coding",
        createdAtUnixMs: 1,
        lastActivityAtUnixMs: 2,
        attention: [],
        llmProviderSnapshot: null,
        effectiveModel: null,
        initialPrompt: "Implement the objective",
        briefDelivered: true,
        outcome: null,
      }, {
        id: "evaluation-session",
        targetAgentId: agentId,
        workspaceBindingId: binding.id,
        projectId: project.id,
        environmentId: "smoke-environment",
        harnessId: "claude",
        purpose: "evaluation",
        factoryRunId: run.id,
        parentSessionId: "orchestrator-session",
        herdrAgentName: "evaluation-agent",
        availability: "live",
        lifecycle: "working",
        placement: {
          workspaceId: "workspace-1",
          tabId: "tab-1",
          paneId: "evaluation-pane",
          agentName: "evaluation-agent",
        },
        title: "Evaluation",
        createdAtUnixMs: 2,
        lastActivityAtUnixMs: 3,
        attention: [],
        llmProviderSnapshot: null,
        effectiveModel: null,
        initialPrompt: "Evaluate the implementation",
        briefDelivered: true,
        outcome: null,
      }] : [],
      liveAgents: runContext ? [{
        workspaceBindingId: binding.id,
        managedSessionId: "orchestrator-session",
        factoryRunId: run.id,
        purpose: "orchestration",
        agentName: "orchestrator-agent",
        displayAgent: "Orchestrator",
        agentKind: "claude",
        lifecycle: "idle",
        attention: [],
        placement: {
          workspaceId: "workspace-1",
          tabId: "tab-1",
          paneId: "orchestrator-pane",
          agentName: "orchestrator-agent",
        },
        revision: 1,
        observedAtUnixMs: 3,
      }, {
        workspaceBindingId: binding.id,
        managedSessionId: "coding-session",
        factoryRunId: run.id,
        purpose: "coding",
        agentName: "coding-agent",
        displayAgent: "Coding",
        agentKind: "claude",
        lifecycle: "done",
        attention: [],
        placement: {
          workspaceId: "workspace-1",
          tabId: "tab-1",
          paneId: "coding-pane",
          agentName: "coding-agent",
        },
        revision: 1,
        observedAtUnixMs: 3,
      }, {
        workspaceBindingId: binding.id,
        managedSessionId: "evaluation-session",
        factoryRunId: run.id,
        purpose: "evaluation",
        agentName: "evaluation-agent",
        displayAgent: "Evaluation",
        agentKind: "claude",
        lifecycle: "working",
        attention: [],
        placement: {
          workspaceId: "workspace-1",
          tabId: "tab-1",
          paneId: "evaluation-pane",
          agentName: "evaluation-agent",
        },
        revision: 1,
        observedAtUnixMs: 3,
      }] : [],
      herdr: {
        connected: true,
        freshness: "live",
        observedAtUnixMs: 3,
        issues: [],
      },
      factoryRuns,
      targetWorkspace: {
        targetGroups: hasAgent ? [{
          targetAgent: {
            id: agentId,
            name: "Commerce Copilot",
            repositoryRoot: "/Users/test/code/customer-ai",
            archived: false,
            lastActivityAtUnixMs: 40,
          },
          drafts,
          versions: runtimeMode === "ready"
            ? [latestVersion, version]
            : [version],
          workspaceBindings: [binding, secondBinding],
          workItems,
        }] : [],
        workContexts,
        panes: hasAgent ? [{
          id: "pane-draft",
          workContextId: "context-draft",
          position: 0,
          widthBasisPoints: 10_000,
        }] : [],
        terminals: [],
        focusedPaneId: hasAgent ? "pane-draft" : null,
      },
    }

    Object.assign(window, {
      zero: {
        on: () => () => undefined,
        invoke: async (command: string, payload: unknown) => {
          if (command === "desktop.window.startDrag.v1") {
            nativeWindowDragRequests.push({ command, payload: payload as null })
            return { version: 1, windowId: 1 }
          }
          if (command === "native-sdk.dialog.openFile") {
            return ["/Users/test/code/picked-agent"]
          }
          if (
            command === "desktop.terminal.show.v1" ||
            command === "desktop.terminal.hide.v1"
          ) {
            nativeWindowCommands.push({ command, payload })
            nativeTerminalVisible = command === "desktop.terminal.show.v1"
            return { version: 1, visible: nativeTerminalVisible }
          }
          if (command === "native-sdk.window.list") {
            nativeWindowCommands.push({ command, payload })
            return [{
              id: 1,
              label: "main",
              open: true,
              hidden: false,
              x: 80,
              y: 40,
              width: 1440,
              height: 960,
            }]
          }
          if (
            command === "native-sdk.window.create" ||
            command === "native-sdk.window.focus" ||
            command === "native-sdk.window.close"
          ) {
            nativeWindowCommands.push({ command, payload })
            if (command === "native-sdk.window.focus") {
              throw new Error("window not open")
            }
            return {
              id: 2,
              label: (payload as { label?: string })?.label ?? "draft",
            }
          }
          const request = payload as {
            id: string
            method: string
            params: Record<string, unknown>
          }
          requests.push({ method: request.method, params: request.params })
          if (runtimeMode === "git-error" && request.method === "agentDraft.discard") {
            return {
              kind: "response",
              version: 1,
              id: request.id,
              error: {
                code: "conflict",
                message: "Draft contains local data and cannot be removed: ?? notes.txt",
              },
            }
          }
          if (request.method === "workspacePane.setDock") {
            const context = workContexts.find((candidate) =>
              candidate.id === request.params.workContextId)
            if (context) {
              context.dock = String(request.params.dock)
              snapshot.revision += 1
            }
          }
          if (request.method === "workspacePane.openPrimary") {
            const requestedRun = request.params.workItemKind === "factory_run"
              ? factoryRuns.find((candidate) =>
                  candidate.id === request.params.workItemId)
              : undefined
            const draftId = request.params.workItemKind === "agent_draft"
              ? String(request.params.workItemId)
              : requestedRun ? String(requestedRun.agentDraftId) : null
            const draft = drafts.find((candidate) => candidate.id === draftId)
            const workItemId = requestedRun
              ? String(requestedRun.id)
              : draftId
            const workItemKind = requestedRun ? "factory_run" :
              draftId ? "agent_draft" : null
            workContexts.splice(0, workContexts.length, {
              id: workItemId ? `context-${workItemId}` : "context-agent",
              targetAgentId: agentId,
              workspaceBindingId: draft?.workspaceBindingId ?? binding.id,
              agentDraftId: draftId,
              workItemId,
              workItemKind,
              dock: "closed",
              dockPercent: 32,
              lastViewedAtUnixMs: 50,
            })
            snapshot.targetWorkspace.panes[0]!.workContextId = workContexts[0]!.id
            snapshot.revision += 1
          }
          if (request.method === "agentDraft.update") {
            const draft = drafts.find((candidate) =>
              candidate.id === request.params.agentDraftId)
            if (draft) {
              draft.name = String(request.params.name)
              draft.objective = String(request.params.objective)
              draft.acceptanceCriteria = request.params.acceptanceCriteria as string[]
              snapshot.revision += 1
            }
          }
          if (request.method === "factoryRun.create") {
            factoryRuns.push({
              id: request.params.runId,
              targetAgentId: agentId,
              agentDraftId: request.params.agentDraftId,
              workspaceBindingId: binding.id,
              projectId: project.id,
              environmentId: request.params.environmentId,
              objective: drafts[0]?.objective,
              acceptanceCriteria: drafts[0]?.acceptanceCriteria,
              startingGitHead: drafts[0]?.gitHead,
              finalGitHead: null,
              changedFiles: [],
              testEvidence: [],
              evaluation: null,
              escalation: null,
              completedAtUnixMs: null,
              state: "draft",
            })
            snapshot.revision += 1
          }
          if (request.method === "run.cancel") {
            const run = factoryRuns.find((candidate) =>
              candidate.id === request.params.runId)
            if (run) {
              run.state = "cancelled"
              run.completedAtUnixMs = Date.now()
              snapshot.revision += 1
            }
          }

          const result = request.method === "runtime.hello"
            ? {
                protocolVersion: 1,
                runtimeName: "agent-factory-runtime",
                runtimeVersion: "0.1.0",
              }
            : request.method === "snapshot.get"
              ? snapshot
              : request.method === "harness.list"
                ? {
                    herdr: {
                      connected: true,
                      freshness: "live",
                      observedAtUnixMs: 3,
                      issues: [],
                    },
                    harnesses: [],
                  }
                : request.method === "update.status"
                  ? {
                      enabled: false,
                      configStatus: "disabled",
                      currentVersion: "0.1.0",
                      state: "idle",
                    }
                  : request.method === "secret.list"
                    ? { secrets: [] }
                    : request.method === "registry.list"
                      ? { registries: [] }
                      : request.method === "plugin.list"
                        ? { installed: [], localMcpServers: [] }
                        : request.method === "version.files.list"
                          ? {
                              versionId: request.params.versionId,
                              gitCommit: request.params.versionId === latestVersion.id
                                ? latestVersion.gitCommit
                                : version.gitCommit,
                              entries: [
                                { path: "README.md", kind: "file", size: 24 },
                                { path: "src/agent.ts", kind: "file", size: 32 },
                              ],
                            }
                        : request.method === "version.file.read"
                            ? {
                                versionId: request.params.versionId,
                                gitCommit: request.params.versionId === latestVersion.id
                                  ? latestVersion.gitCommit
                                  : version.gitCommit,
                                path: request.params.path,
                                size: 24,
                                kind: "text",
                              content: "Immutable Version content\n",
                            }
                            : request.method === "agentDraft.openWorkspace"
                              ? {
                                  agentDraftId: request.params.agentDraftId,
                                  workspaceId: "workspace-1",
                                  label: "Commerce Copilot / main",
                                  alreadyOpen: true,
                                  terminal: {
                                    executable: "/opt/homebrew/bin/herdr",
                                    arguments: ["--session", "smoke"],
                                  },
                                  revision: snapshot.revision,
                                }
                        : { revision: snapshot.revision }
          return { kind: "response", version: 1, id: request.id, result }
        },
      },
    })
  }, mode)
}

async function expectNoAxeViolations(page: Page, include?: string) {
  const builder = new AxeBuilder({ page })
  if (include) builder.include(include)
  const results = await builder.analyze()
  expect(results.violations).toEqual([])
}

test("saves the initial Agent Draft before Environment setup", async ({ page }) => {
  await installRuntime(page, "empty-no-environments")
  await page.goto("/")
  await expect(page.getByText("Create your first Agent")).toBeVisible()
  await expect(page.getByText("Create your first Environment")).toBeHidden()
  await page.getByRole("button", { name: "Create Agent" }).last().click()
  const editor = page.getByRole("region", { name: "Define your agent" })
  await expect(
    editor.getByRole("heading", { name: "Define your agent" }),
  ).toBeVisible()
  await expect(
    editor.getByText(
      "Save the initial draft, then configure an Environment before starting a Run.",
    ),
  ).toBeVisible()
  await expect(editor.locator('[data-slot="card"]')).toHaveCount(1)
  await expect(editor.getByRole("button", { name: "Close" })).toBeVisible()
  await expect(editor.getByRole("button", { name: "Cancel" })).toBeVisible()
  await expect(editor.getByRole("button", { name: "Discard" })).toHaveCount(0)
  await expect(editor.getByRole("button", { name: "Add criterion" })).toHaveCount(0)
  await expect(editor.getByRole("button", { name: "Create & Run" })).toHaveCount(0)
  await expect(editor.getByRole("button", { name: "Create" })).toHaveCount(0)
  await expect(editor.getByLabel("First Run Environment")).toHaveCount(0)
  await editor.getByLabel("Name", { exact: true }).fill("Commerce Copilot")
  await editor.getByLabel("Objective").fill("Resolve commerce support requests")
  await editor.getByRole("textbox", { name: "Success criterion 1", exact: true })
    .fill("Refund requests are classified correctly")
  await expect(
    editor.getByRole("textbox", { name: "Success criterion 2", exact: true }),
  ).toBeVisible()
  await editor.getByRole("button", { name: "Choose workspace folder" }).click()
  await editor.getByRole("button", { name: "Save" }).click()

  await expect.poll(() => requestFor(page, "targetAgent.create")).toMatchObject({
    objective: "Resolve commerce support requests",
    acceptanceCriteria: ["Refund requests are classified correctly"],
    repositoryRoot: "/Users/test/code/picked-agent",
    draftName: "main",
    trusted: true,
  })
  const request = await requestFor(page, "targetAgent.create")
  const serializedRequest = JSON.parse(JSON.stringify(request))
  expect(serializedRequest).not.toHaveProperty("startRun")
  expect(serializedRequest).not.toHaveProperty("environmentId")
})

test("sidebar lists user-named Drafts without Versions in a flat Agent folder", async ({ page }) => {
  await installRuntime(page, "ready")
  await page.goto("/")
  const sidebar = page.getByRole("navigation", { name: "Agents" })
  const toggle = sidebar.getByRole("button", {
    name: "Commerce Copilot",
  })
  await expect(toggle.getByText("Commerce Copilot")).toBeVisible()
  await expect(toggle.locator("xpath=..").locator(":scope > button"))
    .toHaveCount(1)
  await expect(toggle).not.toHaveAttribute("data-active", "true")
  await expect(toggle.locator('[data-folder-state="open"]')).toBeVisible()
  await expect(sidebar.locator('[data-sidebar-entry]')).toHaveText([
    "main",
    "experiment",
  ])
  await expect(sidebar.locator('[data-sidebar-entry="version"]')).toHaveCount(0)
  const agentLeft = await toggle.evaluate((element) =>
    element.getBoundingClientRect().left)
  for (const entry of await sidebar.locator('[data-sidebar-entry]').all()) {
    await expect.poll(() => entry.evaluate((element) =>
      element.getBoundingClientRect().left)).toBe(agentLeft)
  }
  await expect(sidebar.locator(
    '[data-sidebar-entry] [data-slot="sidebar-menu-button"] svg',
  )).toHaveCount(0)
  await expect(sidebar.getByText("Overview")).toHaveCount(0)
  const openPrimaryCount = (await requestMethods(page))
    .filter((method) => method === "workspacePane.openPrimary").length
  await toggle.click()
  await expect.poll(async () => (await requestMethods(page))
    .filter((method) => method === "workspacePane.openPrimary").length)
    .toBe(openPrimaryCount)
  await expect(toggle.locator('[data-folder-state="closed"]')).toBeVisible()
  await expect(sidebar.locator('[data-sidebar-entry]')).toHaveCount(0)
  await expectNoAxeViolations(page, 'nav[aria-label="Agents"]')
})

test("aligns Draft names with the Agent name axis", async ({ page }) => {
  await installRuntime(page, "ready")
  await page.goto("/")
  const sidebar = page.getByRole("navigation", { name: "Agents" })
  const agent = sidebar.getByRole("button", { name: "Commerce Copilot" })
  const agentNameLeft = await agent.getByText("Commerce Copilot").evaluate(
    (element) => element.getBoundingClientRect().left,
  )

  for (const draftName of ["main", "experiment"]) {
    await expect.poll(() =>
      sidebar.getByRole("button", { name: draftName }).getByText(draftName)
        .evaluate((element) => element.getBoundingClientRect().left),
    ).toBe(agentNameLeft)
  }
})

test("dedicated Draft window renders the Draft without a presentation error", async ({ page }) => {
  await installRuntime(page, "ready")
  await page.goto(
    "/?draftWindow=1&draftId=22222222-2222-4222-8222-222222222222&targetAgentId=11111111-1111-4111-8111-111111111111&workspaceBindingId=66666666-6666-4666-8666-666666666666&title=Commerce%20Copilot",
  )
  await expect(page.getByText("Agent Factory could not render")).toHaveCount(0)
  await expect(page.getByRole("navigation", { name: "Agents" })).toHaveCount(0)
  await expect(page.getByRole("region", { name: "Commerce Copilot Draft" }))
    .toBeVisible()
})

test("Draft context menu opens the Draft in a new application window", async ({ page }) => {
  await installRuntime(page, "ready")
  await page.goto("/")
  const sidebar = page.getByRole("navigation", { name: "Agents" })
  const draftEntry = sidebar.locator('[data-sidebar-entry="draft"]').first()
  await draftEntry.getByRole("button").click({ button: "right" })
  await page.getByRole("menuitem", { name: "Open in new window" }).click()
  await expect.poll(async () => {
    const commands = await nativeWindowCommands(page)
    return commands.find((entry) => entry.command === "native-sdk.window.create")
  }).toMatchObject({
    command: "native-sdk.window.create",
    payload: {
      label: "draft-22222222-2222-4222-8222-222222222222",
      titlebar: "hidden_inset",
      restoreState: false,
    },
  })
  const create = (await nativeWindowCommands(page)).find(
    (entry) => entry.command === "native-sdk.window.create",
  )
  const payload = create?.payload as { url?: string; title?: string }
  expect(payload.url).toContain("draftWindow=1")
  expect(payload.url).toContain("draftId=22222222-2222-4222-8222-222222222222")
  expect(payload.title).toContain("Commerce Copilot")
})

test("title-bar Version selector opens a horizontally stacked read-only inspector", async ({ page }) => {
  await installRuntime(page, "ready")
  await page.goto("/")
  const sidebar = page.getByRole("navigation", { name: "Agents" })
  const draftEntry = sidebar.locator('[data-sidebar-entry="draft"]')
    .first()
    .getByRole("button")
  const pane = page.getByRole("region", { name: "Commerce Copilot pane" })
  const titleBar = pane.locator("header").first()
  const draft = page.getByRole("region", { name: "Commerce Copilot Draft" })
  await page.evaluate(() => {
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: (value: string) => {
          window.localStorage.setItem("test-clipboard", value)
          return Promise.resolve()
        },
      },
    })
  })

  await expect(draftEntry).toHaveAttribute("data-active")
  // Draft badge after the agent name; Version is navigation-only on the right.
  await expect(titleBar.getByText("Draft", { exact: true })).toBeVisible()
  const versionTrigger = titleBar.getByRole("combobox", {
    name: "Open version selector",
  })
  await expect(versionTrigger).toHaveText("Version")
  await expect(versionTrigger).not.toContainText("v0.")
  await versionTrigger.click()
  const versionPicker = page.getByRole("dialog", { name: "Select version" })
  await expect(versionPicker).toBeVisible()
  await expect(versionPicker.getByRole("combobox", { name: "Search versions" }))
    .toBeVisible()
  await versionPicker.getByRole("option", { name: /v0\.2\.0/ }).click()

  await expect.poll(() => requestFor(page, "version.files.list")).toEqual({
    versionId: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
  })
  const inspector = page.getByRole("region", {
    name: "Commerce Copilot v0.2.0 Version inspector",
  })
  const header = inspector.locator("header")
  await expect(header.getByRole("heading", {
    name: "Commerce Copilot v0.2.0",
  })).toBeVisible()
  await expect(header.locator("code")).toHaveText("fedcba987654")
  await expect(header).not.toContainText("Files")
  // Selecting a Version opens the Inspector without leaving the Draft view.
  await expect(draftEntry).toHaveAttribute("data-active")
  await expect(draft.getByRole("button", { name: "Start Run" })).toBeVisible()
  await expect(titleBar.getByText("Draft", { exact: true })).toBeVisible()
  const [headingBox, badgeBox] = await Promise.all([
    header.getByRole("heading").boundingBox(),
    header.getByText("Read-only", { exact: true }).boundingBox(),
  ])
  expect(headingBox).not.toBeNull()
  expect(badgeBox).not.toBeNull()
  expect(badgeBox?.x).toBeGreaterThan(
    (headingBox?.x ?? 0) + (headingBox?.width ?? 0),
  )
  await expect(inspector.locator('[data-slot="separator"]').first())
    .toBeVisible()
  await header.getByRole("button", { name: "Copy full Git commit" }).click()
  await expect.poll(() => page.evaluate(() =>
    window.localStorage.getItem("test-clipboard")))
    .toBe("fedcba9876543210")
  await expect(page.getByRole("separator", {
    name: "Resize workspace and Inspector",
  })).toHaveCount(0)
  await expect(page.getByRole("separator", {
    name: "Resize Draft and context",
  })).toBeVisible()
  await expect(page.getByRole("dialog", {
    name: "Commerce Copilot v0.2.0 Version inspector",
  })).toHaveCount(0)
  const [draftBox, inspectorBox] = await Promise.all([
    draft.boundingBox(),
    inspector.boundingBox(),
  ])
  expect(draftBox).not.toBeNull()
  expect(inspectorBox).not.toBeNull()
  expect(inspectorBox?.x).toBeGreaterThanOrEqual((draftBox?.x ?? 0) - 1)
  expect((inspectorBox?.x ?? 0) + (inspectorBox?.width ?? 0))
    .toBeLessThanOrEqual((draftBox?.x ?? 0) + (draftBox?.width ?? 0) + 1)
  await inspector.getByRole("treeitem", { name: "README.md" }).click()
  await expect.poll(() => requestFor(page, "version.file.read")).toEqual({
    versionId: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
    path: "README.md",
  })
  await expect(inspector.getByText("Immutable Version content")).toBeVisible()
  await expect(inspector.locator('[contenteditable="true"]')).toHaveCount(0)
  await header.getByRole("button", { name: "Create Draft" }).click()
  const createDraft = page.getByRole("dialog", {
    name: "Create Draft from v0.2.0",
  })
  await expect(createDraft).toBeVisible()
  await createDraft.getByRole("button", { name: "Close" }).first().click()
  await expectNoAxeViolations(
    page,
    '[aria-label="Commerce Copilot v0.2.0 Version inspector"]',
  )
  await expect(draftEntry).toHaveAttribute("data-active")
  await expect(draft).toBeVisible()
  await expect(draft.getByRole("button", { name: "Start Run" })).toBeVisible()
})

test("title-bar Terminal action toggles the single native Herdr terminal", async ({ page }) => {
  await installRuntime(page, "ready")
  await page.goto("/")
  const pane = page.getByRole("region", { name: "Commerce Copilot pane" })
  const titleBar = pane.locator("header").first()

  await titleBar.getByRole("button", { name: "Open Terminal" }).click()
  await expect.poll(() => requestFor(page, "agentDraft.openWorkspace")).toEqual({
    agentDraftId: "22222222-2222-4222-8222-222222222222",
  })
  await expect.poll(async () => (await nativeWindowCommands(page)).find(
    (entry) => entry.command === "desktop.terminal.show.v1",
  )).toMatchObject({
    payload: {
      workspaceId: "workspace-1",
      label: "Commerce Copilot / main",
    },
  })
  await expect(titleBar.getByRole("button", { name: "Close Terminal" }))
    .toBeVisible()
  await expect(page.getByRole("region", { name: "Terminal" })).toHaveCount(0)

  await titleBar.getByRole("button", { name: "Close Terminal" }).click()
  await expect.poll(async () => (await nativeWindowCommands(page)).filter(
    (entry) => entry.command === "desktop.terminal.hide.v1",
  )).toHaveLength(1)
  await expect(titleBar.getByRole("button", { name: "Open Terminal" }))
    .toBeVisible()
})

test("Start opens the terminal and Cancel resets the Draft immediately", async ({ page }) => {
  await installRuntime(page, "ready")
  await page.goto("/")
  const draft = page.getByRole("region", { name: "Commerce Copilot Draft" })

  await draft.getByRole("button", { name: "Start Run" }).click()
  await expect.poll(() => requestFor(page, "factoryRun.create")).toMatchObject({
    agentDraftId: "22222222-2222-4222-8222-222222222222",
    environmentId: "smoke-environment",
  })
  await expect.poll(() => requestFor(page, "agentDraft.openWorkspace"))
    .toEqual({ agentDraftId: "22222222-2222-4222-8222-222222222222" })
  await expect(draft.getByRole("button", { name: "Cancel Run" })).toBeVisible()

  await draft.getByRole("button", { name: "Cancel Run" }).click()
  const createdRun = await requestFor(page, "factoryRun.create")
  await expect.poll(() => requestFor(page, "run.cancel")).toEqual({
    runId: (createdRun as { runId?: string }).runId,
  })
  await expect.poll(async () => (await nativeWindowCommands(page)).filter(
    (entry) => entry.command === "desktop.terminal.hide.v1",
  )).toHaveLength(1)
  await expect(draft.getByRole("button", { name: "Start Run" })).toBeVisible()
  await expect(draft.getByRole("button", { name: "Cancel Run" })).toHaveCount(0)
})

test("Version inspector creates a Draft from the selected Version", async ({ page }) => {
  await installRuntime(page, "ready")
  await page.goto("/")
  const pane = page.getByRole("region", { name: "Commerce Copilot pane" })
  const titleBar = pane.locator("header").first()
  await titleBar.getByRole("combobox", { name: "Open version selector" }).click()
  await page.getByRole("dialog", { name: "Select version" })
    .getByRole("option", { name: /v0\.2\.0/ })
    .click()
  const inspector = page.getByRole("region", {
    name: "Commerce Copilot v0.2.0 Version inspector",
  })
  await inspector.getByRole("button", { name: "Create Draft" }).click()

  const dialog = page.getByRole("dialog", {
    name: "Create Draft from v0.2.0",
  })
  await expect(dialog).toBeVisible()
  await dialog.getByRole("button", { name: "Create Draft" }).click()
  await expect.poll(() => requestFor(page, "agentDraft.create")).toEqual({
    targetAgentId: "11111111-1111-4111-8111-111111111111",
    baseVersionId: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
    draftName: "v0.2.0 changes",
  })
})

test("edits the Draft and starts a Run isolated to its Draft ID", async ({ page }) => {
  await installRuntime(page, "ready")
  await page.goto("/")
  const pane = page.getByRole("region", { name: "Commerce Copilot pane" })
  const draft = page.getByRole("region", { name: "Commerce Copilot Draft" })
  // Workflow menu lives in the pane title bar after Version, not the overview.
  await pane.locator("header").first().getByRole("button", {
    name: "Actions for Commerce Copilot",
  }).click()
  await page.getByRole("menuitem", { name: "Edit" }).click()
  const editor = page.getByRole("region", { name: "Define your agent" })
  await editor.getByLabel("Objective").fill("Resolve every commerce question")
  await editor.getByRole("button", { name: "Save" }).click()
  await expect.poll(() => requestFor(page, "agentDraft.update")).toMatchObject({
    agentDraftId: "22222222-2222-4222-8222-222222222222",
    objective: "Resolve every commerce question",
  })
  await draft.getByRole("button", { name: "Start Run" }).click()
  await expect.poll(() => requestFor(page, "factoryRun.create")).toMatchObject({
    agentDraftId: "22222222-2222-4222-8222-222222222222",
    environmentId: "smoke-environment",
  })
  await expect.poll(() => requestMethods(page)).not.toContain("run.startCoding")
})

test("Draft Overview switches between inline and popover modes", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 900 })
  await installRuntime(page, "ready")
  await page.goto("/")
  const pane = page.getByRole("region", { name: "Commerce Copilot pane" })
  const titleBar = pane.locator("header").first()
  const draft = page.getByRole("region", { name: "Commerce Copilot Draft" })
  const overviewToggle = titleBar.getByRole("button", {
    name: "Show Draft Overview",
  })

  await expect(titleBar.getByText("Draft", { exact: true })).toBeVisible()
  const versionTrigger = titleBar.getByRole("combobox", {
    name: "Open version selector",
  })
  await expect(versionTrigger).toHaveText("Version")
  await expect(versionTrigger).not.toContainText("v0.")
  await expect(titleBar.getByRole("button", {
    name: "Actions for Commerce Copilot",
  })).toBeVisible()
  await expect(titleBar.getByText("Active", { exact: true })).toHaveCount(0)
  await expect(titleBar.getByLabel(/^Branch /)).toHaveCount(0)
  await expect(titleBar.getByLabel(/^Project /)).toHaveCount(0)
  await expect(titleBar.getByLabel(/^Agent /)).toHaveCount(0)
  await expect(overviewToggle).toHaveAttribute("aria-expanded", "false")
  await expect(page.getByRole("complementary", { name: "Draft Overview" }))
    .toHaveCount(0)
  await expect(draft.getByRole("heading", { name: "Herdr agents" })).toBeVisible()
  await expect(draft.getByRole("heading", { name: "Session History" }))
    .toBeVisible()
  await expect(draft.getByText("No session history yet")).toBeVisible()
  await expect(draft.getByRole("heading", { name: "Version history" }))
    .toHaveCount(0)
  const runHistory = draft.getByRole("heading", { name: "Session History" })
    .locator("xpath=ancestor::section[1]")
  const [closedDraftBox, closedRunHistoryBox] = await Promise.all([
    draft.boundingBox(),
    runHistory.boundingBox(),
  ])
  await overviewToggle.click()
  const overview = page.getByRole("complementary", { name: "Draft Overview" })
  await expect(overview).toBeVisible()
  await expect(overview.getByText("Smoke Environment")).toBeVisible()
  await expect(overview.getByText("Environment", { exact: true })).toHaveCount(0)
  const [wideOverviewBox, openDraftBox, openRunHistoryBox] =
    await Promise.all([
      overview.boundingBox(),
      draft.boundingBox(),
      runHistory.boundingBox(),
    ])
  expect(wideOverviewBox?.width).toBe(320)
  expect(await overview.evaluate((element) => element.tagName)).toBe("ASIDE")
  expect(openDraftBox?.width).toBeLessThan(closedDraftBox?.width ?? 0)
  expect(openRunHistoryBox?.width).toBeLessThan(
    closedRunHistoryBox?.width ?? 0,
  )
  await expectNoAxeViolations(page, '[aria-label="Draft Overview"]')

  await page.setViewportSize({ width: 520, height: 780 })
  await expect(page.getByRole("complementary", { name: "Draft Overview" }))
    .toHaveCount(0)
  await expect(titleBar.getByRole("button", { name: "Show Draft Overview" }))
    .toHaveAttribute("aria-expanded", "false")
  const narrowRunHistoryBefore = await runHistory.boundingBox()
  await titleBar.getByRole("button", { name: "Show Draft Overview" }).click()
  const narrowOverview = page.getByRole("complementary", {
    name: "Draft Overview",
  })
  await expect(narrowOverview).toBeVisible()
  const [narrowOverviewBox, narrowRunHistoryAfter] = await Promise.all([
    narrowOverview.boundingBox(),
    runHistory.boundingBox(),
  ])
  expect(narrowOverviewBox?.width).toBe(320)
  expect(await narrowOverview.evaluate((element) => element.tagName))
    .not.toBe("ASIDE")
  expect(narrowRunHistoryAfter).toEqual(narrowRunHistoryBefore)
  await titleBar.getByRole("button", { name: "Hide Draft Overview" }).click()
  await expect(page.getByRole("complementary", { name: "Draft Overview" }))
    .toHaveCount(0)
})

test("Run context resolves its Draft into the same layout at wide and narrow sizes", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 900 })
  await installRuntime(page, "run-context")
  await page.goto("/")
  const draft = page.getByRole("region", { name: "Commerce Copilot Draft" })
  await expect(draft.getByRole("heading", { name: "Herdr agents" })).toBeVisible()
  const managedAgents = draft.getByRole("region", {
    name: "Selected Run managed agents",
  })
  await expect(managedAgents.getByText("Coding Agent", { exact: true }))
    .toBeVisible()
  await expect(managedAgents.getByText("Evaluation Agent", { exact: true }))
    .toBeVisible()
  await expect(managedAgents.getByText("Orchestrator", { exact: true }))
    .toBeVisible()
  await page.setViewportSize({ width: 520, height: 780 })
  await page.getByRole("button", { name: "Show Draft Overview" }).click()
  const overview = page.getByRole("complementary", { name: "Draft Overview" })
  await expect(overview.getByText("Frozen for this Run")).toBeVisible()
  await expect(overview.getByText("1 files")).toBeVisible()
  await expect(overview.getByRole("heading", { name: "Commerce Copilot" }))
    .toBeVisible()
  await expect(overview.getByLabel("Agent name")).toHaveCount(0)
})

test("Code changes opens a horizontally stacked read-only inspector", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 900 })
  await installRuntime(page, "run-context")
  await page.goto("/")
  const draft = page.getByRole("region", { name: "Commerce Copilot Draft" })

  await page.getByRole("button", { name: "Show Draft Overview" }).click()
  await page.getByRole("complementary", { name: "Draft Overview" })
    .getByRole("button", { name: "Inspect Code changes" }).click()

  const inspector = page.getByRole("region", {
    name: "Code changes inspector",
  })
  await expect(inspector).toBeVisible()
  await expect(inspector.getByText("src/agent.ts").first()).toBeVisible()
  await expect(inspector.getByText("old")).toBeVisible()
  await expect(inspector.getByText("new")).toBeVisible()
  await expect(page.getByRole("separator", {
    name: "Resize workspace and Inspector",
  })).toBeVisible()
  const [draftBox, inspectorBox] = await Promise.all([
    draft.boundingBox(),
    inspector.boundingBox(),
  ])
  expect(draftBox).not.toBeNull()
  expect(inspectorBox).not.toBeNull()
  expect(inspectorBox?.x).toBeGreaterThanOrEqual(
    (draftBox?.x ?? 0) + (draftBox?.width ?? 0) - 1,
  )
  await expectNoAxeViolations(page, '[aria-label="Code changes inspector"]')
})

test("Create Version warns without passing evidence and publishes the Draft", async ({ page }) => {
  await installRuntime(page, "ready")
  await page.goto("/")
  await page.getByRole("button", {
    name: "Actions for Commerce Copilot",
  }).click()
  await page.getByRole("menuitem", { name: "Create Version" }).click()
  const dialog = page.getByRole("dialog", { name: "Create immutable Version" })
  await expect(dialog.getByText("No passing Run")).toBeVisible()
  await dialog.getByRole("button", { name: "Create Version" }).click()
  await expect.poll(() => requestFor(page, "agentDraft.publish")).toEqual({
    agentDraftId: "22222222-2222-4222-8222-222222222222",
    bump: "patch",
    confirmWithoutPassingRun: true,
  })
})

test("Draft overflow menu does not offer Delete or Remove", async ({ page }) => {
  await installRuntime(page, "ready")
  await page.goto("/")
  await page.getByRole("button", {
    name: "Actions for Commerce Copilot",
  }).click()
  await expect(page.getByRole("menuitem", { name: "Delete" })).toHaveCount(0)
  await expect(page.getByRole("menuitem", { name: "Remove" })).toHaveCount(0)
})

test("an Agent with no active Draft offers Versions in the title bar", async ({ page }) => {
  await installRuntime(page, "no-draft")
  await page.goto("/")
  await expect(page.getByRole("complementary", { name: "Draft Overview" }))
    .toHaveCount(0)
  await page.getByRole("button", { name: "Show Draft Overview" }).click()
  const overview = page.getByRole("complementary", { name: "Draft Overview" })
  await expect(overview.getByRole("list", { name: "Versions" })).toBeVisible()
  await expect(overview.getByText("v0.1.0")).toBeVisible()
  await expect(overview.getByText("No versions yet")).toHaveCount(0)
  await expect(page.getByRole("heading", { name: "Version history" }))
    .toHaveCount(0)
  const sidebar = page.getByRole("navigation", { name: "Agents" })
  await expect(sidebar.locator('[data-sidebar-entry="version"]')).toHaveCount(0)
  const pane = page.getByRole("region", { name: /pane$/ })
  const titleBar = pane.locator("header").first()
  await titleBar.getByRole("combobox", { name: "Open version selector" }).click()
  const versionPicker = page.getByRole("dialog", { name: "Select version" })
  await expect(versionPicker.getByRole("option", { name: /v0\.1\.0/ }))
    .toBeVisible()
  await versionPicker.getByRole("option", { name: /v0\.1\.0/ }).click()
  const draft = page.getByRole("region", { name: /Draft$/ })
  const inspector = draft.getByRole("region", {
    name: /v0\.1\.0 Version inspector/,
  })
  await expect(inspector).toBeVisible()
  await expect(page.getByRole("separator", {
    name: "Resize workspace and Inspector",
  })).toHaveCount(0)
  await expectNoAxeViolations(page)
})

test("native drag regions preserve sidebar controls", async ({ page }) => {
  await installRuntime(page, "ready")
  await page.goto("/")
  const dragHeader = page.locator('[data-slot="sidebar-header"][data-native-drag-region]')
  const box = await dragHeader.boundingBox()
  expect(box).not.toBeNull()
  await page.mouse.move(box!.x + box!.width - 20, box!.y + box!.height / 2)
  await page.mouse.down()
  await page.mouse.up()
  await expect.poll(() => nativeWindowDragCount(page)).toBe(1)
  const hideSidebar = page.getByRole("button", { name: "Hide sidebar" })
  await hideSidebar.focus()
  await hideSidebar.press("Enter")
  await expect(page.getByRole("button", { name: "Show sidebar" })).toBeVisible()
})

test("first run nudges into Environment creation", async ({ page }) => {
  await installRuntime(page, "no-environments")
  await page.goto("/")
  await expect(page.getByText("Create your first Environment")).toBeVisible()
  await expect(page.getByText("Create your first Agent")).toBeHidden()
})

test("Settings details open from the row and come back from Back", async ({
  page,
}) => {
  await installRuntime(page, "ready")
  await page.goto("/")
  await page.getByRole("button", { name: "Settings" }).click()
  await page.getByRole("button", { name: "Environments" }).click()

  // The row is the way in: no separate Manage control to aim for.
  const row = page.getByRole("button", { name: "Open Smoke Environment" })
  await expect(row).toBeVisible()
  await expectNoAxeViolations(page)
  await row.click()
  await expect(
    page.getByRole("button", { name: "Back to Environments" }),
  ).toBeVisible()
  await expectNoAxeViolations(page)

  await page.getByRole("button", { name: "Back to Environments" }).click()
  await expect(row).toBeVisible()

  // And the same row opens from the keyboard.
  await row.focus()
  await page.keyboard.press("Enter")
  await expect(
    page.getByRole("navigation", { name: "Breadcrumb" }),
  ).toContainText("Smoke Environment")
  await page
    .getByRole("navigation", { name: "Breadcrumb" })
    .getByRole("button", { name: "Environments" })
    .click()
  await expect(row).toBeVisible()
})

async function requestMethods(page: Page) {
  return page.evaluate(() => (
    window as typeof window & { __runtimeRequests: Array<{ method: string }> }
  ).__runtimeRequests.map((request) => request.method))
}

async function requestFor(page: Page, method: string) {
  return page.evaluate((requestedMethod) => (
    window as typeof window & {
      __runtimeRequests: Array<{ method: string; params: unknown }>
    }
  ).__runtimeRequests.find((request) => request.method === requestedMethod)?.params, method)
}

async function nativeWindowDragCount(page: Page) {
  return page.evaluate(() => (
    window as typeof window & { __nativeWindowDragRequests: unknown[] }
  ).__nativeWindowDragRequests.length)
}

async function nativeWindowCommands(page: Page) {
  return page.evaluate(() => (
    window as typeof window & {
      __nativeWindowCommands: Array<{ command: string; payload: unknown }>
    }
  ).__nativeWindowCommands)
}
