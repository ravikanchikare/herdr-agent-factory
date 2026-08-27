import { runtimeProtocolVersion } from "./generated/runtime-ipc"
import type {
  AgentLifecycle as GeneratedAgentLifecycle,
  AgentDraftProjection,
  AgentSessionProjection as AgentSessionDto,
  AgentTranscriptProjection as AgentTranscriptDto,
  ApplicationProjection as RuntimeSnapshotDto,
  ChangedFile as ChangedFileDto,
  CreateEnvironmentParams as CreateEnvironmentParamsDto,
  DiffHunk as DiffHunkDto,
  DiffLine as DiffLineDto,
  DirectoryEntry as DirectoryEntryDto,
  DirectoryPage as DirectoryPageDto,
  EntryKind as DirectoryEntryKind,
  Event as GeneratedEvent,
  EvaluationResult as EvaluationResultDto,
  FactoryRun as FactoryRunDto,
  AgentDraftWorkspaceResultDto,
  FactoryRunState as GeneratedFactoryRunState,
  FileRead as FileReadDto,
  Frame as GeneratedFrame,
  ProjectCreateResultDto,
  NotificationRequestedDto,
  NotificationRequestedEventDto,
  PluginDetailsDto,
  PluginListDto,
  PluginRegistryDto,
  PluginRegistryListDto,
  RegistryCatalogDto,
  ProjectProjection,
  RuntimeHelloDto,
  RuntimeMethod,
  RunIdParams as RunIdParamsDto,
  SecretListDto,
  SecretMetadataDto,
  SetNotificationsParams as SetNotificationsParamsDto,
  SetLayoutParams as SetLayoutParamsDto,
  SetThemeParams as SetThemeParamsDto,
  HarnessListDto,
  HarnessProjection as HarnessDto,
  HarnessPurpose,
  HerdrStatusProjection as HerdrStatusDto,
  LiveAgentProjection as LiveAgentDto,
  ManagedSessionOutcome as ManagedSessionOutcomeDto,
  SessionAvailability as GeneratedSessionAvailability,
  TargetWorkspaceProjection,
  TargetAgentProjection,
  TargetAgentVersionProjection,
  VersionBump,
  TargetAgentWorkGroupProjection,
  TargetWorkItemProjection,
  TargetWorkItemKind,
  WorkspaceBindingProjection,
  WorkContextProjection,
  WorkspacePaneProjection,
  WorkspaceTerminalProjection,
  WorkspaceDock,
  SettingsProjection as SettingsDto,
  SettingsResultDto,
  StructuredDiff as StructuredDiffDto,
  TerminalCreated as TerminalCreatedDto,
  TerminalExit as TerminalExitDto,
  TerminalRead as TerminalReadDto,
  TestEvidence as TestEvidenceDto,
  ThemePreference as GeneratedThemePreference,
  EnvironmentProjection as EnvironmentDto,
  EnvironmentConfigurationDraft as EnvironmentConfigurationDraftDto,
  EnvironmentVariableProjection as EnvironmentVariableDto,
  EnvironmentLlmPolicyDto,
  EnvironmentCreateResultDto,
  UpdateStatusDto,
  EnvironmentsResultDto,
  EnvironmentPermissionProjection,
  EnvironmentPluginProjection,
  LlmProviderConfigurationDto,
  LlmProviderConnectionDto,
  LlmProviderCreateResultDto,
  LlmProviderModelsDto,
  LlmProviderDto,
  LlmProvidersResultDto,
  LlmProviderType,
  ResolvedLlmProviderDto,
  VersionFileEntryDto,
  VersionFileEntryKindDto,
  VersionFileReadDto,
  VersionFileReadKindDto,
  VersionFilesListDto,
} from "./generated/runtime-ipc"

