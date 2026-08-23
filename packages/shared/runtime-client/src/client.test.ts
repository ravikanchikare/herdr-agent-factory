import { afterEach, describe, expect, it, vi } from "vitest"

import { BrowserRuntimeClient } from "./client"
import type {
  FactoryRunDto,
  HarnessListDto,
  NativeSdkBridge,
  RuntimeRequest,
  WorkspaceSettingsProjection,
} from "./contracts"

describe("BrowserRuntimeClient", () => {
  afterEach(() => {
    vi.useRealTimers()
    vi.unstubAllGlobals()
  })

  it("publishes the bootstrap projection", async () => {
    const invoke = vi.fn()
    invoke.mockImplementation((_: string, request: RuntimeRequest) => ({
      kind: "response",
      version: 1,
      id: request.id,
      result:
        request.method === "runtime.hello"
          ? {
              protocolVersion: 1,
              runtimeName: "agent-factory-runtime",
              runtimeVersion: "0.1.0",
            }
          : request.method === "snapshot.get"
            ? {
                revision: 1,
                settings: {
                  theme: "dark",
                  nativeNotifications: false,
                  layout: { inspectorPercent: 28, terminalPercent: 24 },
                },
                activeProjectId: null,
                activeAgentSessionId: null,
                activeRunId: null,
                projects: [],
                environments: [],
                agentSessions: [],
                liveAgents: [],
                harnesses: [],
                herdr: { connected: true, freshness: "live", issues: [] },
                factoryRuns: [],
                targetWorkspace: {
                  targetGroups: [],
                  workContexts: [],
                  panes: [],
                  terminals: [],
                },
              }
            : request.method === "secret.list"
              ? {
                  secrets: [
                    {
                      secretRef: "secret_launch",
                      label: "Launch key",
                      kind: "api_token",
                      referencedBy: [],
                      createdAtUnixMs: 1,
                      updatedAtUnixMs: 1,
                    },
                  ],
                }
              : { herdr: { connected: true, freshness: "live", issues: [] }, harnesses: [] },
    }))
    const bridge: NativeSdkBridge = {
      invoke: invoke as NativeSdkBridge["invoke"],
      on: vi.fn(() => () => undefined),
    }
    vi.stubGlobal("window", { zero: bridge })
    vi.stubGlobal("crypto", { randomUUID: () => "request" })
    const client = new BrowserRuntimeClient()
    const listener = vi.fn()
    client.subscribe(listener)

    await client.connect()

    expect(client.getSnapshot().connection).toBe("ready")
    expect(client.getSnapshot().revision).toBe(1)
    expect(client.getSnapshot().settings).toEqual({
      theme: "dark",
      nativeNotifications: false,
      layout: { inspectorPercent: 28, terminalPercent: 24 },
    })
    expect(client.getSnapshot().secrets).toEqual([
      expect.objectContaining({
        secretRef: "secret_launch",
        label: "Launch key",
      }),
    ])
    expect(
      invoke.mock.calls.some(
        ([, request]) => request.method === "secret.list",
      ),
    ).toBe(true)
    expect(listener).toHaveBeenCalled()
  })

  it("projects durable session identity joined with live Herdr state", async () => {
    const snapshot = {
      ...emptyRuntimeSnapshot({
        theme: "system" as const,
        nativeNotifications: true,
        layout: { inspectorPercent: 28, terminalPercent: 24 },
      }),
      harnesses: [
        {
          id: "claude",
          name: "Claude Code",
          readiness: "ready" as const,
          guidance: "Ready to launch with Herdr.",
          action: null,
        },
      ],
      agentSessions: [
        {
          id: "session-1",
          targetAgentId: "target-1",
          workspaceBindingId: "binding-1",
          projectId: "project-1",
          environmentId: "default",
          harnessId: "claude",
          purpose: "coding" as const,
          factoryRunId: null,
          parentSessionId: null,
          herdrAgentName: "coding-1",
          availability: "live" as const,
          lifecycle: "blocked" as const,
          placement: {
            workspaceId: "w1",
            tabId: "w1:t1",
            paneId: "w1:p2",
            agentName: "coding-1",
          },
          title: "Improve refunds",
          createdAtUnixMs: 1,
          lastActivityAtUnixMs: 3,
          attention: ["Approve the edit?"],
          llmProviderSnapshot: null,
          effectiveModel: null,
          initialPrompt: "Improve refunds",
          briefDelivered: true,
          outcome: null,
        },
      ],
      liveAgents: [],
    }
    const invoke = vi.fn()
    invoke.mockImplementation((_: string, request: RuntimeRequest) => ({
      kind: "response",
      version: 1,
      id: request.id,
      result:
        request.method === "runtime.hello"
          ? {
              protocolVersion: 1,
              runtimeName: "agent-factory-runtime",
              runtimeVersion: "0.1.0",
            }
          : request.method === "snapshot.get"
            ? snapshot
            : {
                herdr: { connected: true, freshness: "live", issues: [] },
                harnesses: [
                  {
                    id: "claude",
                    name: "Claude Code",
                    readiness: "ready",
                    guidance: "Ready to launch with Herdr.",
                    action: null,
                  },
                ],
              },
    }))
    const bridge: NativeSdkBridge = {
      invoke: invoke as NativeSdkBridge["invoke"],
      on: vi.fn(() => () => undefined),
    }
    vi.stubGlobal("window", { zero: bridge })
    vi.stubGlobal("crypto", { randomUUID: () => "request" })
    const client = new BrowserRuntimeClient()
    await client.connect()

    const projected = client.getSnapshot()
    // The runtime joins this live state from Herdr. The browser projects it
    // directly and has no persisted lifecycle/event thread to replay.
    expect(projected.sessions[0]).toEqual(
      expect.objectContaining({
        id: "session-1",
        lifecycle: "blocked",
        harnessId: "claude",
        paneId: "w1:p2",
        agentName: "coding-1",
        attention: ["Approve the edit?"],
      }),
    )
    expect(projected.harnesses[0]?.id).toBe("claude")
    expect(projected.herdr.connected).toBe(true)
  })


  it("reports the missing native bridge as degraded", async () => {
    vi.stubGlobal("window", {})
    const client = new BrowserRuntimeClient()

    await client.connect()

    expect(client.getSnapshot().connection).toBe("degraded")
  })

  it("preserves and closes independent terminal sessions", async () => {
    let createCount = 0
    const workContextId = "work-context-1"
    const liveTerminals: string[] = []
    const project = {
      id: "project-1",
      name: "Refund agent",
      path: "/Users/test/code/refund-agent",
      trusted: true,
    }
    const invoke = vi.fn(async (_: string, request: RuntimeRequest) => {
      let result: unknown
      if (request.method === "runtime.hello") {
        result = {
          protocolVersion: 1,
          runtimeName: "agent-factory-runtime",
          runtimeVersion: "0.1.0",
        }
      } else if (request.method === "snapshot.get") {
        const base = emptyRuntimeSnapshot({
          theme: "system",
          nativeNotifications: true,
          layout: { inspectorPercent: 28, terminalPercent: 24 },
        })
        result = {
          ...base,
          activeProjectId: project.id,
          projects: [project],
          targetWorkspace: {
            ...base.targetWorkspace,
            terminals: liveTerminals.map((id, index) => ({
              createdAtUnixMs: index,
              id,
              state: "running" as const,
              title: `Terminal ${index + 1}`,
              workContextId,
              workspaceBindingId: "binding-1",
            })),
          },
        }
      } else if (request.method === "harness.list") {
        result = { herdr: { connected: true, freshness: "live", issues: [] }, harnesses: [] }
      } else if (request.method === "workspaceTerminal.create") {
        createCount += 1
        liveTerminals.push(`terminal-${createCount}`)
        result = {
          terminalId: `terminal-${createCount}`,
          processId: createCount,
          cols: 80,
          rows: 24,
        }
      } else if (request.method === "workspaceTerminal.read") {
        result = {
          terminalId: "terminal-1",
          dataBase64: btoa("exit\r\n"),
          startCursor: 0,
          nextCursor: 6,
          truncated: false,
          readerClosed: true,
          exitStatus: { code: 0, signal: null },
        }
      } else {
        if (request.method === "workspaceTerminal.close") {
          const closed = (request.params as { terminalId: string }).terminalId
          const index = liveTerminals.indexOf(closed)
          if (index >= 0) liveTerminals.splice(index, 1)
        }
        result = { terminalId: "terminal-2" }
      }
      return {
        kind: "response" as const,
        version: 1 as const,
        id: request.id,
        result,
      }
    })
    const bridge: NativeSdkBridge = {
      invoke,
      on: vi.fn(() => () => undefined),
    }
    vi.stubGlobal("window", { zero: bridge })
    vi.stubGlobal("crypto", { randomUUID: () => "request" })

    const client = new BrowserRuntimeClient()
    await client.connect()
    await client.dispatch({
      type: "workspaceTerminal.create",
      workContextId,
      cols: 80,
      rows: 24,
    })
    await client.dispatch({
      type: "workspaceTerminal.read",
      terminalId: "terminal-1",
      cursor: 0,
      maxBytes: 256_000,
    })
    await client.dispatch({
      type: "workspaceTerminal.create",
      workContextId,
      cols: 80,
      rows: 24,
    })

    await client.dispatch({
      type: "terminal.select",
      terminalId: "terminal-1",
    })
    expect(client.getSnapshot().activeTerminalId).toBe("terminal-1")
    expect(
      client
        .getSnapshot()
        .terminals.find((terminal) => terminal.id === "terminal-1"),
    ).toMatchObject({ output: "exit\r\n" })
    await client.dispatch({
      type: "terminal.select",
      terminalId: "terminal-2",
    })
    await client.dispatch({
      type: "workspaceTerminal.close",
      terminalId: "terminal-1",
    })

    expect(client.getSnapshot().activeTerminalId).toBe("terminal-2")
    expect(client.getSnapshot().terminals[0]).toMatchObject({
      id: "terminal-2",
      state: "running",
      output: "",
    })
    expect(client.getSnapshot().terminals).toHaveLength(1)
    expect(
      invoke.mock.calls
        .map(([, request]) => request)
        .filter((request) => request.method === "workspaceTerminal.create")
        .map((request) => request.params),
    ).toEqual([
      { workContextId, cols: 80, rows: 24 },
      { workContextId, cols: 80, rows: 24 },
    ])
  })

  it("keeps workspace-terminal failures scoped to the terminal", async () => {
    const snapshot = {
      ...emptyRuntimeSnapshot({
        theme: "system" as const,
        nativeNotifications: true,
        layout: { inspectorPercent: 28, terminalPercent: 24 },
      }),
      targetWorkspace: {
        ...targetWorkspaceSnapshot(),
        terminals: [
          {
            id: "terminal-1",
            workContextId: "context-1",
            workspaceBindingId: "binding-1",
            title: "Terminal 1",
            state: "running" as const,
            createdAtUnixMs: 1,
          },
        ],
      },
    }
    const invoke = vi.fn(async (_: string, request: RuntimeRequest) => {
      const result =
        request.method === "runtime.hello"
          ? {
              protocolVersion: 1,
              runtimeName: "agent-factory-runtime",
              runtimeVersion: "0.1.0",
            }
          : request.method === "snapshot.get"
            ? snapshot
            : { agents: [] }
      if (request.method === "workspaceTerminal.read") {
        return {
          kind: "response" as const,
          version: 1 as const,
          id: request.id,
          error: { code: -32602, message: "unknown terminal terminal-1" },
        }
      }
      return {
        kind: "response" as const,
        version: 1 as const,
        id: request.id,
        result,
      }
    })
    const bridge: NativeSdkBridge = {
      invoke: invoke as NativeSdkBridge["invoke"],
      on: vi.fn(() => () => undefined),
    }
    vi.stubGlobal("window", { zero: bridge })
    vi.stubGlobal("crypto", { randomUUID: () => "request" })
    const client = new BrowserRuntimeClient()
    await client.connect()

    await client.dispatch({
      type: "workspaceTerminal.read",
      terminalId: "terminal-1",
      cursor: 0,
      maxBytes: 262_144,
    })

    expect(client.getSnapshot().connection).toBe("ready")
    expect(client.getSnapshot().terminals[0]?.state).toBe("failed")
  })

  it("surfaces workspaceTerminal.create failures on the work context", async () => {
    const snapshot = {
      ...emptyRuntimeSnapshot({
        theme: "system" as const,
        nativeNotifications: true,
        layout: { inspectorPercent: 28, terminalPercent: 24 },
      }),
      targetWorkspace: {
        ...targetWorkspaceSnapshot(),
        terminals: [],
      },
    }
    const invoke = vi.fn(async (_: string, request: RuntimeRequest) => {
      if (request.method === "workspaceTerminal.create") {
        return {
          kind: "response" as const,
          version: 1 as const,
          id: request.id,
          error: {
            code: -32602,
            message: "Trust this workspace before opening a terminal.",
          },
        }
      }
      const result =
        request.method === "runtime.hello"
          ? {
              protocolVersion: 1,
              runtimeName: "agent-factory-runtime",
              runtimeVersion: "0.1.0",
            }
          : request.method === "snapshot.get"
            ? snapshot
            : { agents: [] }
      return {
        kind: "response" as const,
        version: 1 as const,
        id: request.id,
        result,
      }
    })
    const bridge: NativeSdkBridge = {
      invoke: invoke as NativeSdkBridge["invoke"],
      on: vi.fn(() => () => undefined),
    }
    vi.stubGlobal("window", { zero: bridge })
    vi.stubGlobal("crypto", { randomUUID: () => "request" })
    const client = new BrowserRuntimeClient()
    await client.connect()

    await client.dispatch({
      type: "workspaceTerminal.create",
      workContextId: "context-1",
      cols: 80,
      rows: 24,
    })

    expect(client.getSnapshot().connection).toBe("ready")
    const terminals = client.getSnapshot().terminals
    expect(terminals).toHaveLength(1)
    expect(terminals[0]).toMatchObject({
      workContextId: "context-1",
      state: "failed",
      output: "Trust this workspace before opening a terminal.",
    })
    expect(terminals[0]?.id).toBeUndefined()
  })

  it("projects persisted terminal exits over stale live state", async () => {
    let snapshotState: "running" | "exited" = "running"
    const invoke = vi.fn(async (_: string, request: RuntimeRequest) => ({
      kind: "response" as const,
      version: 1 as const,
      id: request.id,
      result:
        request.method === "runtime.hello"
          ? {
              protocolVersion: 1,
              runtimeName: "agent-factory-runtime",
              runtimeVersion: "0.1.0",
            }
          : request.method === "snapshot.get"
            ? {
                ...emptyRuntimeSnapshot({
                  theme: "system",
                  nativeNotifications: true,
                  layout: { inspectorPercent: 28, terminalPercent: 24 },
                }),
                targetWorkspace: {
                  ...targetWorkspaceSnapshot(),
                  terminals: [
                    {
                      id: "terminal-1",
                      workContextId: "context-1",
                      workspaceBindingId: "binding-1",
                      title: "Terminal 1",
                      state: snapshotState,
                      createdAtUnixMs: 1,
                    },
                  ],
                },
              }
            : { herdr: { connected: true, freshness: "live", issues: [] }, harnesses: [] },
    }))
    const bridge: NativeSdkBridge = {
      invoke: invoke as NativeSdkBridge["invoke"],
      on: vi.fn(() => () => undefined),
    }
    vi.stubGlobal("window", { zero: bridge })
    vi.stubGlobal("crypto", { randomUUID: () => "request" })
    const client = new BrowserRuntimeClient()
    await client.connect()
    expect(client.getSnapshot().terminals[0]?.state).toBe("running")

    snapshotState = "exited"
    await client.connect()

    expect(client.getSnapshot().terminals[0]).toMatchObject({
      state: "exited",
      readerClosed: true,
    })
  })

  it("projects and sends Rust-owned settings through generated methods", async () => {
    let revision = 1
    let settings: WorkspaceSettingsProjection = {
      theme: "system",
      nativeNotifications: true,
      layout: { inspectorPercent: 28, terminalPercent: 24 },
    }
    const invoke = vi.fn(async (_: string, request: RuntimeRequest) => {
      const params = request.params as Record<string, unknown>
      if (request.method === "settings.setTheme") {
        revision += 1
        settings = { ...settings, theme: params.theme as "dark" }
      }
      if (request.method === "settings.setNotifications") {
        revision += 1
        settings = {
          ...settings,
          nativeNotifications: params.enabled as boolean,
        }
      }
      if (request.method === "settings.setLayout") {
        revision += 1
        settings = {
          ...settings,
          layout: {
            inspectorPercent: params.inspectorPercent as number,
            terminalPercent: params.terminalPercent as number,
          },
        }
      }
      return {
        kind: "response" as const,
        version: 1 as const,
        id: request.id,
        result:
          request.method === "runtime.hello"
            ? {
                protocolVersion: 1,
                runtimeName: "agent-factory-runtime",
                runtimeVersion: "0.1.0",
              }
            : request.method === "snapshot.get"
              ? {
                  revision,
                  settings,
                  activeProjectId: null,
                  activeAgentSessionId: null,
                  activeRunId: null,
                  projects: [],
                  environments: [],
                  agentSessions: [],
                  liveAgents: [],
                  harnesses: [],
                  herdr: { connected: true, freshness: "live", issues: [] },
                  factoryRuns: [],
                  targetWorkspace: targetWorkspaceSnapshot(),
                }
              : request.method === "harness.list"
                ? { agents: [] }
                : { revision, settings },
      }
    })
    const bridge: NativeSdkBridge = {
      invoke,
      on: vi.fn(() => () => undefined),
    }
    vi.stubGlobal("window", { zero: bridge })
    vi.stubGlobal("crypto", { randomUUID: () => "request" })
    const client = new BrowserRuntimeClient()
    await client.connect()

    await client.dispatch({ type: "settings.theme", theme: "dark" })
    await client.dispatch({
      type: "settings.notifications",
      enabled: false,
    })
    await client.dispatch({
      type: "settings.layout",
      inspectorPercent: 32,
      terminalPercent: 27,
    })

    expect(client.getSnapshot().settings).toEqual({
      theme: "dark",
      nativeNotifications: false,
      layout: { inspectorPercent: 32, terminalPercent: 27 },
    })
    expect(
      invoke.mock.calls
        .map(([, request]) => request)
        .filter((request) => request.method.startsWith("settings."))
        .map(({ method, params }) => ({ method, params })),
    ).toEqual([
      { method: "settings.setTheme", params: { theme: "dark" } },
      {
        method: "settings.setNotifications",
        params: { enabled: false },
      },
      {
        method: "settings.setLayout",
        params: { inspectorPercent: 32, terminalPercent: 27 },
      },
    ])
  })

  it("keeps secret values out of projections while sending exact CRUD methods", async () => {
    const secretRef = "secret_550e8400e29b41d4a716446655440000"
    let secrets: Array<{
      secretRef: string
      label: string
      createdAtUnixMs: number
      updatedAtUnixMs: number
    }> = []
    const invoke = vi.fn(async (_: string, request: RuntimeRequest) => {
      const params = request.params as Record<string, unknown>
      if (request.method === "secret.create") {
        secrets = [{
          secretRef,
          label: String(params.label),
          createdAtUnixMs: 1,
          updatedAtUnixMs: 1,
        }]
      }
      if (request.method === "secret.replace") {
        secrets = secrets.map((secret) => ({
          ...secret,
          updatedAtUnixMs: 2,
        }))
      }
      if (request.method === "secret.delete") secrets = []
      return {
        kind: "response" as const,
        version: 1 as const,
        id: request.id,
        result:
          request.method === "runtime.hello"
            ? {
                protocolVersion: 1,
                runtimeName: "agent-factory-runtime",
                runtimeVersion: "0.1.0",
              }
            : request.method === "snapshot.get"
              ? emptyRuntimeSnapshot({
                  theme: "system",
                  nativeNotifications: false,
                  layout: { inspectorPercent: 28, terminalPercent: 24 },
                })
              : request.method === "harness.list"
                ? { agents: [] }
                : { secrets },
      }
    })
    const bridge: NativeSdkBridge = {
      invoke,
      on: vi.fn(() => () => undefined),
    }
    vi.stubGlobal("window", { zero: bridge })
    vi.stubGlobal("crypto", { randomUUID: () => "request" })
    const client = new BrowserRuntimeClient()
    await client.connect()

    await client.dispatch({
      type: "secret.create",
      label: "Registry token",
      value: "create-secret",
    })
    expect(JSON.stringify(client.getSnapshot())).not.toContain("create-secret")
    await client.dispatch({
      type: "secret.replace",
      secretRef,
      value: "replacement-secret",
    })
    expect(JSON.stringify(client.getSnapshot())).not.toContain("replacement-secret")
    await client.dispatch({ type: "secret.delete", secretRef })

    expect(client.getSnapshot().secrets).toEqual([])
    expect(
      invoke.mock.calls
        .map(([, request]) => request)
        .filter((request) => request.method.startsWith("secret."))
        .map(({ method, params }) => ({ method, params })),
    ).toEqual([
      { method: "secret.list", params: {} },
      {
        method: "secret.create",
        params: { label: "Registry token", value: "create-secret" },
      },
      {
        method: "secret.replace",
        params: { secretRef, value: "replacement-secret" },
      },
      { method: "secret.delete", params: { secretRef } },
    ])
  })

  it("sends exact signed-registry and executable-trust methods", async () => {
    const invoke = vi.fn(async (_: string, request: RuntimeRequest) => ({
      kind: "response" as const,
      version: 1 as const,
      id: request.id,
      result:
        request.method === "runtime.hello"
          ? {
              protocolVersion: 1,
              runtimeName: "agent-factory-runtime",
              runtimeVersion: "0.1.0",
            }
          : request.method === "snapshot.get"
            ? emptyRuntimeSnapshot({
                theme: "system",
                nativeNotifications: false,
                layout: { inspectorPercent: 28, terminalPercent: 24 },
              })
            : request.method === "harness.list"
              ? { agents: [] }
              : request.method === "registry.refresh"
                ? {
                    registryId: "official",
                    generatedAt: "2026-08-08T00:00:00Z",
                    plugins: [],
                  }
                : request.method === "plugin.details"
                  ? {
                      registryId: "official",
                      pluginId: "quality-tools",
                      name: "quality-tools",
                      version: "1.0.0",
                      description: "Quality tools",
                      authorName: "Agent Factory",
                      sourceUrl:
                        "https://github.com/acme/plugins/tree/main/plugins/quality-tools",
                      skills: [],
                      mcpServers: [],
                      mcpDisabledReason: null,
                    }
                : request.method.startsWith("registry.")
                  ? { registries: [] }
                  : { installed: [], localMcpServers: [] },
    }))
    const bridge: NativeSdkBridge = {
      invoke,
      on: vi.fn(() => () => undefined),
    }
    vi.stubGlobal("window", { zero: bridge })
    vi.stubGlobal("crypto", { randomUUID: () => "request" })
    const client = new BrowserRuntimeClient()
    await client.connect()

    await client.dispatch({ type: "registry.list" })
    await client.dispatch({
      type: "registry.put",
      id: "official",
      catalogUrl: "https://plugins.example/catalog.json",
      signatureUrl: "https://plugins.example/catalog.sig",
      publicKeyBase64: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
    })
    await client.dispatch({ type: "registry.refresh", registryId: "official" })
    await client.dispatch({
      type: "plugin.details",
      registryId: "official",
      pluginId: "quality-tools",
    })
    await client.dispatch({
      type: "plugin.install",
      registryId: "official",
      pluginId: "quality-tools",
    })
    await client.dispatch({
      type: "plugin.uninstall",
      pluginName: "quality-tools",
    })
    await client.dispatch({
      type: "plugin.rollback",
      pluginName: "quality-tools",
    })
    const trust = {
      environmentId: "default",
      pluginName: "quality-tools",
      serverName: "lint",
      fingerprint: "a".repeat(64),
    }
    await client.dispatch({ type: "plugin.trustLocalMcp", ...trust })
    await client.dispatch({ type: "plugin.revokeLocalMcp", ...trust })
    await client.dispatch({ type: "registry.delete", registryId: "official" })

    expect(
      invoke.mock.calls
        .map(([, request]) => request)
        .filter(
          (request) =>
            request.method.startsWith("registry.") ||
            request.method.startsWith("plugin."),
        )
        .map(({ method, params }) => ({ method, params })),
    ).toEqual([
      { method: "registry.list", params: {} },
      {
        method: "registry.put",
        params: {
          id: "official",
          catalogUrl: "https://plugins.example/catalog.json",
          signatureUrl: "https://plugins.example/catalog.sig",
          publicKeyBase64: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
        },
      },
      { method: "registry.refresh", params: { registryId: "official" } },
      {
        method: "plugin.details",
        params: { registryId: "official", pluginId: "quality-tools" },
      },
      {
        method: "plugin.install",
        params: { registryId: "official", pluginId: "quality-tools" },
      },
      { method: "plugin.uninstall", params: { pluginName: "quality-tools" } },
      { method: "plugin.rollback", params: { pluginName: "quality-tools" } },
      { method: "plugin.trustLocalMcp", params: trust },
      { method: "plugin.revokeLocalMcp", params: trust },
      { method: "registry.delete", params: { registryId: "official" } },
    ])
    expect(client.getSnapshot().pluginDetails?.name).toBe("quality-tools")
  })

  it("saves a environment's whole configuration through the exact wire method", async () => {
    const environments = [
      {
        id: "acme",
        name: "Acme",
        codingAgentId: "claude-code",
        evaluationAgentId: "claude-code",
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
      },
    ] as const
    const configuration = {
      name: "Acme",
      environmentVariables: [],
      llm: null,
      plugins: [
        {
          name: "observability",
          enabledMcpServers: ["httpbin"],
          defaultSkills: ["trace-reader"],
        },
      ],
      registries: ["official"],
    }
    const saved = {
      ...environments[0],
      plugins: configuration.plugins,
      registryIds: ["official"],
    }
    const invoke = vi.fn(async (_: string, request: RuntimeRequest) => ({
      kind: "response" as const,
      version: 1 as const,
      id: request.id,
      result:
        request.method === "runtime.hello"
          ? {
              protocolVersion: 1,
              runtimeName: "agent-factory-runtime",
              runtimeVersion: "0.1.0",
            }
          : request.method === "snapshot.get"
            ? {
                ...emptyRuntimeSnapshot({
                  theme: "system",
                  nativeNotifications: false,
                  layout: { inspectorPercent: 28, terminalPercent: 24 },
                }),
                environments: [...environments],
              }
            : request.method === "harness.list"
              ? { agents: [] }
              : request.method === "environment.configuration.set"
                ? {
                    environments: [saved],
                    revision: 7,
                  }
                : {},
    }))
    const bridge: NativeSdkBridge = {
      invoke,
      on: vi.fn(() => () => undefined),
    }
    vi.stubGlobal("window", { zero: bridge })
    vi.stubGlobal("crypto", { randomUUID: () => "request" })
    const client = new BrowserRuntimeClient()
    await client.connect()

    await client.dispatch({
      type: "environment.configuration.set",
      environmentId: "acme",
      configuration,
    })

    // Plugins and registries ride along with the rest of the configuration;
    // there is no separate wire method for either.
    expect(
      invoke.mock.calls
        .map(([, request]) => request)
        .filter((request) => request.method.startsWith("environment."))
        .map(({ method, params }) => ({ method, params })),
    ).toEqual([
      {
        method: "environment.configuration.set",
        params: { environmentId: "acme", configuration },
      },
    ])
    const snapshot = client.getSnapshot()
    expect(snapshot.revision).toBe(7)
    expect(snapshot.environments[0].registryIds).toEqual(["official"])
    expect(snapshot.environments[0].plugins).toEqual(configuration.plugins)
  })

  it("creates a environment from a name and reports the derived id", async () => {
    const created = {
      id: "local-ollama",
      name: "Local Ollama",
      codingAgentId: "claude-code",
      evaluationAgentId: "claude-code",
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
    const configuration = {
      name: "Local Ollama",
      environmentVariables: [],
      llm: null,
      plugins: [],
      registries: [],
    }
    const invoke = vi.fn(async (_: string, request: RuntimeRequest) => ({
      kind: "response" as const,
      version: 1 as const,
      id: request.id,
      result:
        request.method === "runtime.hello"
          ? {
              protocolVersion: 1,
              runtimeName: "agent-factory-runtime",
              runtimeVersion: "0.1.0",
            }
          : request.method === "snapshot.get"
            ? emptyRuntimeSnapshot({
                theme: "system",
                nativeNotifications: false,
                layout: { inspectorPercent: 28, terminalPercent: 24 },
              })
            : request.method === "harness.list"
              ? { agents: [] }
              : request.method === "environment.create"
                ? {
                    environmentId: "local-ollama",
                    environments: [created],
                    revision: 3,
                  }
                : request.method === "environment.delete"
                  ? { environments: [], revision: 4 }
                  : {},
    }))
    const bridge: NativeSdkBridge = {
      invoke,
      on: vi.fn(() => () => undefined),
    }
    vi.stubGlobal("window", { zero: bridge })
    vi.stubGlobal("crypto", { randomUUID: () => "request" })
    const client = new BrowserRuntimeClient()
    await client.connect()

    // No id is sent: the caller has no way to know it, so the runtime reports it.
    await client.dispatch({ type: "environment.create", configuration })
    expect(
      invoke.mock.calls
        .map(([, request]) => request)
        .filter((request) => request.method === "environment.create")
        .map(({ method, params }) => ({ method, params })),
    ).toEqual([{ method: "environment.create", params: { configuration } }])

    // Deleting the last Environment leaves the catalog empty.
    await client.dispatch({ type: "environment.delete", environmentId: "local-ollama" })
    expect(client.getSnapshot().environments).toEqual([])
  })

  it("requires the selected version for update installation", async () => {
    let state = "idle"
    const invoke = vi.fn(async (_: string, request: RuntimeRequest) => {
      const params = request.params as Record<string, unknown>
      if (request.method === "update.check") state = "awaiting_confirmation"
      if (request.method === "update.confirmAndInstall") {
        expect(params.version).toBe("1.1.0")
        state = "ready_to_restart"
      }
      if (request.method === "update.rollback") state = "idle"
      return {
        kind: "response" as const,
        version: 1 as const,
        id: request.id,
        result:
          request.method === "runtime.hello"
            ? {
                protocolVersion: 1,
                runtimeName: "agent-factory-runtime",
                runtimeVersion: "0.1.0",
              }
            : request.method === "snapshot.get"
              ? emptyRuntimeSnapshot({
                  theme: "system",
                  nativeNotifications: false,
                  layout: { inspectorPercent: 28, terminalPercent: 24 },
                })
              : request.method === "harness.list"
                ? { agents: [] }
                : {
                    enabled: true,
                    configStatus: "loaded",
                    currentVersion: "0.1.0",
                    state,
                    targetVersion:
                      state === "awaiting_confirmation" ||
                      state === "ready_to_restart"
                        ? "1.1.0"
                        : null,
                    message: null,
                  },
      }
    })
    const bridge: NativeSdkBridge = {
      invoke,
      on: vi.fn(() => () => undefined),
    }
    vi.stubGlobal("window", { zero: bridge })
    vi.stubGlobal("crypto", { randomUUID: () => "request" })
    const client = new BrowserRuntimeClient()
    await client.connect()

    await client.dispatch({ type: "update.status" })
    await client.dispatch({ type: "update.check" })
    expect(client.getSnapshot().updateStatus?.state).toBe(
      "awaiting_confirmation",
    )
    await client.dispatch({
      type: "update.confirmAndInstall",
      version: "1.1.0",
    })
    expect(client.getSnapshot().updateStatus?.state).toBe("ready_to_restart")
    await client.dispatch({ type: "update.rollback" })

    expect(
      invoke.mock.calls
        .map(([, request]) => request)
        .filter((request) => request.method.startsWith("update."))
        .map(({ method, params }) => ({ method, params })),
    ).toEqual([
      { method: "update.status", params: {} },
      { method: "update.check", params: {} },
      {
        method: "update.confirmAndInstall",
        params: { version: "1.1.0" },
      },
      { method: "update.rollback", params: {} },
    ])
  })

  it("refreshes Herdr status when the runtime reopens a lost subscription", async () => {
    let eventListener: ((detail: unknown) => void) | undefined
    let harnessResult: HarnessListDto = {
      herdr: {
        connected: false,
        freshness: "last_observed",
        issues: ["The Herdr event stream closed."],
      },
      harnesses: [],
    }
    const invoke = vi.fn(async (_: string, request: RuntimeRequest) => ({
      kind: "response",
      version: 1,
      id: request.id,
      result:
        request.method === "runtime.hello"
          ? {
              protocolVersion: 1,
              runtimeName: "agent-factory-runtime",
              runtimeVersion: "0.1.0",
            }
          : request.method === "snapshot.get"
            ? {
                revision: 1,
                settings: {
                  theme: "system",
                  nativeNotifications: true,
                  layout: { inspectorPercent: 28, terminalPercent: 24 },
                },
                activeProjectId: null,
                activeAgentSessionId: null,
                activeRunId: null,
                projects: [],
                environments: [],
                agentSessions: [],
                liveAgents: [],
                harnesses: harnessResult.harnesses,
                herdr: harnessResult.herdr,
                factoryRuns: [],
                targetWorkspace: targetWorkspaceSnapshot(),
              }
            : harnessResult,
    }))
    const bridge: NativeSdkBridge = {
      invoke: invoke as NativeSdkBridge["invoke"],
      on: vi.fn((name, listener) => {
        if (name === "runtime:event") eventListener = listener
        return () => undefined
      }),
    }
    vi.stubGlobal("window", { zero: bridge })
    vi.stubGlobal("crypto", { randomUUID: () => "request" })
    const client = new BrowserRuntimeClient()
    await client.connect()
    expect(client.getSnapshot().herdr.connected).toBe(false)

    let sequence = 0
    const harnessChanged = (payload: unknown) => ({
      kind: "event" as const,
      version: 1,
      sequence: (sequence += 1),
      revision: 1,
      topic: "harness.changed",
      payload,
    })

    // A malformed payload leaves the last known status alone rather than
    // blanking the Harness list.
    eventListener?.(harnessChanged({ herdr: { connected: "yes" } }))
    expect(client.getSnapshot().herdr.connected).toBe(false)

    harnessResult = {
      herdr: { connected: true, freshness: "live", issues: [] },
      harnesses: [
        {
          id: "claude",
          name: "Claude Code",
          readiness: "ready",
          guidance: "Ready to launch with Herdr.",
          action: null,
        },
      ],
    }
    eventListener?.(
      harnessChanged({
        herdr: { connected: true, freshness: "live", issues: [] },
        harnesses: [
          {
            id: "claude",
            name: "Claude Code",
            readiness: "ready",
            guidance: "Ready to launch with Herdr.",
            action: null,
          },
        ],
      }),
    )

    await vi.waitFor(() =>
      expect(client.getSnapshot().herdr.connected).toBe(true),
    )
    expect(client.getSnapshot().harnesses.map((it) => it.id)).toEqual(["claude"])
  })

  it("refreshes projections when the runtime emits a newer event", async () => {
    let snapshotRevision = 1
    let eventListener: ((detail: unknown) => void) | undefined
    const invoke = vi.fn(async (_: string, request: RuntimeRequest) => ({
      kind: "response",
      version: 1,
      id: request.id,
      result:
        request.method === "runtime.hello"
          ? {
              protocolVersion: 1,
              runtimeName: "agent-factory-runtime",
              runtimeVersion: "0.1.0",
            }
          : request.method === "snapshot.get"
            ? {
                revision: snapshotRevision,
                settings: {
                  theme: "system",
                  nativeNotifications: true,
                  layout: { inspectorPercent: 28, terminalPercent: 24 },
                },
                activeProjectId: null,
                activeAgentSessionId: null,
                activeRunId: null,
                projects: [],
                environments: [],
                agentSessions: [],
                liveAgents: [],
                harnesses: [],
                herdr: { connected: true, freshness: "live", issues: [] },
                factoryRuns: [],
                targetWorkspace: targetWorkspaceSnapshot(),
              }
            : { herdr: { connected: true, freshness: "live", issues: [] }, harnesses: [] },
    }))
    const bridge: NativeSdkBridge = {
      invoke: invoke as NativeSdkBridge["invoke"],
      on: vi.fn((name, listener) => {
        if (name === "runtime:event") eventListener = listener
        return () => undefined
      }),
    }
    vi.stubGlobal("window", { zero: bridge })
    vi.stubGlobal("crypto", { randomUUID: () => "request" })
    const client = new BrowserRuntimeClient()
    await client.connect()

    eventListener?.({
      kind: "ready",
      version: 1,
      runtime_name: "agent-factory-runtime",
      runtime_version: "0.1.0",
    })
    expect(client.getSnapshot().connection).toBe("ready")

    snapshotRevision = 2
    eventListener?.({
      kind: "event",
      version: 1,
      sequence: 1,
      revision: 2,
      topic: "project.changed",
      payload: { projectId: "project-1" },
    })

    await vi.waitFor(() => expect(client.getSnapshot().revision).toBe(2))
    expect(bridge.on).toHaveBeenCalledWith(
      "runtime:event",
      expect.any(Function),
    )
  })

  it("polls snapshots when a native window receives no runtime events", async () => {
    vi.useFakeTimers()
    let snapshotRevision = 1
    let snapshotFails = false
    const invoke = vi.fn(async (_: string, request: RuntimeRequest) => ({
      kind: "response" as const,
      version: 1 as const,
      id: request.id,
      ...(request.method === "snapshot.get" && snapshotFails
        ? {
            error: {
              code: "internal" as const,
              message: "snapshot unavailable",
            },
          }
        : {
            result:
              request.method === "runtime.hello"
                ? {
                    protocolVersion: 1,
                    runtimeName: "agent-factory-runtime",
                    runtimeVersion: "0.1.0",
                  }
                : request.method === "snapshot.get"
                  ? {
                      ...emptyRuntimeSnapshot({
                        theme: "system",
                        nativeNotifications: true,
                        layout: { inspectorPercent: 28, terminalPercent: 24 },
                      }),
                      revision: snapshotRevision,
                      herdr: {
                        connected: true,
                        freshness: "live" as const,
                        observedAtUnixMs: 1,
                        issues: [],
                      },
                    }
                  : { secrets: [] },
          }),
    }))
    const addEventListener = vi.fn()
    const removeEventListener = vi.fn()
    const bridge: NativeSdkBridge = {
      invoke,
      on: vi.fn(() => () => undefined),
    }
    vi.stubGlobal("window", {
      zero: bridge,
      addEventListener,
      removeEventListener,
    })
    vi.stubGlobal("crypto", { randomUUID: () => "request" })
    const client = new BrowserRuntimeClient()
    await client.connect()

    snapshotRevision = 2
    await vi.advanceTimersByTimeAsync(2_000)
    expect(client.getSnapshot().revision).toBe(2)

    snapshotFails = true
    await vi.advanceTimersByTimeAsync(2_000)
    expect(client.getSnapshot()).toMatchObject({
      connection: "degraded",
      herdr: { freshness: "last_observed" },
    })

    snapshotFails = false
    snapshotRevision = 3
    await vi.advanceTimersByTimeAsync(2_000)
    expect(client.getSnapshot()).toMatchObject({
      connection: "ready",
      revision: 3,
      herdr: { freshness: "live" },
    })

    client.disconnect()
    expect(addEventListener).toHaveBeenCalledWith("focus", expect.any(Function))
    expect(removeEventListener).toHaveBeenCalledWith(
      "focus",
      expect.any(Function),
    )
  })

  it("resynchronizes from a snapshot when an event sequence has a gap", async () => {
    let snapshotRevision = 1
    let eventListener: ((detail: unknown) => void) | undefined
    const invoke = vi.fn(async (_: string, request: RuntimeRequest) => ({
      kind: "response",
      version: 1,
      id: request.id,
      result:
        request.method === "runtime.hello"
          ? {
              protocolVersion: 1,
              runtimeName: "agent-factory-runtime",
              runtimeVersion: "0.1.0",
            }
          : request.method === "snapshot.get"
            ? {
                revision: snapshotRevision,
                settings: {
                  theme: "system",
                  nativeNotifications: true,
                  layout: { inspectorPercent: 28, terminalPercent: 24 },
                },
                activeProjectId: null,
                activeAgentSessionId: null,
                activeRunId: null,
                projects: [],
                environments: [],
                agentSessions: [],
                liveAgents: [],
                harnesses: [],
                herdr: { connected: true, freshness: "live", issues: [] },
                factoryRuns: [],
                targetWorkspace: targetWorkspaceSnapshot(),
              }
            : { herdr: { connected: true, freshness: "live", issues: [] }, harnesses: [] },
    }))
    const bridge: NativeSdkBridge = {
      invoke: invoke as NativeSdkBridge["invoke"],
      on: vi.fn((name, listener) => {
        if (name === "runtime:event") eventListener = listener
        return () => undefined
      }),
    }
    vi.stubGlobal("window", { zero: bridge })
    vi.stubGlobal("crypto", { randomUUID: () => "request" })
    const client = new BrowserRuntimeClient()
    await client.connect()

    snapshotRevision = 2
    eventListener?.({
      kind: "event",
      version: 1,
      sequence: 1,
      revision: 2,
      topic: "project.changed",
      payload: {},
    })
    await vi.waitFor(() => expect(client.getSnapshot().revision).toBe(2))

    snapshotRevision = 3
    eventListener?.({
      kind: "event",
      version: 1,
      sequence: 3,
      revision: 2,
      topic: "project.changed",
      payload: {},
    })

    await vi.waitFor(() => expect(client.getSnapshot().revision).toBe(3))
    expect(
      invoke.mock.calls.filter(
        ([, request]) => request.method === "snapshot.get",
      ),
    ).toHaveLength(3)
  })

  it("keeps Target Agent creation errors scoped to the workspace action", async () => {
    const invoke = vi.fn(async (_: string, request: RuntimeRequest) => {
      if (request.method === "targetAgent.create") {
        return {
          kind: "response" as const,
          version: 1 as const,
          id: request.id,
          error: {
            code: "invalid_params" as const,
            message:
              "existing Agent definition uses unsupported schema version 1; expected 4. Remove the existing Agent definition file and try again",
          },
        }
      }
      const result =
        request.method === "runtime.hello"
          ? {
              protocolVersion: 1,
              runtimeName: "agent-factory-runtime",
              runtimeVersion: "0.1.0",
            }
          : request.method === "snapshot.get"
            ? emptyRuntimeSnapshot({
                theme: "system" as const,
                nativeNotifications: true,
                layout: { inspectorPercent: 28, terminalPercent: 24 },
              })
            : { agents: [] }
      return {
        kind: "response" as const,
        version: 1 as const,
        id: request.id,
        result,
      }
    })
    const bridge: NativeSdkBridge = {
      invoke,
      on: vi.fn(() => () => undefined),
    }
    vi.stubGlobal("window", { zero: bridge })
    vi.stubGlobal("crypto", { randomUUID: () => "request" })
    const client = new BrowserRuntimeClient()
    await client.connect()

    await client.dispatch({
      type: "targetAgent.create",
      name: "Commerce Copilot",
      objective: "Resolve commerce support requests",
      acceptanceCriteria: ["Refund requests are classified correctly"],
      repositoryRoot: "/tmp/customer-ai",
      draftName: "main",
      trusted: true,
    })

    expect(client.getSnapshot()).toMatchObject({
      connection: "ready",
      targetWorkspaceError:
        "existing Agent definition uses unsupported schema version 1; expected 4. Remove the existing Agent definition file and try again",
    })
    expect(client.getSnapshot().connectionDetail).toBeUndefined()
  })

  it("sends exact project trust intents and scopes their errors", async () => {
    const invoke = vi.fn(async (_: string, request: RuntimeRequest) => ({
      kind: "response" as const,
      version: 1 as const,
      id: request.id,
      ...(request.method === "project.trust.set"
        ? {
            error: {
              code: "invalid_params" as const,
              message: "project trust could not be changed",
            },
          }
        : {
            result:
              request.method === "runtime.hello"
                ? {
                    protocolVersion: 1,
                    runtimeName: "agent-factory-runtime",
                    runtimeVersion: "0.1.0",
                  }
                : request.method === "snapshot.get"
                  ? emptyRuntimeSnapshot({
                      theme: "system",
                      nativeNotifications: true,
                      layout: { inspectorPercent: 28, terminalPercent: 24 },
                    })
                  : { herdr: { connected: true, freshness: "live", issues: [] }, harnesses: [] },
          }),
    }))
    const bridge: NativeSdkBridge = {
      invoke,
      on: vi.fn(() => () => undefined),
    }
    vi.stubGlobal("window", { zero: bridge })
    vi.stubGlobal("crypto", { randomUUID: () => "request" })
    const client = new BrowserRuntimeClient()
    await client.connect()

    await client.dispatch({
      type: "project.trust.set",
      projectId: "project-1",
      trusted: false,
    })

    expect(
      invoke.mock.calls
        .map(([, request]) => request as RuntimeRequest)
        .find((request) => request.method === "project.trust.set"),
    ).toMatchObject({
      method: "project.trust.set",
      params: { projectId: "project-1", trusted: false },
    })
    expect(client.getSnapshot()).toMatchObject({
      connection: "ready",
      targetWorkspaceError: "project trust could not be changed",
    })
  })

  it("projects environments and form elicitations through exact runtime methods", async () => {
    let eventListener: ((detail: unknown) => void) | undefined
    const environments = [
      {
        id: "default",
        name: "Default",
        editable: false,
        codingAgentId: "claude-code",
        evaluationAgentId: "claude-code",
        plugins: [],
        permissions: {
          trustedRead: "allow",
          trustedWrite: "ask",
          terminal: "ask",
        },
        registryIds: [],
        environmentVariables: [],
        llm: {
          providerId: "provider-1",
          allowedModels: ["qwen3-coder"],
          defaultModel: "qwen3-coder",
        },
        resolvedLlm: {
          providerId: "provider-1",
          providerName: "Local Ollama",
          type: "ollama",
          endpoint: "http://127.0.0.1:11434",
          allowedModels: ["qwen3-coder"],
          defaultModel: "qwen3-coder",
        },
        llmNeedsSetup: false,
        readiness: { state: "ready", issues: [] },
      },
      {
        id: "restricted",
        name: "Restricted",
        codingAgentId: "claude-code",
        evaluationAgentId: "claude-code",
        plugins: [],
        permissions: {
          trustedRead: "allow",
          trustedWrite: "deny",
          terminal: "deny",
        },
        registryIds: [],
        environmentVariables: [],
        llm: null,
        resolvedLlm: null,
        llmNeedsSetup: false,
        readiness: {
          state: "needs_setup",
          issues: ["Configure an Intelligence Provider"],
        },
      },
    ] as const
    const invoke = vi.fn(async (_: string, request: RuntimeRequest) => ({
      kind: "response" as const,
      version: 1 as const,
      id: request.id,
      result:
        request.method === "runtime.hello"
          ? {
              protocolVersion: 1,
              runtimeName: "agent-factory-runtime",
              runtimeVersion: "0.1.0",
            }
          : request.method === "snapshot.get"
            ? {
                revision: 1,
                settings: {
                  theme: "system",
                  nativeNotifications: false,
                  layout: { inspectorPercent: 28, terminalPercent: 24 },
                },
                activeProjectId: null,
                activeAgentSessionId: null,
                activeRunId: null,
                projects: [],
                llmProviders: [],
                environments,
                agentSessions: [],
                liveAgents: [],
                harnesses: [],
                herdr: { connected: true, freshness: "live", issues: [] },
                factoryRuns: [],
                targetWorkspace: targetWorkspaceSnapshot(),
              }
            : request.method === "harness.list"
              ? { agents: [] }
              : request.method === "environment.create"
                ? {
                    environment: {
                      ...environments[1],
                      id: "ollama",
                      name: "Ollama",
                      environmentVariables: [],
                    },
                    revision: 4,
                  }
                : request.method === "llmProvider.models.list"
                  ? {
                      providerId: null,
                      models: ["qwen3-coder", "code-large"],
                    }
                : request.method === "environment.configuration.set"
                  ? {
                      environments: [...environments],
                      revision: 3,
                    }
                : { responded: true },
    }))
    const bridge: NativeSdkBridge = {
      invoke: invoke as NativeSdkBridge["invoke"],
      on: vi.fn((name, listener) => {
        if (name === "runtime:event") eventListener = listener
        return () => undefined
      }),
    }
    vi.stubGlobal("window", { zero: bridge })
    vi.stubGlobal("crypto", { randomUUID: () => "request" })
    const client = new BrowserRuntimeClient()
    await client.connect()

    eventListener?.({
      kind: "event",
      version: 1,
      sequence: 1,
      revision: 1,
      topic: "session.elicitation_requested",
      payload: {
        localSessionId: "session-1",
        type: "elicitation_requested",
        elicitationRequestId: "elicitation-1",
        request: {
          mode: "form",
          message: "Choose a name.",
          sessionId: "agent-session-1",
          requestedSchema: {
            type: "object",
            properties: {
              name: { type: "string", minLength: 1 },
            },
            required: ["name"],
          },
        },
      },
    })
    await client.dispatch({
      type: "environment.configuration.set",
      environmentId: "restricted",
      configuration: {
        name: "Restricted",
        environmentVariables: [
          {
            name: "ANTHROPIC_BASE_URL",
            source: "literal",
            value: "http://localhost:11434",
          },
          {
            name: "ANTHROPIC_AUTH_TOKEN",
            source: "secret",
            value: "secret_550e8400e29b41d4a716446655440000",
          },
        ],
        llm: null,
        plugins: [],
        registries: [],
      },
    })
    // Discovery carries no Environment id: the provider is all it needs, so an
    // unsaved draft can ask.
    await client.dispatch({
      type: "llmProvider.models.list",
      provider: {
        type: "ollama",
        endpoint: "http://127.0.0.1:11434",
      },
    })

    // Keyed by the provider that was asked, so selecting a different provider
    // cannot leave the previous provider's models on screen.
    expect(client.getSnapshot().llmProviderModelDiscovery).toEqual({
      providerKey: "ollama|http://127.0.0.1:11434|",
      models: ["qwen3-coder", "code-large"],
    })
    expect(invoke).toHaveBeenCalledWith(
      "runtime.invoke",
      expect.objectContaining({
        method: "environment.configuration.set",
        params: {
          environmentId: "restricted",
          configuration: {
            name: "Restricted",
            environmentVariables: [
              {
                name: "ANTHROPIC_BASE_URL",
                source: "literal",
                value: "http://localhost:11434",
              },
              {
                name: "ANTHROPIC_AUTH_TOKEN",
                source: "secret",
                value: "secret_550e8400e29b41d4a716446655440000",
              },
            ],
            llm: null,
            plugins: [],
            registries: [],
          },
        },
      }),
    )
    expect(invoke).toHaveBeenCalledWith(
      "runtime.invoke",
      expect.objectContaining({
        method: "llmProvider.models.list",
        params: {
          providerId: undefined,
          provider: {
            type: "ollama",
            endpoint: "http://127.0.0.1:11434",
          },
        },
      }),
    )
  })

  it("delivers typed toasts and gates native notifications by settings and focus", async () => {
    let eventListener: ((detail: unknown) => void) | undefined
    let settings: WorkspaceSettingsProjection = {
      theme: "system",
      nativeNotifications: true,
      layout: { inspectorPercent: 28, terminalPercent: 24 },
    }
    const invoke = vi.fn(async (command: string, payload: unknown) => {
      if (command === "native-sdk.os.showNotification") return true
      const request = payload as RuntimeRequest
      if (request.method === "settings.setNotifications") {
        settings = {
          ...settings,
          nativeNotifications: Boolean(
            (request.params as { enabled: boolean }).enabled,
          ),
        }
      }
      return {
        kind: "response" as const,
        version: 1 as const,
        id: request.id,
        result:
          request.method === "runtime.hello"
            ? {
                protocolVersion: 1,
                runtimeName: "agent-factory-runtime",
                runtimeVersion: "0.1.0",
              }
            : request.method === "snapshot.get"
              ? emptyRuntimeSnapshot(settings)
              : request.method === "harness.list"
                ? { agents: [] }
                : { revision: 1, settings },
      }
    })
    const bridge: NativeSdkBridge = {
      invoke: invoke as NativeSdkBridge["invoke"],
      on: vi.fn((name, listener) => {
        if (name === "runtime:event") eventListener = listener
        return () => undefined
      }),
    }
    vi.stubGlobal("window", { zero: bridge })
    vi.stubGlobal("crypto", { randomUUID: () => "request" })
    const focus = vi.fn(() => true)
    vi.stubGlobal("document", {
      visibilityState: "visible",
      hasFocus: focus,
    })
    setVisibility("visible")
    const client = new BrowserRuntimeClient()
    const notificationListener = vi.fn()
    client.subscribeNotifications(notificationListener)
    await client.connect()

    eventListener?.(notificationEvent(1))
    expect(notificationListener).toHaveBeenCalledTimes(1)
    expect(nativeNotificationCalls(invoke)).toHaveLength(0)

    setVisibility("hidden")
    eventListener?.(notificationEvent(2))
    await vi.waitFor(() =>
      expect(nativeNotificationCalls(invoke)).toEqual([
        {
          title: "Factory Run passed",
          body: "All acceptance criteria passed.",
        },
      ]),
    )

    await client.dispatch({
      type: "settings.notifications",
      enabled: false,
    })
    eventListener?.(notificationEvent(3))
    expect(notificationListener).toHaveBeenCalledTimes(3)
    expect(nativeNotificationCalls(invoke)).toHaveLength(1)

    await client.dispatch({
      type: "settings.notifications",
      enabled: true,
    })
    setVisibility("visible")
    focus.mockReturnValue(false)
    eventListener?.(notificationEvent(4))
    await vi.waitFor(() =>
      expect(nativeNotificationCalls(invoke)).toHaveLength(2),
    )
  })

  it("ignores malformed and unrelated notification event payloads", async () => {
    let eventListener: ((detail: unknown) => void) | undefined
    const invoke = vi.fn(async (command: string, payload: unknown) => {
      if (command === "native-sdk.os.showNotification") return true
      const request = payload as RuntimeRequest
      return {
        kind: "response" as const,
        version: 1 as const,
        id: request.id,
        result:
          request.method === "runtime.hello"
            ? {
                protocolVersion: 1,
                runtimeName: "agent-factory-runtime",
                runtimeVersion: "0.1.0",
              }
            : request.method === "snapshot.get"
              ? emptyRuntimeSnapshot({
                  theme: "system",
                  nativeNotifications: true,
                  layout: { inspectorPercent: 28, terminalPercent: 24 },
                })
              : { herdr: { connected: true, freshness: "live", issues: [] }, harnesses: [] },
      }
    })
    const bridge: NativeSdkBridge = {
      invoke: invoke as NativeSdkBridge["invoke"],
      on: vi.fn((name, listener) => {
        if (name === "runtime:event") eventListener = listener
        return () => undefined
      }),
    }
    vi.stubGlobal("window", { zero: bridge })
    vi.stubGlobal("crypto", { randomUUID: () => "request" })
    vi.stubGlobal("document", {
      visibilityState: "hidden",
      hasFocus: () => false,
    })
    setVisibility("hidden")
    const client = new BrowserRuntimeClient()
    const notificationListener = vi.fn()
    client.subscribeNotifications(notificationListener)
    await client.connect()

    eventListener?.({
      ...notificationEvent(1),
      topic: "session.update",
    })
    eventListener?.({
      ...notificationEvent(2),
      payload: {
        category: "arbitrary",
        entityId: "run-1",
        title: "Untrusted",
        body: "Do not show this.",
      },
    })
    eventListener?.({
      ...notificationEvent(3),
      payload: {
        ...notificationEvent(3).payload,
        arbitrary: true,
      },
    })

    expect(notificationListener).not.toHaveBeenCalled()
    expect(nativeNotificationCalls(invoke)).toHaveLength(0)
  })

  it("projects Factory Run evidence and emits exact run requests", async () => {
    const run: FactoryRunDto = {
      id: "run-1",
      targetAgentId: "target-1",
      agentDraftId: "draft-1",
      workspaceBindingId: "binding-1",
      projectId: "project-1",
      environmentId: "default",
      objective: "Build a refund agent",
      acceptanceCriteria: ["Refund checks pass"],
      startingGitHead: "0123456789abcdef",
      finalGitHead: "fedcba9876543210",
      completedAtUnixMs: 42,
      state: "needs_review",
      changedFiles: [
        {
          path: "src/agent.ts",
          change: "modified",
          beforeHash: "before",
          afterHash: "after",
          diff: {
            hunks: [
              {
                oldStart: 1,
                oldLines: 1,
                newStart: 1,
                newLines: 1,
                lines: [
                  {
                    kind: "insert",
                    oldLine: null,
                    newLine: 1,
                    text: "export const ready = true",
                  },
                ],
              },
            ],
          },
        },
      ],
      testEvidence: [
        {
          name: "Integration suite",
          status: "passed",
          summary: "All protocol cases passed.",
        },
      ],
      evaluation: {
        verdict: "needs_review",
        summary: "The evaluator response was incomplete.",
        protocolValid: false,
        validationError: "Missing schemaVersion.",
        findings: [
          {
            severity: "major",
            title: "Missing failure recovery",
            evidence: "The process exits without retrying.",
            file: "src/agent.ts",
            line: 12,
          },
        ],
      },
    }
    const snapshot = {
      revision: 7,
      settings: {
        theme: "system",
        nativeNotifications: true,
        layout: { inspectorPercent: 28, terminalPercent: 24 },
      },
      activeProjectId: "project-1",
      activeAgentSessionId: "coding-1",
      activeRunId: "run-1",
      projects: [
        {
          id: "project-1",
          name: "Agent",
          root: "/tmp/agent",
          trusted: true,
        },
      ],
      environments: [],
      agentSessions: [],
      liveAgents: [],
      harnesses: [],
      herdr: { connected: true, freshness: "live", issues: [] },
      factoryRuns: [run],
      targetWorkspace: targetWorkspaceSnapshot(),
    }
    const invoke = vi.fn(async (_: string, request: RuntimeRequest) => ({
      kind: "response",
      version: 1,
      id: request.id,
      result:
        request.method === "runtime.hello"
          ? {
              protocolVersion: 1,
              runtimeName: "agent-factory-runtime",
              runtimeVersion: "0.1.0",
            }
          : request.method === "snapshot.get"
            ? snapshot
            : request.method === "harness.list"
              ? { agents: [] }
              : { revision: 7 },
    }))
    const bridge: NativeSdkBridge = {
      invoke,
      on: vi.fn(() => () => undefined),
    }
    vi.stubGlobal("window", { zero: bridge })
    vi.stubGlobal("crypto", { randomUUID: () => "request" })
    const client = new BrowserRuntimeClient()
    await client.connect()

    expect(client.getSnapshot().factoryRuns[0]).toEqual(
      expect.objectContaining({
        id: "run-1",
        finalGitHead: run.finalGitHead,
        completedAtUnixMs: run.completedAtUnixMs,
        changedFiles: run.changedFiles,
        testEvidence: run.testEvidence,
        evaluation: run.evaluation,
      }),
    )

    // The Orchestrator advances its own Run through the agent control socket,
    // so stopping it is the only Run request the UI still makes.
    await client.dispatch({ type: "run.cancel", runId: "run-1" })

    const runRequests = invoke.mock.calls
      .map(([, request]) => request)
      .filter((request) => request.method.startsWith("run."))
      .map((request) => ({ method: request.method, params: request.params }))
    expect(runRequests).toEqual([
      { method: "run.cancel", params: { runId: "run-1" } },
    ])
  })

  it("emits the exact Draft lifecycle and Draft-bound Run requests", async () => {
    const invoke = vi.fn(async (_: string, request: RuntimeRequest) => ({
      kind: "response" as const,
      version: 1 as const,
      id: request.id,
      result: request.method === "runtime.hello"
        ? {
            protocolVersion: 1,
            runtimeName: "agent-factory-runtime",
            runtimeVersion: "0.1.0",
          }
        : request.method === "snapshot.get"
          ? emptyRuntimeSnapshot({
              theme: "system",
              nativeNotifications: true,
              layout: { inspectorPercent: 28, terminalPercent: 24 },
            })
          : request.method === "harness.list"
            ? { herdr: { connected: false, freshness: "last_observed", issues: [] }, harnesses: [] }
            : {},
    }))
    const bridge: NativeSdkBridge = {
      invoke,
      on: vi.fn(() => () => undefined),
    }
    vi.stubGlobal("window", { zero: bridge })
    vi.stubGlobal("crypto", { randomUUID: () => "request" })
    const client = new BrowserRuntimeClient()
    await client.connect()

    await client.dispatch({
      type: "agentDraft.update",
      agentDraftId: "draft-1",
      name: "Agent",
      objective: "Objective",
      acceptanceCriteria: ["Criterion"],
      trusted: true,
    })
    await client.dispatch({
      type: "agentDraft.create",
      targetAgentId: "agent-1",
      baseVersionId: "version-1",
      draftName: "maintenance",
    })
    await client.dispatch({
      type: "agentDraft.publish",
      agentDraftId: "draft-1",
      bump: "patch",
      confirmWithoutPassingRun: true,
    })
    await client.dispatch({ type: "agentDraft.discard", agentDraftId: "draft-2" })
    await client.dispatch({
      type: "targetAgent.remove",
      targetAgentId: "agent-1",
    })
    await client.dispatch({
      type: "factoryRun.create",
      runId: "run-1",
      agentDraftId: "draft-1",
      environmentId: "default",
    })

    expect(invoke.mock.calls.map(([, request]) => request)
      .filter((request) => request.method.startsWith("agentDraft.") ||
        request.method === "targetAgent.remove" ||
        request.method === "factoryRun.create")
      .map((request) => ({ method: request.method, params: request.params })))
      .toEqual([
        {
          method: "agentDraft.update",
          params: {
            agentDraftId: "draft-1",
            name: "Agent",
            objective: "Objective",
            acceptanceCriteria: ["Criterion"],
            trusted: true,
          },
        },
        {
          method: "agentDraft.create",
          params: {
            targetAgentId: "agent-1",
            baseVersionId: "version-1",
            draftName: "maintenance",
          },
        },
        {
          method: "agentDraft.publish",
          params: {
            agentDraftId: "draft-1",
            bump: "patch",
            confirmWithoutPassingRun: true,
          },
        },
        { method: "agentDraft.discard", params: { agentDraftId: "draft-2" } },
        {
          method: "targetAgent.remove",
          params: { targetAgentId: "agent-1" },
        },
        {
          method: "factoryRun.create",
          params: {
            runId: "run-1",
            agentDraftId: "draft-1",
            environmentId: "default",
          },
        },
        {
          method: "agentDraft.openWorkspace",
          params: { agentDraftId: "draft-1" },
        },
      ])
  })

  it("keeps Version file requests scoped to versionId and repository paths", async () => {
    const invoke = vi.fn((_: string, request: RuntimeRequest) => ({
      kind: "response",
      version: 1,
      id: request.id,
      result: request.method === "version.files.list"
        ? {
            versionId: "version-1",
            gitCommit: "abcdef",
            entries: [{ path: "README.md", kind: "file", size: 12 }],
          }
        : {
            versionId: "version-1",
            gitCommit: "abcdef",
            path: "README.md",
            size: 12,
            kind: "text",
            content: "Version text",
          },
    }))
    vi.stubGlobal("window", {
      zero: {
        invoke,
        on: vi.fn(() => () => undefined),
      },
    })
    vi.stubGlobal("crypto", { randomUUID: () => "request" })
    const client = new BrowserRuntimeClient()

    await client.listVersionFiles("version-1")
    await client.readVersionFile("version-1", "README.md")

    expect(invoke.mock.calls.map(([, request]) => ({
      method: request.method,
      params: request.params,
    }))).toEqual([
      { method: "version.files.list", params: { versionId: "version-1" } },
      {
        method: "version.file.read",
        params: { versionId: "version-1", path: "README.md" },
      },
    ])
  })

  it("opens the Draft terminal on Start and closes it after cancellation", async () => {
    const invoke = vi.fn(async (command: string, payload: unknown) => {
      if (command === "desktop.terminal.show.v1") {
        return { version: 1, visible: true }
      }
      if (command === "desktop.terminal.hide.v1") {
        return { version: 1, visible: false }
      }
      const request = payload as RuntimeRequest
      return {
        kind: "response" as const,
        version: 1 as const,
        id: request.id,
        result: request.method === "runtime.hello"
          ? {
              protocolVersion: 1,
              runtimeName: "agent-factory-runtime",
              runtimeVersion: "0.1.0",
            }
          : request.method === "snapshot.get"
            ? emptyRuntimeSnapshot({
                theme: "system",
                nativeNotifications: true,
                layout: { inspectorPercent: 28, terminalPercent: 24 },
              })
            : request.method === "harness.list"
              ? {
                  herdr: {
                    connected: true,
                    freshness: "live",
                    issues: [],
                  },
                  harnesses: [],
                }
              : request.method === "secret.list"
                ? { secrets: [] }
                : request.method === "agentDraft.openWorkspace"
                  ? {
                      agentDraftId: "22222222-2222-4222-8222-222222222222",
                      workspaceId: "w7",
                      label: "Weather Reporter / main",
                      alreadyOpen: true,
                      terminal: {
                        executable: "/opt/homebrew/bin/herdr",
                        arguments: ["--session", "agent-factory-dev"],
                      },
                      revision: 1,
                    }
                  : {},
      }
    })
    const bridge: NativeSdkBridge = {
      invoke: invoke as NativeSdkBridge["invoke"],
      on: vi.fn(() => () => undefined),
    }
    vi.stubGlobal("window", { zero: bridge })
    vi.stubGlobal("crypto", { randomUUID: () => "request" })
    const client = new BrowserRuntimeClient()
    const visibilityListener = vi.fn()
    client.subscribeNativeTerminalVisibility(visibilityListener)
    await client.connect()

    await client.dispatch({
      type: "factoryRun.create",
      runId: "11111111-1111-4111-8111-111111111111",
      agentDraftId: "22222222-2222-4222-8222-222222222222",
      environmentId: "default",
    })

    expect(invoke).toHaveBeenCalledWith("desktop.terminal.show.v1", {
      executable: "/opt/homebrew/bin/herdr",
      arguments: ["--session", "agent-factory-dev"],
      workspaceId: "w7",
      label: "Weather Reporter / main",
    })
    expect(client.getNativeTerminalVisibility()).toBe(true)
    expect(visibilityListener).toHaveBeenCalledOnce()

    await client.dispatch({
      type: "run.cancel",
      runId: "11111111-1111-4111-8111-111111111111",
    })

    expect(invoke).toHaveBeenCalledWith("desktop.terminal.hide.v1", null)
    expect(client.getNativeTerminalVisibility()).toBe(false)
    expect(visibilityListener).toHaveBeenCalledTimes(2)
    expect(invoke.mock.calls.filter(([, payload]) =>
      (payload as RuntimeRequest | null)?.method ===
        "agentDraft.openWorkspace")).toHaveLength(1)

    await client.dispatch({
      type: "agentDraft.toggleWorkspace",
      agentDraftId: "22222222-2222-4222-8222-222222222222",
    })
    await client.dispatch({
      type: "agentDraft.toggleWorkspace",
      agentDraftId: "22222222-2222-4222-8222-222222222222",
    })
    expect(invoke.mock.calls.filter(([, payload]) =>
      (payload as RuntimeRequest | null)?.method ===
        "agentDraft.openWorkspace")).toHaveLength(2)
  })
})

function emptyRuntimeSnapshot(settings: WorkspaceSettingsProjection) {
  return {
    revision: 1,
    settings,
    activeProjectId: null,
    activeAgentSessionId: null,
    activeRunId: null,
    projects: [],
    environments: [],
    agentSessions: [],
    liveAgents: [],
    harnesses: [],
    herdr: { connected: true, freshness: "live", issues: [] },
    factoryRuns: [],
    targetWorkspace: {
      targetGroups: [],
      workContexts: [],
      panes: [],
      terminals: [],
    },
  }
}

function targetWorkspaceSnapshot() {
  return {
    targetGroups: [],
    workContexts: [],
    panes: [],
    terminals: [],
  }
}

function notificationEvent(sequence: number) {
  return {
    kind: "event" as const,
    version: 1,
    sequence,
    revision: 1,
    topic: "notification.requested" as const,
    payload: {
      category: "factory_run_passed" as const,
      entityId: "run-1",
      title: "Factory Run passed",
      body: "All acceptance criteria passed.",
    },
  }
}

function nativeNotificationCalls(invoke: ReturnType<typeof vi.fn>) {
  return invoke.mock.calls
    .filter(([command]) => command === "native-sdk.os.showNotification")
    .map(([, payload]) => payload)
}

function setVisibility(value: "visible" | "hidden") {
  Object.defineProperty(document, "visibilityState", {
    configurable: true,
    value,
  })
}
