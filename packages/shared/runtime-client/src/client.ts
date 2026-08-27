import type {
  DirectoryPageDto,
  FactoryRunProjection,
  AgentDraftWorkspaceResultDto,
  FileReadDto,
  NativeSdkBridge,
  NativeTerminalVisibilityEventV1,
  NotificationRequestedDto,
  PluginDetailsDto,
  PluginListDto,
  PluginRegistryListDto,
  RegistryCatalogDto,
  RuntimeHelloDto,
  RuntimeEvent,
  RuntimeIntent,
  RuntimeMethod,
  RuntimeRequest,
  RuntimeResponse,
  RuntimeSnapshotDto,
  SecretListDto,
  SessionProjection,
  SettingsResultDto,
  StructuredDiffDto,
  TerminalCreatedDto,
  TerminalReadDto,
  TerminalProjection,
  WorkspaceProjection,
  EnvironmentCreateResultDto,
  EnvironmentDto,
  EnvironmentsResultDto,
  LlmProviderCreateResultDto,
  LlmProviderModelsDto,
  LlmProvidersResultDto,
  LlmProviderConnectionDto,
  UpdateStatusDto,
  VersionFileReadDto,
  VersionFilesListDto,
  AgentTranscriptDto,
} from "./contracts"
import { runtimeProtocolVersion } from "./contracts"

export const initialProjection: WorkspaceProjection = {
  revision: 0,
  connection: "loading",
  projects: [],
  herdr: {
    connected: false,
    freshness: "reconnecting",
    issues: ["Waiting for the runtime to report Herdr connectivity."],
  },
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
  environments: [],
  llmProviders: [],
  secrets: [],
  pluginRegistries: [],
  pluginCatalogs: [],
  plugins: { installed: [], localMcpServers: [] },
  files: {
    state: "idle",
    entries: [],
  },
}

const bridgeUnavailableProjection: WorkspaceProjection = {
  ...initialProjection,
  connection: "degraded",
  connectionDetail:
    "The Native-SDK runtime bridge is unavailable. Restart the desktop host.",
}

export interface RuntimeClient {
  connect(): Promise<void>
  disconnect(): void
  dispatch(intent: RuntimeIntent): Promise<void>
  listVersionFiles(versionId: string): Promise<VersionFilesListDto>
  readVersionFile(versionId: string, path: string): Promise<VersionFileReadDto>
  readAgentTranscript(agentSessionId: string): Promise<AgentTranscriptDto>
  readAgentScreen(agentSessionId: string): Promise<AgentTranscriptDto>
  writeAgentInput(
    agentSessionId: string,
    input: {
      text?: string
      keys?: readonly string[]
      cols?: number
      rows?: number
    },
  ): Promise<void>
  getSnapshot(): WorkspaceProjection
  subscribe(listener: () => void): () => void
  getNativeTerminalVisibility(): boolean
  subscribeNativeTerminalVisibility(listener: () => void): () => void
  subscribeNotifications(
    listener: (notification: NotificationRequestedDto) => void,
  ): () => void
}

/// Roughly fifteen seconds of waiting, which comfortably covers a debug-profile
/// sidecar start without leaving a genuinely dead runtime spinning forever.
const CONNECT_ATTEMPTS = 12
const SNAPSHOT_WATCHDOG_INTERVAL_MS = 2_000

function connectBackoffMs(attempt: number): number {
  return Math.min(250 * 2 ** (attempt - 1), 2_000)
}

function delay(milliseconds: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, milliseconds))
}

export class BrowserRuntimeClient implements RuntimeClient {
  private projection = initialProjection
  private readonly listeners = new Set<() => void>()
  private readonly notificationListeners = new Set<
    (notification: NotificationRequestedDto) => void
  >()
  private nativeTerminalVisible = false
  private readonly nativeTerminalVisibilityListeners = new Set<() => void>()
  private connectionGeneration = 0
  private lastEventSequence?: number
  private unsubscribeEvent?: () => void
  private unsubscribeCheckUpdates?: () => void
  private unsubscribeNativeTerminalVisibility?: () => void
  private unsubscribeWindowRefresh?: () => void
  private snapshotWatchdog?: ReturnType<typeof setInterval>
  private projectionRefreshRequested = false
  private projectionRefreshRunning = false
  private projectionRefreshReportsErrors = false

  getSnapshot = () => this.projection