export { runtimeProtocolVersion }
export type {
  AgentSessionDto,
  AgentTranscriptDto,
  ChangedFileDto,
  CreateEnvironmentParamsDto,
  DiffHunkDto,
  DiffLineDto,
  DirectoryEntryDto,
  DirectoryEntryKind,
  DirectoryPageDto,
  EvaluationResultDto,
  FactoryRunDto,
  AgentDraftWorkspaceResultDto,
  FileReadDto,
  ProjectCreateResultDto,
  NotificationRequestedDto,
  NotificationRequestedEventDto,
  PluginDetailsDto,
  PluginListDto,
  PluginRegistryDto,
  PluginRegistryListDto,
  RegistryCatalogDto,
  ProjectProjection,
  RuntimeHelloDto,
  RuntimeMethod,
  RuntimeSnapshotDto,
  RunIdParamsDto,
  SecretListDto,
  SecretMetadataDto,
  SetNotificationsParamsDto,
  SetLayoutParamsDto,
  SetThemeParamsDto,
  HarnessDto,
  HarnessListDto,
  HarnessPurpose,
  HerdrStatusDto,
  LiveAgentDto,
  ManagedSessionOutcomeDto,
  TargetWorkspaceProjection,
  TargetAgentProjection,
  TargetAgentVersionProjection,
  AgentDraftProjection,
  VersionBump,
  TargetAgentWorkGroupProjection,
  TargetWorkItemProjection,
  TargetWorkItemKind,
  WorkspaceBindingProjection,
  WorkContextProjection,
  WorkspacePaneProjection,
  WorkspaceTerminalProjection,
  WorkspaceDock,
  SettingsDto,
  SettingsResultDto,
  StructuredDiffDto,
  TerminalCreatedDto,
  TerminalExitDto,
  TerminalReadDto,
  TestEvidenceDto,
  EnvironmentDto,
  EnvironmentConfigurationDraftDto,
  EnvironmentVariableDto,
  EnvironmentLlmPolicyDto,
  EnvironmentCreateResultDto,
  UpdateStatusDto,
  EnvironmentsResultDto,
  EnvironmentPermissionProjection,
  EnvironmentPluginProjection,
  LlmProviderConfigurationDto,
  LlmProviderConnectionDto,
  LlmProviderCreateResultDto,
  LlmProviderModelsDto,
  LlmProviderDto,
  LlmProvidersResultDto,
  LlmProviderType,
  ResolvedLlmProviderDto,
  VersionFileEntryDto,
  VersionFileEntryKindDto,
  VersionFileReadDto,
  VersionFileReadKindDto,
  VersionFilesListDto,
}

export type RuntimeRequest = Extract<GeneratedFrame, { kind: "request" }> & {
  method: RuntimeMethod
}
export type RuntimeResponse = Extract<GeneratedFrame, { kind: "response" }>
export type RuntimeEvent = GeneratedEvent & { kind: "event" }

export type RuntimeConnectionState =
  | "loading"
  | "ready"
  | "degraded"
  | "error"

/// The lifecycle Herdr reports for a session's agent.
export type AgentLifecycle = GeneratedAgentLifecycle

export type SessionAvailability = GeneratedSessionAvailability

export type SessionPurpose = HarnessPurpose

export type FactoryRunState = GeneratedFactoryRunState

export type ThemePreference = GeneratedThemePreference

/// A Harness is an agent kind Herdr can launch. Availability comes from Herdr.
export type HarnessProjection = HarnessDto

/// Whether Herdr itself is reachable, and on a protocol this build supports.
export type HerdrStatusProjection = HerdrStatusDto

/// Durable managed-session identity joined with the latest Herdr observation.
export interface SessionProjection {
  id: string
  projectId: string
  environmentId: string
  targetAgentId: string
  workspaceBindingId: string
  factoryRunId?: string
  parentSessionId?: string
  title: string
  purpose: SessionPurpose
  harnessId: string
  herdrAgentName: string
  availability: SessionAvailability
  lifecycle?: AgentLifecycle
  paneId?: string
  agentName?: string
  /// What Herdr says the agent is waiting on while it is blocked.
  attention: readonly string[]
  outcome?: ManagedSessionOutcomeDto
  llmProviderSnapshot?: ResolvedLlmProviderDto
  effectiveModel?: AgentSessionDto["effectiveModel"]
  /// First prompt sent to this session. Later prompts do not replace it.
  initialPrompt?: string
  briefDelivered: boolean
  createdAtUnixMs: number
  lastActivityAtUnixMs: number
}

export interface FactoryRunProjection {
  id: string
  targetAgentId: string
  agentDraftId: string
  workspaceBindingId: string
  projectId: string
  environmentId: string
  objective: string
  state: FactoryRunState
  acceptanceCriteria: readonly string[]
  startingGitHead: string
  finalGitHead?: string
  completedAtUnixMs?: number
  changedFiles: readonly ChangedFileDto[]
  testEvidence: readonly TestEvidenceDto[]
  evaluation?: EvaluationResultDto
  /// What the Orchestrator stopped to ask a person, when it could not decide.
  escalation?: string
}

