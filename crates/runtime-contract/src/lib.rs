//! Versioned IPC DTOs and deterministic generated bindings.

use std::collections::BTreeMap;

use app_core::{
    AgentDraftProjection, AgentSessionProjection, AgentTranscriptProjection, ApplicationProjection,
    EnvironmentLlmPolicyDto, EnvironmentPermissionProjection, EnvironmentPluginProjection,
    EnvironmentProjection, EnvironmentVariableProjection, EvaluationResult, EvaluatorVerdict,
    FactoryRun, HarnessProjection, HarnessPurpose, HerdrStatusProjection,
    LlmProviderConfigurationDto, LlmProviderDto, LlmProviderType, SettingsProjection,
    TargetAgentManifest, TargetAgentProjection, TargetAgentVersionProjection, TargetWorkItemKind,
    ThemePreference, WorkspaceBindingProjection, WorkspaceDock,
};
use filesystem_runtime::{DirectoryPage, FileRead, StructuredDiff};
use ipc_contract::{Event, Frame, Request, Response};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use terminal_runtime::{TerminalCreated, TerminalExit, TerminalRead};
use uuid::Uuid;

pub const CONTRACT_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum RuntimeMethod {
    #[serde(rename = "runtime.hello")]
    RuntimeHello,
    #[serde(rename = "snapshot.get")]
    SnapshotGet,
    #[serde(rename = "harness.list")]
    HarnessList,
    #[serde(rename = "project.create")]
    ProjectCreate,
    #[serde(rename = "project.trust.set")]
    ProjectTrustSet,
    #[serde(rename = "targetAgent.create")]
    TargetAgentCreate,
    #[serde(rename = "targetAgent.remove")]
    TargetAgentRemove,
    #[serde(rename = "agentDraft.create")]
    AgentDraftCreate,
    #[serde(rename = "agentDraft.update")]
    AgentDraftUpdate,
    #[serde(rename = "agentDraft.environment.set")]
    AgentDraftEnvironmentSet,
    #[serde(rename = "agentDraft.publish")]
    AgentDraftPublish,
    #[serde(rename = "agentDraft.discard")]
    AgentDraftDiscard,
    #[serde(rename = "agentSession.create")]
    AgentSessionCreate,
    #[serde(rename = "agentSession.prompt")]
    AgentSessionPrompt,
    #[serde(rename = "agentSession.interrupt")]
    AgentSessionInterrupt,
    #[serde(rename = "agentSession.sendKeys")]
    AgentSessionSendKeys,
    #[serde(rename = "agentSession.transcript")]
    AgentSessionTranscript,
    #[serde(rename = "agentSession.screen")]
    AgentSessionScreen,
    #[serde(rename = "agentSession.input")]
    AgentSessionInput,
    #[serde(rename = "agentSession.focus")]
    AgentSessionFocus,
    #[serde(rename = "agentSession.stop")]
    AgentSessionStop,
    #[serde(rename = "factoryRun.create")]
    FactoryRunCreate,
    #[serde(rename = "factoryRun.cancel")]
    FactoryRunCancel,
    #[serde(rename = "agentDraft.openWorkspace")]
    AgentDraftOpenWorkspace,
    #[serde(rename = "workspacePane.openPrimary")]
    WorkspacePaneOpenPrimary,
    #[serde(rename = "workspacePane.openToSide")]
    WorkspacePaneOpenToSide,
    #[serde(rename = "workspacePane.focus")]
    WorkspacePaneFocus,
    #[serde(rename = "workspacePane.close")]
    WorkspacePaneClose,
    #[serde(rename = "workspacePane.resize")]
    WorkspacePaneResize,
    #[serde(rename = "workspacePane.move")]
    WorkspacePaneMove,
    #[serde(rename = "workspacePane.setDock")]
    WorkspacePaneSetDock,
    #[serde(rename = "run.cancel")]
    RunCancel,
    #[serde(rename = "workspaceTerminal.create")]
    WorkspaceTerminalCreate,
    #[serde(rename = "workspaceTerminal.write")]
    WorkspaceTerminalWrite,
    #[serde(rename = "workspaceTerminal.resize")]
    WorkspaceTerminalResize,
    #[serde(rename = "workspaceTerminal.read")]
    WorkspaceTerminalRead,
    #[serde(rename = "workspaceTerminal.kill")]
    WorkspaceTerminalKill,
    #[serde(rename = "workspaceTerminal.close")]
    WorkspaceTerminalClose,
    #[serde(rename = "file.list")]
    FileList,
    #[serde(rename = "file.read")]
    FileRead,
    #[serde(rename = "file.diff")]
    FileDiff,
    #[serde(rename = "version.files.list")]
    VersionFilesList,
    #[serde(rename = "version.file.read")]
    VersionFileRead,
    #[serde(rename = "settings.setTheme")]
    SettingsSetTheme,
    #[serde(rename = "settings.setNotifications")]
    SettingsSetNotifications,
    #[serde(rename = "settings.setLayout")]
    SettingsSetLayout,
    #[serde(rename = "environment.create")]
    EnvironmentCreate,
    #[serde(rename = "environment.configuration.set")]
    EnvironmentConfigurationSet,
    #[serde(rename = "environment.delete")]
    EnvironmentDelete,
    #[serde(rename = "llmProvider.create")]
    LlmProviderCreate,
    #[serde(rename = "llmProvider.configuration.set")]
    LlmProviderConfigurationSet,
    #[serde(rename = "llmProvider.delete")]
    LlmProviderDelete,
    #[serde(rename = "llmProvider.models.list")]
    LlmProviderModelsList,
    #[serde(rename = "secret.list")]
    SecretList,
    #[serde(rename = "secret.create")]
    SecretCreate,
    #[serde(rename = "secret.replace")]
    SecretReplace,
    #[serde(rename = "secret.delete")]
    SecretDelete,
    #[serde(rename = "registry.list")]
    RegistryList,
    #[serde(rename = "registry.put")]
    RegistryPut,
    #[serde(rename = "registry.delete")]
    RegistryDelete,
    #[serde(rename = "registry.refresh")]
    RegistryRefresh,
    #[serde(rename = "plugin.list")]
    PluginList,
    #[serde(rename = "plugin.details")]
    PluginDetails,
    #[serde(rename = "plugin.install")]
    PluginInstall,
    #[serde(rename = "plugin.uninstall")]
    PluginUninstall,
    #[serde(rename = "plugin.rollback")]
    PluginRollback,
    #[serde(rename = "plugin.trustLocalMcp")]
    PluginTrustLocalMcp,
    #[serde(rename = "plugin.revokeLocalMcp")]
    PluginRevokeLocalMcp,
    #[serde(rename = "update.status")]
    UpdateStatus,
    #[serde(rename = "update.check")]
    UpdateCheck,
    #[serde(rename = "update.confirmAndInstall")]
    UpdateConfirmAndInstall,
    #[serde(rename = "update.rollback")]
    UpdateRollback,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EmptyParams {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateProjectParams {
    pub name: String,
    pub root: String,
    #[serde(default = "default_project_trusted")]
    pub trusted: bool,
}

const fn default_project_trusted() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetProjectTrustParams {
    pub project_id: Uuid,
    pub trusted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateTargetAgentParams {
    pub name: String,
    pub objective: String,
    pub acceptance_criteria: Vec<String>,
    pub repository_root: String,
    pub draft_name: String,
    #[serde(default = "default_project_trusted")]
    pub trusted: bool,
    #[serde(default)]
    pub start_run: bool,
    #[serde(default)]
    pub environment_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateAgentDraftParams {
    pub agent_draft_id: Uuid,
    pub name: String,
    pub objective: String,
    pub acceptance_criteria: Vec<String>,
    pub trusted: bool,
}

/// Choose the Environment a Draft's Runs use. `None` clears the choice, which
/// is what removing the selected Environment leaves behind.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetAgentDraftEnvironmentParams {
    pub agent_draft_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TargetAgentIdParams {
    pub target_agent_id: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateAgentDraftParams {
    pub target_agent_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_version_id: Option<Uuid>,
    pub draft_name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VersionBump {
    Patch,
    Minor,
    Major,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublishAgentDraftParams {
    pub agent_draft_id: Uuid,
    #[serde(default = "default_version_bump")]
    pub bump: VersionBump,
    #[serde(default)]
    pub confirm_without_passing_run: bool,
}

const fn default_version_bump() -> VersionBump {
    VersionBump::Patch
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentDraftIdParams {
    pub agent_draft_id: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateAgentSessionParams {
    pub target_agent_id: Uuid,
    pub workspace_binding_id: Uuid,
    pub environment_id: String,
    pub purpose: HarnessPurpose,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentSessionIdParams {
    pub agent_session_id: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PromptAgentSessionParams {
    pub agent_session_id: Uuid,
    pub text: String,
}

/// Logical keys forwarded to an agent's own interactive surface, such as `esc`
/// or `ctrl+c`. Approval prompts belong to the agent, so this is how a blocked
/// agent is answered.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SendAgentKeysParams {
    pub agent_session_id: Uuid,
    pub keys: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadAgentTranscriptParams {
    pub agent_session_id: Uuid,
    #[serde(default)]
    pub lines: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WriteAgentSessionParams {
    pub agent_session_id: Uuid,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub keys: Vec<String>,
    #[serde(default)]
    pub cols: Option<u16>,
    #[serde(default)]
    pub rows: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateFactoryRunParams {
    pub run_id: Uuid,
    pub agent_draft_id: Uuid,
    pub environment_id: String,
    pub objective: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenWorkspaceItemParams {
    pub target_agent_id: Uuid,
    pub workspace_binding_id: Uuid,
    #[serde(default)]
    pub work_item_id: Option<Uuid>,
    #[serde(default)]
    pub work_item_kind: Option<TargetWorkItemKind>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspacePaneIdParams {
    pub workspace_pane_id: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspacePaneLayoutItem {
    pub workspace_pane_id: Uuid,
    pub width_basis_points: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResizeWorkspacePanesParams {
    pub layout: Vec<WorkspacePaneLayoutItem>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MoveWorkspacePaneParams {
    pub workspace_pane_id: Uuid,
    pub position: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetWorkspaceDockParams {
    pub work_context_id: Uuid,
    pub dock: WorkspaceDock,
    #[serde(default = "default_dock_percent")]
    pub dock_percent: u8,
}

const fn default_dock_percent() -> u8 {
    32
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunIdParams {
    pub run_id: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateWorkspaceTerminalParams {
    pub work_context_id: Uuid,
    #[serde(default = "default_terminal_cols")]
    pub cols: u16,
    #[serde(default = "default_terminal_rows")]
    pub rows: u16,
}

const fn default_terminal_cols() -> u16 {
    80
}

const fn default_terminal_rows() -> u16 {
    24
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalIdParams {
    pub terminal_id: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WriteTerminalParams {
    pub terminal_id: Uuid,
    pub data: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResizeTerminalParams {
    pub terminal_id: Uuid,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadTerminalParams {
    pub terminal_id: Uuid,
    #[serde(default)]
    pub cursor: u64,
    #[serde(default = "default_terminal_read_bytes")]
    pub max_bytes: usize,
}

const fn default_terminal_read_bytes() -> usize {
    256 * 1024
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListFilesParams {
    pub path: String,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default = "default_page_size")]
    pub page_size: usize,
}

const fn default_page_size() -> usize {
    100
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadFileParams {
    pub path: String,
    #[serde(default = "default_file_read_bytes")]
    pub max_bytes: usize,
}

const fn default_file_read_bytes() -> usize {
    256 * 1024
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiffFilesParams {
    pub before_path: String,
    pub after_path: String,
    #[serde(default = "default_context_lines")]
    pub context_lines: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListVersionFilesParams {
    pub version_id: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadVersionFileParams {
    pub version_id: Uuid,
    pub path: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VersionFileEntryKindDto {
    File,
    Symlink,
    Submodule,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VersionFileEntryDto {
    pub path: String,
    pub kind: VersionFileEntryKindDto,
    pub size: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VersionFilesListDto {
    pub version_id: Uuid,
    pub git_commit: String,
    pub entries: Vec<VersionFileEntryDto>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VersionFileReadKindDto {
    Text,
    Binary,
    TooLarge,
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VersionFileReadDto {
    pub version_id: Uuid,
    pub git_commit: String,
    pub path: String,
    pub size: Option<u64>,
    pub kind: VersionFileReadKindDto,
    pub content: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetThemeParams {
    pub theme: ThemePreference,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetNotificationsParams {
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetLayoutParams {
    pub inspector_percent: u8,
    pub terminal_percent: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateSecretParams {
    pub label: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReplaceSecretParams {
    pub secret_ref: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteSecretParams {
    pub secret_ref: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SecretMetadataDto {
    pub secret_ref: String,
    pub label: String,
    pub kind: CredentialKindDto,
    pub referenced_by: Vec<SecretEnvironmentReferenceDto>,
    pub created_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CredentialKindDto {
    ApiToken,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SecretEnvironmentReferenceDto {
    pub environment_id: String,
    pub environment_name: String,
    pub kind: SecretEnvironmentReferenceKind,
    pub label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SecretEnvironmentReferenceKind {
    LlmProvider,
    HarnessEnvironmentVariable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SecretListDto {
    pub secrets: Vec<SecretMetadataDto>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginRegistryDto {
    pub id: String,
    pub catalog_url: String,
    pub signature_url: String,
    pub public_key_base64: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginRegistryListDto {
    pub registries: Vec<PluginRegistryDto>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PutPluginRegistryParams {
    pub id: String,
    pub catalog_url: String,
    pub signature_url: String,
    pub public_key_base64: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegistryIdParams {
    pub registry_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegistryCatalogPluginDto {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub source_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegistryCatalogDto {
    pub registry_id: String,
    pub generated_at: String,
    pub plugins: Vec<RegistryCatalogPluginDto>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallPluginParams {
    pub registry_id: String,
    pub plugin_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginDetailsParams {
    pub registry_id: String,
    pub plugin_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginNameParams {
    pub plugin_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstalledPluginDto {
    pub name: String,
    pub active_version: String,
    pub previous_version: Option<String>,
    /// Skills the installed plugin offers (read from SKILL.md frontmatter). The
    /// environment's `defaultSkills` selection picks a subset of these names.
    pub skills: Vec<InstalledSkillDto>,
    /// MCP servers the installed plugin offers (read from mcp.json). The environment's
    /// `enabledMcpServers` selection picks a subset of these names.
    pub mcp_servers: Vec<InstalledMcpServerDto>,
    /// Set when the plugin's MCP component was present but disabled at load time
    /// (e.g. an unsafe entry); the offered server list is then empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_disabled_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstalledSkillDto {
    pub name: String,
    pub description: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum InstalledMcpServerKindDto {
    Stdio,
    StreamableHttp,
    Sse,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstalledMcpServerDto {
    pub name: String,
    pub kind: InstalledMcpServerKindDto,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginDetailsDto {
    pub registry_id: String,
    pub plugin_id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub author_name: Option<String>,
    pub source_url: String,
    pub skills: Vec<InstalledSkillDto>,
    pub mcp_servers: Vec<InstalledMcpServerDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_disabled_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalMcpServerDto {
    pub environment_id: String,
    pub plugin_name: String,
    pub server_name: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub environment_keys: Vec<String>,
    pub trust_class: String,
    pub fingerprint: String,
    pub trusted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginListDto {
    pub installed: Vec<InstalledPluginDto>,
    pub local_mcp_servers: Vec<LocalMcpServerDto>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalMcpTrustParams {
    pub environment_id: String,
    pub plugin_name: String,
    pub server_name: String,
    pub fingerprint: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfirmUpdateParams {
    pub version: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateStatusDto {
    pub enabled: bool,
    pub config_status: String,
    pub current_version: String,
    pub state: String,
    pub target_version: Option<String>,
    pub message: Option<String>,
}

/// An Environment's complete configuration, as authored in one form.
///
/// Creating and saving carry the same payload so there is exactly one shape to
/// reason about, and a save can never land a mix of two edits. Note the absence
/// of an id: it is derived from the name by the runtime and is stable across
/// renames.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentConfigurationDraft {
    pub name: String,
    pub environment_variables: Vec<EnvironmentVariableProjection>,
    pub llm: Option<EnvironmentLlmPolicyDto>,
    pub plugins: Vec<EnvironmentPluginProjection>,
    pub registries: Vec<String>,
    /// How much an agent in this Environment may do without being asked.
    /// Omitted means the cautious default: reads allowed, writes and terminal
    /// use ask. A Factory Run only advances unattended when both are allowed.
    #[serde(default)]
    pub permissions: Option<EnvironmentPermissionProjection>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetEnvironmentConfigurationParams {
    pub environment_id: String,
    pub configuration: EnvironmentConfigurationDraft,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteEnvironmentParams {
    pub environment_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LlmProviderConnectionDto {
    #[serde(rename = "type")]
    pub provider_type: LlmProviderType,
    pub endpoint: String,
    pub credential_ref: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListLlmProviderModelsParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<Uuid>,
    pub provider: LlmProviderConnectionDto,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateLlmProviderParams {
    pub configuration: LlmProviderConfigurationDto,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetLlmProviderConfigurationParams {
    pub provider_id: Uuid,
    pub configuration: LlmProviderConfigurationDto,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteLlmProviderParams {
    pub provider_id: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateEnvironmentParams {
    pub configuration: EnvironmentConfigurationDraft,
}

const fn default_context_lines() -> usize {
    3
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeHelloDto {
    pub protocol_version: u32,
    pub runtime_name: String,
    pub runtime_version: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HarnessListDto {
    /// Agent kinds Herdr can launch, and whether Herdr itself is reachable.
    pub herdr: HerdrStatusProjection,
    pub harnesses: Vec<HarnessProjection>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCreateResultDto {
    pub project: app_core::ProjectProjection,
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TargetAgentCreateResultDto {
    pub target_agent: TargetAgentProjection,
    pub draft: AgentDraftProjection,
    pub workspace_binding: WorkspaceBindingProjection,
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentDraftMutationResultDto {
    pub draft: AgentDraftProjection,
    pub workspace_binding: WorkspaceBindingProjection,
    pub project: app_core::ProjectProjection,
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentDraftPublishResultDto {
    pub version: TargetAgentVersionProjection,
    pub cleanup_required: bool,
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceMutationResultDto {
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionResultDto {
    pub session: AgentSessionProjection,
    /// Whether an open draft was reused instead of creating a new session.
    pub reused: bool,
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentTranscriptResultDto {
    pub transcript: AgentTranscriptProjection,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunResultDto {
    pub run: FactoryRun,
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HerdrWorkspaceTerminalLaunchDto {
    pub executable: String,
    pub arguments: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentDraftWorkspaceResultDto {
    pub agent_draft_id: Uuid,
    pub workspace_id: String,
    pub label: String,
    pub already_open: bool,
    pub terminal: HerdrWorkspaceTerminalLaunchDto,
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunSessionResultDto {
    pub run: FactoryRun,
    pub session: AgentSessionProjection,
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionAcceptedResultDto {
    pub agent_session_id: Uuid,
    pub accepted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionInterruptResultDto {
    pub agent_session_id: Uuid,
    pub interrupted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionStopResultDto {
    pub agent_session_id: Uuid,
    pub stopped: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TerminalWriteResultDto {
    pub terminal_id: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TerminalResizeResultDto {
    pub terminal_id: Uuid,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TerminalKillResultDto {
    pub terminal_id: Uuid,
    pub exit_status: TerminalExit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SettingsResultDto {
    pub settings: SettingsProjection,
    pub revision: u64,
}

/// The result of every Environment mutation, and the `environment.changed` event payload.
///
/// One shape for all of them: a delete has no single Environment to report, and
/// a create needs the whole list anyway to place the new Environment in order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentsResultDto {
    pub environments: Vec<EnvironmentProjection>,
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LlmProviderModelsDto {
    pub provider_id: Option<Uuid>,
    pub models: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LlmProvidersResultDto {
    pub providers: Vec<LlmProviderDto>,
    pub environments: Vec<EnvironmentProjection>,
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LlmProviderCreateResultDto {
    pub provider_id: Uuid,
    pub providers: Vec<LlmProviderDto>,
    pub environments: Vec<EnvironmentProjection>,
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentCreateResultDto {
    /// The id the runtime derived from the name. The caller had no way to
    /// predict it, so it is reported rather than assumed.
    pub environment_id: String,
    pub environments: Vec<EnvironmentProjection>,
    pub revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NotificationCategory {
    SessionCompleted,
    SessionBlocked,
    SessionFailed,
    FactoryRunPassed,
    FactoryRunFailed,
    FactoryRunNeedsReview,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationRequestedDto {
    pub title: String,
    pub body: String,
    pub category: NotificationCategory,
    pub entity_id: Uuid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum NotificationEventTopic {
    #[serde(rename = "notification.requested")]
    NotificationRequested,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum HarnessEventTopic {
    #[serde(rename = "harness.changed")]
    HarnessChanged,
}

/// Herdr connectivity after the runtime reopens a lost subscription.
///
/// Herdr reachability is derived at projection time rather than stored, so
/// without this event the UI keeps whatever it learned from the launch-time
/// `harness.list` and reports Herdr as unavailable until the user reloads.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HarnessChangedEventDto {
    pub version: u16,
    pub sequence: u64,
    pub revision: u64,
    pub topic: HarnessEventTopic,
    pub payload: HarnessListDto,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NotificationRequestedEventDto {
    pub version: u16,
    pub sequence: u64,
    pub revision: u64,
    pub topic: NotificationEventTopic,
    pub payload: NotificationRequestedDto,
}

/// A schema-only registry. Each named DTO is generated independently so its
/// TypeScript name remains stable even when its internal shape evolves.
pub fn named_schemas() -> BTreeMap<&'static str, Value> {
    let mut schemas = BTreeMap::new();
    macro_rules! register {
        ($($ty:ty),+ $(,)?) => {$({
            let name = stringify!($ty).rsplit("::").next().expect("type name");
            schemas.insert(name, serde_json::to_value(schema_for!($ty)).expect("schema"));
        })+};
    }
    register!(
        Frame,
        RuntimeMethod,
        Request,
        Response,
        Event,
        ApplicationProjection,
        TargetAgentManifest,
        EvaluatorVerdict,
        EvaluationResult,
        EmptyParams,
        CreateProjectParams,
        SetProjectTrustParams,
        CreateTargetAgentParams,
        TargetAgentIdParams,
        CreateAgentDraftParams,
        UpdateAgentDraftParams,
        PublishAgentDraftParams,
        AgentDraftIdParams,
        VersionBump,
        CreateAgentSessionParams,
        AgentSessionIdParams,
        PromptAgentSessionParams,
        SendAgentKeysParams,
        ReadAgentTranscriptParams,
        WriteAgentSessionParams,
        CreateFactoryRunParams,
        OpenWorkspaceItemParams,
        WorkspacePaneIdParams,
        WorkspacePaneLayoutItem,
        ResizeWorkspacePanesParams,
        MoveWorkspacePaneParams,
        SetWorkspaceDockParams,
        RunIdParams,
        CreateWorkspaceTerminalParams,
        TerminalIdParams,
        WriteTerminalParams,
        ResizeTerminalParams,
        ReadTerminalParams,
        ListFilesParams,
        ReadFileParams,
        DiffFilesParams,
        ListVersionFilesParams,
        ReadVersionFileParams,
        VersionFileEntryKindDto,
        VersionFileEntryDto,
        VersionFilesListDto,
        VersionFileReadKindDto,
        VersionFileReadDto,
        SetThemeParams,
        SetNotificationsParams,
        SetLayoutParams,
        CreateSecretParams,
        ReplaceSecretParams,
        DeleteSecretParams,
        SecretMetadataDto,
        CredentialKindDto,
        SecretEnvironmentReferenceDto,
        SecretEnvironmentReferenceKind,
        SecretListDto,
        PluginRegistryDto,
        PluginRegistryListDto,
        PutPluginRegistryParams,
        RegistryIdParams,
        RegistryCatalogPluginDto,
        RegistryCatalogDto,
        InstallPluginParams,
        PluginDetailsParams,
        PluginNameParams,
        InstalledPluginDto,
        InstalledSkillDto,
        InstalledMcpServerKindDto,
        InstalledMcpServerDto,
        PluginDetailsDto,
        LocalMcpServerDto,
        PluginListDto,
        LocalMcpTrustParams,
        ConfirmUpdateParams,
        UpdateStatusDto,
        CreateEnvironmentParams,
        EnvironmentConfigurationDraft,
        SetEnvironmentConfigurationParams,
        DeleteEnvironmentParams,
        LlmProviderConnectionDto,
        ListLlmProviderModelsParams,
        CreateLlmProviderParams,
        SetLlmProviderConfigurationParams,
        DeleteLlmProviderParams,
        RuntimeHelloDto,
        HarnessListDto,
        ProjectCreateResultDto,
        TargetAgentCreateResultDto,
        AgentDraftMutationResultDto,
        AgentDraftPublishResultDto,
        WorkspaceMutationResultDto,
        AgentSessionResultDto,
        AgentTranscriptResultDto,
        RunResultDto,
        HerdrWorkspaceTerminalLaunchDto,
        AgentDraftWorkspaceResultDto,
        RunSessionResultDto,
        AgentSessionAcceptedResultDto,
        AgentSessionInterruptResultDto,
        AgentSessionStopResultDto,
        TerminalCreated,
        TerminalRead,
        TerminalWriteResultDto,
        TerminalResizeResultDto,
        TerminalKillResultDto,
        SettingsResultDto,
        EnvironmentsResultDto,
        LlmProviderModelsDto,
        LlmProvidersResultDto,
        LlmProviderCreateResultDto,
        EnvironmentCreateResultDto,
        NotificationCategory,
        NotificationRequestedDto,
        NotificationRequestedEventDto,
        HarnessEventTopic,
        HarnessChangedEventDto,
        DirectoryPage,
        FileRead,
        StructuredDiff,
    );
    schemas
}

#[cfg(test)]
mod draft_contract_tests {
    use super::*;

    #[test]
    fn draft_methods_are_exact_and_the_obsolete_update_method_is_absent() {
        let methods = [
            RuntimeMethod::AgentDraftCreate,
            RuntimeMethod::AgentDraftUpdate,
            RuntimeMethod::AgentDraftPublish,
            RuntimeMethod::AgentDraftDiscard,
            RuntimeMethod::FactoryRunCreate,
        ]
        .map(|method| serde_json::to_value(method).unwrap());
        assert_eq!(
            methods,
            [
                Value::String("agentDraft.create".into()),
                Value::String("agentDraft.update".into()),
                Value::String("agentDraft.publish".into()),
                Value::String("agentDraft.discard".into()),
                Value::String("factoryRun.create".into()),
            ]
        );
        let schema = serde_json::to_string(&named_schemas()).unwrap();
        assert!(!schema.contains("targetAgent.update"));
        assert!(!schema.contains("targetAgentVersionId"));
        assert!(schema.contains("agentDraftId"));
    }
}

#[cfg(test)]
mod version_files_contract_tests {
    use super::*;

    #[test]
    fn version_file_methods_are_version_id_scoped() {
        assert_eq!(
            serde_json::to_value(RuntimeMethod::VersionFilesList).unwrap(),
            Value::String("version.files.list".into()),
        );
        assert_eq!(
            serde_json::to_value(RuntimeMethod::VersionFileRead).unwrap(),
            Value::String("version.file.read".into()),
        );

        let list_schema = serde_json::to_string(&schema_for!(ListVersionFilesParams)).unwrap();
        let read_schema = serde_json::to_string(&schema_for!(ReadVersionFileParams)).unwrap();
        assert!(list_schema.contains("versionId"));
        assert!(read_schema.contains("versionId"));
        assert!(read_schema.contains("path"));
        for forbidden in ["repositoryRoot", "gitRef", "gitCommit"] {
            assert!(!list_schema.contains(forbidden));
            assert!(!read_schema.contains(forbidden));
        }
    }
}