  subscribe = (listener: () => void) => {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  getNativeTerminalVisibility = () => this.nativeTerminalVisible

  subscribeNativeTerminalVisibility = (listener: () => void) => {
    this.nativeTerminalVisibilityListeners.add(listener)
    return () => this.nativeTerminalVisibilityListeners.delete(listener)
  }

  subscribeNotifications = (
    listener: (notification: NotificationRequestedDto) => void,
  ) => {
    this.notificationListeners.add(listener)
    return () => this.notificationListeners.delete(listener)
  }

  /// Connect, retrying while the runtime is still coming up.
  ///
  /// The WebView and the Rust sidecar start together, so the first `runtime.hello`
  /// routinely races the sidecar — most visibly under `pnpm native:dev`. Retrying
  /// with backoff turns that race into a short wait instead of a dead end. A
  /// protocol mismatch is not retried: waiting cannot change the answer.
  async connect() {
    const generation = ++this.connectionGeneration
    this.stopSnapshotWatchdog()
    this.unsubscribeEvent?.()
    this.unsubscribeEvent = undefined
    this.unsubscribeCheckUpdates?.()
    this.unsubscribeCheckUpdates = undefined
    this.unsubscribeNativeTerminalVisibility?.()
    this.unsubscribeNativeTerminalVisibility = undefined
    this.lastEventSequence = undefined
    const bridge = this.getBridge()
    if (!bridge) {
      this.replaceProjection(bridgeUnavailableProjection)
      return
    }

    this.replaceProjection({
      ...this.projection,
      connection: "loading",
      secrets: [],
      secretsError: undefined,
    })

    try {
      const hello = await this.helloWithRetry(bridge, generation)
      if (generation !== this.connectionGeneration) return
      if (!hello) return
      if (hello.protocolVersion !== runtimeProtocolVersion) {
        this.reportError("The runtime uses an incompatible protocol version.")
        return
      }
      this.unsubscribeEvent = bridge.on("runtime:event", (detail) => {
        this.handleRuntimeEvent(bridge, generation, detail)
      })
      // The native menu-bar status item emits "desktop:check-updates" when
      // the user chooses Check for Updates… (even with no window visible).
      // The runtime client owns the update-intent contract, so it runs the
      // check and surfaces the result through a native notification.
      this.unsubscribeCheckUpdates = bridge.on(
        "desktop:check-updates",
        () => {
          void this.handleCheckUpdates(bridge)
        },
      )
      this.unsubscribeNativeTerminalVisibility = bridge.on(
        "desktop:terminal-visibility",
        (detail) => {
          if (
            generation === this.connectionGeneration &&
            isNativeTerminalVisibility(detail)
          ) {
            this.replaceNativeTerminalVisibility(detail.visible)
          }
        },
      )
      await Promise.all([
        this.refresh(bridge, generation),
        this.refreshSecrets(bridge, generation),
      ])
      if (generation === this.connectionGeneration) {
        this.startSnapshotWatchdog(bridge, generation)
      }
    } catch (error) {
      if (generation !== this.connectionGeneration) return
      this.reportError(runtimeErrorMessage(error))
    }
  }

  /// Ask the runtime to say hello until it does, or until the budget runs out.
  /// Returns `undefined` when the attempt was superseded or gave up, having
  /// already reported the outcome.
  private async helloWithRetry(
    bridge: NativeSdkBridge,
    generation: number,
  ): Promise<RuntimeHelloDto | undefined> {
    let lastError: unknown
    for (let attempt = 1; attempt <= CONNECT_ATTEMPTS; attempt += 1) {
      if (generation !== this.connectionGeneration) return undefined
      try {
        return await this.invoke<RuntimeHelloDto>(bridge, "runtime.hello", {})
      } catch (error) {
        lastError = error
        if (generation !== this.connectionGeneration) return undefined
        if (attempt === CONNECT_ATTEMPTS) break
        this.replaceProjection({
          ...this.projection,
          connection: "loading",
          connectionDetail: `Waiting for the Rust runtime (attempt ${attempt} of ${CONNECT_ATTEMPTS})…`,
        })
        await delay(connectBackoffMs(attempt))
      }
    }
    if (generation !== this.connectionGeneration) return undefined
    this.reportError(runtimeErrorMessage(lastError))
    return undefined
  }

  /// Start over after a failed connection. The error state is a prompt, not a
  /// dead end.
  async retry() {
    await this.connect()
  }

  disconnect() {
    this.connectionGeneration += 1
    this.stopSnapshotWatchdog()
    this.unsubscribeEvent?.()
    this.unsubscribeEvent = undefined
    this.unsubscribeCheckUpdates?.()
    this.unsubscribeCheckUpdates = undefined
    this.unsubscribeNativeTerminalVisibility?.()
    this.unsubscribeNativeTerminalVisibility = undefined
    this.lastEventSequence = undefined
    this.projectionRefreshRequested = false
    this.projectionRefreshReportsErrors = false
  }

  async listVersionFiles(versionId: string) {
    const bridge = this.getBridge()
    if (!bridge) throw new Error(bridgeUnavailableProjection.connectionDetail)
    return this.invoke<VersionFilesListDto>(bridge, "version.files.list", {
      versionId,
    })
  }

  async readVersionFile(versionId: string, path: string) {
    const bridge = this.getBridge()
    if (!bridge) throw new Error(bridgeUnavailableProjection.connectionDetail)
    return this.invoke<VersionFileReadDto>(bridge, "version.file.read", {
      versionId,
      path,
    })
  }

  async readAgentTranscript(agentSessionId: string) {
    const bridge = this.getBridge()
    if (!bridge) throw new Error(bridgeUnavailableProjection.connectionDetail)
    const result = await this.invoke<{ transcript: AgentTranscriptDto }>(
      bridge,
      "agentSession.transcript",
      { agentSessionId },
    )
    return result.transcript
  }

  async readAgentScreen(agentSessionId: string) {
    const bridge = this.getBridge()
    if (!bridge) throw new Error(bridgeUnavailableProjection.connectionDetail)
    const result = await this.invoke<{ transcript: AgentTranscriptDto }>(
      bridge,
      "agentSession.screen",
      { agentSessionId },
    )
    return result.transcript
  }

  async writeAgentInput(
    agentSessionId: string,
    input: {
      text?: string
      keys?: readonly string[]
      cols?: number
      rows?: number
    },
  ) {
    const bridge = this.getBridge()
    if (!bridge) throw new Error(bridgeUnavailableProjection.connectionDetail)
    await this.invoke(bridge, "agentSession.input", {
      agentSessionId,
      ...(input.text ? { text: input.text } : {}),
      keys: input.keys ? [...input.keys] : [],
      ...(input.cols && input.rows
        ? { cols: input.cols, rows: input.rows }
        : {}),
    })
  }

  async dispatch(intent: RuntimeIntent) {
    if (intent.type === "runtime.retry") {
      await this.connect()
      return
    }

    const bridge = this.getBridge()
    if (!bridge) {
      this.replaceProjection(bridgeUnavailableProjection)
      return
    }

    try {
      if (
        intent.type.startsWith("agentSession.") ||
        intent.type === "workspacePane.openPrimary" ||
        intent.type === "workspacePane.openToSide" ||
        intent.type === "workspacePane.focus"
      ) {
        this.replaceProjection({
          ...this.projection,
        })
      }
      if (intent.type.startsWith("run.")) {
        this.replaceProjection({
          ...this.projection,
          runError: undefined,
        })
      }
      if (intent.type.startsWith("factoryRun.")) {
        this.replaceProjection({
          ...this.projection,
          runError: undefined,
        })
      }
      if (intent.type.startsWith("settings.")) {
        this.replaceProjection({
          ...this.projection,
          settingsError: undefined,
        })
      }
      if (intent.type.startsWith("environment.")) {
        this.replaceProjection({
          ...this.projection,
          environmentError: undefined,
        })
      }
      if (intent.type.startsWith("llmProvider.")) {
        this.replaceProjection({
          ...this.projection,
          llmProviderError: undefined,
        })
      }
      if (intent.type.startsWith("secret.")) {
        this.replaceProjection({
          ...this.projection,
          secretsError: undefined,
        })
      }
      if (
        intent.type.startsWith("registry.") ||
        intent.type.startsWith("plugin.")
      ) {
        this.replaceProjection({
          ...this.projection,
          pluginError: undefined,
        })
      }
      if (intent.type.startsWith("update.")) {
        this.replaceProjection({
          ...this.projection,
          updateError: undefined,
        })
      }
      if (
        intent.type === "targetAgent.create" ||
        intent.type === "targetAgent.remove" ||
        intent.type.startsWith("agentDraft.") ||
        intent.type === "project.trust.set"
      ) {
        this.replaceProjection({
          ...this.projection,
          targetWorkspaceError: undefined,
        })
      }

      if (intent.type === "targetAgent.create") {
        await this.invoke(bridge, "targetAgent.create", {
          name: intent.name,
          objective: intent.objective,
          acceptanceCriteria: intent.acceptanceCriteria,
          repositoryRoot: intent.repositoryRoot,
          draftName: intent.draftName,
          trusted: intent.trusted,
          startRun: intent.startRun,
          environmentId: intent.environmentId,
        })
        await this.refresh(bridge)
        return
      }

      if (intent.type === "targetAgent.remove") {
        await this.invoke(bridge, "targetAgent.remove", {
          targetAgentId: intent.targetAgentId,
        })
        await this.refresh(bridge)
        return
      }

      if (intent.type === "agentDraft.update") {
        await this.invoke(bridge, "agentDraft.update", {
          agentDraftId: intent.agentDraftId,
          name: intent.name,
          objective: intent.objective,
          acceptanceCriteria: intent.acceptanceCriteria,
          trusted: intent.trusted,
        })
        await this.refresh(bridge)
        return
      }

      if (intent.type === "agentDraft.environment.set") {
        await this.invoke(bridge, "agentDraft.environment.set", {
          agentDraftId: intent.agentDraftId,
          ...(intent.environmentId === undefined
            ? {}
            : { environmentId: intent.environmentId }),
        })
        await this.refresh(bridge)
        return
      }

      if (intent.type === "agentDraft.create") {
        await this.invoke(bridge, "agentDraft.create", {
          targetAgentId: intent.targetAgentId,
          ...(intent.baseVersionId
            ? { baseVersionId: intent.baseVersionId }
            : {}),
          draftName: intent.draftName,
        })
        await this.refresh(bridge)
        return
      }

      if (intent.type === "agentDraft.publish") {
        await this.invoke(bridge, "agentDraft.publish", {
          agentDraftId: intent.agentDraftId,
          bump: intent.bump,
          confirmWithoutPassingRun: intent.confirmWithoutPassingRun,
        })
        await this.refresh(bridge)
        return
      }

      if (intent.type === "agentDraft.discard") {
        await this.invoke(bridge, "agentDraft.discard", {
          agentDraftId: intent.agentDraftId,
        })
        await this.refresh(bridge)
        return
      }

      if (intent.type === "agentSession.create") {
        await this.invoke(bridge, "agentSession.create", {
          targetAgentId: intent.targetAgentId,
          workspaceBindingId: intent.workspaceBindingId,
          environmentId: intent.environmentId,
          purpose: intent.purpose,
          model: intent.model,
        })
        await this.refresh(bridge)
        return
      }

      if (intent.type === "agentSession.prompt") {
        await this.invoke(bridge, "agentSession.prompt", {
          agentSessionId: intent.agentSessionId,
          text: intent.body,
        })
        await this.refresh(bridge)
        return
      }

      // Approvals live in the agent's own interface. Herdr reports `blocked`;
      // answering it means sending that interface the keys it expects.
      if (intent.type === "agentSession.sendKeys") {
        await this.invoke(bridge, "agentSession.sendKeys", {
          agentSessionId: intent.agentSessionId,
          keys: [...intent.keys],
        })
        await this.refresh(bridge)
        return
      }

      if (
        intent.type === "agentSession.interrupt" ||
        intent.type === "agentSession.focus" ||
        intent.type === "agentSession.stop"
      ) {
        await this.invoke(bridge, intent.type, {
          agentSessionId: intent.agentSessionId,
        })
        await this.refresh(bridge)
        return
      }

      if (intent.type === "factoryRun.create") {
        await this.invoke(bridge, "factoryRun.create", {
          runId: intent.runId,
          agentDraftId: intent.agentDraftId,
          environmentId: intent.environmentId,
          objective: intent.objective,
        })
        try {
          if (
            !this.nativeTerminalVisible &&
            this.projection.herdr?.freshness === "live"
          ) {
            await this.showNativeTerminal(bridge, intent.agentDraftId)
          }
        } finally {
          await this.refresh(bridge)
        }
        return
      }

      if (intent.type === "factoryRun.cancel") {
        await this.invoke(bridge, "factoryRun.cancel", { runId: intent.runId })
        try {
          await this.hideNativeTerminal(bridge)
        } finally {
          await this.refresh(bridge)
        }
        return
      }

      if (intent.type === "agentDraft.toggleWorkspace") {
        if (this.nativeTerminalVisible) {
          await this.hideNativeTerminal(bridge)
          return
        }
        await this.showNativeTerminal(bridge, intent.agentDraftId)
        await this.refresh(bridge)
        return
      }

      if (
        intent.type === "workspacePane.openPrimary" ||
        intent.type === "workspacePane.openToSide"
      ) {
        await this.invoke(bridge, intent.type, {
          targetAgentId: intent.targetAgentId,
          workspaceBindingId: intent.workspaceBindingId,
          workItemId: intent.workItemId,
          workItemKind: intent.workItemKind,
        })
        await this.refresh(bridge)
        return
      }

      if (
        intent.type === "workspacePane.focus" ||
        intent.type === "workspacePane.close"
      ) {
        await this.invoke(bridge, intent.type, {
          workspacePaneId: intent.workspacePaneId,
        })
        await this.refresh(bridge)
        return
      }

      if (intent.type === "workspacePane.resize") {
        await this.invoke(bridge, "workspacePane.resize", {
          layout: intent.layout,
        })
        await this.refresh(bridge)
        return
      }

      if (intent.type === "workspacePane.move") {
        await this.invoke(bridge, "workspacePane.move", {
          workspacePaneId: intent.workspacePaneId,
          position: intent.position,
        })
        await this.refresh(bridge)
        return
      }

      if (intent.type === "workspacePane.setDock") {
        await this.invoke(bridge, "workspacePane.setDock", {
          workContextId: intent.workContextId,
          dock: intent.dock,
          dockPercent: intent.dockPercent,
        })
        await this.refresh(bridge)
        return
      }

      if (intent.type === "project.create") {
        await this.invoke(bridge, "project.create", {
          name: intent.name,
          root: intent.root,
          trusted: intent.trusted,
        })
        await this.refresh(bridge)
        return
      }

      if (intent.type === "project.trust.set") {
        await this.invoke(bridge, "project.trust.set", {
          projectId: intent.projectId,
          trusted: intent.trusted,
        })
        await this.refresh(bridge)
        return
      }

      if (intent.type === "run.cancel") {
        await this.invoke(bridge, "run.cancel", { runId: intent.runId })
        try {
          await this.hideNativeTerminal(bridge)
        } finally {
          await this.refresh(bridge)
        }
        return
      }

      if (intent.type === "workspaceTerminal.create") {
        const created = await this.invoke<TerminalCreatedDto>(
          bridge,
          "workspaceTerminal.create",
          {
            workContextId: intent.workContextId,
            cols: intent.cols,
            rows: intent.rows,
          },
        )
        const contextTerminals = this.projection.terminals.filter(
          (terminal) => terminal.workContextId === intent.workContextId,
        )
        const terminal: TerminalProjection = {
          id: created.terminalId,
          workContextId: intent.workContextId,
          title: `Terminal ${contextTerminals.length + 1}`,
          state: "running",
          output: "",
          cursor: 0,
          cols: created.cols,
          rows: created.rows,
          truncated: false,
          readerClosed: false,
        }
        const existing = this.projection.terminals.filter(
          (candidate) => Boolean(candidate.id),
        )
        this.replaceProjection({
          ...this.projection,
          activeTerminalId: terminal.id,
          terminals: [...existing, terminal],
        })
        await this.refresh(bridge)
        return
      }

      if (intent.type === "terminal.select") {
        const terminal = this.projection.terminals.find(
          (candidate) => candidate.id === intent.terminalId,
        )
        if (!terminal) return
        this.replaceProjection({
          ...this.projection,
          activeTerminalId: intent.terminalId,
        })
        return
      }

      if (
        intent.type === "workspaceTerminal.write"
      ) {
        await this.invoke(bridge, intent.type, {
          terminalId: intent.terminalId,
          data: intent.data,
        })
        return
      }

      if (
        intent.type === "workspaceTerminal.resize"
      ) {
        await this.invoke(bridge, intent.type, {
          terminalId: intent.terminalId,
          cols: intent.cols,
          rows: intent.rows,
        })
        this.replaceProjection({
          ...this.projection,
          ...updateTerminalState(
            this.projection,
            intent.terminalId,
            (terminal) => ({
              ...terminal,
              cols: intent.cols,
              rows: intent.rows,
            }),
          ),
        })
        return
      }

      if (
        intent.type === "workspaceTerminal.read"
      ) {
        const read = await this.invoke<TerminalReadDto>(
          bridge,
          intent.type,
          {
            terminalId: intent.terminalId,
            cursor: intent.cursor,
            maxBytes: intent.maxBytes,
          },
        )
        this.projectTerminalRead(read)
        return
      }

      if (
        intent.type === "workspaceTerminal.kill"
      ) {
        const killed = await this.invoke<{
          terminalId: string
          exitStatus: { code: number; signal: string | null }
        }>(bridge, intent.type, { terminalId: intent.terminalId })
        this.replaceProjection({
          ...this.projection,
          ...updateTerminalState(
            this.projection,
            intent.terminalId,
            (terminal) => ({
              ...terminal,
              state: "exited",
              readerClosed: true,
              exitStatus: killed.exitStatus,
            }),
          ),
        })
        return
      }

      if (
        intent.type === "workspaceTerminal.close"
      ) {
        await this.invoke(bridge, intent.type, {
          terminalId: intent.terminalId,
        })
        const index = this.projection.terminals.findIndex(
          (terminal) => terminal.id === intent.terminalId,
        )
        if (index < 0) return
        const terminals = this.projection.terminals.filter(
          (terminal) => terminal.id !== intent.terminalId,
        )
        const nextActive =
          terminals.find(
            (terminal) => terminal.id === this.projection.activeTerminalId,
          ) ??
          terminals[index] ??
          terminals[index - 1] ??
          terminals[0]
        this.replaceProjection({
          ...this.projection,
          activeTerminalId: nextActive?.id,
          terminals,
        })
        return
      }

      if (intent.type === "file.list") {
        this.replaceProjection({
          ...this.projection,
          files: {
            ...this.projection.files,
            state: "loading",
            error: undefined,
          },
        })
        const page = await this.invoke<DirectoryPageDto>(
          bridge,
          "file.list",
          {
            path: intent.path,
            cursor: intent.cursor,
            pageSize: intent.pageSize,
          },
        )
        this.replaceProjection({
          ...this.projection,
          files: {
            state: "ready",
            path: page.path,
            entries: intent.cursor
              ? [...this.projection.files.entries, ...page.entries]
              : page.entries,
            nextCursor: page.nextCursor ?? undefined,
          },
        })
        return
      }

      if (intent.type === "file.read") {
        const file = await this.invoke<FileReadDto>(
          bridge,
          "file.read",
          { path: intent.path, maxBytes: intent.maxBytes },
        )
        this.replaceProjection({
          ...this.projection,
          files: {
            ...this.projection.files,
            state: "ready",
            selectedFile: file,
            error: undefined,
          },
        })
        return
      }

      if (intent.type === "file.diff") {
        const diff = await this.invoke<StructuredDiffDto>(
          bridge,
          "file.diff",
          {
            beforePath: intent.beforePath,
            afterPath: intent.afterPath,
            contextLines: intent.contextLines,
          },
        )
        this.replaceProjection({
          ...this.projection,
          files: {
            ...this.projection.files,
            state: "ready",
            diff,
            error: undefined,
          },
        })
        return
      }

      if (intent.type === "settings.theme") {
        const updated = await this.invoke<SettingsResultDto>(
          bridge,
          "settings.setTheme",
          { theme: intent.theme },
        )
        this.replaceProjection({
          ...this.projection,
          revision: updated.revision,
          settings: updated.settings,
          settingsError: undefined,
        })
        return
      }

      if (intent.type === "settings.notifications") {
        const updated = await this.invoke<SettingsResultDto>(
          bridge,
          "settings.setNotifications",
          { enabled: intent.enabled },
        )
        this.replaceProjection({
          ...this.projection,
          revision: updated.revision,
          settings: updated.settings,
          settingsError: undefined,
        })
        return
      }

      if (intent.type === "settings.layout") {
        const updated = await this.invoke<SettingsResultDto>(
          bridge,
          "settings.setLayout",
          {
            inspectorPercent: intent.inspectorPercent,
            terminalPercent: intent.terminalPercent,
          },
        )
        this.replaceProjection({
          ...this.projection,
          revision: updated.revision,
          settings: updated.settings,
          settingsError: undefined,
        })
        return
      }

      if (intent.type === "environment.create") {
        const created = await this.invoke<EnvironmentCreateResultDto>(
          bridge,
          "environment.create",
          { configuration: intent.configuration },
        )
        this.replaceProjection({
          ...applyEnvironmentsResult(this.projection, created),
        })
        return
      }

      if (intent.type === "environment.configuration.set") {
        const result = await this.invoke<EnvironmentsResultDto>(
          bridge,
          "environment.configuration.set",
          {
            environmentId: intent.environmentId,
            configuration: intent.configuration,
          },
        )
        this.replaceProjection(applyEnvironmentsResult(this.projection, result))
        return
      }

      if (intent.type === "environment.delete") {
        const result = await this.invoke<EnvironmentsResultDto>(
          bridge,
          "environment.delete",
          { environmentId: intent.environmentId },
        )
        this.replaceProjection(applyEnvironmentsResult(this.projection, result))
        return
      }

      if (intent.type === "llmProvider.create") {
        const result = await this.invoke<LlmProviderCreateResultDto>(
          bridge,
          "llmProvider.create",
          { configuration: intent.configuration },
        )
        this.replaceProjection({
          ...applyLlmProvidersResult(this.projection, result),
        })
        return
      }

      if (intent.type === "llmProvider.configuration.set") {
        const result = await this.invoke<LlmProvidersResultDto>(
          bridge,
          "llmProvider.configuration.set",
          {
            providerId: intent.providerId,
            configuration: intent.configuration,
          },
        )
        this.replaceProjection(applyLlmProvidersResult(this.projection, result))
        return
      }

      if (intent.type === "llmProvider.delete") {
        const result = await this.invoke<LlmProvidersResultDto>(
          bridge,
          "llmProvider.delete",
          { providerId: intent.providerId },
        )
        this.replaceProjection(applyLlmProvidersResult(this.projection, result))
        return
      }

      if (intent.type === "llmProvider.models.list") {
        const provider = llmProviderConnection(intent.provider)
        const providerKey = llmProviderKey(provider)
        this.replaceProjection({
          ...this.projection,
          llmProviderModelDiscovery: undefined,
          llmProviderError: undefined,
        })
        const discovered = await this.invoke<LlmProviderModelsDto>(
          bridge,
          "llmProvider.models.list",
          { providerId: intent.providerId, provider },
        )
        this.replaceProjection({
          ...this.projection,
          llmProviderModelDiscovery: {
            providerKey,
            models: discovered.models,
          },
          llmProviderError: undefined,
        })
        return
      }

      if (intent.type === "secret.create") {
        const result = await this.invoke<SecretListDto>(
          bridge,
          "secret.create",
          { label: intent.label, value: intent.value },
        )
        this.replaceProjection({
          ...this.projection,
          secrets: result.secrets,
          secretsError: undefined,
        })
        return
      }

      if (intent.type === "secret.list") {
        const result = await this.invoke<SecretListDto>(
          bridge,
          "secret.list",
          {},
        )
        this.replaceProjection({
          ...this.projection,
          secrets: result.secrets,
          secretsError: undefined,
        })
        return
      }

      if (intent.type === "secret.replace") {
        const result = await this.invoke<SecretListDto>(
          bridge,
          "secret.replace",
          { secretRef: intent.secretRef, value: intent.value },
        )
        this.replaceProjection({
          ...this.projection,
          secrets: result.secrets,
          secretsError: undefined,
        })
        return
      }

      if (intent.type === "secret.delete") {
        const result = await this.invoke<SecretListDto>(
          bridge,
          "secret.delete",
          { secretRef: intent.secretRef },
        )
        this.replaceProjection({
          ...this.projection,
          secrets: result.secrets,
          secretsError: undefined,
        })
        return
      }

      if (intent.type === "registry.list") {
        const result = await this.invoke<PluginRegistryListDto>(
          bridge,
          "registry.list",
          {},
        )
        this.replaceProjection({
          ...this.projection,
          pluginRegistries: result.registries,
          pluginError: undefined,
        })
        return
      }

      if (intent.type === "registry.put") {
        const result = await this.invoke<PluginRegistryListDto>(
          bridge,
          "registry.put",
          {
            id: intent.id,
            catalogUrl: intent.catalogUrl,
            signatureUrl: intent.signatureUrl,
            publicKeyBase64: intent.publicKeyBase64,
          },
        )
        this.replaceProjection({
          ...this.projection,
          pluginRegistries: result.registries,
          pluginError: undefined,
        })
        return
      }

      if (intent.type === "registry.delete") {
        const result = await this.invoke<PluginRegistryListDto>(
          bridge,
          "registry.delete",
          { registryId: intent.registryId },
        )
        this.replaceProjection({
          ...this.projection,
          pluginRegistries: result.registries,
          pluginCatalogs: this.projection.pluginCatalogs.filter(
            (catalog) => catalog.registryId !== intent.registryId,
          ),
          pluginError: undefined,
        })
        return
      }

      if (intent.type === "registry.refresh") {
        const result = await this.invoke<RegistryCatalogDto>(
          bridge,
          "registry.refresh",
          { registryId: intent.registryId },
        )
        this.replaceProjection({
          ...this.projection,
          pluginCatalogs: replaceOrAppendCatalog(
            this.projection.pluginCatalogs,
            result,
          ),
          pluginError: undefined,
        })
        return
      }

      if (intent.type === "plugin.list") {
        const result = await this.invoke<PluginListDto>(
          bridge,
          "plugin.list",
          {},
        )
        this.replaceProjection({
          ...this.projection,
          plugins: result,
          pluginError: undefined,
        })
        return
      }

      if (intent.type === "plugin.details") {
        const result = await this.invoke<PluginDetailsDto>(
          bridge,
          "plugin.details",
          { registryId: intent.registryId, pluginId: intent.pluginId },
        )
        this.replaceProjection({
          ...this.projection,
          pluginDetails: result,
          pluginError: undefined,
        })
        return
      }

      if (intent.type === "plugin.install") {
        const result = await this.invoke<PluginListDto>(
          bridge,
          "plugin.install",
          { registryId: intent.registryId, pluginId: intent.pluginId },
        )
        this.replaceProjection({
          ...this.projection,
          plugins: result,
          pluginError: undefined,
        })
        return
      }

      if (intent.type === "plugin.uninstall") {
        const result = await this.invoke<PluginListDto>(
          bridge,
          "plugin.uninstall",
          { pluginName: intent.pluginName },
        )
        this.replaceProjection({
          ...this.projection,
          plugins: result,
          pluginError: undefined,
        })
        return
      }

      if (intent.type === "plugin.rollback") {
        const result = await this.invoke<PluginListDto>(
          bridge,
          "plugin.rollback",
          { pluginName: intent.pluginName },
        )
        this.replaceProjection({
          ...this.projection,
          plugins: result,
          pluginError: undefined,
        })
        return
      }

      if (
        intent.type === "plugin.trustLocalMcp" ||
        intent.type === "plugin.revokeLocalMcp"
      ) {
        const result = await this.invoke<PluginListDto>(
          bridge,
          intent.type,
          {
            environmentId: intent.environmentId,
            pluginName: intent.pluginName,
            serverName: intent.serverName,
            fingerprint: intent.fingerprint,
          },
        )
        this.replaceProjection({
          ...this.projection,
          plugins: result,
          pluginError: undefined,
        })
        return
      }

      if (intent.type === "update.status" || intent.type === "update.check") {
        const result = await this.invoke<UpdateStatusDto>(
          bridge,
          intent.type,
          {},
        )
        this.replaceProjection({
          ...this.projection,
          updateStatus: result,
          updateError: undefined,
        })
        return
      }

      if (intent.type === "update.confirmAndInstall") {
        const result = await this.invoke<UpdateStatusDto>(
          bridge,
          "update.confirmAndInstall",
          { version: intent.version },
        )
        this.replaceProjection({
          ...this.projection,
          updateStatus: result,
          updateError: undefined,
        })
        return
      }

      if (intent.type === "update.rollback") {
        const result = await this.invoke<UpdateStatusDto>(
          bridge,
          "update.rollback",
          {},
        )
        this.replaceProjection({
          ...this.projection,
          updateStatus: result,
          updateError: undefined,
        })
        return
      }

      this.reportError(
        "This action is not available in the current runtime contract.",
      )
    } catch (error) {
      if (
        intent.type === "workspacePane.openPrimary" ||
        intent.type === "workspacePane.openToSide" ||
        intent.type === "workspacePane.focus"
      ) {
        await this.refresh(bridge).catch(() => undefined)
      }
      this.reportIntentError(intent, runtimeErrorMessage(error))
    }
  }

  private getBridge(): NativeSdkBridge | undefined {
    return typeof window === "undefined" ? undefined : window.zero
  }

  private async refresh(
    bridge: NativeSdkBridge,
    generation = this.connectionGeneration,
  ) {
    const snapshot = await this.invoke<RuntimeSnapshotDto>(
      bridge,
      "snapshot.get",
      {},
    )
    if (generation !== this.connectionGeneration) return
    this.replaceProjection(projectSnapshot(snapshot, this.projection))
  }

  private startSnapshotWatchdog(
    bridge: NativeSdkBridge,
    generation: number,
  ) {
    if (
      typeof window === "undefined" ||
      typeof window.addEventListener !== "function"
    ) return

    const requestRefresh = () => {
      if (generation !== this.connectionGeneration) return
      this.queueProjectionRefresh(bridge, generation, false)
    }
    const refreshWhenVisible = () => {
      if (
        typeof document !== "undefined" &&
        document.visibilityState === "hidden"
      ) return
      requestRefresh()
    }

    window.addEventListener("focus", requestRefresh)
    if (typeof document !== "undefined") {
      document.addEventListener("visibilitychange", refreshWhenVisible)
    }
    this.unsubscribeWindowRefresh = () => {
      window.removeEventListener("focus", requestRefresh)
      if (typeof document !== "undefined") {
        document.removeEventListener("visibilitychange", refreshWhenVisible)
      }
    }
    this.snapshotWatchdog = setInterval(
      requestRefresh,
      SNAPSHOT_WATCHDOG_INTERVAL_MS,
    )
  }

  private stopSnapshotWatchdog() {
    if (this.snapshotWatchdog !== undefined) {
      clearInterval(this.snapshotWatchdog)
      this.snapshotWatchdog = undefined
    }
    this.unsubscribeWindowRefresh?.()
    this.unsubscribeWindowRefresh = undefined
  }

  private queueProjectionRefresh(
    bridge: NativeSdkBridge,
    generation: number,
    reportAsError: boolean,
  ) {
    this.projectionRefreshRequested = true
    this.projectionRefreshReportsErrors ||= reportAsError
    if (this.projectionRefreshRunning) return
    this.projectionRefreshRunning = true
    void this.drainProjectionRefreshes(bridge, generation)
  }

  private async drainProjectionRefreshes(
    bridge: NativeSdkBridge,
    generation: number,
  ) {
    try {
      while (
        this.projectionRefreshRequested &&
        generation === this.connectionGeneration
      ) {
        this.projectionRefreshRequested = false
        const reportAsError = this.projectionRefreshReportsErrors
        this.projectionRefreshReportsErrors = false
        try {
          await this.refresh(bridge, generation)
        } catch (error) {
          if (generation !== this.connectionGeneration) return
          const message = runtimeErrorMessage(error)
          if (reportAsError) this.reportError(message)
          else this.reportSnapshotUnavailable(message)
        }
      }
    } finally {
      this.projectionRefreshRunning = false
      if (
        this.projectionRefreshRequested &&
        generation === this.connectionGeneration
      ) {
        this.queueProjectionRefresh(bridge, generation, false)
      }
    }
  }

  /// Secret metadata is backed by the platform credential store rather than
  /// SQLite, so it is loaded beside the durable snapshot at connection time.
  /// Keeping this at the client boundary makes every Settings section see the
  /// same launch state instead of making Providers depend on visiting Secrets.
  private async refreshSecrets(
    bridge: NativeSdkBridge,
    generation = this.connectionGeneration,
  ) {
    try {
      const result = await this.invoke<SecretListDto>(bridge, "secret.list", {})
      if (generation !== this.connectionGeneration) return
      this.replaceProjection({
        ...this.projection,
        secrets: result.secrets,
        secretsError: undefined,
      })
    } catch (error) {
      if (generation !== this.connectionGeneration) return
      this.replaceProjection({
        ...this.projection,
        secretsError: runtimeErrorMessage(error),
      })
    }
  }

  private handleRuntimeEvent(
    bridge: NativeSdkBridge,
    generation: number,
    detail: unknown,
  ) {
    if (generation !== this.connectionGeneration) return
    if (isRuntimeReadyFrame(detail)) return
    if (!isRuntimeEvent(detail)) {
      this.reportError("The native bridge emitted an invalid runtime event.")
      return
    }

    if (
      this.lastEventSequence !== undefined &&
      detail.sequence <= this.lastEventSequence
    ) {
      return
    }

    const hasGap =
      this.lastEventSequence === undefined
        ? detail.sequence !== 1
        : detail.sequence !== this.lastEventSequence + 1
    this.lastEventSequence = detail.sequence

    const notification = decodeNotificationEvent(detail)
    if (notification) {
      this.publishNotification(bridge, notification)
    }
    // Runtime events invalidate a projection. Herdr-only changes do not
    // advance the durable SQLite revision, so every non-notification event
    // obtains a complete joined snapshot instead of applying event payloads.
    if (!hasGap && notification) return
    this.queueProjectionRefresh(bridge, generation, true)
  }

  private publishNotification(
    bridge: NativeSdkBridge,
    notification: NotificationRequestedDto,
  ) {
    this.notificationListeners.forEach((listener) => listener(notification))
    if (
      !this.projection.settings?.nativeNotifications ||
      !documentNeedsNativeNotification()
    ) {
      return
    }
    void bridge
      .invoke("native-sdk.os.showNotification", {
        title: notification.title,
        body: notification.body,
      })
      .catch(() => undefined)
  }

  /// Run a menu-bar-initiated update check and surface the result as a
  /// native notification. This is an explicit user action (the tray's
  /// Check for Updates… item), so the notification is shown even when
  /// native notifications are otherwise disabled — otherwise a hidden
  /// menu-bar app would give no feedback at all. Install confirmation is
  /// intentionally not driven from here: the update contract requires
  /// explicit user confirmation, which belongs in the in-app UI.
  private async handleCheckUpdates(bridge: NativeSdkBridge) {
    await this.dispatch({ type: "update.check" })
    const error = this.projection.updateError
    const status = this.projection.updateStatus
    const notification = updateCheckNotification(error, status)
    void bridge
      .invoke("native-sdk.os.showNotification", {
        title: notification.title,
        body: notification.body,
      })
      .catch(() => undefined)
  }

  private async invoke<Result>(
    bridge: NativeSdkBridge,
    method: RuntimeMethod,
    params: Record<string, unknown>,
  ): Promise<Result> {
    const request: RuntimeRequest = {
      kind: "request",
      version: runtimeProtocolVersion,
      id: crypto.randomUUID(),
      method,
      params,
    }
    const value = await bridge.invoke("runtime.invoke", request)
    if (!isRuntimeResponse(value, request.id)) {
      throw new Error("The native bridge returned an invalid response.")
    }
    if ("error" in value) throw new Error(value.error.message)
    return value.result as Result
  }

  private async showNativeTerminal(
    bridge: NativeSdkBridge,
    agentDraftId: string,
  ) {
    if (this.nativeTerminalVisible) return
    const result = await this.invoke<AgentDraftWorkspaceResultDto>(
      bridge,
      "agentDraft.openWorkspace",
      { agentDraftId },
    )
    const visibility = await bridge.invoke("desktop.terminal.show.v1", {
      executable: result.terminal.executable,
      arguments: result.terminal.arguments,
      workspaceId: result.workspaceId,
      label: result.label,
    })
    if (!isNativeTerminalVisibility(visibility)) {
      throw new Error("The native host returned an invalid terminal state.")
    }
    this.replaceNativeTerminalVisibility(visibility.visible)
  }

  private async hideNativeTerminal(bridge: NativeSdkBridge) {
    if (!this.nativeTerminalVisible) return
    const visibility = await bridge.invoke("desktop.terminal.hide.v1", null)
    if (!isNativeTerminalVisibility(visibility)) {
      throw new Error("The native host returned an invalid terminal state.")
    }
    this.replaceNativeTerminalVisibility(visibility.visible)
  }

  private reportError(message: string) {
    this.replaceProjection({
      ...this.projection,
      connection: "error",
      connectionDetail: message,
    })
  }

  private reportSnapshotUnavailable(message: string) {
    const issue = `Agent Factory could not refresh Herdr state: ${message}`
    this.replaceProjection({
      ...this.projection,
      connection: "degraded",
      connectionDetail: message,
      herdr: {
        ...this.projection.herdr,
        freshness: this.projection.herdr.observedAtUnixMs
          ? "last_observed"
          : "reconnecting",
        issues: this.projection.herdr.issues.includes(issue)
          ? this.projection.herdr.issues
          : [...this.projection.herdr.issues, issue],
      },
    })
  }

  private replaceNativeTerminalVisibility(visible: boolean) {
    if (this.nativeTerminalVisible === visible) return
    this.nativeTerminalVisible = visible
    this.nativeTerminalVisibilityListeners.forEach((listener) => listener())
  }

  private reportIntentError(intent: RuntimeIntent, message: string) {
    if (
      intent.type.startsWith("agentSession.") ||
      intent.type === "workspacePane.openPrimary" ||
      intent.type === "workspacePane.openToSide" ||
      intent.type === "workspacePane.focus"
    ) {
      this.replaceProjection({
        ...this.projection,
      })
      return
    }
    if (intent.type.startsWith("file.")) {
      this.replaceProjection({
        ...this.projection,
        files: {
          ...this.projection.files,
          state: "error",
          error: message,
        },
      })
      return
    }
    if (intent.type === "workspaceTerminal.create") {
      // Create has no terminalId yet. Leave a work-context-scoped failed seed so
      // the panel can show the error instead of a permanent empty state.
      const failed: TerminalProjection = {
        workContextId: intent.workContextId,
        title: "Terminal",
        state: "failed",
        output: message,
        cursor: 0,
        cols: 80,
        rows: 24,
        truncated: false,
        readerClosed: true,
      }
      const retained = this.projection.terminals.filter(
        (terminal) =>
          Boolean(terminal.id) ||
          (terminal.workContextId !== undefined &&
            terminal.workContextId !== intent.workContextId),
      )
      this.replaceProjection({
        ...this.projection,
        terminals: [...retained, failed],
        activeTerminalId: undefined,
      })
      return
    }
    if (
      intent.type.startsWith("terminal.") ||
      intent.type.startsWith("workspaceTerminal.")
    ) {
      const terminalId = "terminalId" in intent ? intent.terminalId : undefined
      this.replaceProjection({
        ...this.projection,
        ...updateTerminalState(
          this.projection,
          terminalId,
          (terminal) => ({ ...terminal, state: "failed" }),
        ),
      })
      return
    }
    if (
      intent.type.startsWith("run.") ||
      intent.type.startsWith("factoryRun.")
    ) {
      this.replaceProjection({
        ...this.projection,
        runError: message,
      })
      return
    }
    if (intent.type.startsWith("settings.")) {
      this.replaceProjection({
        ...this.projection,
        settingsError: message,
      })
      return
    }
    if (intent.type.startsWith("environment.")) {
      this.replaceProjection({
        ...this.projection,
        environmentError: message,
      })
      return
    }
    if (intent.type.startsWith("llmProvider.")) {
      this.replaceProjection({
        ...this.projection,
        llmProviderError: message,
      })
      return
    }
    if (intent.type.startsWith("secret.")) {
      this.replaceProjection({
        ...this.projection,
        secretsError: message,
      })
      return
    }
    if (
      intent.type.startsWith("registry.") ||
      intent.type.startsWith("plugin.")
    ) {
      this.replaceProjection({
        ...this.projection,
        pluginError: message,
      })
      return
    }
    if (intent.type.startsWith("update.")) {
      this.replaceProjection({
        ...this.projection,
        updateError: message,
      })
      return
    }
    if (
      intent.type === "targetAgent.create" ||
      intent.type === "targetAgent.remove" ||
      intent.type.startsWith("agentDraft.") ||
      intent.type === "project.trust.set"
    ) {
      this.replaceProjection({
        ...this.projection,
        targetWorkspaceError: message,
      })
      return
    }
    this.reportError(message)
  }

  private projectTerminalRead(read: TerminalReadDto) {
    const terminal = this.projection.terminals.find(
      (candidate) => candidate.id === read.terminalId,
    )
    if (!terminal) return
    const decoded = decodeBase64Utf8(read.dataBase64)
    const canAppend =
      !read.truncated && read.startCursor === terminal.cursor
    const output = canAppend ? terminal.output + decoded : decoded
    this.replaceProjection({
      ...this.projection,
      ...updateTerminalState(
        this.projection,
        read.terminalId,
        (current) => ({
          ...current,
          state: read.exitStatus ? "exited" : current.state,
          output,
          cursor: read.nextCursor,
          truncated: read.truncated,
          readerClosed: read.readerClosed,
          exitStatus: read.exitStatus ?? undefined,
        }),
      ),
    })
  }

  private replaceProjection(projection: WorkspaceProjection) {
    this.projection = projection
    this.listeners.forEach((listener) => listener())
  }
}

function isRuntimeResponse(
  value: unknown,
  requestId: string,
): value is RuntimeResponse {
  if (!value || typeof value !== "object") return false
  const response = value as Partial<RuntimeResponse>
  return (
    response.kind === "response" &&
    response.version === runtimeProtocolVersion &&
    response.id === requestId
  )
}

function updateTerminalState(
  projection: WorkspaceProjection,
  terminalId: string | undefined,
  update: (terminal: TerminalProjection) => TerminalProjection,
): Pick<WorkspaceProjection, "terminals"> {
  const targetId = terminalId ?? projection.activeTerminalId
  if (!targetId) return { terminals: projection.terminals }
  return {
    terminals: projection.terminals.map((terminal) =>
      terminal.id === targetId ? update(terminal) : terminal,
    ),
  }
}

function isRuntimeEvent(value: unknown): value is RuntimeEvent {
  if (!value || typeof value !== "object") return false
  const event = value as Partial<RuntimeEvent>
  return (
    hasOnlyKeys(value as Record<string, unknown>, [
      "kind",
      "version",
      "sequence",
      "revision",
      "topic",
      "payload",
    ]) &&
    event.kind === "event" &&
    event.version === runtimeProtocolVersion &&
    typeof event.sequence === "number" &&
    Number.isSafeInteger(event.sequence) &&
    event.sequence >= 0 &&
    typeof event.revision === "number" &&
    Number.isSafeInteger(event.revision) &&
    event.revision >= 0 &&
    typeof event.topic === "string" &&
    "payload" in event
  )
}

function isRuntimeReadyFrame(value: unknown) {
  const frame = recordValue(value)
  return frame?.kind === "ready" && frame.version === runtimeProtocolVersion
}

function projectSnapshot(
  snapshot: RuntimeSnapshotDto,
  previous: WorkspaceProjection,
): WorkspaceProjection {
  const projects = snapshot.projects

  const sessions = snapshot.agentSessions.map(projectSession)
  const factoryRuns = snapshot.factoryRuns.map(projectFactoryRun)
  const environments = snapshot.environments
  const terminals = snapshot.targetWorkspace.terminals.map((descriptor) => {
    const live = previous.terminals.find(
      (terminal) => terminal.id === descriptor.id,
    )
    const state =
      descriptor.state === "exited" ? "exited" : live?.state ?? "running"
    return {
      ...(live ?? {
        id: descriptor.id,
        output: "",
        cursor: 0,
        cols: 80,
        rows: 24,
        truncated: false,
      }),
      id: descriptor.id,
      workContextId: descriptor.workContextId,
      title: descriptor.title,
      state,
      readerClosed: state === "exited" ? true : live?.readerClosed ?? false,
    } satisfies TerminalProjection
  })
  const activeTerminal =
    terminals.find((terminal) => terminal.id === previous.activeTerminalId) ??
    terminals[0]

  return {
    ...previous,
    revision: snapshot.revision,
    connection: "ready",
    connectionDetail: undefined,
    projects,
    herdr: snapshot.herdr,
    harnesses: snapshot.harnesses,
    sessions,
    liveAgents: snapshot.liveAgents,
    targetWorkspace: snapshot.targetWorkspace,
    targetWorkspaceError: undefined,
    terminals,
    activeTerminalId: activeTerminal?.id,
    factoryRuns,
    runError: undefined,
    settings: snapshot.settings,
    settingsError: undefined,
    environments,
    llmProviders: snapshot.llmProviders,
    environmentError: undefined,
    secrets: previous.secrets,
    secretsError: undefined,
  }
}

function projectSession(
  session: RuntimeSnapshotDto["agentSessions"][number],
): SessionProjection {
  return {
    id: session.id,
    projectId: session.projectId,
    environmentId: session.environmentId,
    targetAgentId: session.targetAgentId,
    workspaceBindingId: session.workspaceBindingId,
    factoryRunId: session.factoryRunId ?? undefined,
    parentSessionId: session.parentSessionId ?? undefined,
    harnessId: session.harnessId,
    herdrAgentName: session.herdrAgentName,
    purpose: session.purpose,
    availability: session.availability,
    lifecycle: session.lifecycle ?? undefined,
    title: session.title,
    paneId: session.placement?.paneId,
    agentName: session.placement?.agentName,
    attention: session.attention,
    outcome: session.outcome ?? undefined,
    llmProviderSnapshot: session.llmProviderSnapshot ?? undefined,
    effectiveModel: session.effectiveModel,
    initialPrompt: session.initialPrompt ?? undefined,
    briefDelivered: session.briefDelivered ?? false,
    createdAtUnixMs: session.createdAtUnixMs,
    lastActivityAtUnixMs: session.lastActivityAtUnixMs,
  }
}


function projectFactoryRun(
  run: RuntimeSnapshotDto["factoryRuns"][number],
): FactoryRunProjection {
  return {
    id: run.id,
    targetAgentId: run.targetAgentId,
    agentDraftId: run.agentDraftId,
    workspaceBindingId: run.workspaceBindingId,
    projectId: run.projectId,
    environmentId: run.environmentId,
    objective: run.objective,
    state: normalizeFactoryRunState(run.state),
    acceptanceCriteria: run.acceptanceCriteria,
    startingGitHead: run.startingGitHead,
    finalGitHead: run.finalGitHead ?? undefined,
    completedAtUnixMs: run.completedAtUnixMs ?? undefined,
    changedFiles: run.changedFiles,
    testEvidence: run.testEvidence,
    evaluation: run.evaluation ?? undefined,
    escalation: run.escalation ?? undefined,
  }
}

function normalizeFactoryRunState(
  state: string,
): FactoryRunProjection["state"] {
  const states: FactoryRunProjection["state"][] = [
    "draft",
    "orchestrating",
    "coding",
    "evaluating",
    "escalated",
    "passed",
    "failed",
    "needs_review",
    "cancelled",
  ]
  return states.includes(state as FactoryRunProjection["state"])
    ? (state as FactoryRunProjection["state"])
    : "needs_review"
}

function runtimeErrorMessage(error: unknown) {
  return error instanceof Error ? error.message : "The Rust runtime did not respond."
}

function decodeBase64Utf8(encoded: string) {
  const binary = globalThis.atob(encoded)
  const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0))
  return new TextDecoder().decode(bytes)
}