export interface TerminalProjection {
  id?: string
  workContextId?: string
  title: string
  state: "closed" | "starting" | "running" | "exited" | "failed"
  output: string
  cursor: number
  cols: number
  rows: number
  truncated: boolean
  readerClosed: boolean
  exitStatus?: TerminalExitDto
}

export interface FileBrowserProjection {
  state: "idle" | "loading" | "ready" | "error"
  path?: string
  entries: readonly DirectoryEntryDto[]
  nextCursor?: string
  selectedFile?: FileReadDto
  diff?: StructuredDiffDto
  error?: string
}

export type WorkspaceSettingsProjection = SettingsDto

export interface WorkspaceProjection {
  revision: number
  connection: RuntimeConnectionState
  connectionDetail?: string
  projects: readonly ProjectProjection[]
  herdr: HerdrStatusProjection
  harnesses: readonly HarnessProjection[]
  sessions: readonly SessionProjection[]
  liveAgents: readonly LiveAgentDto[]
  targetWorkspace: TargetWorkspaceProjection
  targetWorkspaceError?: string
  factoryRuns: readonly FactoryRunProjection[]
  runError?: string
  terminals: readonly TerminalProjection[]
  activeTerminalId?: string
  files: FileBrowserProjection
  settings?: WorkspaceSettingsProjection
  settingsError?: string
  environments: readonly EnvironmentDto[]
  llmProviders: readonly LlmProviderDto[]
  llmProviderModelDiscovery?: {
    providerKey: string
    models: readonly string[]
  }
  llmProviderError?: string
  environmentError?: string
  secrets: readonly SecretMetadataDto[]
  secretsError?: string
  pluginRegistries: readonly PluginRegistryDto[]
  pluginCatalogs: readonly RegistryCatalogDto[]
  pluginDetails?: PluginDetailsDto
  plugins: PluginListDto
  pluginError?: string
  updateStatus?: UpdateStatusDto
  updateError?: string
}

type RunIdIntent<Type extends string> = { type: Type } & RunIdParamsDto
type SetThemeIntent = { type: "settings.theme" } & SetThemeParamsDto
type SetNotificationsIntent = {
  type: "settings.notifications"
} & SetNotificationsParamsDto
type SetLayoutIntent = { type: "settings.layout" } & SetLayoutParamsDto

