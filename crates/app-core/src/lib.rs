//! Application domain types. This crate has no transport or platform dependencies.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectProjection {
    pub id: Uuid,
    pub name: String,
    pub root: PathBuf,
    pub trusted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TargetAgentManifest {
    pub schema_version: u32,
    pub target_agent_id: Uuid,
    pub name: String,
    pub objective: String,
    pub acceptance_criteria: Vec<String>,
    pub lifecycle: TargetAgentManifestLifecycle,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum TargetAgentManifestLifecycle {
    Draft {
        #[serde(rename = "draftId")]
        draft_id: Uuid,
        #[serde(
            rename = "baseVersion",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        base_version: Option<String>,
    },
    Version {
        version: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TargetAgentProjection {
    pub id: Uuid,
    pub name: String,
    pub repository_root: PathBuf,
    pub archived: bool,
    pub last_activity_at_unix_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentDraftLifecycle {
    Active,
    Publishing,
    Archived,
    CleanupRequired,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentDraftProjection {
    pub id: Uuid,
    pub target_agent_id: Uuid,
    pub workspace_binding_id: Uuid,
    pub name: String,
    pub objective: String,
    pub acceptance_criteria: Vec<String>,
    pub base_version: Option<String>,
    pub branch_ref: String,
    pub worktree_path: PathBuf,
    pub git_head: String,
    pub lifecycle: AgentDraftLifecycle,
    pub cleanup_guidance: Option<String>,
    /// The Environment this Draft's next Run uses, as the user last chose it.
    ///
    /// A durable preference, not a promise: the Environment it names may since
    /// have been deleted or stopped being ready, so a launch still resolves and
    /// validates it. `None` means nobody has chosen yet.
    pub environment_id: Option<String>,
    pub created_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TargetAgentVersionProjection {
    pub id: Uuid,
    pub target_agent_id: Uuid,
    pub version: String,
    pub name: String,
    pub objective: String,
    pub acceptance_criteria: Vec<String>,
    pub source_draft_id: Uuid,
    pub git_commit: String,
    pub git_tag: String,
    pub created_at_unix_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceBindingProjection {
    pub id: Uuid,
    pub target_agent_id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub primary_root: PathBuf,
    pub additional_roots: Vec<PathBuf>,
    pub source_ref_label: Option<String>,
    pub archived: bool,
    pub last_used_at_unix_ms: u64,
}

/// The human-facing readiness of an Agent Factory-supported Harness.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HarnessReadinessState {
    Ready,
    SetupRequired,
    InstallationRequired,
}

/// A command the user can copy and run outside Agent Factory.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HarnessActionProjection {
    pub label: String,
    pub command: String,
}

/// A Harness is an allowlisted Herdr agent kind presented to the user.
///
/// Support and readiness come from the intersection of Agent Factory's
/// supported-manifest seed and Herdr's agent manifests. Agent Factory never
/// probes `PATH` for agent executables itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HarnessProjection {
    /// The Herdr agent kind, such as `claude` or `codex`.
    pub id: String,
    pub name: String,
    pub readiness: HarnessReadinessState,
    pub guidance: String,
    pub action: Option<HarnessActionProjection>,
}

/// Whether Herdr itself is reachable, and on a protocol Agent Factory supports.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityFreshness {
    Live,
    Reconnecting,
    #[default]
    LastObserved,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HerdrStatusProjection {
    pub connected: bool,
    pub freshness: AuthorityFreshness,
    pub observed_at_unix_ms: Option<u64>,
    pub version: Option<String>,
    pub protocol: Option<u32>,
    pub session: Option<String>,
    pub issues: Vec<String>,
}

impl Default for HerdrStatusProjection {
    fn default() -> Self {
        Self {
            connected: false,
            freshness: AuthorityFreshness::LastObserved,
            observed_at_unix_ms: None,
            version: None,
            protocol: None,
            session: None,
            issues: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HarnessPurpose {
    Orchestration,
    Coding,
    Evaluation,
}

impl HarnessPurpose {
    /// The prefix Herdr agent names carry so a pane's occupant is identifiable.
    pub fn agent_name_prefix(self) -> &'static str {
        match self {
            Self::Orchestration => "orch",
            Self::Coding => "coding",
            Self::Evaluation => "eval",
        }
    }
}

/// Where a session lives inside Herdr's topology.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HerdrPlacement {
    pub workspace_id: String,
    pub tab_id: String,
    pub pane_id: String,
    /// The unique Herdr agent name; the handle every control call targets.
    pub agent_name: String,
}

/// The lifecycle Herdr reports for a live agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentLifecycle {
    /// Ready for input.
    Idle,
    /// Producing output.
    Working,
    /// Herdr recognized an approval or question surface inside the agent.
    Blocked,
    /// Unseen background work finished.
    Done,
    /// Herdr sees the agent but cannot classify it confidently.
    Unknown,
}

impl AgentLifecycle {
    pub fn accepts_prompt(self) -> bool {
        matches!(self, Self::Idle | Self::Done | Self::Blocked)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionAvailability {
    Live,
    Reconnecting,
    LastObserved,
    Historical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ManagedSessionOutcomeKind {
    Completed,
    Stopped,
    Interrupted,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedSessionOutcome {
    pub kind: ManagedSessionOutcomeKind,
    pub summary: Option<String>,
    pub recorded_at_unix_ms: u64,
}

/// Durable Factory-managed session lineage joined with a fresh Herdr
/// observation when its agent still exists.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentSessionProjection {
    pub id: Uuid,
    pub target_agent_id: Uuid,
    pub workspace_binding_id: Uuid,
    pub project_id: Uuid,
    pub environment_id: String,
    /// The Herdr agent kind this session runs.
    pub harness_id: String,
    pub purpose: HarnessPurpose,
    pub factory_run_id: Option<Uuid>,
    pub parent_session_id: Option<Uuid>,
    /// Stable correlation identity retained after the Herdr object disappears.
    pub herdr_agent_name: String,
    pub availability: SessionAvailability,
    pub lifecycle: Option<AgentLifecycle>,
    pub placement: Option<HerdrPlacement>,
    pub title: String,
    pub created_at_unix_ms: u64,
    pub last_activity_at_unix_ms: u64,
    pub llm_provider_snapshot: Option<ResolvedLlmProviderDto>,
    pub effective_model: Option<String>,
    /// What Herdr says the agent is waiting on while it is blocked.
    pub attention: Vec<String>,
    /// First prompt sent to this session. Later prompts do not replace it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_prompt: Option<String>,
    /// Whether the agent has actually accepted [`Self::initial_prompt`].
    ///
    /// This is durable state of its own rather than a lifecycle value, because
    /// `lifecycle` reflects Herdr and Herdr reports an agent sitting on its
    /// startup screen as idle. Encoding "the brief has not landed" as a
    /// lifecycle would put two writers on one field, and reconciling against
    /// Herdr would silently strand the brief.
    #[serde(default)]
    pub brief_delivered: bool,
    pub outcome: Option<ManagedSessionOutcome>,
}

/// Every agent Herdr currently reports in a Factory-managed Workspace.
/// `managed_session_id` is absent for other runtime activity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveAgentProjection {
    pub agent_name: Option<String>,
    pub agent_kind: Option<String>,
    pub display_agent: Option<String>,
    pub lifecycle: AgentLifecycle,
    pub placement: HerdrPlacement,
    pub attention: Vec<String>,
    pub revision: u64,
    pub observed_at_unix_ms: u64,
    pub workspace_binding_id: Option<Uuid>,
    pub managed_session_id: Option<Uuid>,
    pub factory_run_id: Option<Uuid>,
    pub purpose: Option<HarnessPurpose>,
}

/// A pane transcript captured from Herdr on demand.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentTranscriptProjection {
    pub agent_session_id: Uuid,
    pub text: String,
    pub revision: u64,
    pub truncated: bool,
    pub captured_at_unix_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentProjection {
    pub id: String,
    pub name: String,
    pub coding_harness_id: String,
    pub evaluation_harness_id: String,
    pub plugins: Vec<EnvironmentPluginProjection>,
    pub permissions: EnvironmentPermissionProjection,
    pub registry_ids: Vec<String>,
    pub environment_variables: Vec<EnvironmentVariableProjection>,
    pub llm: Option<EnvironmentLlmPolicyDto>,
    pub resolved_llm: Option<ResolvedLlmProviderDto>,
    pub llm_needs_setup: bool,
    pub readiness: EnvironmentReadinessProjection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum LlmProviderType {
    Ollama,
    Litellm,
    Meta,
    OpenAi,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LlmProviderConfigurationDto {
    pub name: String,
    #[serde(rename = "type")]
    pub provider_type: LlmProviderType,
    pub endpoint: String,
    pub credential_ref: Option<String>,
    pub allowed_models: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LlmProviderDto {
    pub id: Uuid,
    pub name: String,
    #[serde(rename = "type")]
    pub provider_type: LlmProviderType,
    pub endpoint: String,
    pub credential_ref: Option<String>,
    pub allowed_models: Vec<String>,
    pub readiness: LlmProviderReadinessProjection,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentLlmPolicyDto {
    pub provider_id: Uuid,
    pub allowed_models: Vec<String>,
    pub default_model: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedLlmProviderDto {
    pub provider_id: Uuid,
    pub provider_name: String,
    #[serde(rename = "type")]
    pub provider_type: LlmProviderType,
    pub endpoint: String,
    pub credential_ref: Option<String>,
    pub allowed_models: Vec<String>,
    pub default_model: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LlmProviderReadinessProjection {
    pub state: EnvironmentReadinessState,
    pub issues: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentReadinessState {
    Ready,
    NeedsSetup,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentReadinessProjection {
    pub state: EnvironmentReadinessState,
    pub issues: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentVariableProjection {
    pub name: String,
    pub source: EnvironmentVariableSource,
    /// Literal values are shown as-is. Secret values are represented only by
    /// their opaque Keychain reference.
    pub value: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum EnvironmentVariableSource {
    Literal,
    Secret,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentPluginProjection {
    pub name: String,
    pub enabled_mcp_servers: Vec<String>,
    pub default_skills: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum EnvironmentPermissionPolicy {
    Allow,
    Ask,
    Deny,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentPermissionProjection {
    pub trusted_read: EnvironmentPermissionPolicy,
    pub trusted_write: EnvironmentPermissionPolicy,
    pub terminal: EnvironmentPermissionPolicy,
}

impl Default for EnvironmentPermissionProjection {
    fn default() -> Self {
        Self {
            trusted_read: EnvironmentPermissionPolicy::Allow,
            trusted_write: EnvironmentPermissionPolicy::Ask,
            terminal: EnvironmentPermissionPolicy::Ask,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TargetWorkItemKind {
    AgentDraft,
    OrchestrationThread,
    CodingThread,
    EvaluationThread,
    FactoryRun,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TargetWorkItemProjection {
    pub id: Uuid,
    pub kind: TargetWorkItemKind,
    pub target_agent_id: Uuid,
    pub workspace_binding_id: Uuid,
    pub project_id: Uuid,
    pub agent_draft_id: Option<Uuid>,
    pub title: String,
    pub status: String,
    pub last_activity_at_unix_ms: u64,
    pub project_label: String,
    pub workspace_label: String,
    pub source_ref_label: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TargetAgentWorkGroupProjection {
    pub target_agent: TargetAgentProjection,
    pub drafts: Vec<AgentDraftProjection>,
    pub versions: Vec<TargetAgentVersionProjection>,
    pub workspace_bindings: Vec<WorkspaceBindingProjection>,
    pub work_items: Vec<TargetWorkItemProjection>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceDock {
    #[default]
    Closed,
    Terminal,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkContextProjection {
    pub id: Uuid,
    pub target_agent_id: Uuid,
    pub workspace_binding_id: Uuid,
    pub agent_draft_id: Option<Uuid>,
    pub work_item_id: Option<Uuid>,
    pub work_item_kind: Option<TargetWorkItemKind>,
    pub dock: WorkspaceDock,
    pub dock_percent: u8,
    pub last_viewed_at_unix_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspacePaneProjection {
    pub id: Uuid,
    pub work_context_id: Uuid,
    pub position: u8,
    pub width_basis_points: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceTerminalState {
    Running,
    Exited,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceTerminalProjection {
    pub id: Uuid,
    pub work_context_id: Uuid,
    pub workspace_binding_id: Uuid,
    pub title: String,
    pub state: WorkspaceTerminalState,
    pub created_at_unix_ms: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TargetWorkspaceProjection {
    pub target_groups: Vec<TargetAgentWorkGroupProjection>,
    pub work_contexts: Vec<WorkContextProjection>,
    pub panes: Vec<WorkspacePaneProjection>,
    pub terminals: Vec<WorkspaceTerminalProjection>,
    pub focused_pane_id: Option<Uuid>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsProjection {
    pub theme: ThemePreference,
    pub native_notifications: bool,
    pub layout: LayoutProjection,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LayoutProjection {
    pub inspector_percent: u8,
    pub terminal_percent: u8,
}

impl Default for SettingsProjection {
    fn default() -> Self {
        Self {
            theme: ThemePreference::System,
            native_notifications: true,
            layout: LayoutProjection {
                inspector_percent: 28,
                terminal_percent: 24,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationProjection {
    pub revision: u64,
    pub settings: SettingsProjection,
    pub active_project_id: Option<Uuid>,
    pub active_agent_session_id: Option<Uuid>,
    pub active_run_id: Option<Uuid>,
    pub projects: Vec<ProjectProjection>,
    pub llm_providers: Vec<LlmProviderDto>,
    pub environments: Vec<EnvironmentProjection>,
    pub herdr: HerdrStatusProjection,
    pub harnesses: Vec<HarnessProjection>,
    pub agent_sessions: Vec<AgentSessionProjection>,
    pub live_agents: Vec<LiveAgentProjection>,
    pub factory_runs: Vec<FactoryRun>,
    pub target_workspace: TargetWorkspaceProjection,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FactoryRun {
    pub id: Uuid,
    pub target_agent_id: Uuid,
    pub agent_draft_id: Uuid,
    pub workspace_binding_id: Uuid,
    pub project_id: Uuid,
    pub environment_id: String,
    pub objective: String,
    pub acceptance_criteria: Vec<String>,
    pub starting_git_head: String,
    pub final_git_head: Option<String>,
    pub changed_files: Vec<ChangedFile>,
    pub test_evidence: Vec<TestEvidence>,
    pub evaluation: Option<EvaluationResult>,
    pub state: FactoryRunState,
    /// What the Orchestrator stopped to ask a person.
    ///
    /// A Factory Run advances unattended, so this is the one thing that should
    /// pull someone back to it. The Orchestrator stays live in its pane while
    /// this is set: the answer is typed to it directly, and it clears the
    /// question by making its next move.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub escalation: Option<String>,
    pub completed_at_unix_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FactoryRunInput {
    pub target_agent_id: Uuid,
    pub agent_draft_id: Uuid,
    pub workspace_binding_id: Uuid,
    pub project_id: Uuid,
    pub environment_id: String,
    pub objective: String,
    pub acceptance_criteria: Vec<String>,
    pub starting_git_head: String,
}

impl FactoryRun {
    pub fn new(input: FactoryRunInput) -> Result<Self, FactoryRunError> {
        validate_text("objective", &input.objective)?;
        if input.acceptance_criteria.is_empty() {
            return Err(FactoryRunError::MissingAcceptanceCriteria);
        }
        for criterion in &input.acceptance_criteria {
            validate_text("acceptance criterion", criterion)?;
        }

        Ok(Self {
            id: Uuid::new_v4(),
            target_agent_id: input.target_agent_id,
            agent_draft_id: input.agent_draft_id,
            workspace_binding_id: input.workspace_binding_id,
            project_id: input.project_id,
            environment_id: input.environment_id,
            objective: input.objective,
            acceptance_criteria: input.acceptance_criteria,
            starting_git_head: input.starting_git_head,
            final_git_head: None,
            changed_files: Vec::new(),
            test_evidence: Vec::new(),
            evaluation: None,
            escalation: None,
            state: FactoryRunState::Draft,
            completed_at_unix_ms: None,
        })
    }

    pub fn transition(&mut self, next: FactoryRunState) -> Result<(), FactoryRunError> {
        if self.state == next {
            return Ok(());
        }

        let valid = matches!(
            (self.state, next),
            (FactoryRunState::Draft, FactoryRunState::Orchestrating)
                | (FactoryRunState::Draft, FactoryRunState::Coding)
                | (FactoryRunState::Orchestrating, FactoryRunState::Coding)
                | (FactoryRunState::Orchestrating, FactoryRunState::Evaluating)
                | (FactoryRunState::Orchestrating, FactoryRunState::Passed)
                | (FactoryRunState::Orchestrating, FactoryRunState::Failed)
                | (FactoryRunState::Orchestrating, FactoryRunState::NeedsReview)
                | (FactoryRunState::Orchestrating, FactoryRunState::Escalated)
                | (FactoryRunState::Coding, FactoryRunState::Orchestrating)
                | (FactoryRunState::Coding, FactoryRunState::Evaluating)
                | (FactoryRunState::Coding, FactoryRunState::NeedsReview)
                | (FactoryRunState::Coding, FactoryRunState::Escalated)
                | (FactoryRunState::Evaluating, FactoryRunState::Orchestrating)
                // The Orchestrator drives its own loop, so iterating goes
                // straight back to Coding rather than through a state that only
                // existed to hand control back to Rust.
                | (FactoryRunState::Evaluating, FactoryRunState::Coding)
                | (FactoryRunState::Evaluating, FactoryRunState::Passed)
                | (FactoryRunState::Evaluating, FactoryRunState::Failed)
                | (FactoryRunState::Evaluating, FactoryRunState::NeedsReview)
                | (FactoryRunState::Evaluating, FactoryRunState::Escalated)
                // Escalation is a live pause. The next explicit command either
                // resumes the workflow or finishes it with a durable verdict.
                | (FactoryRunState::Escalated, FactoryRunState::Coding)
                | (FactoryRunState::Escalated, FactoryRunState::Evaluating)
                | (FactoryRunState::Escalated, FactoryRunState::Passed)
                | (FactoryRunState::Escalated, FactoryRunState::Failed)
                | (FactoryRunState::Escalated, FactoryRunState::NeedsReview)
                | (_, FactoryRunState::Cancelled)
        );
        if !valid || self.state.is_terminal() {
            return Err(FactoryRunError::InvalidTransition {
                from: self.state,
                to: next,
            });
        }

        self.state = next;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFile {
    pub path: String,
    pub change: ChangedFileKind,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub diff: Option<StructuredTextDiff>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChangedFileKind {
    Added,
    Modified,
    Deleted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StructuredTextDiff {
    pub hunks: Vec<TextDiffHunk>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TextDiffHunk {
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    pub lines: Vec<TextDiffLine>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TextDiffLine {
    pub kind: TextDiffLineKind,
    pub old_line: Option<usize>,
    pub new_line: Option<usize>,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TextDiffLineKind {
    Context,
    Delete,
    Insert,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TestEvidence {
    pub name: String,
    pub status: TestEvidenceStatus,
    pub summary: String,
}

impl TestEvidence {
    pub fn validate(&self) -> Result<(), FactoryRunError> {
        validate_text("test evidence name", &self.name)?;
        validate_text("test evidence summary", &self.summary)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TestEvidenceStatus {
    Passed,
    Failed,
    NotRun,
}

pub const ORCHESTRATOR_DECISION_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OrchestratorDecisionKind {
    StartCoding,
    StartEvaluation,
    Iterate,
    Pass,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OrchestratorDecision {
    pub schema_version: u32,
    pub decision: OrchestratorDecisionKind,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff: Option<String>,
}

impl OrchestratorDecision {
    pub fn parse(text: &str) -> Result<Self, String> {
        let decision = serde_json::from_str::<Self>(text.trim()).map_err(|error| {
            format!("orchestrator response was not a valid versioned decision: {error}")
        })?;
        decision.validate()?;
        Ok(decision)
    }

    fn validate(&self) -> Result<(), String> {
        if self.schema_version != ORCHESTRATOR_DECISION_VERSION {
            return Err(format!(
                "unsupported orchestrator decision schemaVersion {}; expected {}",
                self.schema_version, ORCHESTRATOR_DECISION_VERSION
            ));
        }
        validate_bounded("orchestrator summary", &self.summary, 16 * 1024)?;
        if let Some(handoff) = &self.handoff {
            validate_bounded("orchestrator handoff", handoff, 32 * 1024)?;
        }
        Ok(())
    }
}

pub const EVALUATOR_VERDICT_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvaluatorVerdict {
    #[schemars(range(min = 1, max = 1))]
    pub schema_version: u32,
    pub verdict: Verdict,
    pub summary: String,
    pub findings: Vec<EvaluatorFinding>,
}

impl EvaluatorVerdict {
    pub fn parse(text: &str) -> EvaluationResult {
        let parsed = serde_json::from_str::<Self>(text.trim());
        match parsed {
            Ok(verdict) if verdict.validate().is_ok() => EvaluationResult {
                verdict: verdict.verdict,
                summary: verdict.summary,
                findings: verdict.findings,
                protocol_valid: true,
                validation_error: None,
            },
            Ok(verdict) => EvaluationResult::needs_review(
                verdict
                    .validate()
                    .expect_err("guard proved the verdict is invalid"),
            ),
            Err(error) => EvaluationResult::needs_review(format!(
                "evaluator response was not a valid versioned verdict: {error}"
            )),
        }
    }

    fn validate(&self) -> Result<(), String> {
        if self.schema_version != EVALUATOR_VERDICT_VERSION {
            return Err(format!(
                "unsupported evaluator verdict schemaVersion {}; expected {}",
                self.schema_version, EVALUATOR_VERDICT_VERSION
            ));
        }
        validate_bounded("evaluator summary", &self.summary, 16 * 1024)?;
        if self.findings.len() > 1_000 {
            return Err("evaluator findings exceed the limit of 1000".into());
        }
        for finding in &self.findings {
            finding.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationResult {
    pub verdict: Verdict,
    pub summary: String,
    pub findings: Vec<EvaluatorFinding>,
    pub protocol_valid: bool,
    pub validation_error: Option<String>,
}

impl EvaluationResult {
    /// The Orchestrator concluded the objective is met. This is an asserted
    /// verdict, not one salvaged from an agent's output, so it is well-formed
    /// by construction.
    pub fn passed(summary: impl Into<String>) -> Self {
        Self {
            verdict: Verdict::Pass,
            summary: summary.into(),
            findings: Vec::new(),
            protocol_valid: true,
            validation_error: None,
        }
    }

    /// The Orchestrator could not conclude and is handing the Run to a person.
    pub fn review_requested(summary: impl Into<String>) -> Self {
        Self {
            verdict: Verdict::NeedsReview,
            summary: summary.into(),
            findings: Vec::new(),
            protocol_valid: true,
            validation_error: None,
        }
    }

    pub fn needs_review(message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            verdict: Verdict::NeedsReview,
            summary: "Evaluator response requires human review".into(),
            findings: Vec::new(),
            protocol_valid: false,
            validation_error: Some(message),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Pass,
    Fail,
    NeedsReview,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvaluatorFinding {
    pub severity: FindingSeverity,
    pub title: String,
    pub evidence: String,
    pub file: Option<String>,
    pub line: Option<u32>,
}

impl EvaluatorFinding {
    fn validate(&self) -> Result<(), String> {
        validate_bounded("finding title", &self.title, 4 * 1024)?;
        validate_bounded("finding evidence", &self.evidence, 16 * 1024)?;
        if let Some(file) = &self.file {
            validate_bounded("finding file", file, 4 * 1024)?;
        }
        if self.line == Some(0) {
            return Err("finding line must be positive".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    Critical,
    Major,
    Minor,
    Note,
}

fn validate_bounded(field: &str, value: &str, maximum: usize) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{field} must not be empty"));
    }
    if value.len() > maximum {
        return Err(format!("{field} exceeds the {maximum}-byte limit"));
    }
    Ok(())
}

fn validate_text(field: &'static str, value: &str) -> Result<(), FactoryRunError> {
    if value.trim().is_empty() {
        return Err(FactoryRunError::EmptyField(field));
    }
    if value.len() > 16 * 1024 {
        return Err(FactoryRunError::FieldTooLong(field));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FactoryRunState {
    Draft,
    Orchestrating,
    Coding,
    Evaluating,
    Escalated,
    Passed,
    Failed,
    NeedsReview,
    Cancelled,
}

impl FactoryRunState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Passed | Self::Failed | Self::NeedsReview | Self::Cancelled
        )
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FactoryRunError {
    #[error("{0} must not be empty")]
    EmptyField(&'static str),
    #[error("{0} is too long")]
    FieldTooLong(&'static str),
    #[error("at least one acceptance criterion is required")]
    MissingAcceptanceCriteria,
    #[error("invalid Run transition from {from:?} to {to:?}")]
    InvalidTransition {
        from: FactoryRunState,
        to: FactoryRunState,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run() -> FactoryRun {
        FactoryRun::new(FactoryRunInput {
            target_agent_id: Uuid::new_v4(),
            agent_draft_id: Uuid::new_v4(),
            workspace_binding_id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            environment_id: "default".into(),
            objective: "Build a release-ready coding agent".into(),
            acceptance_criteria: vec!["Protocol tests pass".into()],
            starting_git_head: "0123456789abcdef".into(),
        })
        .unwrap()
    }

    #[test]
    fn accepts_the_complete_factory_workflow() {
        let mut run = run();
        for state in [
            FactoryRunState::Coding,
            FactoryRunState::Evaluating,
            FactoryRunState::Coding,
            FactoryRunState::Evaluating,
            FactoryRunState::Passed,
        ] {
            run.transition(state).unwrap();
        }
        assert_eq!(run.state, FactoryRunState::Passed);
    }

    #[test]
    fn coding_may_advance_straight_to_evaluating() {
        let mut run = run();
        run.transition(FactoryRunState::Coding).unwrap();
        run.transition(FactoryRunState::Evaluating).unwrap();
        assert_eq!(run.state, FactoryRunState::Evaluating);
    }

    #[test]
    fn rejects_skipping_evaluation() {
        let mut run = run();
        let error = run.transition(FactoryRunState::Passed).unwrap_err();
        assert_eq!(
            error,
            FactoryRunError::InvalidTransition {
                from: FactoryRunState::Draft,
                to: FactoryRunState::Passed,
            }
        );
    }

    #[test]
    fn a_terminal_run_cannot_restart() {
        let mut run = run();
        run.transition(FactoryRunState::Cancelled).unwrap();
        assert!(run.transition(FactoryRunState::Coding).is_err());
    }

    #[test]
    fn an_escalated_run_can_resume_but_a_review_verdict_is_terminal() {
        let mut run = run();
        run.transition(FactoryRunState::Coding).unwrap();
        run.transition(FactoryRunState::Escalated).unwrap();
        assert!(!run.state.is_terminal());
        run.transition(FactoryRunState::Evaluating).unwrap();
        run.transition(FactoryRunState::NeedsReview).unwrap();
        assert!(run.state.is_terminal());
        assert!(run.transition(FactoryRunState::Coding).is_err());
    }

    #[test]
    fn objective_and_acceptance_criteria_are_required() {
        assert!(matches!(
            FactoryRun::new(FactoryRunInput {
                target_agent_id: Uuid::new_v4(),
                agent_draft_id: Uuid::new_v4(),
                workspace_binding_id: Uuid::new_v4(),
                project_id: Uuid::new_v4(),
                environment_id: "default".into(),
                objective: " ".into(),
                acceptance_criteria: vec!["yes".into()],
                starting_git_head: "0123456789abcdef".into(),
            }),
            Err(FactoryRunError::EmptyField("objective"))
        ));
        assert!(matches!(
            FactoryRun::new(FactoryRunInput {
                target_agent_id: Uuid::new_v4(),
                agent_draft_id: Uuid::new_v4(),
                workspace_binding_id: Uuid::new_v4(),
                project_id: Uuid::new_v4(),
                environment_id: "default".into(),
                objective: "objective".into(),
                acceptance_criteria: vec![],
                starting_git_head: "0123456789abcdef".into(),
            }),
            Err(FactoryRunError::MissingAcceptanceCriteria)
        ));
    }

    #[test]
    fn portable_manifest_v4_uses_camel_case_lifecycle_fields() {
        let manifest = TargetAgentManifest {
            schema_version: 4,
            target_agent_id: Uuid::new_v4(),
            name: "Agent".into(),
            objective: "Objective".into(),
            acceptance_criteria: vec!["Criterion".into()],
            lifecycle: TargetAgentManifestLifecycle::Draft {
                draft_id: Uuid::new_v4(),
                base_version: Some("0.1.0".into()),
            },
        };
        let json = serde_json::to_value(manifest).unwrap();
        assert!(json["lifecycle"]["draftId"].is_string());
        assert_eq!(json["lifecycle"]["baseVersion"], "0.1.0");
        assert!(json["lifecycle"].get("draft_id").is_none());
        assert!(json["lifecycle"].get("base_version").is_none());
    }

    #[test]
    fn only_settled_lifecycles_accept_a_prompt() {
        for lifecycle in [
            AgentLifecycle::Idle,
            AgentLifecycle::Done,
            AgentLifecycle::Blocked,
        ] {
            assert!(lifecycle.accepts_prompt(), "{lifecycle:?}");
        }
        for lifecycle in [AgentLifecycle::Working, AgentLifecycle::Unknown] {
            assert!(!lifecycle.accepts_prompt(), "{lifecycle:?}");
        }
    }

    #[test]
    fn session_availability_is_distinct_from_herdr_lifecycle() {
        assert_ne!(SessionAvailability::Live, SessionAvailability::Historical);
        assert_ne!(SessionAvailability::Live, SessionAvailability::LastObserved);
    }

    #[test]
    fn purposes_name_their_herdr_agents_distinctly() {
        assert_ne!(
            HarnessPurpose::Orchestration.agent_name_prefix(),
            HarnessPurpose::Coding.agent_name_prefix()
        );
        assert_ne!(
            HarnessPurpose::Coding.agent_name_prefix(),
            HarnessPurpose::Evaluation.agent_name_prefix()
        );
    }

    #[test]
    fn parses_only_the_versioned_evaluator_contract() {
        let decision = OrchestratorDecision::parse(
            r#"{"schemaVersion":1,"decision":"start_coding","summary":"Begin"}"#,
        )
        .expect("decision");
        assert_eq!(decision.decision, OrchestratorDecisionKind::StartCoding);
        assert!(OrchestratorDecision::parse("not json").is_err());

        let parsed = EvaluatorVerdict::parse(
            r#"{"schemaVersion":1,"verdict":"pass","summary":"All checks passed","findings":[]}"#,
        );
        assert_eq!(parsed.verdict, Verdict::Pass);
        assert!(parsed.protocol_valid);

        let prose = EvaluatorVerdict::parse("Everything looks good to me.");
        assert_eq!(prose.verdict, Verdict::NeedsReview);
        assert!(!prose.protocol_valid);

        let unknown_version = EvaluatorVerdict::parse(
            r#"{"schemaVersion":2,"verdict":"pass","summary":"fine","findings":[]}"#,
        );
        assert_eq!(unknown_version.verdict, Verdict::NeedsReview);
        assert!(!unknown_version.protocol_valid);

        let bad_line = EvaluatorVerdict::parse(
            r#"{"schemaVersion":1,"verdict":"fail","summary":"bad","findings":[{"severity":"major","title":"Bug","evidence":"reproduced","file":"src/lib.rs","line":0}]}"#,
        );
        assert_eq!(bad_line.verdict, Verdict::NeedsReview);
        assert!(!bad_line.protocol_valid);
    }
}