function recordValue(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : undefined
}

function isNativeTerminalVisibility(
  value: unknown,
): value is NativeTerminalVisibilityEventV1 {
  const record = recordValue(value)
  return Boolean(
    record &&
      hasOnlyKeys(record, ["version", "visible"]) &&
      record.version === 1 &&
      typeof record.visible === "boolean",
  )
}

function stringValue(value: unknown) {
  return typeof value === "string" ? value : undefined
}

/// Identifies an LLM provider for the purpose of caching its model catalog.
/// Two providers that differ in any of these fields expose different models, so
/// a discovery result must not be read across a change to any of them.
export function replaceOrAppendCatalog(
  catalogs: readonly RegistryCatalogDto[],
  catalog: RegistryCatalogDto,
): readonly RegistryCatalogDto[] {
  const index = catalogs.findIndex(
    (candidate) => candidate.registryId === catalog.registryId,
  )
  if (index === -1) return [...catalogs, catalog]
  const updated = [...catalogs]
  updated[index] = catalog
  return updated
}

export function llmProviderConnection(
  provider: LlmProviderConnectionDto,
): LlmProviderConnectionDto {
  return {
    type: provider.type,
    endpoint: provider.endpoint,
    credentialRef: provider.credentialRef,
  }
}

export function llmProviderKey(provider: LlmProviderConnectionDto) {
  return [provider.type, provider.endpoint, provider.credentialRef ?? ""].join(
    "|",
  )
}