export type RuntimeIntent =
  | { type: "runtime.retry" }
  | {
      type: "targetAgent.create"
      name: string
      objective: string
      acceptanceCriteria: readonly string[]
      repositoryRoot: string
      draftName: string
      trusted: boolean
      startRun?: boolean
      environmentId?: string
    }
  | { type: "targetAgent.remove"; targetAgentId: string }
  | {
      type: "agentDraft.update"
      agentDraftId: string
      name: string
      objective: string
      acceptanceCriteria: readonly string[]
      trusted: boolean
    }
  | {
      type: "agentDraft.environment.set"
      agentDraftId: string
      environmentId?: string
    }
  | {
      type: "agentDraft.create"
      targetAgentId: string
      baseVersionId?: string
      draftName: string
    }
  | {
      type: "agentDraft.publish"
      agentDraftId: string
      bump: VersionBump
      confirmWithoutPassingRun?: boolean
    }
  | { type: "agentDraft.discard"; agentDraftId: string }
  | {
      type: "agentSession.create"
      targetAgentId: string
      workspaceBindingId: string
      environmentId: string
      purpose: HarnessPurpose
      model?: string
    }
  | {
      type:
        | "agentSession.interrupt"
        | "agentSession.focus"
        | "agentSession.stop"
      agentSessionId: string
    }
  | {
      type: "agentSession.prompt"
      agentSessionId: string
      body: string
    }
  | {
      /// Logical keys forwarded to a blocked agent's own approval surface.
      type: "agentSession.sendKeys"
      agentSessionId: string
      keys: readonly string[]
    }
  | {
      type: "factoryRun.create"
      runId: string
      agentDraftId: string
      environmentId: string
      objective: string
    }
  | { type: "factoryRun.cancel"; runId: string }
  | { type: "agentDraft.toggleWorkspace"; agentDraftId: string }
  | {
      type: "workspacePane.openPrimary" | "workspacePane.openToSide"
      targetAgentId: string
      workspaceBindingId: string
      workItemId?: string
      workItemKind?: TargetWorkItemKind
    }
  | {
      type: "workspacePane.focus" | "workspacePane.close"
      workspacePaneId: string
    }
  | {
      type: "workspacePane.resize"
      layout: readonly {
        workspacePaneId: string
        widthBasisPoints: number
      }[]
    }
  | {
      type: "workspacePane.move"
      workspacePaneId: string
      position: number
    }
  | {
      type: "workspacePane.setDock"
      workContextId: string
      dock: WorkspaceDock
      dockPercent?: number
    }
  | {
      type: "workspaceTerminal.create"
      workContextId: string
      cols?: number
      rows?: number
    }
  | {
      type: "workspaceTerminal.write"
      terminalId: string
      data: string
    }
  | {
      type: "workspaceTerminal.resize"
      terminalId: string
      cols: number
      rows: number
    }
  | {
      type: "workspaceTerminal.read"
      terminalId: string
      cursor: number
      maxBytes: number
    }
  | { type: "workspaceTerminal.kill"; terminalId: string }
  | { type: "workspaceTerminal.close"; terminalId: string }
  | {
      type: "project.create"
      name: string
      root: string
      trusted: boolean
    }
  | { type: "project.trust.set"; projectId: string; trusted: boolean }
  | RunIdIntent<"run.cancel">
  | { type: "terminal.select"; terminalId: string }
  | {
      type: "file.list"
      path: string
      cursor?: string
      pageSize: number
    }
  | { type: "file.read"; path: string; maxBytes: number }
  | {
      type: "file.diff"
      beforePath: string
      afterPath: string
      contextLines: number
    }
  | SetThemeIntent
  | SetNotificationsIntent
  | SetLayoutIntent
  | { type: "environment.create"; configuration: EnvironmentConfigurationDraftDto }
  | {
      type: "environment.configuration.set"
      environmentId: string
      configuration: EnvironmentConfigurationDraftDto
    }
  | { type: "environment.delete"; environmentId: string }
  | {
      type: "llmProvider.create"
      configuration: LlmProviderConfigurationDto
    }
  | {
      type: "llmProvider.configuration.set"
      providerId: string
      configuration: LlmProviderConfigurationDto
    }
  | { type: "llmProvider.delete"; providerId: string }
  | {
      type: "llmProvider.models.list"
      providerId?: string
      provider: LlmProviderConnectionDto
    }
  | { type: "secret.create"; label: string; value: string }
  | { type: "secret.list" }
  | { type: "secret.replace"; secretRef: string; value: string }
  | { type: "secret.delete"; secretRef: string }
  | { type: "registry.list" }
  | {
      type: "registry.put"
      id: string
      catalogUrl: string
      signatureUrl: string
      publicKeyBase64: string
    }
  | { type: "registry.delete"; registryId: string }
  | { type: "registry.refresh"; registryId: string }
  | { type: "plugin.list" }
  | { type: "plugin.details"; registryId: string; pluginId: string }
  | { type: "plugin.install"; registryId: string; pluginId: string }
  | { type: "plugin.uninstall"; pluginName: string }
  | { type: "plugin.rollback"; pluginName: string }
  | {
      type: "plugin.trustLocalMcp" | "plugin.revokeLocalMcp"
      environmentId: string
      pluginName: string
      serverName: string
      fingerprint: string
    }
  | { type: "update.status" }
  | { type: "update.check" }
  | { type: "update.confirmAndInstall"; version: string }
  | { type: "update.rollback" }

export interface NativeSdkBridge {
  invoke(
    command:
      | "runtime.invoke"
      | "native-sdk.dialog.openFile"
      | "native-sdk.os.showNotification"
      | "desktop.terminal.show.v1"
      | "desktop.terminal.hide.v1",
    payload:
      | RuntimeRequest
      | NativeOpenFileOptions
      | NativeNotificationOptions
      | NativeTerminalShowRequestV1
      | null,
  ): Promise<unknown>
  on(
    name: string,
    listener: (detail: unknown) => void,
  ): () => void
}

export interface NativeOpenFileOptions {
  title?: string
  defaultPath?: string
  allowDirectories?: boolean
  allowMultiple?: boolean
}

export interface NativeNotificationOptions {
  title: string
  body: string
}

export interface NativeTerminalShowRequestV1 {
  executable: string
  arguments: readonly string[]
  workspaceId: string
  label: string
}

export interface NativeTerminalVisibilityEventV1 {
  version: 1
  visible: boolean
}

declare global {
  interface Window {
    zero?: NativeSdkBridge
  }
}