function applyLlmProvidersResult(
  projection: WorkspaceProjection,
  result: LlmProvidersResultDto,
): WorkspaceProjection {
  return {
    ...projection,
    revision: result.revision,
    llmProviders: result.providers,
    environments: result.environments,
    llmProviderError: undefined,
    environmentError: undefined,
  }
}

/// Applies the full Environment list returned by every mutation.
function applyEnvironmentsResult(
  projection: WorkspaceProjection,
  result: {
    revision: number
    environments: readonly EnvironmentDto[]
  },
): WorkspaceProjection {
  return {
    ...projection,
    revision: result.revision,
    environments: result.environments,
    environmentError: undefined,
    pluginError: undefined,
  }
}

function decodeNotificationEvent(
  event: RuntimeEvent,
): NotificationRequestedDto | undefined {
  if (event.topic !== "notification.requested") return undefined
  const payload = recordValue(event.payload)
  if (
    !payload ||
    !hasOnlyKeys(payload, ["category", "entityId", "title", "body"])
  ) {
    return undefined
  }
  const category = notificationCategory(payload.category)
  const entityId = stringValue(payload.entityId)
  const title = stringValue(payload.title)
  const body = stringValue(payload.body)
  if (
    !category ||
    entityId === undefined ||
    title === undefined ||
    body === undefined
  ) {
    return undefined
  }
  return { category, entityId, title, body }
}

function documentNeedsNativeNotification() {
  if (typeof document === "undefined") return false
  return document.visibilityState === "hidden" || !document.hasFocus()
}

function updateCheckNotification(
  error: string | undefined,
  status: UpdateStatusDto | undefined,
): { title: string; body: string } {
  if (error) {
    return {
      title: "Update check failed",
      body: error,
    }
  }
  if (!status) {
    return {
      title: "Update check unavailable",
      body: "The runtime did not report an update status.",
    }
  }
  if (!status.enabled) {
    return {
      title: "Updates unavailable",
      body: "Updates aren't configured in this build.",
    }
  }
  switch (status.state) {
    case "available":
    case "awaiting_confirmation":
      return {
        title: "Update available",
        body: `Agent Factory ${status.targetVersion ?? "a new version"} is available. Open Agent Factory to install.`,
      }
    case "idle":
      return {
        title: "You're up to date",
        body: `Agent Factory ${status.currentVersion} is up to date.`,
      }
    case "failed":
      return {
        title: "Update check failed",
        body: status.message ?? "The update check could not complete.",
      }
    default:
      return {
        title: "Update check",
        body: `Update status: ${status.state}.`,
      }
  }
}

function hasOnlyKeys(
  record: Record<string, unknown>,
  expectedKeys: readonly string[],
) {
  const keys = Object.keys(record)
  return (
    keys.length === expectedKeys.length &&
    keys.every((key) => expectedKeys.includes(key))
  )
}

function notificationCategory(value: unknown) {
  const categories = [
    "session_completed",
    "session_failed",
    "factory_run_passed",
    "factory_run_failed",
    "factory_run_needs_review",
  ] as const
  return categories.find((category) => category === value)
}
