//! Agent Factory runtime request dispatcher.

mod control_socket;
mod herdr_sessions;
mod repository_config;

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use app_core::{
    AgentDraftLifecycle, AgentDraftProjection, AgentLifecycle, AgentSessionProjection,
    AuthorityFreshness, ChangedFile, ChangedFileKind, EnvironmentLlmPolicyDto,
    EnvironmentPermissionPolicy, EnvironmentPermissionProjection, EnvironmentPluginProjection,
    EnvironmentProjection, EnvironmentReadinessProjection, EnvironmentReadinessState,
    EnvironmentVariableProjection, EnvironmentVariableSource, EvaluationResult, FactoryRun,
    FactoryRunError, FactoryRunInput, FactoryRunState, HarnessPurpose, HerdrPlacement,
    LiveAgentProjection, LlmProviderConfigurationDto, LlmProviderDto,
    LlmProviderReadinessProjection, LlmProviderType as ProjectedLlmProviderType,
    ManagedSessionOutcome, ManagedSessionOutcomeKind, ResolvedLlmProviderDto, SessionAvailability,
    TargetAgentManifest, TargetAgentManifestLifecycle, WorkspaceBindingProjection, WorkspaceDock,
};
use base64::Engine;
use environment_runtime::{
    CatalogLimits, EnvironmentCatalog, EnvironmentError, MAX_ENVIRONMENT_ID_CHARS,
    PermissionPolicy, StoredSecretResolver,
};
use filesystem_runtime::{FileError, FileSystem};
use git_runtime::{
    CommitFileKind, CommitTreeEntryKind, GitError, GitRuntime, WorkingTreeChangeKind,
};
use herdr_sessions::{AgentLaunchSpec, HerdrRuntime};
use ipc_contract::{ErrorCode, Event, Frame, PROTOCOL_VERSION, Request, Response};
use llm_gateway::{GatewayConfig, GatewayHandle, discover_models};
use llm_provider_runtime::{LlmProviderConfiguration, LlmProviderKind, validate_provider_name};
use platform_secrets::{InMemorySecretStore, SecretError, SecretRef, SecretStore, SecretValue};
use plugin_runtime::{
    ArchiveLimits, EnvironmentPluginEntry, EnvironmentPluginSelection, ExecutableTrustClass,
    HttpsRegistryDownloader, McpComponent, McpServerDefinition, PluginError, PluginStore,
    RegistryClient, ResolvedMcpServer,
};
use project_store::{LocalMcpTrustRecord, PluginRegistryRecord, ProjectStore, StoreError};
use repository_config::{RepositoryConfig, RepositoryConfigError, reject_path_collision};
use runtime_contract::{
    AgentDraftIdParams, AgentDraftMutationResultDto, AgentDraftPublishResultDto,
    AgentDraftWorkspaceResultDto, AgentSessionAcceptedResultDto, AgentSessionIdParams,
    AgentSessionInterruptResultDto, AgentSessionResultDto, AgentSessionStopResultDto,
    AgentTranscriptResultDto, ConfirmUpdateParams, CreateAgentDraftParams,
    CreateAgentSessionParams, CreateEnvironmentParams, CreateFactoryRunParams,
    CreateLlmProviderParams, CreateProjectParams, CreateSecretParams, CreateTargetAgentParams,
    CreateWorkspaceTerminalParams, CredentialKindDto, DeleteEnvironmentParams,
    DeleteLlmProviderParams, DeleteSecretParams, DiffFilesParams, EnvironmentConfigurationDraft,
    EnvironmentCreateResultDto, EnvironmentsResultDto, HarnessListDto,
    HerdrWorkspaceTerminalLaunchDto, InstallPluginParams, InstalledMcpServerDto,
    InstalledMcpServerKindDto, InstalledPluginDto, InstalledSkillDto, ListFilesParams,
    ListLlmProviderModelsParams, ListVersionFilesParams, LlmProviderCreateResultDto,
    LlmProviderModelsDto, LlmProvidersResultDto, LocalMcpServerDto, LocalMcpTrustParams,
    MoveWorkspacePaneParams, NotificationCategory, NotificationRequestedDto,
    OpenWorkspaceItemParams, PluginDetailsDto, PluginDetailsParams, PluginListDto,
    PluginNameParams, PluginRegistryDto, PluginRegistryListDto, ProjectCreateResultDto,
    PromptAgentSessionParams, PublishAgentDraftParams, PutPluginRegistryParams,
    ReadAgentTranscriptParams, ReadFileParams, ReadTerminalParams, ReadVersionFileParams,
    RegistryCatalogDto, RegistryCatalogPluginDto, RegistryIdParams, ReplaceSecretParams,
    ResizeTerminalParams, ResizeWorkspacePanesParams, RunIdParams, RunResultDto,
    RunSessionResultDto, SecretEnvironmentReferenceDto, SecretEnvironmentReferenceKind,
    SecretListDto, SecretMetadataDto, SendAgentKeysParams, SetAgentDraftEnvironmentParams,
    SetEnvironmentConfigurationParams, SetLayoutParams, SetLlmProviderConfigurationParams,
    SetNotificationsParams, SetProjectTrustParams, SetThemeParams, SetWorkspaceDockParams,
    SettingsResultDto, TargetAgentCreateResultDto, TargetAgentIdParams, TerminalIdParams,
    TerminalKillResultDto, TerminalResizeResultDto, TerminalWriteResultDto, UpdateAgentDraftParams,
    UpdateStatusDto, VersionBump, VersionFileEntryDto, VersionFileEntryKindDto, VersionFileReadDto,
    VersionFileReadKindDto, VersionFilesListDto, WorkspaceMutationResultDto, WorkspacePaneIdParams,
    WriteAgentSessionParams, WriteTerminalParams,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use terminal_runtime::{CreateTerminal, TerminalError, TerminalManager};
use update_runtime::{
    Architecture, HttpsDownloader, LoadedUpdateClientConfig, Release, ReleaseArtifact,
    SelectionRequest, UpdateClient, UpdateConfigLoadStatus, UpdateError, UpdateState,
    UpdateStateMachine, extract_macos_bundle, load_packaged_update_client_config, select_release,
};
use url::Url;
use uuid::Uuid;

pub const RUNTIME_NAME: &str = "agent-factory-runtime";
pub const RUNTIME_VERSION: &str = env!("CARGO_PKG_VERSION");
const MAX_FACTORY_PROMPT_BYTES: usize = 256 * 1024;
const MAX_NOTIFICATION_TITLE_CHARS: usize = 80;
const MAX_NOTIFICATION_BODY_CHARS: usize = 240;
const MAX_PUBLIC_DIAGNOSTIC_BYTES: usize = 8 * 1024;
const MAX_DEFAULT_SKILL_FILE_BYTES: u64 = 64 * 1024;
const MAX_DEFAULT_SKILL_PREFIX_BYTES: usize = 256 * 1024;
const MAX_SESSION_TITLE_CHARS: usize = 200;
const PENDING_PROMPT_RETRY: Duration = Duration::from_secs(3);
const DEFAULT_PLUGIN_REGISTRY_ID: &str = "official";
const DEFAULT_PLUGIN_REGISTRY_SOURCE_URL: &str =
    "https://github.com/ravikanchikare/desktop-shell-plugins";
const DEFAULT_PLUGIN_REGISTRY_CATALOG_URL: &str = "https://raw.githubusercontent.com/ravikanchikare/desktop-shell-plugins/main/registry/catalog.json";
const DEFAULT_PLUGIN_REGISTRY_SIGNATURE_URL: &str = "https://raw.githubusercontent.com/ravikanchikare/desktop-shell-plugins/main/registry/catalog.sig";
// The application-bundled trust anchor for the default registry. Rotating it
// is an application update, never data fetched from the registry itself.
const DEFAULT_PLUGIN_REGISTRY_PUBLIC_KEY: &str = "EtsR2MU5TUcK34/ybIO3nfaEGIqUtt3c2Yd8IddoZgw=";

/// What a Factory Run phase sends its agent. Evaluation composes its prompt
/// after the session exists, because the verdict file is named after it.
enum PromptBody {
    Text(String),
    Evaluation,
}

pub struct Runtime {
    store: ProjectStore,
    git: GitRuntime,
    search_paths: Vec<PathBuf>,
    next_event_sequence: u64,
    terminals: TerminalManager,
    herdr: HerdrRuntime,
    gateways: BTreeMap<Uuid, GatewayHandle>,
    environments: EnvironmentCatalog,
    plugin_store: PluginStore,
    secret_store: Arc<dyn SecretStore>,
    updates: UpdateService,
    pending_prompt_retry_at: BTreeMap<Uuid, Instant>,
    /// Where an Orchestrator's commands are served. `None` until the process
    /// actually binds a socket, which unit tests never do.
    control_endpoint: Option<PathBuf>,
}

/// The execution boundary a session's pane is created with. It is intentionally
/// internal and non-serializable: raw secret values exist only in the resolved
/// Environment Variables member and are dropped with the boundary.
///
/// Herdr agents read their own configuration and enforce their own approvals, so
/// the boundary Agent Factory can actually apply is the process environment and
/// working directory a pane is created with. The Environment's permission
/// policy is presented in Settings and is not smuggled into the agent.
struct ResolvedEnvironmentBoundary {
    environment_variables: environment_runtime::ResolvedEnvironment,
}

#[derive(Clone, Debug)]
pub struct EnvironmentServicePaths {
    pub user_environments: PathBuf,
    pub plugins: PathBuf,
}

struct UpdateService {
    loaded: LoadedUpdateClientConfig,
    client: UpdateClient<HttpsDownloader>,
    state: UpdateStateMachine,
    pending: Option<(Release, ReleaseArtifact)>,
    install_paths: Option<UpdateInstallPaths>,
    layout_error: Option<String>,
}

#[derive(Clone)]
struct UpdateInstallPaths {
    current_bundle: PathBuf,
    helper: PathBuf,
    extraction_parent: PathBuf,
}

impl UpdateService {
    fn discover(plugin_root: &Path) -> Self {
        let data_root = plugin_root.parent().unwrap_or(plugin_root);
        let staging = data_root.join("updates").join("staging");
        let extraction_parent = data_root.join("updates").join("extracted");
        let executable = std::env::current_exe();
        let (loaded, install_paths, layout_error) = match executable {
            Ok(executable) => {
                let loaded = load_packaged_update_client_config(&executable);
                match packaged_update_install_paths(&executable, extraction_parent) {
                    Ok(paths) => (loaded, Some(paths), None),
                    Err(error) => (loaded, None, Some(error.into())),
                }
            }
            Err(_) => (
                LoadedUpdateClientConfig {
                    config: update_runtime::UpdateClientConfig::disabled(),
                    status: UpdateConfigLoadStatus::Invalid,
                },
                None,
                Some("runtime executable path is unavailable".into()),
            ),
        };
        Self {
            loaded,
            client: UpdateClient::new(HttpsDownloader::default(), staging),
            state: UpdateStateMachine::default(),
            pending: None,
            install_paths,
            layout_error,
        }
    }

    fn enabled(&self) -> bool {
        self.loaded.config.enabled && self.install_paths.is_some()
    }
}

impl Runtime {
    /// A runtime whose Environments and plugins live beside its database, the way the
    /// packaged application arranges them, so reopening the same database finds
    /// the same Environments.
    ///
    /// An in-memory store has no home and no persistence, so it gets a private
    /// scratch root rather than sharing one — otherwise two of them would see
    /// each other's Environments.
    pub fn new(store: ProjectStore, search_paths: Vec<PathBuf>) -> Self {
        static SCRATCH: AtomicU64 = AtomicU64::new(0);
        let data_root = store
            .path()
            .and_then(|path| path.parent())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| {
                std::env::temp_dir().join(format!(
                    "agent-factory-scratch-{}-{}",
                    std::process::id(),
                    SCRATCH.fetch_add(1, Ordering::Relaxed)
                ))
            });
        Self::with_herdr(
            store,
            search_paths,
            EnvironmentServicePaths {
                user_environments: data_root.join("environments"),
                plugins: data_root.join("plugins"),
            },
            Arc::new(InMemorySecretStore::default()),
            // Unit tests must never reach the developer's own Herdr session.
            HerdrRuntime::detached(),
        )
        .expect("an empty environment root is always valid")
    }

    /// A runtime pointed at an explicit Herdr socket. Tests use this so they
    /// never reach the developer's live Herdr session.
    #[cfg(test)]
    fn connected_to_herdr(store: ProjectStore, socket: PathBuf) -> Self {
        static SCRATCH: AtomicU64 = AtomicU64::new(10_000);
        let data_root = std::env::temp_dir().join(format!(
            "agent-factory-herdr-{}-{}",
            std::process::id(),
            SCRATCH.fetch_add(1, Ordering::Relaxed)
        ));
        Self::with_herdr(
            store,
            vec![],
            EnvironmentServicePaths {
                user_environments: data_root.join("environments"),
                plugins: data_root.join("plugins"),
            },
            Arc::new(InMemorySecretStore::default()),
            HerdrRuntime::connect_to(socket),
        )
        .expect("an empty environment root is always valid")
    }

    pub fn with_environment_services(
        store: ProjectStore,
        search_paths: Vec<PathBuf>,
        paths: EnvironmentServicePaths,
        secret_store: Arc<dyn SecretStore>,
    ) -> Result<Self, RuntimeInitError> {
        // Herdr is the authority on which Harnesses exist. When it is
        // unreachable the catalog still loads, so Environments stay editable and
        // the Harness they select is validated the moment a session starts.
        let herdr = HerdrRuntime::connect(std::env::var("AGENT_FACTORY_HERDR_SESSION").ok());
        Self::with_herdr(store, search_paths, paths, secret_store, herdr)
    }

    fn with_herdr(
        store: ProjectStore,
        search_paths: Vec<PathBuf>,
        paths: EnvironmentServicePaths,
        secret_store: Arc<dyn SecretStore>,
        mut herdr: HerdrRuntime,
    ) -> Result<Self, RuntimeInitError> {
        let git = GitRuntime::default();
        reconcile_draft_publications(&store, &git)?;
        let updates = UpdateService::discover(&paths.plugins);
        let supported_harnesses = herdr
            .harnesses()
            .iter()
            .map(|harness| harness.id.clone())
            .chain(std::iter::once(
                environment_runtime::DEFAULT_HARNESS_ID.to_owned(),
            ))
            .collect::<BTreeSet<_>>();
        let environments = EnvironmentCatalog::load(
            &paths.user_environments,
            &supported_harnesses,
            CatalogLimits::default(),
        )?;
        let plugin_store = PluginStore::open(paths.plugins, ArchiveLimits::default())?;
        ensure_default_plugin_registry(&store)?;
        let available_secret_refs = secret_store
            .list_metadata()?
            .into_iter()
            .map(|metadata| metadata.reference.to_string())
            .collect::<BTreeSet<_>>();
        let persisted = store.snapshot()?;
        let mut providers = persisted.llm_providers;
        for provider in &mut providers {
            let readiness = llm_provider_projection_readiness(provider, &available_secret_refs);
            if provider.readiness != readiness {
                provider.readiness = readiness;
                store.put_llm_provider(provider, &[])?;
            }
        }
        let setup_by_environment = persisted
            .environments
            .into_iter()
            .map(|environment| (environment.id, environment.llm_needs_setup))
            .collect::<BTreeMap<_, _>>();
        let summaries = environments
            .iter()
            .map(|(environment_id, environment)| {
                environment_projection(
                    &environment.descriptor,
                    &available_secret_refs,
                    &providers,
                    setup_by_environment
                        .get(environment_id)
                        .copied()
                        .unwrap_or(false),
                    &plugin_store,
                )
            })
            .collect::<Vec<_>>();
        // Reconciling on boot tombstones any Environment that left the catalog while
        // the app was closed. An empty catalog is the valid first-run state.
        store.reconcile_environments(&summaries)?;
        herdr.refresh();
        let mut runtime = Self {
            store,
            git,
            search_paths,
            next_event_sequence: 1,
            terminals: TerminalManager::default(),
            herdr,
            gateways: BTreeMap::new(),
            environments,
            plugin_store,
            secret_store,
            updates,
            pending_prompt_retry_at: BTreeMap::new(),
            control_endpoint: None,
        };
        runtime.retry_authorized_draft_cleanup();
        Ok(runtime)
    }

    pub fn handle_request(&mut self, request: Request) -> Vec<Frame> {
        let request_id = request.id;
        let request_method = request.method.clone();
        let result = match request.method.as_str() {
            "runtime.hello" => self.runtime_hello(),
            "snapshot.get" => self.snapshot(),
            "harness.list" => self.harness_list(),
            "project.create" => self.project_create(request.params),
            "project.trust.set" => self.project_trust_set(request.params),
            "targetAgent.create" => self.target_agent_create(request.params),
            "targetAgent.remove" => self.target_agent_remove(request.params),
            "agentDraft.create" => self.agent_draft_create(request.params),
            "agentDraft.update" => self.agent_draft_update(request.params),
            "agentDraft.environment.set" => self.agent_draft_environment_set(request.params),
            "agentDraft.publish" => self.agent_draft_publish(request.params),
            "agentDraft.discard" => self.agent_draft_discard(request.params),
            "agentSession.create" => self.agent_session_create(request.params),
            "agentSession.prompt" => self.agent_session_prompt(request.params),
            "agentSession.interrupt" => self.agent_session_interrupt(request.params),
            "agentSession.sendKeys" => self.agent_session_send_keys(request.params),
            "agentSession.transcript" => self.agent_session_transcript(request.params),
            "agentSession.screen" => self.agent_session_screen(request.params),
            "agentSession.input" => self.agent_session_input(request.params),
            "agentSession.focus" => self.agent_session_focus(request.params),
            "agentSession.stop" => self.agent_session_stop(request.params),
            "factoryRun.create" => self.factory_run_create(request.params),
            "factoryRun.cancel" => self.factory_run_cancel(request.params),
            "agentDraft.openWorkspace" => self.agent_draft_open_workspace(request.params),
            "workspacePane.openPrimary" => self.workspace_pane_open(request.params, false),
            "workspacePane.openToSide" => self.workspace_pane_open(request.params, true),
            "workspacePane.focus" => self.workspace_pane_focus(request.params),
            "workspacePane.close" => self.workspace_pane_close(request.params),
            "workspacePane.resize" => self.workspace_pane_resize(request.params),
            "workspacePane.move" => self.workspace_pane_move(request.params),
            "workspacePane.setDock" => self.workspace_pane_set_dock(request.params),
            "run.cancel" => self.run_cancel(request.params),
            "workspaceTerminal.create" => self.workspace_terminal_create(request.params),
            "workspaceTerminal.write" => self.terminal_write(request.params),
            "workspaceTerminal.resize" => self.terminal_resize(request.params),
            "workspaceTerminal.read" => self.terminal_read(request.params),
            "workspaceTerminal.kill" => self.workspace_terminal_kill(request.params),
            "workspaceTerminal.close" => self.workspace_terminal_close(request.params),
            "file.list" => self.file_list(request.params),
            "file.read" => self.file_read(request.params),
            "file.diff" => self.file_diff(request.params),
            "version.files.list" => self.version_files_list(request.params),
            "version.file.read" => self.version_file_read(request.params),
            "settings.setTheme" => self.settings_set_theme(request.params),
            "settings.setNotifications" => self.settings_set_notifications(request.params),
            "settings.setLayout" => self.settings_set_layout(request.params),
            "environment.create" => self.environment_create(request.params),
            "environment.configuration.set" => self.environment_configuration_set(request.params),
            "environment.delete" => self.environment_delete(request.params),
            "llmProvider.create" => self.llm_provider_create(request.params),
            "llmProvider.configuration.set" => self.llm_provider_configuration_set(request.params),
            "llmProvider.delete" => self.llm_provider_delete(request.params),
            "llmProvider.models.list" => self.llm_provider_models_list(request.params),
            "secret.list" => self.secret_list(),
            "secret.create" => self.secret_create(request.params),
            "secret.replace" => self.secret_replace(request.params),
            "secret.delete" => self.secret_delete(request.params),
            "registry.list" => self.registry_list(),
            "registry.put" => self.registry_put(request.params),
            "registry.delete" => self.registry_delete(request.params),
            "registry.refresh" => self.registry_refresh(request.params),
            "plugin.list" => self.plugin_list(),
            "plugin.details" => self.plugin_details(request.params),
            "plugin.install" => self.plugin_install(request.params),
            "plugin.uninstall" => self.plugin_uninstall(request.params),
            "plugin.rollback" => self.plugin_rollback(request.params),
            "plugin.trustLocalMcp" => self.plugin_trust_local_mcp(request.params),
            "plugin.revokeLocalMcp" => self.plugin_revoke_local_mcp(request.params),
            "update.status" => self.update_status(),
            "update.check" => self.update_check(),
            "update.confirmAndInstall" => self.update_confirm_and_install(request.params),
            "update.rollback" => self.update_rollback(),
            _ => {
                return vec![Frame::Response(Response::error(
                    request_id,
                    ErrorCode::MethodNotFound,
                    format!("unknown runtime method `{}`", request.method),
                ))];
            }
        };

        match result {
            Ok(DispatchResult { result, event }) => {
                let mut frames = vec![Frame::Response(Response::success(request_id, result))];
                if let Some((topic, revision, payload)) = event {
                    frames.push(Frame::Event(Event {
                        version: PROTOCOL_VERSION,
                        sequence: self.take_sequence(),
                        revision,
                        topic,
                        payload,
                    }));
                }
                frames
            }
            Err(error) => {
                // Runtime stdout is reserved for framed IPC. Log only the
                // method and error class on stderr so diagnostics cannot copy
                // request payloads, credentials, or credential-bearing URLs.
                eprintln!(
                    "runtime request failed: method={request_method} code={:?}",
                    error.code()
                );
                vec![Frame::Response(Response::error(
                    request_id,
                    error.code(),
                    error.to_string(),
                ))]
            }
        }
    }

    fn runtime_hello(&self) -> Result<DispatchResult, DispatchError> {
        Ok(DispatchResult::response(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "runtimeName": RUNTIME_NAME,
            "runtimeVersion": RUNTIME_VERSION,
        })))
    }

    fn snapshot(&self) -> Result<DispatchResult, DispatchError> {
        Ok(DispatchResult::response(serde_json::to_value(
            self.projection()?,
        )?))
    }

    /// Join the durable Factory ledger with fresh Herdr and Git observations.
    ///
    /// Neither observation is copied back into SQLite. A stale Herdr cache is
    /// still useful for presentation, but only `Live` availability authorizes
    /// runtime commands.
    fn projection(&self) -> Result<app_core::ApplicationProjection, DispatchError> {
        let mut projection = self.store.snapshot()?;
        for group in &mut projection.target_workspace.target_groups {
            for draft in &mut group.drafts {
                if draft.lifecycle == AgentDraftLifecycle::Active
                    && let Ok(head) = self.git.head(&draft.worktree_path)
                {
                    draft.git_head = head;
                }
            }
        }
        projection.herdr = self.herdr.status().clone();
        projection.harnesses = self.herdr.harnesses().to_vec();
        projection.live_agents.clear();

        let freshness = projection.herdr.freshness;
        let observed_at = projection.herdr.observed_at_unix_ms.unwrap_or_default();
        let bindings = projection
            .target_workspace
            .target_groups
            .iter()
            .flat_map(|group| group.workspace_bindings.iter())
            .map(|binding| (binding.id, binding.clone()))
            .collect::<BTreeMap<_, _>>();
        let workspace_bindings = self
            .herdr
            .snapshot()
            .into_iter()
            .flat_map(|snapshot| snapshot.workspaces.iter())
            .filter_map(|workspace| {
                bindings.iter().find_map(|(binding_id, binding)| {
                    let label = herdr_sessions::workspace_label(
                        projection
                            .target_workspace
                            .target_groups
                            .iter()
                            .find(|group| group.target_agent.id == binding.target_agent_id)
                            .map(|group| group.target_agent.name.as_str())
                            .unwrap_or("Target Agent"),
                        &binding.name,
                        *binding_id,
                    );
                    (workspace.label == label)
                        .then_some((workspace.workspace_id.clone(), *binding_id))
                })
            })
            .collect::<BTreeMap<_, _>>();
        let managed_by_name = projection
            .agent_sessions
            .iter()
            .map(|session| {
                (
                    session.herdr_agent_name.clone(),
                    (
                        session.id,
                        session.workspace_binding_id,
                        session.factory_run_id,
                        session.purpose,
                    ),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut observed_sessions = BTreeSet::new();

        if let Some(snapshot) = self.herdr.snapshot() {
            for agent in &snapshot.agents {
                let Some(workspace_id) = agent.workspace_id.as_ref() else {
                    continue;
                };
                let Some(binding_id) = workspace_bindings.get(workspace_id).copied() else {
                    continue;
                };
                let agent_name = agent.name.clone();
                let managed = agent_name
                    .as_ref()
                    .and_then(|name| managed_by_name.get(name))
                    .copied()
                    .filter(|value| value.1 == binding_id);
                let placement = HerdrPlacement {
                    workspace_id: workspace_id.clone(),
                    tab_id: agent.tab_id.clone().unwrap_or_default(),
                    pane_id: agent.pane_id.clone(),
                    agent_name: agent_name
                        .clone()
                        .unwrap_or_else(|| "Herdr agent".to_owned()),
                };
                let lifecycle = herdr_sessions::lifecycle_from_status(agent.agent_status);
                let mut attention = agent
                    .state_labels
                    .values()
                    .filter(|value| !value.trim().is_empty())
                    .cloned()
                    .collect::<Vec<_>>();
                attention.sort();
                attention.dedup();
                projection.live_agents.push(LiveAgentProjection {
                    agent_name,
                    agent_kind: agent.agent.clone(),
                    display_agent: agent.display_agent.clone(),
                    lifecycle,
                    placement: placement.clone(),
                    attention: attention.clone(),
                    revision: agent.revision,
                    observed_at_unix_ms: observed_at,
                    workspace_binding_id: Some(binding_id),
                    managed_session_id: managed.map(|value| value.0),
                    factory_run_id: managed.and_then(|value| value.2),
                    purpose: managed.map(|value| value.3),
                });
                if let Some((session_id, _, _, _)) = managed {
                    observed_sessions.insert(session_id);
                    if let Some(session) = projection
                        .agent_sessions
                        .iter_mut()
                        .find(|session| session.id == session_id)
                    {
                        session.availability = match freshness {
                            AuthorityFreshness::Live => SessionAvailability::Live,
                            AuthorityFreshness::Reconnecting => SessionAvailability::Reconnecting,
                            AuthorityFreshness::LastObserved => SessionAvailability::LastObserved,
                        };
                        session.lifecycle = Some(lifecycle);
                        session.placement = Some(placement);
                        session.attention = attention;
                    }
                }
            }
        }

        for session in &mut projection.agent_sessions {
            if observed_sessions.contains(&session.id) {
                continue;
            }
            session.availability = if freshness == AuthorityFreshness::Reconnecting
                && self.herdr.snapshot().is_none()
            {
                SessionAvailability::Reconnecting
            } else {
                SessionAvailability::Historical
            };
            session.lifecycle = None;
            session.placement = None;
            session.attention.clear();
        }

        let session_status = projection
            .agent_sessions
            .iter()
            .map(|session| {
                let status = match (session.availability, session.lifecycle) {
                    (SessionAvailability::Live, Some(lifecycle)) => {
                        lifecycle_label(lifecycle).to_owned()
                    }
                    (SessionAvailability::Reconnecting, _) => "reconnecting".into(),
                    (SessionAvailability::LastObserved, Some(lifecycle)) => {
                        format!("last observed {}", lifecycle_label(lifecycle))
                    }
                    _ => "historical".into(),
                };
                (session.id, status)
            })
            .collect::<BTreeMap<_, _>>();
        for group in &mut projection.target_workspace.target_groups {
            for item in &mut group.work_items {
                if let Some(status) = session_status.get(&item.id) {
                    item.status.clone_from(status);
                }
            }
        }
        Ok(projection)
    }

    /// Harnesses are the agent kinds Herdr can launch. Agent Factory never
    /// probes `PATH` itself; Herdr owns detection and reports availability.
    fn harness_list(&mut self) -> Result<DispatchResult, DispatchError> {
        self.herdr.refresh();
        Ok(DispatchResult::response(serde_json::to_value(
            HarnessListDto {
                herdr: self.herdr.status().clone(),
                harnesses: self.herdr.harnesses().to_vec(),
            },
        )?))
    }

    fn project_create(&self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: CreateProjectParams = serde_json::from_value(params)
            .map_err(|error| DispatchError::InvalidParams(error.to_string()))?;
        let project =
            self.store
                .create_project(&params.name, Path::new(&params.root), params.trusted)?;
        let revision = self.store.snapshot()?.revision;
        let project_value = serde_json::to_value(&project)?;
        Ok(DispatchResult {
            result: serde_json::to_value(ProjectCreateResultDto { project, revision })?,
            event: Some(("project.changed".into(), revision, project_value)),
        })
    }

    fn project_trust_set(&mut self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: SetProjectTrustParams = decode_params(params)?;
        let existing = self
            .store
            .list_projects()?
            .into_iter()
            .find(|project| project.id == params.project_id)
            .ok_or_else(|| DispatchError::InvalidParams("unknown project".into()))?;
        if existing.trusted && !params.trusted {
            // Withdrawing trust ends every agent working in that project.
            let session_ids = self
                .projection()?
                .agent_sessions
                .into_iter()
                .filter(|session| {
                    session.project_id == params.project_id
                        && session.availability == SessionAvailability::Live
                })
                .map(|session| session.id)
                .collect::<Vec<_>>();
            for session_id in session_ids {
                self.stop_agent_session(session_id)?;
            }
        }
        let project = self
            .store
            .set_project_trust(params.project_id, params.trusted)?;
        let revision = self.store.snapshot()?.revision;
        Ok(DispatchResult {
            result: serde_json::to_value(ProjectCreateResultDto {
                project: project.clone(),
                revision,
            })?,
            event: Some((
                "project.changed".into(),
                revision,
                serde_json::to_value(project)?,
            )),
        })
    }

    fn target_agent_create(&mut self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: CreateTargetAgentParams = decode_params(params)?;
        let name = normalize_required("Agent name", &params.name, 200)?;
        let objective = normalize_required("Agent objective", &params.objective, 16 * 1024)?;
        let draft_name = normalize_required("Draft name", &params.draft_name, 200)?;
        let acceptance_criteria = normalize_criteria(&params.acceptance_criteria)?;
        let repository = self
            .git
            .ensure_repository(Path::new(&params.repository_root))?;
        let target_agent_id = Uuid::new_v4();
        let draft_id = Uuid::new_v4();
        let workspace_binding_id = Uuid::new_v4();
        let branch_ref = draft_branch(target_agent_id, &name, draft_id, &draft_name);
        let worktree_path = draft_worktree_path(&repository.root, &name, &draft_name, draft_id)?;
        let workspace_label =
            herdr_sessions::workspace_label(&name, &draft_name, workspace_binding_id);
        let (worktree_path, git_head) = self.create_draft_worktree(
            &repository.root,
            &worktree_path,
            &branch_ref,
            &repository.head,
            &workspace_label,
        )?;
        let target_agent =
            self.store
                .create_target_agent(target_agent_id, &name, &repository.root)?;
        let project = self.store.create_project(
            &format!("{name} — {draft_name}"),
            &worktree_path,
            params.trusted,
        )?;
        let workspace_binding = self.store.create_workspace_binding_with_id(
            workspace_binding_id,
            target_agent.id,
            project.id,
            &draft_name,
            &worktree_path,
            &[],
            Some(&branch_ref),
        )?;
        let timestamp = now_unix_ms();
        let draft = AgentDraftProjection {
            id: draft_id,
            target_agent_id: target_agent.id,
            workspace_binding_id: workspace_binding.id,
            name,
            objective,
            acceptance_criteria,
            base_version: None,
            branch_ref,
            worktree_path: worktree_path.clone(),
            git_head,
            lifecycle: AgentDraftLifecycle::Active,
            cleanup_guidance: None,
            environment_id: params.environment_id.clone(),
            created_at_unix_ms: timestamp,
            updated_at_unix_ms: timestamp,
        };
        write_draft_manifest(&draft)?;
        let draft = self.store.create_agent_draft(&draft)?;
        self.store.open_work_item(
            target_agent.id,
            workspace_binding.id,
            Some(draft.id),
            Some(app_core::TargetWorkItemKind::AgentDraft),
            false,
        )?;
        if params.start_run {
            let environment_id = params.environment_id.as_deref().ok_or_else(|| {
                DispatchError::InvalidParams(
                    "environmentId is required when starting the first Run".into(),
                )
            })?;
            self.create_run(Uuid::new_v4(), draft.id, environment_id, &draft.objective)?;
        }
        let revision = self.store.snapshot()?.revision;
        Ok(DispatchResult {
            result: serde_json::to_value(TargetAgentCreateResultDto {
                target_agent: target_agent.clone(),
                draft,
                workspace_binding,
                revision,
            })?,
            event: Some((
                "targetWorkspace.changed".into(),
                revision,
                serde_json::to_value(target_agent)?,
            )),
        })
    }

    fn create_draft_worktree(
        &mut self,
        repository_root: &Path,
        requested_path: &Path,
        branch: &str,
        start_point: &str,
        workspace_label: &str,
    ) -> Result<(PathBuf, String), DispatchError> {
        if !self.herdr.is_connected() {
            return Err(DispatchError::InvalidParams(format!(
                "Herdr is unavailable: {}",
                self.herdr.status().issues.join("; "),
            )));
        }
        let created = self
            .herdr
            .create_worktree(
                repository_root,
                branch,
                start_point,
                requested_path,
                workspace_label,
            )
            .map_err(|error| DispatchError::Herdr(error.public_message()))?;
        let validation: Result<(PathBuf, String), DispatchError> = (|| {
            let actual_path = std::fs::canonicalize(&created.worktree.path)
                .map_err(|_| GitError::InvalidPath(created.worktree.path.clone()))?;
            let expected_path = std::fs::canonicalize(requested_path)
                .map_err(|_| GitError::InvalidPath(requested_path.display().to_string()))?;
            if actual_path != expected_path {
                return Err(DispatchError::InvalidParams(format!(
                    "Herdr created the Draft at `{}` instead of the configured path `{}`",
                    actual_path.display(),
                    expected_path.display(),
                )));
            }
            let observed = self.git.preflight(&actual_path)?;
            let expected_head = self.git.resolve_ref(repository_root, start_point)?;
            if observed.head != expected_head {
                return Err(DispatchError::InvalidParams(
                    "Herdr created the Draft from an unexpected Git commit".into(),
                ));
            }
            Ok((actual_path, observed.head))
        })();
        if validation.is_err() {
            let created_path = Path::new(&created.worktree.path);
            let safe_to_remove = !created_path.exists()
                || self
                    .git
                    .prepare_clean_worktree_removal(created_path)
                    .is_ok();
            if safe_to_remove {
                let _ = self
                    .herdr
                    .remove_worktree(&created.workspace.workspace_id, false);
            }
        }
        validation
    }

    fn target_agent_remove(&mut self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: TargetAgentIdParams = decode_params(params)?;
        let agent = self.store.target_agent(params.target_agent_id)?;
        self.store.archive_target_agent(agent.id)?;
        let revision = self.store.snapshot()?.revision;
        Ok(DispatchResult {
            result: serde_json::to_value(WorkspaceMutationResultDto { revision })?,
            event: Some((
                "targetWorkspace.changed".into(),
                revision,
                json!({"targetAgentId": agent.id}),
            )),
        })
    }

    /// Record which Environment a Draft's Runs use.
    ///
    /// The Environment must exist, so a typo cannot be stored, but it need not
    /// be ready: a user may choose one they are still setting up, and readiness
    /// is enforced where it matters, when a Run starts.
    fn agent_draft_environment_set(
        &mut self,
        params: Value,
    ) -> Result<DispatchResult, DispatchError> {
        let params: SetAgentDraftEnvironmentParams = decode_params(params)?;
        if let Some(environment_id) = params.environment_id.as_deref()
            && self.environments.get(environment_id).is_none()
        {
            return Err(DispatchError::InvalidParams(format!(
                "Environment `{environment_id}` does not exist"
            )));
        }
        let draft = self
            .store
            .set_agent_draft_environment(params.agent_draft_id, params.environment_id.as_deref())?;
        let revision = self.store.snapshot()?.revision;
        Ok(DispatchResult {
            result: json!({ "draft": draft, "revision": revision }),
            event: Some((
                "targetWorkspace.changed".into(),
                revision,
                serde_json::to_value(draft)?,
            )),
        })
    }

    fn agent_draft_update(&mut self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: UpdateAgentDraftParams = decode_params(params)?;
        let name = normalize_required("Agent name", &params.name, 200)?;
        let objective = normalize_required("Agent objective", &params.objective, 16 * 1024)?;
        let acceptance_criteria = normalize_criteria(&params.acceptance_criteria)?;
        let current = self.store.agent_draft(params.agent_draft_id)?;
        let binding = self.store.workspace_binding(current.workspace_binding_id)?;
        let project = self
            .store
            .list_projects()?
            .into_iter()
            .find(|project| project.id == binding.project_id)
            .ok_or_else(|| DispatchError::InvalidParams("Agent project is missing".into()))?;
        if project.trusted && !params.trusted {
            let session_ids = self
                .projection()?
                .agent_sessions
                .into_iter()
                .filter(|session| {
                    session.project_id == project.id
                        && session.availability == SessionAvailability::Live
                })
                .map(|session| session.id)
                .collect::<Vec<_>>();
            for session_id in session_ids {
                self.stop_agent_session(session_id)?;
            }
        }
        let updated = AgentDraftProjection {
            name,
            objective,
            acceptance_criteria,
            ..current.clone()
        };
        write_draft_manifest(&updated)?;
        let git_head = self.git.head(&updated.worktree_path)?;
        let draft = self.store.update_agent_draft(
            updated.id,
            &updated.name,
            &updated.objective,
            &updated.acceptance_criteria,
            &git_head,
        )?;
        let project = self.store.set_project_trust(project.id, params.trusted)?;
        let workspace_binding = binding;
        let revision = self.store.snapshot()?.revision;
        Ok(DispatchResult {
            result: serde_json::to_value(AgentDraftMutationResultDto {
                draft: draft.clone(),
                workspace_binding,
                project,
                revision,
            })?,
            event: Some((
                "targetWorkspace.changed".into(),
                revision,
                serde_json::to_value(draft)?,
            )),
        })
    }

    fn agent_draft_create(&mut self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: CreateAgentDraftParams = decode_params(params)?;
        let draft_name = normalize_required("Draft name", &params.draft_name, 200)?;
        let agent = self.store.target_agent(params.target_agent_id)?;
        let version = match params.base_version_id {
            Some(version_id) => {
                let version = self.store.target_agent_version(version_id)?;
                if version.target_agent_id != agent.id {
                    return Err(DispatchError::InvalidParams(
                        "Version does not belong to the Agent".into(),
                    ));
                }
                Some(version)
            }
            None => None,
        };
        let start_point = match &version {
            Some(version) => version.git_commit.clone(),
            None => self.git.ensure_repository(&agent.repository_root)?.head,
        };
        let definition_name = version
            .as_ref()
            .map(|candidate| candidate.name.clone())
            .unwrap_or_else(|| agent.name.clone());
        let (objective, acceptance_criteria) = match &version {
            Some(version) => (
                version.objective.clone(),
                version.acceptance_criteria.clone(),
            ),
            None => prior_draft_definition(&self.store, agent.id).unwrap_or_else(|| {
                (
                    format!("Build {}", agent.name),
                    vec!["The Agent is ready to evaluate.".into()],
                )
            }),
        };
        let draft_id = Uuid::new_v4();
        let workspace_binding_id = Uuid::new_v4();
        let branch_ref = draft_branch(agent.id, &agent.name, draft_id, &draft_name);
        let worktree_path = draft_worktree_path(
            &agent.repository_root,
            &definition_name,
            &draft_name,
            draft_id,
        )?;
        let workspace_label =
            herdr_sessions::workspace_label(&agent.name, &draft_name, workspace_binding_id);
        let (worktree_path, git_head) = self.create_draft_worktree(
            &agent.repository_root,
            &worktree_path,
            &branch_ref,
            &start_point,
            &workspace_label,
        )?;
        let trusted = self
            .store
            .snapshot()?
            .target_workspace
            .target_groups
            .iter()
            .find(|group| group.target_agent.id == agent.id)
            .and_then(|group| group.workspace_bindings.first())
            .and_then(|binding| {
                self.store
                    .list_projects()
                    .ok()?
                    .into_iter()
                    .find(|project| project.id == binding.project_id)
            })
            .map(|project| project.trusted)
            .unwrap_or(true);
        let project = self.store.create_project(
            &format!("{definition_name} — {draft_name}"),
            &worktree_path,
            trusted,
        )?;
        let binding = self.store.create_workspace_binding_with_id(
            workspace_binding_id,
            agent.id,
            project.id,
            &draft_name,
            &worktree_path,
            &[],
            Some(&branch_ref),
        )?;
        let timestamp = now_unix_ms();
        let draft = AgentDraftProjection {
            id: draft_id,
            target_agent_id: agent.id,
            workspace_binding_id: binding.id,
            name: definition_name,
            objective,
            acceptance_criteria,
            base_version: version.as_ref().map(|candidate| candidate.version.clone()),
            branch_ref,
            worktree_path,
            git_head,
            lifecycle: AgentDraftLifecycle::Active,
            cleanup_guidance: None,
            // A Draft cut from a Version starts without a choice; the Draft view
            // offers a ready Environment until someone picks one.
            environment_id: None,
            created_at_unix_ms: timestamp,
            updated_at_unix_ms: timestamp,
        };
        write_draft_manifest(&draft)?;
        let draft = self.store.create_agent_draft(&draft)?;
        self.store.open_work_item(
            agent.id,
            binding.id,
            Some(draft.id),
            Some(app_core::TargetWorkItemKind::AgentDraft),
            false,
        )?;
        let revision = self.store.snapshot()?.revision;
        Ok(DispatchResult {
            result: serde_json::to_value(AgentDraftMutationResultDto {
                draft: draft.clone(),
                workspace_binding: binding,
                project,
                revision,
            })?,
            event: Some((
                "targetWorkspace.changed".into(),
                revision,
                serde_json::to_value(draft)?,
            )),
        })
    }

    fn agent_draft_publish(&mut self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: PublishAgentDraftParams = decode_params(params)?;
        let draft = self.store.agent_draft(params.agent_draft_id)?;
        if self.store.agent_draft_has_live_run(draft.id)? {
            return Err(DispatchError::InvalidParams(
                "Create Version is unavailable while this Draft has a live Run".into(),
            ));
        }
        let definition_changed = draft
            .base_version
            .as_deref()
            .and_then(|base| {
                self.store
                    .target_agent_versions(draft.target_agent_id)
                    .ok()?
                    .into_iter()
                    .find(|version| version.version == base)
            })
            .map(|base| {
                base.name != draft.name
                    || base.objective != draft.objective
                    || base.acceptance_criteria != draft.acceptance_criteria
            })
            .unwrap_or(draft.base_version.is_none());
        if !definition_changed && !self.git.has_changes_except_manifest(&draft.worktree_path)? {
            return Err(DispatchError::InvalidParams(
                "This Draft has no substantive changes from its base".into(),
            ));
        }
        let current_head = self.git.head(&draft.worktree_path)?;
        let has_passing_run = self.store.snapshot()?.factory_runs.iter().any(|run| {
            run.agent_draft_id == draft.id
                && run.state == FactoryRunState::Passed
                && run.starting_git_head == current_head
                && run.objective == draft.objective
                && run.acceptance_criteria == draft.acceptance_criteria
        });
        if !has_passing_run && !params.confirm_without_passing_run {
            return Err(DispatchError::InvalidParams(
                "This Draft has no passing Run. Confirm to create the Version anyway".into(),
            ));
        }
        let versions = self.store.target_agent_versions(draft.target_agent_id)?;
        let version = next_version(draft.base_version.as_deref(), &versions, params.bump)?;
        let agent = self.store.target_agent(draft.target_agent_id)?;
        let tag = version_tag(draft.target_agent_id, &agent.name, &version);
        self.store.reserve_agent_draft_version(draft.id, &version)?;
        write_version_manifest(&draft, &version)?;
        let published = match self.git.publish(
            &draft.worktree_path,
            &tag,
            &format!("Create Agent Version v{version}"),
        ) {
            Ok(published) => published,
            Err(error) => {
                if self.git.head(&draft.worktree_path).ok().as_deref()
                    == Some(draft.git_head.as_str())
                {
                    self.store.restore_agent_draft(draft.id)?;
                    write_draft_manifest(&draft)?;
                }
                return Err(error.into());
            }
        };
        let version = self.store.finish_agent_draft_publication(
            draft.id,
            &version,
            &published.commit,
            &published.tag,
        )?;
        let cleanup_error = self.remove_draft_worktree(&draft, false).err();
        self.store.set_agent_draft_cleanup(
            draft.id,
            cleanup_error.is_some(),
            cleanup_error.as_ref().map(ToString::to_string).as_deref(),
        )?;
        let revision = self.store.snapshot()?.revision;
        Ok(DispatchResult {
            result: serde_json::to_value(AgentDraftPublishResultDto {
                version: version.clone(),
                cleanup_required: cleanup_error.is_some(),
                revision,
            })?,
            event: Some((
                "targetWorkspace.changed".into(),
                revision,
                serde_json::to_value(version)?,
            )),
        })
    }

    fn agent_draft_discard(&mut self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: AgentDraftIdParams = decode_params(params)?;
        let draft = self.store.agent_draft(params.agent_draft_id)?;
        if self.store.agent_draft_has_live_run(draft.id)? {
            return Err(DispatchError::InvalidParams(
                "Stop the live Run before discarding this Draft".into(),
            ));
        }
        if let Err(error) = self.remove_draft_worktree(&draft, true) {
            let _ = write_draft_manifest(&draft);
            return Err(error);
        }
        self.store.archive_agent_draft(draft.id)?;
        let revision = self.store.snapshot()?.revision;
        Ok(DispatchResult {
            result: serde_json::to_value(WorkspaceMutationResultDto { revision })?,
            event: Some((
                "targetWorkspace.changed".into(),
                revision,
                json!({"draftId": draft.id}),
            )),
        })
    }

    fn remove_draft_worktree(
        &mut self,
        draft: &AgentDraftProjection,
        discard: bool,
    ) -> Result<(), DispatchError> {
        // A deleted checkout is already clean from the perspective of Draft
        // cleanup. Keep discard available so the durable record can be
        // reconciled instead of trapping the user on a broken Draft.
        if !draft.worktree_path.exists() {
            return Ok(());
        }
        if discard {
            self.git.prepare_draft_discard(&draft.worktree_path)?;
        } else {
            self.git
                .prepare_clean_worktree_removal(&draft.worktree_path)?;
        }
        if !self.herdr.is_connected() {
            return Err(DispatchError::InvalidParams(format!(
                "Herdr is unavailable: {}",
                self.herdr.status().issues.join("; "),
            )));
        }
        let binding = self.store.workspace_binding(draft.workspace_binding_id)?;
        let agent = self.store.target_agent(draft.target_agent_id)?;
        let before = self.projection().ok();
        let workspace_label =
            herdr_sessions::workspace_label(&agent.name, &binding.name, binding.id);
        let workspace_id = match self.herdr.workspace_for_label(&workspace_label) {
            Some(workspace_id) => workspace_id.to_owned(),
            None => {
                self.herdr
                    .open_worktree(
                        &agent.repository_root,
                        &draft.worktree_path,
                        &workspace_label,
                        false,
                    )
                    .map_err(|error| DispatchError::Herdr(error.public_message()))?
                    .workspace
                    .workspace_id
            }
        };
        self.herdr
            .remove_worktree(&workspace_id, false)
            .map_err(|error| DispatchError::Herdr(error.public_message()))?;
        if draft.worktree_path.exists() {
            return Err(DispatchError::InvalidParams(format!(
                "Herdr reported success but the Draft checkout still exists at `{}`",
                draft.worktree_path.display(),
            )));
        }
        if let (Some(before), Ok(after)) = (before, self.projection()) {
            self.record_runtime_interruptions(&before, &after);
        }
        Ok(())
    }

    fn retry_authorized_draft_cleanup(&mut self) {
        let Ok(drafts) = self.store.agent_drafts_requiring_cleanup() else {
            return;
        };
        for draft in drafts {
            match self.remove_draft_worktree(&draft, false) {
                Ok(()) => {
                    let _ = self.store.set_agent_draft_cleanup(draft.id, false, None);
                }
                Err(error) => {
                    let _ = self.store.set_agent_draft_cleanup(
                        draft.id,
                        true,
                        Some(&error.to_string()),
                    );
                }
            }
        }
    }

    fn agent_session_create(&mut self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: CreateAgentSessionParams = decode_params(params)?;
        let binding = self.store.workspace_binding(params.workspace_binding_id)?;
        let target_agent = self.store.target_agent(params.target_agent_id)?;
        if binding.target_agent_id != params.target_agent_id {
            return Err(DispatchError::InvalidParams(
                "workspace binding does not belong to the target agent".into(),
            ));
        }
        let snapshot = self.projection()?;
        let environment = snapshot
            .environments
            .iter()
            .find(|environment| environment.id == params.environment_id)
            .cloned()
            .ok_or_else(|| DispatchError::InvalidParams("unknown Environment".into()))?;
        let harness_id = match params.purpose {
            HarnessPurpose::Orchestration | HarnessPurpose::Coding => {
                environment.coding_harness_id.clone()
            }
            HarnessPurpose::Evaluation => environment.evaluation_harness_id.clone(),
        };

        let existing = snapshot.agent_sessions.iter().find(|session| {
            session.target_agent_id == params.target_agent_id
                && session.workspace_binding_id == binding.id
                && session.purpose == params.purpose
                && session.environment_id == params.environment_id
                && session.factory_run_id.is_none()
                && session.outcome.is_none()
        });
        if let Some(existing) =
            existing.filter(|session| session.availability == SessionAvailability::Live)
        {
            let session = existing.clone();
            self.open_session_work_item(&session)?;
            return Ok(DispatchResult::response(serde_json::to_value(
                AgentSessionResultDto {
                    session,
                    reused: true,
                    revision: self.store.snapshot()?.revision,
                },
            )?));
        }

        let (llm_provider_snapshot, effective_model) = self
            .validate_new_agent_session_prerequisites(
                binding.project_id,
                &environment,
                &harness_id,
                params.model.as_deref(),
            )?;
        let now = now_unix_ms();
        let session_id = existing
            .map(|session| session.id)
            .unwrap_or_else(Uuid::new_v4);
        let proposed = AgentSessionProjection {
            id: session_id,
            target_agent_id: params.target_agent_id,
            workspace_binding_id: binding.id,
            project_id: binding.project_id,
            environment_id: environment.id.clone(),
            harness_id: harness_id.clone(),
            purpose: params.purpose,
            factory_run_id: None,
            parent_session_id: None,
            herdr_agent_name: existing
                .map(|session| session.herdr_agent_name.clone())
                .unwrap_or_else(|| {
                    herdr_sessions::agent_name(
                        params.purpose.agent_name_prefix(),
                        &format!("{}-{}", target_agent.name, binding.name),
                        session_id,
                    )
                }),
            availability: SessionAvailability::Reconnecting,
            lifecycle: None,
            placement: None,
            title: default_session_title(params.purpose, &target_agent.name, &binding.name),
            created_at_unix_ms: now,
            last_activity_at_unix_ms: now,
            llm_provider_snapshot: Some(llm_provider_snapshot),
            effective_model: Some(effective_model),
            attention: Vec::new(),
            initial_prompt: None,
            brief_delivered: false,
            outcome: None,
        };
        let (mut session, reused) = self.store.reserve_draft_agent_session(&proposed)?;
        session.harness_id = harness_id;
        session.llm_provider_snapshot = proposed.llm_provider_snapshot.clone();
        session.effective_model = proposed.effective_model.clone();
        match self.start_agent_session(
            &mut session,
            &binding,
            &environment,
            BTreeMap::new(),
            SessionPlacement::default(),
        ) {
            Ok(()) => {}
            Err(error) => {
                if !reused {
                    let _ = self.store.discard_unstarted_agent_session(session.id);
                }
                return Err(error);
            }
        }
        self.open_session_work_item(&session)?;
        Ok(DispatchResult::response(serde_json::to_value(
            AgentSessionResultDto {
                session,
                reused,
                revision: self.store.snapshot()?.revision,
            },
        )?))
    }

    fn open_session_work_item(
        &self,
        session: &AgentSessionProjection,
    ) -> Result<(), DispatchError> {
        self.store.open_work_item(
            session.target_agent_id,
            session.workspace_binding_id,
            Some(session.id),
            Some(match session.purpose {
                HarnessPurpose::Orchestration => app_core::TargetWorkItemKind::OrchestrationThread,
                HarnessPurpose::Coding => app_core::TargetWorkItemKind::CodingThread,
                HarnessPurpose::Evaluation => app_core::TargetWorkItemKind::EvaluationThread,
            }),
            false,
        )?;
        Ok(())
    }

    /// Create the pane, apply the Environment boundary, and start the agent.
    fn start_agent_session(
        &mut self,
        session: &mut AgentSessionProjection,
        binding: &WorkspaceBindingProjection,
        environment: &EnvironmentProjection,
        extra_environment: BTreeMap<String, String>,
        placement: SessionPlacement,
    ) -> Result<(), DispatchError> {
        let target_agent = self.store.target_agent(session.target_agent_id)?;
        let workspace_label =
            herdr_sessions::workspace_label(&target_agent.name, &binding.name, binding.id);
        let boundary = self.resolve_environment_boundary(&environment.id)?;
        let effective_model = session
            .effective_model
            .clone()
            .ok_or_else(|| DispatchError::InvalidParams("session has no model".into()))?;
        let provider_snapshot = session.llm_provider_snapshot.clone().ok_or_else(|| {
            DispatchError::InvalidParams("session has no Intelligence Provider".into())
        })?;

        let mut environment_variables = BTreeMap::new();
        boundary.environment_variables.for_each(|name, value| {
            environment_variables.insert(name.to_owned(), value.to_owned());
        });
        if let Some(path) = joined_search_path(&self.search_paths) {
            environment_variables.entry("PATH".into()).or_insert(path);
        }
        let gateway = {
            let provider = environment_llm_provider(provider_snapshot)?;
            let credential = provider
                .credential_ref
                .as_ref()
                .map(|reference| self.secret_store.read(reference))
                .transpose()?;
            let gateway = GatewayHandle::start(GatewayConfig {
                provider,
                model_id: effective_model.clone(),
                credential,
            })
            .map_err(|error| DispatchError::InvalidParams(error.to_string()))?;
            for (name, value) in gateway.anthropic_environment(&effective_model) {
                environment_variables.insert(name, value);
            }
            gateway
        };
        for (name, value) in herdr_provider_overrides() {
            environment_variables.insert(name.into(), value.into());
        }
        // An Orchestrator drives its own loop, so it is handed the control
        // endpoint and the token that scopes it to this run. Nothing else gets
        // them: a Coding or Evaluation agent has no business advancing a run.
        environment_variables.extend(extra_environment);
        if session.purpose == HarnessPurpose::Orchestration
            && let Some(directory) = control_cli_directory()
        {
            // Prepended, not replaced: the Environment's own PATH still decides
            // which agent executables the Orchestrator can reach.
            let inherited = environment_variables
                .get("PATH")
                .cloned()
                .unwrap_or_default();
            environment_variables.insert(
                "PATH".into(),
                format!("{}:{inherited}", directory.display()),
            );
        }

        let spec = AgentLaunchSpec {
            agent_name: session.herdr_agent_name.clone(),
            harness_id: session.harness_id.clone(),
            workspace_label,
            tab_label: placement.tab_label.unwrap_or_else(|| session.title.clone()),
            column_beside: placement.column_beside,
            cwd: binding.primary_root.clone(),
            environment: environment_variables,
            agent_args: harness_start_args(
                &session.harness_id,
                &effective_model,
                environment.permissions,
            ),
        };
        session.availability = SessionAvailability::Reconnecting;
        session.lifecycle = None;
        session.placement = None;
        session.last_activity_at_unix_ms = now_unix_ms();
        self.store.save_agent_session(session)?;

        match self.herdr.start(&spec) {
            Ok(started) => {
                session.placement = Some(started.placement);
                session.availability = SessionAvailability::Live;
                session.lifecycle = Some(if started.ready {
                    AgentLifecycle::Idle
                } else {
                    AgentLifecycle::Unknown
                });
                session.last_activity_at_unix_ms = now_unix_ms();
                self.gateways.insert(session.id, gateway);
                Ok(())
            }
            Err(error) => {
                let message = sanitize_public_diagnostic(&error.public_message());
                session.availability = SessionAvailability::Historical;
                session.lifecycle = None;
                session.placement = None;
                session.outcome = Some(ManagedSessionOutcome {
                    kind: ManagedSessionOutcomeKind::Failed,
                    summary: Some(message.clone()),
                    recorded_at_unix_ms: now_unix_ms(),
                });
                session.last_activity_at_unix_ms = now_unix_ms();
                self.store.save_agent_session(session)?;
                Err(DispatchError::InvalidParams(message))
            }
        }
    }

    fn validate_new_agent_session_prerequisites(
        &self,
        project_id: Uuid,
        environment: &EnvironmentProjection,
        harness_id: &str,
        requested_model: Option<&str>,
    ) -> Result<(ResolvedLlmProviderDto, String), DispatchError> {
        let project = self
            .store
            .list_projects()?
            .into_iter()
            .find(|project| project.id == project_id)
            .ok_or_else(|| DispatchError::InvalidParams("unknown project".into()))?;
        if !project.trusted {
            return Err(DispatchError::InvalidParams(format!(
                "Project `{}` is not trusted. Trust `{}` before starting a session; an agent runs with full access to that directory.",
                project.name,
                project.root.display(),
            )));
        }
        if environment.readiness.state != EnvironmentReadinessState::Ready {
            return Err(DispatchError::InvalidParams(format!(
                "Environment `{}` needs setup: {}",
                environment.name,
                environment.readiness.issues.join("; "),
            )));
        }
        if !self.herdr.is_connected() {
            return Err(DispatchError::InvalidParams(format!(
                "Herdr is unavailable: {}",
                self.herdr.status().issues.join("; ")
            )));
        }
        match self.herdr.harness(harness_id) {
            Some(harness) if harness.readiness == app_core::HarnessReadinessState::Ready => {}
            Some(harness) => {
                return Err(DispatchError::InvalidParams(format!(
                    "Harness `{harness_id}` is not usable: {}",
                    harness.guidance
                )));
            }
            None => {
                return Err(DispatchError::InvalidParams(format!(
                    "Herdr does not provide the Harness `{harness_id}` this Environment selects"
                )));
            }
        }
        let provider_snapshot = environment.resolved_llm.clone().ok_or_else(|| {
            DispatchError::InvalidParams("Environment has no Intelligence Provider".into())
        })?;
        let effective_model =
            resolve_model(Some(&provider_snapshot), requested_model).map_err(|error| {
                DispatchError::InvalidParams(format!("Invalid session model: {error}"))
            })?;
        let provider = environment_llm_provider(provider_snapshot.clone())?;
        let credential = provider
            .credential_ref
            .as_ref()
            .map(|reference| self.secret_store.read(reference))
            .transpose()?;
        let discovered = discover_models(&provider, credential).map_err(|error| {
            DispatchError::InvalidParams(format!(
                "Could not verify model `{effective_model}` with the Environment Intelligence Provider: {error}"
            ))
        })?;
        if !discovered.iter().any(|model| model == &effective_model) {
            return Err(DispatchError::InvalidParams(format!(
                "Environment model `{effective_model}` is not available from model discovery"
            )));
        }
        Ok((provider_snapshot, effective_model))
    }

    /// Prefix the Environment's default Agent Skills to a prompt.
    ///
    /// Herdr agents read their own configuration, so skills the Environment
    /// declares are delivered as prompt context rather than a protocol payload.
    fn with_default_skills(
        &self,
        environment_id: &str,
        user_text: String,
    ) -> Result<String, DispatchError> {
        let environment = self.environments.get(environment_id).ok_or_else(|| {
            DispatchError::InvalidParams("session environment is unavailable".into())
        })?;
        let plan = environment
            .descriptor
            .resolve_plugin_plan(&self.plugin_store)?;
        let mut prefix = String::new();
        for skill in plan.default_skills {
            let metadata = std::fs::metadata(&skill.skill_file)?;
            if metadata.len() > MAX_DEFAULT_SKILL_FILE_BYTES {
                return Err(DispatchError::DefaultSkillTooLarge(skill.name));
            }
            let content = std::fs::read_to_string(&skill.skill_file)?;
            let section = format!(
                "## Default skill: {}/{}\n{}\n\n{}\n\n",
                skill.plugin_name, skill.name, skill.description, content
            );
            if prefix.len().saturating_add(section.len()) > MAX_DEFAULT_SKILL_PREFIX_BYTES {
                return Err(DispatchError::DefaultSkillPrefixTooLarge);
            }
            prefix.push_str(&section);
        }
        if prefix.is_empty() {
            return Ok(user_text);
        }
        Ok(format!(
            "The session Environment provides these default Agent Skills:\n\n{prefix}## User request\n{user_text}"
        ))
    }

    fn agent_session(&self, session_id: Uuid) -> Result<AgentSessionProjection, DispatchError> {
        self.projection()?
            .agent_sessions
            .into_iter()
            .find(|session| session.id == session_id)
            .ok_or_else(|| DispatchError::InvalidParams("unknown agent session".into()))
    }

    fn live_placement(&self, session_id: Uuid) -> Result<HerdrPlacement, DispatchError> {
        let session = self.agent_session(session_id)?;
        if session.availability != SessionAvailability::Live {
            return Err(DispatchError::InvalidParams(
                "this session is not live in a fresh Herdr snapshot".into(),
            ));
        }
        session.placement.ok_or_else(|| {
            DispatchError::InvalidParams("this session has no agent running in Herdr".into())
        })
    }

    fn agent_session_prompt(&mut self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: PromptAgentSessionParams = decode_params(params)?;
        require_prompt_text(&params.text)?;
        let mut session = self.agent_session(params.agent_session_id)?;
        self.require_focused_agent_session(session.id)?;
        let placement = self.live_placement(session.id)?;
        if !session
            .lifecycle
            .is_some_and(AgentLifecycle::accepts_prompt)
        {
            return Err(DispatchError::SessionBusy(session.id));
        }
        let prompt = self.with_default_skills(&session.environment_id, params.text.clone())?;
        self.herdr
            .prompt(&placement, &prompt)
            .map_err(|error| DispatchError::Herdr(error.public_message()))?;

        if let Some(title) = provisional_session_title(&params.text) {
            session.title = title;
        }
        if session.initial_prompt.is_none() {
            session.initial_prompt = Some(params.text.clone());
        }
        session.brief_delivered = true;
        session.last_activity_at_unix_ms = now_unix_ms();
        self.store.save_agent_session(&session)?;
        Ok(DispatchResult::response(serde_json::to_value(
            AgentSessionAcceptedResultDto {
                agent_session_id: session.id,
                accepted: true,
            },
        )?))
    }

    fn agent_session_interrupt(&mut self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: AgentSessionIdParams = decode_params(params)?;
        let placement = self.live_placement(params.agent_session_id)?;
        self.herdr
            .interrupt(&placement)
            .map_err(|error| DispatchError::Herdr(error.public_message()))?;
        Ok(DispatchResult::response(serde_json::to_value(
            AgentSessionInterruptResultDto {
                agent_session_id: params.agent_session_id,
                interrupted: true,
            },
        )?))
    }

    /// Approval prompts belong to the agent's own interface. A blocked session
    /// is answered by forwarding keys to it, not by a separate protocol.
    fn agent_session_send_keys(&mut self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: SendAgentKeysParams = decode_params(params)?;
        if params.keys.is_empty() || params.keys.len() > 8 {
            return Err(DispatchError::InvalidParams(
                "between one and eight keys are required".into(),
            ));
        }
        let placement = self.live_placement(params.agent_session_id)?;
        self.herdr
            .send_keys(&placement, &params.keys)
            .map_err(|error| DispatchError::Herdr(error.public_message()))?;
        Ok(DispatchResult::response(serde_json::to_value(
            AgentSessionAcceptedResultDto {
                agent_session_id: params.agent_session_id,
                accepted: true,
            },
        )?))
    }

    fn agent_session_transcript(&mut self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: ReadAgentTranscriptParams = decode_params(params)?;
        let placement = match self.live_placement(params.agent_session_id) {
            Ok(placement) => placement,
            Err(_) => return self.empty_agent_transcript(params.agent_session_id),
        };
        let (text, revision, truncated) = self
            .herdr
            .transcript(&placement, params.lines)
            .map_err(|error| DispatchError::Herdr(error.public_message()))?;
        Ok(DispatchResult::response(serde_json::to_value(
            AgentTranscriptResultDto {
                transcript: app_core::AgentTranscriptProjection {
                    agent_session_id: params.agent_session_id,
                    text,
                    revision,
                    truncated,
                    captured_at_unix_ms: now_unix_ms(),
                },
            },
        )?))
    }

    fn agent_session_screen(&mut self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: ReadAgentTranscriptParams = decode_params(params)?;
        let placement = match self.live_placement(params.agent_session_id) {
            Ok(placement) => placement,
            Err(_) => return self.empty_agent_transcript(params.agent_session_id),
        };
        let (text, revision, truncated) = self
            .herdr
            .screen(&placement)
            .map_err(|error| DispatchError::Herdr(error.public_message()))?;
        Ok(DispatchResult::response(serde_json::to_value(
            AgentTranscriptResultDto {
                transcript: app_core::AgentTranscriptProjection {
                    agent_session_id: params.agent_session_id,
                    text,
                    revision,
                    truncated,
                    captured_at_unix_ms: now_unix_ms(),
                },
            },
        )?))
    }

    fn empty_agent_transcript(
        &self,
        agent_session_id: Uuid,
    ) -> Result<DispatchResult, DispatchError> {
        Ok(DispatchResult::response(serde_json::to_value(
            AgentTranscriptResultDto {
                transcript: app_core::AgentTranscriptProjection {
                    agent_session_id,
                    text: String::new(),
                    revision: 0,
                    truncated: false,
                    captured_at_unix_ms: now_unix_ms(),
                },
            },
        )?))
    }

    fn agent_session_input(&mut self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: WriteAgentSessionParams = decode_params(params)?;
        let text = params.text.as_deref().filter(|value| !value.is_empty());
        if text.is_none()
            && params.keys.is_empty()
            && params.cols.is_none()
            && params.rows.is_none()
        {
            return Err(DispatchError::InvalidParams(
                "agent input requires text, keys, or a resize".into(),
            ));
        }
        if params.keys.len() > 8 {
            return Err(DispatchError::InvalidParams(
                "at most eight keys can be sent at once".into(),
            ));
        }
        let placement = self.live_placement(params.agent_session_id)?;
        self.herdr
            .write(&placement, text, &params.keys, params.cols, params.rows)
            .map_err(|error| DispatchError::Herdr(error.public_message()))?;
        Ok(DispatchResult::response(serde_json::to_value(
            AgentSessionAcceptedResultDto {
                agent_session_id: params.agent_session_id,
                accepted: true,
            },
        )?))
    }

    fn agent_session_focus(&mut self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: AgentSessionIdParams = decode_params(params)?;
        let placement = self.live_placement(params.agent_session_id)?;
        self.herdr
            .focus(&placement)
            .map_err(|error| DispatchError::Herdr(error.public_message()))?;
        Ok(DispatchResult::response(serde_json::to_value(
            AgentSessionAcceptedResultDto {
                agent_session_id: params.agent_session_id,
                accepted: true,
            },
        )?))
    }

    fn agent_session_stop(&mut self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: AgentSessionIdParams = decode_params(params)?;
        self.stop_agent_session(params.agent_session_id)?;
        Ok(DispatchResult::response(serde_json::to_value(
            AgentSessionStopResultDto {
                agent_session_id: params.agent_session_id,
                stopped: true,
            },
        )?))
    }

    fn stop_agent_session(&mut self, session_id: Uuid) -> Result<(), DispatchError> {
        let observed = self.agent_session(session_id)?;
        if observed.availability != SessionAvailability::Live {
            return Err(DispatchError::InvalidParams(
                "this session is not live in a fresh Herdr snapshot".into(),
            ));
        }
        let placement = observed.placement.ok_or_else(|| {
            DispatchError::InvalidParams("this session has no agent running in Herdr".into())
        })?;
        let mut session = self
            .store
            .snapshot()?
            .agent_sessions
            .into_iter()
            .find(|session| session.id == session_id)
            .ok_or_else(|| DispatchError::InvalidParams("unknown agent session".into()))?;
        self.herdr
            .stop_managed_agent(&session.herdr_agent_name, Some(&placement))
            .map_err(|error| DispatchError::Herdr(error.public_message()))?;
        self.record_stopped_agent_session(&mut session, true)
    }

    fn record_stopped_agent_session(
        &mut self,
        session: &mut AgentSessionProjection,
        replace_outcome: bool,
    ) -> Result<(), DispatchError> {
        self.gateways.remove(&session.id);
        self.pending_prompt_retry_at.remove(&session.id);
        session.availability = SessionAvailability::Historical;
        session.lifecycle = None;
        session.placement = None;
        session.attention.clear();
        if replace_outcome || session.outcome.is_none() {
            session.last_activity_at_unix_ms = now_unix_ms();
            session.outcome = Some(ManagedSessionOutcome {
                kind: ManagedSessionOutcomeKind::Stopped,
                summary: None,
                recorded_at_unix_ms: session.last_activity_at_unix_ms,
            });
        }
        self.store.save_agent_session(session)?;
        Ok(())
    }

    fn factory_run_create(&mut self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: CreateFactoryRunParams = decode_params(params)?;
        self.create_run(
            params.run_id,
            params.agent_draft_id,
            &params.environment_id,
            &params.objective,
        )
    }

    fn create_run(
        &mut self,
        run_id: Uuid,
        agent_draft_id: Uuid,
        environment_id: &str,
        objective: &str,
    ) -> Result<DispatchResult, DispatchError> {
        let draft = self.store.agent_draft(agent_draft_id)?;
        if draft.lifecycle != AgentDraftLifecycle::Active {
            return Err(DispatchError::InvalidParams(
                "only an active Draft can start a Run".into(),
            ));
        }
        Self::require_existing_draft_checkout(&draft)?;
        if let Some(existing) = self.live_run_for_draft(draft.id)? {
            return self.resume_or_attach_orchestrator(existing);
        }
        let binding = self.store.workspace_binding(draft.workspace_binding_id)?;
        self.require_trusted_project(binding.project_id)?;
        self.require_ready_environment(environment_id)?;
        let objective = normalize_required("Run objective", objective, MAX_FACTORY_PROMPT_BYTES)?;
        let mut run = FactoryRun::new(FactoryRunInput {
            target_agent_id: draft.target_agent_id,
            agent_draft_id: draft.id,
            workspace_binding_id: binding.id,
            project_id: binding.project_id,
            environment_id: environment_id.into(),
            objective,
            acceptance_criteria: draft.acceptance_criteria,
            starting_git_head: self.git.head(&draft.worktree_path)?,
        })?;
        run.id = run_id;
        self.store.save_factory_run(&run)?;
        self.store.open_work_item(
            draft.target_agent_id,
            binding.id,
            Some(run.id),
            Some(app_core::TargetWorkItemKind::FactoryRun),
            false,
        )?;
        self.start_orchestrator_session(run)
    }

    fn factory_run_cancel(&mut self, params: Value) -> Result<DispatchResult, DispatchError> {
        self.run_cancel(params)
    }

    fn agent_draft_open_workspace(
        &mut self,
        params: Value,
    ) -> Result<DispatchResult, DispatchError> {
        let params: AgentDraftIdParams = decode_params(params)?;
        let draft = self.store.agent_draft(params.agent_draft_id)?;
        if draft.lifecycle != AgentDraftLifecycle::Active {
            return Err(DispatchError::InvalidParams(
                "only an active Draft can open its Herdr workspace".into(),
            ));
        }
        Self::require_existing_draft_checkout(&draft)?;
        if !self.herdr.is_connected() {
            return Err(DispatchError::Herdr(self.herdr.status().issues.join("; ")));
        }
        let binding = self.store.workspace_binding(draft.workspace_binding_id)?;
        self.require_trusted_project(binding.project_id)?;
        let target_agent = self.store.target_agent(binding.target_agent_id)?;
        let label = herdr_sessions::workspace_label(&target_agent.name, &binding.name, binding.id);
        let opened = self
            .herdr
            .open_worktree(
                &target_agent.repository_root,
                &binding.primary_root,
                &label,
                true,
            )
            .map_err(|error| DispatchError::Herdr(error.public_message()))?;
        let terminal = self
            .herdr
            .workspace_terminal_launch()
            .map_err(|error| DispatchError::Herdr(error.public_message()))?;
        let revision = self.store.snapshot()?.revision;
        Ok(DispatchResult::response(serde_json::to_value(
            AgentDraftWorkspaceResultDto {
                agent_draft_id: draft.id,
                workspace_id: opened.workspace.workspace_id,
                label,
                already_open: opened.already_open,
                terminal: HerdrWorkspaceTerminalLaunchDto {
                    executable: terminal.executable,
                    arguments: terminal.arguments,
                },
                revision,
            },
        )?))
    }

    fn require_existing_draft_checkout(draft: &AgentDraftProjection) -> Result<(), DispatchError> {
        if draft.worktree_path.is_dir() {
            return Ok(());
        }
        Err(DispatchError::InvalidParams(format!(
            "The Draft checkout no longer exists at `{}`. Discard this Draft and create a new one from an existing repository.",
            draft.worktree_path.display(),
        )))
    }

    fn workspace_pane_open(
        &mut self,
        params: Value,
        open_to_side: bool,
    ) -> Result<DispatchResult, DispatchError> {
        let params: OpenWorkspaceItemParams = decode_params(params)?;
        self.store.open_work_item(
            params.target_agent_id,
            params.workspace_binding_id,
            params.work_item_id,
            params.work_item_kind,
            open_to_side,
        )?;
        self.workspace_mutation_dispatch()
    }

    fn workspace_pane_focus(&mut self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: WorkspacePaneIdParams = decode_params(params)?;
        self.store.focus_workspace_pane(params.workspace_pane_id)?;
        self.workspace_mutation_dispatch()
    }

    fn workspace_pane_close(&self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: WorkspacePaneIdParams = decode_params(params)?;
        self.store.close_workspace_pane(params.workspace_pane_id)?;
        self.workspace_mutation_dispatch()
    }

    fn workspace_pane_resize(&self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: ResizeWorkspacePanesParams = decode_params(params)?;
        let layout = params
            .layout
            .into_iter()
            .map(|item| (item.workspace_pane_id, item.width_basis_points))
            .collect::<Vec<_>>();
        self.store.resize_workspace_panes(&layout)?;
        self.workspace_mutation_dispatch()
    }

    fn workspace_pane_move(&self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: MoveWorkspacePaneParams = decode_params(params)?;
        self.store
            .move_workspace_pane(params.workspace_pane_id, params.position)?;
        self.workspace_mutation_dispatch()
    }

    fn workspace_pane_set_dock(&self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: SetWorkspaceDockParams = decode_params(params)?;
        self.store.set_work_context_dock(
            params.work_context_id,
            params.dock,
            params.dock_percent,
        )?;
        self.workspace_mutation_dispatch()
    }

    fn workspace_mutation_dispatch(&self) -> Result<DispatchResult, DispatchError> {
        let snapshot = self.store.snapshot()?;
        Ok(DispatchResult {
            result: serde_json::to_value(WorkspaceMutationResultDto {
                revision: snapshot.revision,
            })?,
            event: Some((
                "targetWorkspace.changed".into(),
                snapshot.revision,
                serde_json::to_value(snapshot.target_workspace)?,
            )),
        })
    }

    fn require_focused_agent_session(&self, agent_session_id: Uuid) -> Result<(), DispatchError> {
        let snapshot = self.store.snapshot()?;
        let workspace = &snapshot.target_workspace;
        let focused_pane_id = workspace
            .focused_pane_id
            .ok_or_else(|| DispatchError::InvalidParams("no focused workspace pane".into()))?;
        let pane = workspace
            .panes
            .iter()
            .find(|pane| pane.id == focused_pane_id)
            .ok_or_else(|| {
                DispatchError::InvalidParams("focused workspace pane is missing".into())
            })?;
        let context = workspace
            .work_contexts
            .iter()
            .find(|context| context.id == pane.work_context_id)
            .ok_or_else(|| {
                DispatchError::InvalidParams("focused work context is missing".into())
            })?;
        if context.work_item_id != Some(agent_session_id)
            || !matches!(
                context.work_item_kind,
                Some(
                    app_core::TargetWorkItemKind::CodingThread
                        | app_core::TargetWorkItemKind::EvaluationThread
                )
            )
        {
            return Err(DispatchError::InvalidParams(
                "the prompted session must be the focused pane work item".into(),
            ));
        }
        Ok(())
    }

    fn settings_set_theme(&self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: SetThemeParams = decode_params(params)?;
        let settings = self.store.set_theme(params.theme)?;
        self.settings_dispatch(settings)
    }

    fn settings_set_notifications(&self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: SetNotificationsParams = decode_params(params)?;
        let settings = self.store.set_native_notifications(params.enabled)?;
        self.settings_dispatch(settings)
    }

    fn settings_set_layout(&self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: SetLayoutParams = decode_params(params)?;
        let settings = self
            .store
            .set_layout(params.inspector_percent, params.terminal_percent)?;
        self.settings_dispatch(settings)
    }

    fn settings_dispatch(
        &self,
        settings: app_core::SettingsProjection,
    ) -> Result<DispatchResult, DispatchError> {
        let revision = self.store.snapshot()?.revision;
        let payload = serde_json::to_value(&settings)?;
        Ok(DispatchResult {
            result: serde_json::to_value(SettingsResultDto { settings, revision })?,
            event: Some(("settings.changed".into(), revision, payload)),
        })
    }

    fn llm_providers_result(&self) -> Result<LlmProvidersResultDto, DispatchError> {
        let snapshot = self.store.snapshot()?;
        Ok(LlmProvidersResultDto {
            providers: snapshot.llm_providers,
            environments: snapshot.environments,
            revision: snapshot.revision,
        })
    }

    fn llm_provider_mutation_result(&self) -> Result<DispatchResult, DispatchError> {
        let providers = self.llm_providers_result()?;
        let revision = providers.revision;
        let payload = serde_json::to_value(&providers)?;
        Ok(DispatchResult {
            result: payload.clone(),
            event: Some(("llmProvider.changed".into(), revision, payload)),
        })
    }

    fn llm_provider_create(&self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: CreateLlmProviderParams = decode_params(params)?;
        let providers = self.store.list_llm_providers()?;
        ensure_unique_provider_name(&providers, None, &params.configuration.name)?;
        let provider = project_llm_provider(
            Uuid::new_v4(),
            params.configuration,
            &self.available_secret_refs()?,
        )?;
        let provider_id = provider.id;
        self.store.put_llm_provider(&provider, &[])?;
        self.synchronize_environment_readiness()?;
        let providers = self.llm_providers_result()?;
        let revision = providers.revision;
        let payload = serde_json::to_value(&providers)?;
        Ok(DispatchResult {
            result: serde_json::to_value(LlmProviderCreateResultDto {
                provider_id,
                providers: providers.providers,
                environments: providers.environments,
                revision,
            })?,
            event: Some(("llmProvider.changed".into(), revision, payload)),
        })
    }

    fn llm_provider_configuration_set(
        &self,
        params: Value,
    ) -> Result<DispatchResult, DispatchError> {
        let params: SetLlmProviderConfigurationParams = decode_params(params)?;
        let providers = self.store.list_llm_providers()?;
        let existing = providers
            .iter()
            .find(|provider| provider.id == params.provider_id)
            .ok_or_else(|| DispatchError::InvalidParams("unknown Intelligence Provider".into()))?;
        ensure_unique_provider_name(
            &providers,
            Some(params.provider_id),
            &params.configuration.name,
        )?;
        let provider = project_llm_provider(
            params.provider_id,
            params.configuration,
            &self.available_secret_refs()?,
        )?;
        let execution_affecting = provider_execution_fields_changed(existing, &provider);
        let affected_environment_ids = if execution_affecting {
            let mut providers_after = providers.clone();
            if let Some(current) = providers_after
                .iter_mut()
                .find(|candidate| candidate.id == params.provider_id)
            {
                *current = provider.clone();
            }
            self.environments
                .iter()
                .filter_map(|(environment_id, environment)| {
                    let policy = environment.descriptor.llm.as_ref()?;
                    (policy.provider_id == params.provider_id
                        && resolve_environment_llm(policy, &providers_after).is_err())
                    .then(|| environment_id.to_string())
                })
                .collect()
        } else {
            Vec::new()
        };
        self.store
            .put_llm_provider(&provider, &affected_environment_ids)?;
        self.synchronize_environment_readiness()?;
        self.llm_provider_mutation_result()
    }

    fn llm_provider_delete(&mut self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: DeleteLlmProviderParams = decode_params(params)?;
        let provider = self
            .store
            .list_llm_providers()?
            .into_iter()
            .find(|provider| provider.id == params.provider_id)
            .ok_or_else(|| DispatchError::InvalidParams("unknown Intelligence Provider".into()))?;
        let linked_environment_ids = self.linked_environment_ids(params.provider_id);

        // The authored Environment files are unlinked first. If a later store
        // operation fails, the provider still exists and no dangling provider
        // reference can have been committed.
        for environment_id in &linked_environment_ids {
            let descriptor = self
                .environments
                .get(environment_id)
                .ok_or_else(|| DispatchError::InvalidParams("unknown Environment".into()))?
                .descriptor
                .clone();
            let mut draft = environment_draft_from_descriptor(&descriptor);
            draft.llm = None;
            self.environments
                .save_configuration(environment_id, draft)?;
        }
        self.store
            .put_llm_provider(&provider, &linked_environment_ids)?;
        self.synchronize_environment_readiness()?;
        self.store.delete_llm_provider(params.provider_id)?;
        self.synchronize_environment_readiness()?;
        self.llm_provider_mutation_result()
    }

    fn linked_environment_ids(&self, provider_id: Uuid) -> Vec<String> {
        self.environments
            .iter()
            .filter(|(_, environment)| {
                environment
                    .descriptor
                    .llm
                    .as_ref()
                    .map(|llm| llm.provider_id)
                    == Some(provider_id)
            })
            .map(|(environment_id, _)| environment_id.to_owned())
            .collect()
    }

    fn validate_environment_plugins(
        &self,
        environment_id: &str,
        plugins: &[environment_runtime::EnvironmentPlugin],
    ) -> Result<(), DispatchError> {
        let selection = EnvironmentPluginSelection {
            environment_id: environment_id.to_owned(),
            plugins: plugins
                .iter()
                .map(|plugin| EnvironmentPluginEntry {
                    name: plugin.name.clone(),
                    enabled_mcp_servers: Some(plugin.enabled_mcp_servers.clone()),
                    default_skills: plugin.default_skills.clone(),
                })
                .collect(),
        };
        self.plugin_store.resolve_environment_plugins(&selection)?;
        Ok(())
    }

    fn secret_metadata_dto(
        &self,
        metadata: platform_secrets::SecretMetadata,
        providers: &[LlmProviderDto],
    ) -> SecretMetadataDto {
        let reference = metadata.reference.to_string();
        let mut referenced_by = providers
            .iter()
            .filter(|provider| provider.credential_ref.as_deref() == Some(reference.as_str()))
            .map(|provider| SecretEnvironmentReferenceDto {
                environment_id: provider.id.to_string(),
                environment_name: provider.name.clone(),
                kind: SecretEnvironmentReferenceKind::LlmProvider,
                label: "Intelligence Provider".into(),
            })
            .chain(self
            .environments
            .iter()
            .flat_map(|(environment_id, environment)| {
                let mut references = Vec::new();
                references.extend(
                    environment
                        .descriptor
                        .environment_variables
                        .iter()
                        .filter_map(|(name, value)| match value {
                            environment_runtime::EnvironmentValue::Secret(value)
                                if value.secret_ref.to_string() == reference =>
                            {
                                Some(SecretEnvironmentReferenceDto {
                                    environment_id: environment_id.to_owned(),
                                    environment_name: environment.descriptor.name.clone(),
                                    kind: SecretEnvironmentReferenceKind::HarnessEnvironmentVariable,
                                    label: name.clone(),
                                })
                            }
                            _ => None,
                        }),
                );
                references
            }))
            .collect::<Vec<_>>();
        referenced_by.sort_by(|left, right| {
            left.environment_name
                .cmp(&right.environment_name)
                .then_with(|| left.label.cmp(&right.label))
        });
        SecretMetadataDto {
            secret_ref: reference,
            label: metadata.label,
            kind: CredentialKindDto::ApiToken,
            referenced_by,
            created_at_unix_ms: metadata.created_at_unix_ms,
            updated_at_unix_ms: metadata.updated_at_unix_ms,
        }
    }

    fn secret_list(&self) -> Result<DispatchResult, DispatchError> {
        let snapshot = self.store.snapshot()?;
        let mut secrets = self
            .secret_store
            .list_metadata()?
            .into_iter()
            .map(|metadata| self.secret_metadata_dto(metadata, &snapshot.llm_providers))
            .collect::<Vec<_>>();
        secrets.sort_by(|left, right| {
            left.label
                .cmp(&right.label)
                .then_with(|| left.secret_ref.cmp(&right.secret_ref))
        });
        Ok(DispatchResult::response(serde_json::to_value(
            SecretListDto { secrets },
        )?))
    }

    fn secret_create(&self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: CreateSecretParams = decode_params(params)?;
        self.secret_store
            .create(&params.label, SecretValue::new(params.value.into_bytes())?)?;
        self.secret_list()
    }

    fn secret_replace(&self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: ReplaceSecretParams = decode_params(params)?;
        let reference = params.secret_ref.parse::<SecretRef>()?;
        self.secret_store
            .replace(&reference, SecretValue::new(params.value.into_bytes())?)?;
        self.secret_list()
    }

    fn secret_delete(&self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: DeleteSecretParams = decode_params(params)?;
        let reference = params.secret_ref.parse::<SecretRef>()?;
        let provider_uses_secret =
            self.store.list_llm_providers()?.iter().any(|provider| {
                provider.credential_ref.as_deref() == Some(params.secret_ref.as_str())
            });
        if provider_uses_secret
            || self.environments.iter().any(|(_, environment)| {
                environment
                    .descriptor
                    .environment_variables
                    .values()
                    .any(|value| {
                        matches!(
                            value,
                            environment_runtime::EnvironmentValue::Secret(value)
                                if value.secret_ref == reference
                        )
                    })
            })
        {
            return Err(DispatchError::InvalidParams(
                "credential is referenced by an Environment or Intelligence Provider; reconfigure it first".into(),
            ));
        }
        self.secret_store.delete(&reference)?;
        self.synchronize_environment_readiness()?;
        let response = self.secret_list()?;
        let revision = self.store.snapshot()?.revision;
        Ok(DispatchResult {
            result: response.result,
            event: Some((
                "environment.changed".into(),
                revision,
                json!({"reason":"secret_deleted"}),
            )),
        })
    }

    fn registry_list(&self) -> Result<DispatchResult, DispatchError> {
        let registries = self
            .store
            .list_plugin_registries()?
            .into_iter()
            .map(plugin_registry_dto)
            .collect();
        Ok(DispatchResult::response(serde_json::to_value(
            PluginRegistryListDto { registries },
        )?))
    }

    fn registry_put(&self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: PutPluginRegistryParams = decode_params(params)?;
        plugin_runtime::validate_registry_url(&params.catalog_url)?;
        plugin_runtime::validate_registry_url(&params.signature_url)?;
        decode_registry_public_key(&params.public_key_base64)?;
        self.store.put_plugin_registry(&PluginRegistryRecord {
            id: params.id,
            catalog_url: params.catalog_url,
            signature_url: params.signature_url,
            public_key_base64: params.public_key_base64,
        })?;
        self.registry_list()
    }

    fn registry_delete(&self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: RegistryIdParams = decode_params(params)?;
        self.store.delete_plugin_registry(&params.registry_id)?;
        self.registry_list()
    }

    fn registry_refresh(&self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: RegistryIdParams = decode_params(params)?;
        let registry = self.registry_record(&params.registry_id)?;
        let client = RegistryClient::new(
            HttpsRegistryDownloader::default(),
            self.plugin_store.clone(),
            decode_registry_public_key(&registry.public_key_base64)?,
        );
        let verified = client.fetch_catalog(&registry.catalog_url, &registry.signature_url)?;
        let catalog = verified.catalog();
        Ok(DispatchResult::response(serde_json::to_value(
            RegistryCatalogDto {
                registry_id: registry.id.clone(),
                generated_at: catalog.generated_at.clone(),
                plugins: catalog
                    .plugins
                    .iter()
                    .map(|plugin| RegistryCatalogPluginDto {
                        id: plugin.id.clone(),
                        name: plugin.name.clone(),
                        version: plugin.version.clone(),
                        description: plugin.description.clone(),
                        source_url: registry_plugin_source_url(&registry, &plugin.id),
                    })
                    .collect(),
            },
        )?))
    }

    fn plugin_install(&self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: InstallPluginParams = decode_params(params)?;
        let registry = self.registry_record(&params.registry_id)?;
        let client = RegistryClient::new(
            HttpsRegistryDownloader::default(),
            self.plugin_store.clone(),
            decode_registry_public_key(&registry.public_key_base64)?,
        );
        let verified = client.fetch_catalog(&registry.catalog_url, &registry.signature_url)?;
        let entry = verified
            .plugin_by_id(&params.plugin_id)
            .ok_or_else(|| DispatchError::InvalidParams("unknown registry plugin".into()))?;
        client.download_and_install(entry)?;
        self.plugin_list()
    }

    fn plugin_details(&self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: PluginDetailsParams = decode_params(params)?;
        let registry = self.registry_record(&params.registry_id)?;
        let client = RegistryClient::new(
            HttpsRegistryDownloader::default(),
            self.plugin_store.clone(),
            decode_registry_public_key(&registry.public_key_base64)?,
        );
        let verified = client.fetch_catalog(&registry.catalog_url, &registry.signature_url)?;
        let entry = verified
            .plugin_by_id(&params.plugin_id)
            .ok_or_else(|| DispatchError::InvalidParams("unknown registry plugin".into()))?;
        let catalog_entry = entry.entry().clone();
        let inspected = client.download_and_inspect(entry)?;
        let (skills, mcp_servers, mcp_disabled_reason) =
            project_plugin_components(&inspected.skills, &inspected.mcp);
        Ok(DispatchResult::response(serde_json::to_value(
            PluginDetailsDto {
                registry_id: registry.id.clone(),
                plugin_id: catalog_entry.id.clone(),
                name: inspected.manifest.name,
                version: inspected.manifest.version.unwrap_or(catalog_entry.version),
                description: inspected.manifest.description.or(catalog_entry.description),
                author_name: inspected.manifest.author.and_then(|author| author.name),
                source_url: registry_plugin_source_url(&registry, &catalog_entry.id),
                skills,
                mcp_servers,
                mcp_disabled_reason,
            },
        )?))
    }

    fn plugin_rollback(&self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: PluginNameParams = decode_params(params)?;
        self.plugin_store.rollback(&params.plugin_name)?;
        self.synchronize_environment_readiness()?;
        self.plugin_list()
    }

    fn plugin_uninstall(&mut self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: PluginNameParams = decode_params(params)?;
        let environment_ids = self
            .environments
            .iter()
            .map(|(environment_id, _)| environment_id.to_string())
            .collect::<Vec<_>>();
        for environment_id in environment_ids {
            let descriptor = self
                .environments
                .get(&environment_id)
                .ok_or_else(|| DispatchError::InvalidParams("unknown Environment".into()))?
                .descriptor
                .clone();
            if !descriptor
                .plugins
                .iter()
                .any(|plugin| plugin.name == params.plugin_name)
            {
                continue;
            }
            let mut draft = environment_draft_from_descriptor(&descriptor);
            draft
                .plugins
                .retain(|plugin| plugin.name != params.plugin_name);
            self.environments
                .save_configuration(&environment_id, draft)?;
        }
        self.plugin_store.uninstall(&params.plugin_name)?;
        self.synchronize_environment_readiness()?;
        self.plugin_list()
    }

    fn plugin_list(&self) -> Result<DispatchResult, DispatchError> {
        let installed = self
            .plugin_store
            .list_installed()?
            .into_iter()
            .map(|plugin| {
                let (skills, mcp_servers, mcp_disabled_reason) = self.plugin_offering(&plugin.name);
                InstalledPluginDto {
                    name: plugin.name,
                    active_version: plugin.active_version,
                    previous_version: plugin.previous_version,
                    skills,
                    mcp_servers,
                    mcp_disabled_reason,
                }
            })
            .collect();
        let local_mcp_servers = self.local_mcp_servers()?;
        Ok(DispatchResult::response(serde_json::to_value(
            PluginListDto {
                installed,
                local_mcp_servers,
            },
        )?))
    }

    fn plugin_trust_local_mcp(&self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: LocalMcpTrustParams = decode_params(params)?;
        let server = self
            .local_mcp_servers()?
            .into_iter()
            .find(|server| {
                server.environment_id == params.environment_id
                    && server.plugin_name == params.plugin_name
                    && server.server_name == params.server_name
            })
            .ok_or_else(|| DispatchError::InvalidParams("unknown local MCP server".into()))?;
        if server.fingerprint != params.fingerprint {
            return Err(DispatchError::InvalidParams(
                "local MCP fingerprint changed; refresh before trusting".into(),
            ));
        }
        self.store.trust_local_mcp(&LocalMcpTrustRecord {
            environment_id: params.environment_id,
            plugin_name: params.plugin_name,
            server_name: params.server_name,
            fingerprint: params.fingerprint,
        })?;
        self.plugin_list()
    }

    fn plugin_revoke_local_mcp(&self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: LocalMcpTrustParams = decode_params(params)?;
        self.store.revoke_local_mcp_trust(
            &params.environment_id,
            &params.plugin_name,
            &params.server_name,
        )?;
        self.plugin_list()
    }

    fn update_status(&self) -> Result<DispatchResult, DispatchError> {
        Ok(DispatchResult::response(serde_json::to_value(
            self.update_status_dto(),
        )?))
    }

    fn update_check(&mut self) -> Result<DispatchResult, DispatchError> {
        if !self.updates.enabled() {
            return Err(DispatchError::UpdatesDisabled(
                self.updates
                    .layout_error
                    .clone()
                    .unwrap_or_else(|| "release update trust is not configured".into()),
            ));
        }
        let operation = (|| -> Result<(), DispatchError> {
            self.updates.state.begin_check()?;
            let config = self.updates.loaded.config.clone();
            let verified = self.updates.client.fetch_and_verify_manifest(
                config
                    .manifest_url
                    .as_ref()
                    .ok_or_else(|| DispatchError::UpdatesDisabled("manifest URL missing".into()))?
                    .as_str(),
                config
                    .detached_signature_url
                    .as_ref()
                    .ok_or_else(|| DispatchError::UpdatesDisabled("signature URL missing".into()))?
                    .as_str(),
                config.key_id.as_deref().ok_or_else(|| {
                    DispatchError::UpdatesDisabled("update key ID missing".into())
                })?,
                config.public_key.as_ref().ok_or_else(|| {
                    DispatchError::UpdatesDisabled("update public key missing".into())
                })?,
            )?;
            let selected = select_release(
                &verified,
                &SelectionRequest {
                    current_version: RUNTIME_VERSION,
                    channel: config.channel,
                    architecture: Architecture::current()?,
                    macos_version: &current_macos_version()?,
                    requested_version: None,
                    allow_rollback: false,
                },
            )?;
            match selected {
                Some((release, artifact)) => {
                    self.updates
                        .state
                        .update_available(release.version.clone())?;
                    self.updates.state.request_confirmation()?;
                    self.updates.pending = Some((release, artifact));
                }
                None => {
                    self.updates.state.no_update()?;
                    self.updates.pending = None;
                }
            }
            Ok(())
        })();
        if let Err(error) = operation {
            self.updates.state.fail(error.to_string());
            return Err(error);
        }
        self.update_status()
    }

    fn update_confirm_and_install(
        &mut self,
        params: Value,
    ) -> Result<DispatchResult, DispatchError> {
        let params: ConfirmUpdateParams = decode_params(params)?;
        let operation = (|| -> Result<(), DispatchError> {
            let (release, artifact) = self.updates.pending.clone().ok_or_else(|| {
                DispatchError::InvalidParams("no update is awaiting confirmation".into())
            })?;
            self.updates.state.confirm(&params.version)?;
            self.updates.state.begin_download()?;
            let staged = self
                .updates
                .client
                .download_and_stage(&release, &artifact)?;
            self.updates.state.begin_verification()?;
            let paths = self.updates.install_paths.clone().ok_or_else(|| {
                DispatchError::UpdatesDisabled("update helper unavailable".into())
            })?;
            let extracted = extract_macos_bundle(&staged, &paths.extraction_parent)?;
            self.updates
                .state
                .staged(extracted.bundle_path().to_string_lossy().into_owned())?;
            self.updates.state.begin_install()?;
            invoke_update_helper(
                &paths.helper,
                json!({
                    "operation":"install",
                    "schemaVersion":1,
                    "currentBundle":paths.current_bundle,
                    "stagedBundle":extracted.bundle_path(),
                    "expectedBundleId":self.updates.loaded.config.expected_bundle_id,
                }),
            )?;
            self.updates.state.ready_to_restart()?;
            self.updates.pending = None;
            Ok(())
        })();
        if let Err(error) = operation {
            self.updates.state.fail(error.to_string());
            return Err(error);
        }
        self.update_status()
    }

    fn update_rollback(&mut self) -> Result<DispatchResult, DispatchError> {
        let paths =
            self.updates.install_paths.clone().ok_or_else(|| {
                DispatchError::UpdatesDisabled("update helper unavailable".into())
            })?;
        let operation = invoke_update_helper(
            &paths.helper,
            json!({
                "operation":"rollback",
                "schemaVersion":1,
                "currentBundle":paths.current_bundle,
                "expectedBundleId":self.updates.loaded.config.expected_bundle_id,
            }),
        );
        if let Err(error) = operation {
            self.updates.state.fail(error.to_string());
            return Err(error);
        }
        self.updates.state = UpdateStateMachine::default();
        self.updates.pending = None;
        self.update_status()
    }

    fn update_status_dto(&self) -> UpdateStatusDto {
        let (state, target_version, state_message) =
            update_state_projection(self.updates.state.state());
        UpdateStatusDto {
            enabled: self.updates.enabled(),
            config_status: match self.updates.loaded.status {
                UpdateConfigLoadStatus::Loaded => "loaded",
                UpdateConfigLoadStatus::Missing => "missing",
                UpdateConfigLoadStatus::Invalid => "invalid",
            }
            .into(),
            current_version: RUNTIME_VERSION.into(),
            state: state.into(),
            target_version,
            message: state_message.or_else(|| self.updates.layout_error.clone()),
        }
    }

    fn registry_record(&self, id: &str) -> Result<PluginRegistryRecord, DispatchError> {
        self.store
            .list_plugin_registries()?
            .into_iter()
            .find(|registry| registry.id == id)
            .ok_or_else(|| DispatchError::InvalidParams("unknown plugin registry".into()))
    }

    fn available_secret_refs(&self) -> Result<BTreeSet<String>, DispatchError> {
        Ok(self
            .secret_store
            .list_metadata()?
            .into_iter()
            .map(|metadata| metadata.reference.to_string())
            .collect())
    }

    fn resolve_environment_boundary(
        &self,
        environment_id: &str,
    ) -> Result<ResolvedEnvironmentBoundary, DispatchError> {
        self.require_ready_environment(environment_id)?;
        let loaded = self
            .environments
            .get(environment_id)
            .ok_or_else(|| DispatchError::InvalidParams("Environment is unavailable".into()))?;
        Ok(ResolvedEnvironmentBoundary {
            environment_variables: loaded
                .descriptor
                .resolve_environment(&StoredSecretResolver::new(self.secret_store.as_ref()))?,
        })
    }

    fn require_ready_environment(
        &self,
        environment_id: &str,
    ) -> Result<EnvironmentProjection, DispatchError> {
        let projected = self
            .store
            .snapshot()?
            .environments
            .into_iter()
            .find(|environment| environment.id == environment_id)
            .ok_or_else(|| DispatchError::InvalidParams("Environment is unavailable".into()))?;
        if projected.readiness.state != EnvironmentReadinessState::Ready {
            return Err(DispatchError::InvalidParams(format!(
                "Environment needs setup: {}",
                projected.readiness.issues.join("; ")
            )));
        }
        Ok(projected)
    }

    /// Projects the whole catalog into the store. Every Environment mutation funnels
    /// through here, so the stored rows and availability tombstones are reconciled
    /// by one code path rather than per-handler upserts.
    fn synchronize_environment_readiness(&self) -> Result<u64, DispatchError> {
        self.synchronize_environment_readiness_clearing(&BTreeSet::new())
    }

    fn synchronize_environment_readiness_clearing(
        &self,
        cleared_environment_ids: &BTreeSet<String>,
    ) -> Result<u64, DispatchError> {
        let available_secret_refs = self.available_secret_refs()?;
        let mut snapshot = self.store.snapshot()?;
        let mut provider_readiness_changed = false;
        for provider in &mut snapshot.llm_providers {
            let readiness = llm_provider_projection_readiness(provider, &available_secret_refs);
            if provider.readiness != readiness {
                provider.readiness = readiness;
                self.store.put_llm_provider(provider, &[])?;
                provider_readiness_changed = true;
            }
        }
        if provider_readiness_changed {
            snapshot = self.store.snapshot()?;
        }
        let setup_by_environment = snapshot
            .environments
            .iter()
            .map(|environment| (environment.id.as_str(), environment.llm_needs_setup))
            .collect::<BTreeMap<_, _>>();
        let environments = self
            .environments
            .iter()
            .map(|(environment_id, environment)| {
                environment_projection(
                    &environment.descriptor,
                    &available_secret_refs,
                    &snapshot.llm_providers,
                    !cleared_environment_ids.contains(environment_id)
                        && setup_by_environment
                            .get(environment_id)
                            .copied()
                            .unwrap_or(false),
                    &self.plugin_store,
                )
            })
            .collect::<Vec<_>>();
        Ok(self.store.reconcile_environments(&environments)?)
    }

    /// The current Environments, in the shape every Environment mutation returns
    /// and the `environment.changed` event carries.
    fn environments_result(&self) -> Result<EnvironmentsResultDto, DispatchError> {
        let snapshot = self.store.snapshot()?;
        Ok(EnvironmentsResultDto {
            environments: snapshot.environments,
            revision: snapshot.revision,
        })
    }

    /// Allocates a unique Environment id for a display name.
    ///
    /// A candidate is taken if the catalog holds it, if the directory exists, or
    /// if *any* stored row claims it — including a tombstoned row from a deleted
    /// Environment. That last check is what stops a recreated Environment from inheriting the
    /// sessions and harness profiles of the one it replaced.
    fn allocate_environment_id(&self, name: &str) -> Result<String, DispatchError> {
        let base = environment_runtime::derive_environment_id_base(name);
        let taken = |candidate: &str| -> Result<bool, DispatchError> {
            Ok(self.environments.get(candidate).is_some()
                || self.store.environment_id_exists(candidate)?
                || self.environments.user_root().join(candidate).exists())
        };
        if !taken(&base)? {
            return Ok(base);
        }
        for suffix in 2..=999_u32 {
            let tail = format!("-{suffix}");
            let head = base
                .chars()
                .take(MAX_ENVIRONMENT_ID_CHARS - tail.chars().count())
                .collect::<String>();
            let candidate = format!("{}{tail}", head.trim_end_matches('-'));
            if !taken(&candidate)? {
                return Ok(candidate);
            }
        }
        Err(DispatchError::InvalidParams(
            "too many Environments share this name; choose a different one".into(),
        ))
    }

    fn environment_create(&mut self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: CreateEnvironmentParams = decode_params(params)?;
        let environment_id = self.allocate_environment_id(&params.configuration.name)?;
        let draft = environment_draft(params.configuration)?;
        self.validate_environment_plugins(&environment_id, &draft.plugins)?;
        self.environments
            .create_user_environment(environment_id.clone(), draft)?;
        self.synchronize_environment_readiness()?;

        let environments = self.environments_result()?;
        let payload = serde_json::to_value(&environments)?;
        Ok(DispatchResult {
            result: serde_json::to_value(EnvironmentCreateResultDto {
                environment_id,
                environments: environments.environments,
                revision: environments.revision,
            })?,
            event: Some(("environment.changed".into(), environments.revision, payload)),
        })
    }

    fn environment_delete(&mut self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: DeleteEnvironmentParams = decode_params(params)?;
        if self.environments.get(&params.environment_id).is_none() {
            return Err(DispatchError::InvalidParams("unknown environment".into()));
        }
        self.environments
            .delete_user_environment(&params.environment_id)?;
        // Reconciliation tombstones the row and purges the Environment's local MCP
        // trust. Sessions keep their `environment_id`, so history survives it.
        self.synchronize_environment_readiness()?;
        self.environment_mutation_result()
    }

    /// Every Environment mutation reports the same full list.
    fn environment_mutation_result(&self) -> Result<DispatchResult, DispatchError> {
        let environments = self.environments_result()?;
        let payload = serde_json::to_value(&environments)?;
        let revision = environments.revision;
        Ok(DispatchResult {
            result: serde_json::to_value(&environments)?,
            event: Some(("environment.changed".into(), revision, payload)),
        })
    }

    /// The single save path for an Environment: name, variables, provider, plugins,
    /// and registries are replaced together.
    ///
    /// Nothing is lost by having one method rather than three.
    fn environment_configuration_set(
        &mut self,
        params: Value,
    ) -> Result<DispatchResult, DispatchError> {
        let params: SetEnvironmentConfigurationParams = decode_params(params)?;
        let mut draft = environment_draft(params.configuration)?;
        let snapshot = self.store.snapshot()?;
        let existing_provider_id = self
            .environments
            .get(&params.environment_id)
            .ok_or_else(|| DispatchError::InvalidParams("unknown Environment".into()))?
            .descriptor
            .llm
            .as_ref()
            .map(|llm| llm.provider_id);
        // A provider swap invalidates the carried model selection. Re-seed from the
        // new provider's pool so a stale payload still lands ready; the UI does this
        // too, so this is a server-side guard.
        if draft.llm.as_ref().map(|llm| llm.provider_id) != existing_provider_id
            && let Some(llm) = &mut draft.llm
            && let Some(provider) = snapshot
                .llm_providers
                .iter()
                .find(|provider| provider.id == llm.provider_id)
        {
            llm.allowed_models = provider.allowed_models.clone();
            llm.default_model = provider.allowed_models.first().cloned().unwrap_or_default();
        }
        let available_secret_refs = self.available_secret_refs()?;
        let readiness = environment_readiness_from_configuration(
            &draft.environment_variables,
            draft.llm.as_ref(),
            &available_secret_refs,
            &snapshot.llm_providers,
            false,
        );
        if readiness.state != EnvironmentReadinessState::Ready {
            return Err(DispatchError::InvalidParams(format!(
                "the Environment configuration is not ready: {}",
                readiness.issues.join("; ")
            )));
        }
        self.validate_environment_plugins(&params.environment_id, &draft.plugins)?;
        self.environments
            .save_configuration(&params.environment_id, draft)?;
        self.synchronize_environment_readiness_clearing(&BTreeSet::from([params.environment_id]))?;
        self.environment_mutation_result()
    }

    /// Lists the models a provider exposes.
    ///
    /// The Environment id is only a correlation hint, so an unsaved draft can ask.
    /// Nothing is read from the Environment: the provider has always come from the
    /// params, and `validate_for_discovery` already confines endpoints to HTTPS
    /// or loopback.
    fn llm_provider_models_list(&self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: ListLlmProviderModelsParams = decode_params(params)?;
        let provider = llm_provider_connection(params.provider)?;
        provider
            .validate_for_discovery()
            .map_err(|error| DispatchError::InvalidParams(error.to_string()))?;
        let credential = provider
            .credential_ref
            .as_ref()
            .map(|reference| self.secret_store.read(reference))
            .transpose()?;
        let models = discover_models(&provider, credential)
            .map_err(|error| DispatchError::InvalidParams(error.to_string()))?;
        Ok(DispatchResult::response(serde_json::to_value(
            LlmProviderModelsDto {
                provider_id: params.provider_id,
                models,
            },
        )?))
    }

    /// Authorize one managed session in a Run, then ask Herdr to create it.
    fn start_factory_session(
        &mut self,
        run: &FactoryRun,
        purpose: HarnessPurpose,
        phase: &str,
        body: PromptBody,
    ) -> Result<AgentSessionProjection, DispatchError> {
        let binding = self.store.factory_run_workspace(run.id)?;
        let target_agent = self.store.target_agent(binding.target_agent_id)?;
        let environment = self
            .store
            .snapshot()?
            .environments
            .into_iter()
            .find(|environment| environment.id == run.environment_id)
            .ok_or_else(|| DispatchError::InvalidParams("Run Environment is unavailable".into()))?;
        let harness_id = match purpose {
            HarnessPurpose::Orchestration | HarnessPurpose::Coding => {
                environment.coding_harness_id.clone()
            }
            HarnessPurpose::Evaluation => environment.evaluation_harness_id.clone(),
        };
        let (llm_provider_snapshot, effective_model) = self
            .validate_new_agent_session_prerequisites(
                run.project_id,
                &environment,
                &harness_id,
                None,
            )?;
        let now = now_unix_ms();
        let session_id = Uuid::new_v4();
        let parent_session_id = if purpose == HarnessPurpose::Orchestration {
            None
        } else {
            self.latest_run_session(run.id, HarnessPurpose::Orchestration)?
                .map(|session| session.id)
        };
        let placement = self.factory_session_placement(run, purpose, &target_agent.name)?;
        let iteration = placement.iteration;
        let mut session = AgentSessionProjection {
            id: session_id,
            target_agent_id: binding.target_agent_id,
            workspace_binding_id: binding.id,
            project_id: run.project_id,
            environment_id: environment.id.clone(),
            harness_id,
            purpose,
            factory_run_id: Some(run.id),
            parent_session_id,
            // The Agent's name is what makes one of these readable in Herdr's
            // agent list; the objective would only be truncated away.
            herdr_agent_name: herdr_sessions::agent_name(
                purpose.agent_name_prefix(),
                &target_agent.name,
                session_id,
            ),
            availability: SessionAvailability::Reconnecting,
            lifecycle: None,
            placement: None,
            title: format!("{phase} · iteration {iteration}"),
            created_at_unix_ms: now,
            last_activity_at_unix_ms: now,
            llm_provider_snapshot: Some(llm_provider_snapshot),
            effective_model: Some(effective_model),
            attention: Vec::new(),
            initial_prompt: None,
            brief_delivered: false,
            outcome: None,
        };
        self.store.save_agent_session(&session)?;
        let control = if purpose == HarnessPurpose::Orchestration {
            self.orchestrator_control_environment(run.id)?
        } else {
            BTreeMap::new()
        };
        self.start_agent_session(&mut session, &binding, &environment, control, placement)?;

        let placement = session
            .placement
            .clone()
            .expect("a started session has a placement");
        let prompt = match body {
            PromptBody::Text(text) => text,
            PromptBody::Evaluation => evaluation_prompt(
                run,
                &evaluator_verdict_path(&binding.primary_root, session.id),
            ),
        };
        session.initial_prompt = Some(prompt.clone());
        self.store.save_agent_session(&session)?;
        if session
            .lifecycle
            .is_some_and(AgentLifecycle::accepts_prompt)
        {
            let prompt = self.with_default_skills(&environment.id, prompt)?;
            match self.herdr.prompt(&placement, &prompt) {
                Ok(attempt) => {
                    session.brief_delivered = true;
                    session.lifecycle = Some(attempt.lifecycle);
                    session.last_activity_at_unix_ms = now_unix_ms();
                    self.store.save_agent_session(&session)?;
                }
                Err(error) if error.is_transient() => {
                    self.pending_prompt_retry_at
                        .insert(session.id, Instant::now() + PENDING_PROMPT_RETRY);
                }
                Err(error) => {
                    let message = sanitize_public_diagnostic(&error.public_message());
                    let _ = self.herdr.stop(&placement);
                    session.availability = SessionAvailability::Historical;
                    session.lifecycle = None;
                    session.placement = None;
                    session.outcome = Some(ManagedSessionOutcome {
                        kind: ManagedSessionOutcomeKind::Failed,
                        summary: Some(message.clone()),
                        recorded_at_unix_ms: now_unix_ms(),
                    });
                    self.store.save_agent_session(&session)?;
                    return Err(DispatchError::Herdr(message));
                }
            }
        } else {
            self.pending_prompt_retry_at
                .insert(session.id, Instant::now() + PENDING_PROMPT_RETRY);
        }
        Ok(session)
    }

    /// Where this session stands in its Draft's Herdr Workspace.
    ///
    /// One Draft is one Workspace, one iteration is one tab, and a Run reads as
    /// columns inside it: the Orchestrator opens iteration 1, the agents it
    /// starts for an iteration line up to its right, and a second Coding agent
    /// begins the next iteration in a tab of its own. The Orchestrator stays
    /// where the Run began, so earlier iterations remain readable.
    fn factory_session_placement(
        &self,
        run: &FactoryRun,
        purpose: HarnessPurpose,
        agent_name: &str,
    ) -> Result<SessionPlacement, DispatchError> {
        let completed_iterations = self
            .run_sessions(run.id)?
            .iter()
            .filter(|session| session.purpose == HarnessPurpose::Coding)
            .count() as u32;
        let iteration = match purpose {
            HarnessPurpose::Orchestration => 1,
            HarnessPurpose::Coding => completed_iterations + 1,
            HarnessPurpose::Evaluation => completed_iterations.max(1),
        };
        let column_beside = match purpose {
            HarnessPurpose::Orchestration => None,
            // The first Coding agent joins the Orchestrator; a later one opens
            // the next iteration's tab and stands alone until Evaluation.
            HarnessPurpose::Coding if completed_iterations == 0 => self
                .latest_run_session(run.id, HarnessPurpose::Orchestration)?
                .and_then(|session| session.placement)
                .map(|placement| placement.pane_id),
            HarnessPurpose::Coding => None,
            // Evaluation judges the Coding agent it follows, so it belongs in
            // that iteration's tab rather than one of its own.
            HarnessPurpose::Evaluation => self
                .latest_run_session(run.id, HarnessPurpose::Coding)?
                .and_then(|session| session.placement)
                .map(|placement| placement.pane_id),
        };
        Ok(SessionPlacement {
            tab_label: Some(iteration_tab_label(agent_name, iteration)),
            column_beside,
            iteration,
        })
    }

    fn run_sessions(&self, run_id: Uuid) -> Result<Vec<AgentSessionProjection>, DispatchError> {
        Ok(self
            .projection()?
            .agent_sessions
            .into_iter()
            .filter(|session| session.factory_run_id == Some(run_id))
            .collect())
    }

    fn latest_run_session(
        &self,
        run_id: Uuid,
        purpose: HarnessPurpose,
    ) -> Result<Option<AgentSessionProjection>, DispatchError> {
        Ok(self
            .run_sessions(run_id)?
            .into_iter()
            .filter(|session| session.purpose == purpose)
            .max_by_key(|session| session.created_at_unix_ms))
    }

    /// What an Orchestrator needs to reach back into its own run.
    ///
    /// The endpoint is only set once the runtime is actually serving one, so a
    /// build without a control socket simply starts an Orchestrator that has no
    /// way to drive — which is visible rather than silently broken.
    fn orchestrator_control_environment(
        &self,
        run_id: Uuid,
    ) -> Result<BTreeMap<String, String>, DispatchError> {
        let Some(endpoint) = self.control_endpoint.as_ref() else {
            return Ok(BTreeMap::new());
        };
        let token = self.store.mint_run_control_token(run_id)?;
        Ok([
            (
                agent_control::ENDPOINT_ENV.to_owned(),
                endpoint.to_string_lossy().into_owned(),
            ),
            (agent_control::TOKEN_ENV.to_owned(), token),
        ]
        .into_iter()
        .collect())
    }

    /// Where the Orchestrator's commands are served from.
    pub fn set_control_endpoint(&mut self, endpoint: PathBuf) {
        self.control_endpoint = Some(endpoint);
    }

    /// Execute one Orchestrator command.
    ///
    /// Everything an Orchestrator cannot do for itself happens here: resolving
    /// the Environment onto a new pane, validating the move against the run's
    /// state machine, capturing workspace evidence, and recording the result.
    /// The Orchestrator drives its own agents with Herdr; this is the boundary,
    /// not the loop.
    pub(crate) fn handle_control(
        &mut self,
        request: agent_control::ControlRequest,
    ) -> (agent_control::ControlResponse, Vec<Frame>) {
        let mut frames = Vec::new();
        let outcome = self.dispatch_control(request, &mut frames);
        let response = match outcome {
            Ok(view) => agent_control::ControlResponse::Ok(view),
            Err(error) => agent_control::ControlResponse::Error {
                code: error.control_code().to_owned(),
                message: sanitize_public_diagnostic(&error.to_string()),
            },
        };
        (response, frames)
    }

    fn dispatch_control(
        &mut self,
        request: agent_control::ControlRequest,
        frames: &mut Vec<Frame>,
    ) -> Result<agent_control::RunView, DispatchError> {
        let run_id = self
            .store
            .factory_run_for_control_token(&request.token)?
            .ok_or_else(|| {
                DispatchError::Unauthorized(
                    "this token does not authorize any live Factory Run".into(),
                )
            })?;
        let run = self.factory_run(run_id)?;
        if run.state.is_terminal() {
            return Err(DispatchError::InvalidParams(format!(
                "this Run already finished as `{}`",
                run_state_label(run.state)
            )));
        }

        let (run, agent, message) = match request.command {
            agent_control::ControlCommand::Status => (run, None, "Run status.".to_owned()),
            agent_control::ControlCommand::StartCoding { brief } => {
                self.control_start_coding(run, brief)?
            }
            agent_control::ControlCommand::StartEvaluation { brief } => {
                self.control_start_evaluation(run, brief)?
            }
            agent_control::ControlCommand::Escalate { question } => {
                self.control_escalate(run, question, frames)?
            }
            agent_control::ControlCommand::Finish { verdict, summary } => {
                self.control_finish(run, verdict, summary)?
            }
        };

        if let Ok(payload) = serde_json::to_value(&run) {
            frames.push(self.event_frame("run.changed", payload));
        }
        let iteration = self
            .run_sessions(run.id)?
            .into_iter()
            .filter(|session| session.purpose == HarnessPurpose::Coding)
            .count() as u32;
        Ok(run_view(&run, agent, iteration, message))
    }

    fn control_start_coding(
        &mut self,
        mut run: FactoryRun,
        brief: String,
    ) -> Result<(FactoryRun, Option<AgentSessionProjection>, String), DispatchError> {
        require_prompt_text(&brief)?;
        // Moving forward is the Orchestrator answering its own question.
        run.escalation = None;
        let iteration = self
            .run_sessions(run.id)?
            .into_iter()
            .filter(|session| session.purpose == HarnessPurpose::Coding)
            .count() as u32;
        let iterating = iteration > 0;
        if iterating {
            self.capture_run_git(&mut run)?;
        }
        run.transition(FactoryRunState::Coding)?;
        let phase = if iterating { "repair" } else { "coding" };
        let session = self.start_factory_session(
            &run,
            HarnessPurpose::Coding,
            phase,
            PromptBody::Text(brief),
        )?;
        self.store.save_factory_run(&run)?;
        let message = format!(
            "Coding agent started for iteration {}. Prompt it by name with `herdr agent prompt`, then read it to judge the work.",
            iteration + 1
        );
        Ok((run, Some(session), message))
    }

    fn control_start_evaluation(
        &mut self,
        mut run: FactoryRun,
        brief: Option<String>,
    ) -> Result<(FactoryRun, Option<AgentSessionProjection>, String), DispatchError> {
        if !self
            .run_sessions(run.id)?
            .iter()
            .any(|session| session.purpose == HarnessPurpose::Coding)
        {
            return Err(DispatchError::InvalidParams(
                "there is nothing to evaluate yet; start Coding first".into(),
            ));
        }
        run.escalation = None;
        self.capture_run_git(&mut run)?;
        run.transition(FactoryRunState::Evaluating)?;
        let body = match brief {
            Some(text) if !text.trim().is_empty() => {
                require_prompt_text(&text)?;
                PromptBody::Text(text)
            }
            _ => PromptBody::Evaluation,
        };
        let session =
            self.start_factory_session(&run, HarnessPurpose::Evaluation, "evaluation", body)?;
        self.store.save_factory_run(&run)?;
        Ok((
            run,
            Some(session),
            "Evaluation agent started. Read its verdict, then finish the Run.".to_owned(),
        ))
    }

    /// Stop and ask a person, without giving up the Run.
    ///
    /// A Factory Run is meant to advance unattended, so this is the one moment
    /// that should pull someone back. The Orchestrator keeps its pane and its
    /// authority: the answer is typed straight to it, and its next command
    /// clears the question.
    fn control_escalate(
        &mut self,
        mut run: FactoryRun,
        question: String,
        frames: &mut Vec<Frame>,
    ) -> Result<(FactoryRun, Option<AgentSessionProjection>, String), DispatchError> {
        let question = normalize_required("question", &question, MAX_FACTORY_PROMPT_BYTES)?;
        run.escalation = Some(question.clone());
        run.transition(FactoryRunState::Escalated)?;
        self.store.save_factory_run(&run)?;

        if self
            .store
            .snapshot()
            .is_ok_and(|snapshot| snapshot.settings.native_notifications)
            && let Ok(payload) = serde_json::to_value(NotificationRequestedDto {
                category: NotificationCategory::FactoryRunNeedsReview,
                title: bounded_notification_text(
                    "A Run needs your decision",
                    MAX_NOTIFICATION_TITLE_CHARS,
                ),
                body: bounded_notification_text(&question, MAX_NOTIFICATION_BODY_CHARS),
                entity_id: run.id,
            })
        {
            frames.push(self.event_frame("notification.requested", payload));
        }
        Ok((
            run,
            None,
            "Asked for a decision. Answer arrives in your pane; your next command clears it."
                .to_owned(),
        ))
    }

    fn control_finish(
        &mut self,
        mut run: FactoryRun,
        verdict: agent_control::FinishVerdict,
        summary: String,
    ) -> Result<(FactoryRun, Option<AgentSessionProjection>, String), DispatchError> {
        let summary = normalize_required("summary", &summary, MAX_FACTORY_PROMPT_BYTES)?;
        let next = match verdict {
            agent_control::FinishVerdict::Pass => FactoryRunState::Passed,
            agent_control::FinishVerdict::NeedsReview => FactoryRunState::NeedsReview,
        };
        self.capture_run_git(&mut run)?;
        run.evaluation = Some(match verdict {
            agent_control::FinishVerdict::Pass => EvaluationResult::passed(summary.clone()),
            agent_control::FinishVerdict::NeedsReview => {
                EvaluationResult::review_requested(summary.clone())
            }
        });
        run.transition(next)?;
        if next.is_terminal() {
            run.completed_at_unix_ms = Some(now_unix_ms());
        }
        self.store.save_factory_run(&run)?;
        if next.is_terminal() {
            self.store.revoke_run_control_tokens(run.id)?;
        }
        Ok((
            run,
            None,
            format!("Run finished as `{}`.", run_state_label(next)),
        ))
    }

    fn live_run_for_draft(&self, draft_id: Uuid) -> Result<Option<FactoryRun>, DispatchError> {
        Ok(self
            .store
            .snapshot()?
            .factory_runs
            .into_iter()
            .find(|run| run.agent_draft_id == draft_id && !run.state.is_terminal()))
    }

    fn resume_or_attach_orchestrator(
        &mut self,
        run: FactoryRun,
    ) -> Result<DispatchResult, DispatchError> {
        if let Some(session) = self.latest_run_session(run.id, HarnessPurpose::Orchestration)?
            && session.availability == SessionAvailability::Live
            && session.placement.is_some()
        {
            return self.run_session_dispatch(run, session);
        }
        self.start_orchestrator_session(run)
    }

    fn start_orchestrator_session(
        &mut self,
        mut run: FactoryRun,
    ) -> Result<DispatchResult, DispatchError> {
        run.transition(FactoryRunState::Orchestrating)?;
        let session = self.start_factory_session(
            &run,
            HarnessPurpose::Orchestration,
            "orchestrator",
            PromptBody::Text(orchestrator_brief_prompt(&run)),
        )?;
        self.store.save_factory_run(&run)?;
        self.run_session_dispatch(run, session)
    }

    fn run_cancel(&mut self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: RunIdParams = decode_params(params)?;
        let run = self.factory_run(params.run_id)?;
        let mut cancelled = run.clone();
        // Validate the semantic transition before touching Herdr. The durable
        // state is committed only after every managed session is confirmed
        // absent from a fresh server snapshot.
        cancelled.transition(FactoryRunState::Cancelled)?;
        // Capture the authorized session set joined with the last observed
        // placement before refreshing live state. That placement remains only
        // an ephemeral locator and is revalidated below; this lets cancellation
        // close the Run-created tab even if Herdr reports a renamed agent there.
        let mut sessions = self
            .projection()?
            .agent_sessions
            .into_iter()
            .filter(|session| session.factory_run_id == Some(run.id))
            .collect::<Vec<_>>();
        self.herdr
            .require_fresh_state()
            .map_err(|error| DispatchError::Herdr(error.public_message()))?;
        // Stop workers before their Orchestrator. This prevents the controller
        // disappearing while its managed work is still alive in Herdr.
        sessions.sort_by_key(|session| {
            (
                session.purpose == HarnessPurpose::Orchestration,
                session.created_at_unix_ms,
            )
        });
        let managed_names = sessions
            .iter()
            .map(|session| session.herdr_agent_name.clone())
            .collect::<BTreeSet<_>>();
        let mut failures = Vec::new();
        for mut session in sessions {
            match self
                .herdr
                .stop_managed_agent(&session.herdr_agent_name, session.placement.as_ref())
            {
                Ok(closed) => {
                    if let Err(error) = self.record_stopped_agent_session(&mut session, closed) {
                        failures.push(format!("{}: {error}", session.herdr_agent_name));
                    }
                }
                Err(error) => failures.push(format!(
                    "{}: {}",
                    session.herdr_agent_name,
                    error.public_message()
                )),
            }
        }
        if !failures.is_empty() {
            return Err(DispatchError::Herdr(format!(
                "Run cancellation is incomplete: {}",
                failures.join("; ")
            )));
        }

        self.herdr
            .require_fresh_state()
            .map_err(|error| DispatchError::Herdr(error.public_message()))?;
        let still_present = self
            .herdr
            .present_managed_agents(managed_names.iter().map(String::as_str));
        if !still_present.is_empty() {
            return Err(DispatchError::Herdr(format!(
                "Run cancellation is incomplete; Herdr still reports: {}",
                still_present.join(", ")
            )));
        }

        cancelled.completed_at_unix_ms = Some(now_unix_ms());
        self.store.save_factory_run(&cancelled)?;
        self.store.revoke_run_control_tokens(cancelled.id)?;
        self.run_dispatch(cancelled)
    }

    fn factory_run(&self, run_id: Uuid) -> Result<FactoryRun, DispatchError> {
        self.store
            .snapshot()?
            .factory_runs
            .into_iter()
            .find(|run| run.id == run_id)
            .ok_or_else(|| DispatchError::InvalidParams(format!("unknown Run `{run_id}`")))
    }

    fn require_trusted_project(
        &self,
        project_id: Uuid,
    ) -> Result<app_core::ProjectProjection, DispatchError> {
        self.store
            .list_projects()?
            .into_iter()
            .find(|project| project.id == project_id && project.trusted)
            .ok_or_else(|| DispatchError::InvalidParams("unknown trusted project".into()))
    }

    fn run_dispatch(&self, run: FactoryRun) -> Result<DispatchResult, DispatchError> {
        let revision = self.store.snapshot()?.revision;
        let payload = serde_json::to_value(&run)?;
        Ok(DispatchResult {
            result: serde_json::to_value(RunResultDto { run, revision })?,
            event: Some(("run.changed".into(), revision, payload)),
        })
    }

    fn run_session_dispatch(
        &self,
        run: FactoryRun,
        session: AgentSessionProjection,
    ) -> Result<DispatchResult, DispatchError> {
        let revision = self.store.snapshot()?.revision;
        let payload = serde_json::to_value(&run)?;
        Ok(DispatchResult {
            result: serde_json::to_value(RunSessionResultDto {
                run,
                session,
                revision,
            })?,
            event: Some(("run.changed".into(), revision, payload)),
        })
    }

    /// Treat Herdr events as invalidations, then publish one complete joined
    /// projection boundary. Event payload order never mutates durable state.
    pub fn poll_events(&mut self) -> Vec<Frame> {
        let mut frames = Vec::new();
        let before = self.projection().ok();
        let reconnected = self.herdr.reconnect_if_due();
        let invalidated = self.herdr.refresh_if_invalidated();
        let polled = !invalidated && self.herdr.refresh_if_poll_due();
        if reconnected || invalidated || polled {
            if let Ok(payload) = serde_json::to_value(HarnessListDto {
                herdr: self.herdr.status().clone(),
                harnesses: self.herdr.harnesses().to_vec(),
            }) {
                frames.push(self.event_frame("harness.changed", payload));
            }
            frames.push(self.event_frame(
                "runtime.invalidated",
                json!({"source": "herdr", "fullSnapshotRequired": true}),
            ));

            if let (Some(before), Ok(after)) = (before, self.projection()) {
                self.record_runtime_interruptions(&before, &after);
                let prior_lifecycle = before
                    .live_agents
                    .iter()
                    .filter_map(|agent| {
                        agent
                            .agent_name
                            .as_ref()
                            .map(|name| (name.as_str(), agent.lifecycle))
                    })
                    .collect::<BTreeMap<_, _>>();
                for agent in &after.live_agents {
                    let Some(session_id) = agent.managed_session_id else {
                        continue;
                    };
                    let changed = agent.agent_name.as_ref().is_none_or(|name| {
                        prior_lifecycle.get(name.as_str()) != Some(&agent.lifecycle)
                    });
                    if !changed {
                        continue;
                    }
                    let Some(session) = after
                        .agent_sessions
                        .iter()
                        .find(|session| session.id == session_id)
                    else {
                        continue;
                    };
                    if let Some(notification) = self.lifecycle_notification(session, &after) {
                        frames.push(self.event_frame("notification.requested", notification));
                    }
                }
            }
        }
        self.flush_pending_factory_prompts(&mut frames);
        frames
    }

    fn record_runtime_interruptions(
        &mut self,
        before: &app_core::ApplicationProjection,
        after: &app_core::ApplicationProjection,
    ) {
        if after.herdr.freshness != AuthorityFreshness::Live {
            return;
        }
        let live_after = after
            .agent_sessions
            .iter()
            .filter(|session| session.availability == SessionAvailability::Live)
            .map(|session| session.id)
            .collect::<BTreeSet<_>>();
        for session in before.agent_sessions.iter().filter(|session| {
            session.availability == SessionAvailability::Live
                && session.outcome.is_none()
                && !live_after.contains(&session.id)
        }) {
            let Ok(Some(mut durable)) = self.store.snapshot().map(|snapshot| {
                snapshot
                    .agent_sessions
                    .into_iter()
                    .find(|candidate| candidate.id == session.id)
            }) else {
                continue;
            };
            durable.outcome = Some(ManagedSessionOutcome {
                kind: ManagedSessionOutcomeKind::Interrupted,
                summary: Some("Herdr no longer reports this managed agent.".into()),
                recorded_at_unix_ms: now_unix_ms(),
            });
            durable.last_activity_at_unix_ms = now_unix_ms();
            let _ = self.store.save_agent_session(&durable);
            self.gateways.remove(&durable.id);
            self.pending_prompt_retry_at.remove(&durable.id);
        }
    }

    fn flush_pending_factory_prompts(&mut self, frames: &mut Vec<Frame>) {
        let Ok(snapshot) = self.projection() else {
            return;
        };
        let now = Instant::now();
        let pending = snapshot
            .agent_sessions
            .into_iter()
            .filter(awaiting_initial_prompt)
            .collect::<Vec<_>>();
        for mut session in pending {
            if self
                .pending_prompt_retry_at
                .get(&session.id)
                .is_some_and(|retry_at| now < *retry_at)
            {
                continue;
            }
            self.pending_prompt_retry_at
                .insert(session.id, now + PENDING_PROMPT_RETRY);
            if self.try_deliver_initial_prompt(&mut session) {
                self.pending_prompt_retry_at.remove(&session.id);
                if let Ok(payload) = serde_json::to_value(&session) {
                    frames.push(self.event_frame("agentSession.changed", payload));
                }
            }
        }
    }

    fn try_deliver_initial_prompt(&mut self, session: &mut AgentSessionProjection) -> bool {
        if session.availability != SessionAvailability::Live {
            return false;
        }
        let Some(placement) = session.placement.clone() else {
            return false;
        };
        let Some(text) = session.initial_prompt.clone() else {
            return false;
        };
        let Ok(prompt) = self.with_default_skills(&session.environment_id, text) else {
            return false;
        };
        let mut attempt = self.herdr.try_prompt(&placement, &prompt);
        if attempt
            .as_ref()
            .is_err_and(herdr_client::HerdrError::is_unbound_agent)
        {
            match self.herdr.try_reconcile_ready(&placement) {
                Ok(true) => {
                    attempt = self.herdr.try_prompt(&placement, &prompt);
                }
                Ok(false) => return false,
                Err(error) if error.is_transient() => return false,
                Err(_) => return false,
            }
        }
        match attempt {
            Ok(attempt) => {
                session.brief_delivered = true;
                session.lifecycle = Some(attempt.lifecycle);
                session.last_activity_at_unix_ms = now_unix_ms();
                if self.store.save_agent_session(session).is_err() {
                    return false;
                }
                true
            }
            Err(_) => false,
        }
    }

    fn event_frame(&mut self, topic: &str, payload: Value) -> Frame {
        let revision = self
            .store
            .snapshot()
            .map(|snapshot| snapshot.revision)
            .unwrap_or_default();
        Frame::Event(Event {
            version: PROTOCOL_VERSION,
            topic: topic.to_owned(),
            revision,
            sequence: self.take_sequence(),
            payload,
        })
    }

    fn lifecycle_notification(
        &self,
        session: &AgentSessionProjection,
        snapshot: &app_core::ApplicationProjection,
    ) -> Option<Value> {
        if !snapshot.settings.native_notifications {
            return None;
        }
        let (category, title, body) = match session.lifecycle {
            Some(AgentLifecycle::Blocked) => (
                NotificationCategory::SessionBlocked,
                "Agent needs an answer",
                session
                    .attention
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "The agent is waiting for input.".to_owned()),
            ),
            Some(AgentLifecycle::Done) => (
                NotificationCategory::SessionCompleted,
                "Agent finished",
                session.title.clone(),
            ),
            _ => return None,
        };
        serde_json::to_value(NotificationRequestedDto {
            category,
            title: bounded_notification_text(title, MAX_NOTIFICATION_TITLE_CHARS),
            body: bounded_notification_text(&body, MAX_NOTIFICATION_BODY_CHARS),
            entity_id: session.id,
        })
        .ok()
    }

    fn capture_run_git(&self, run: &mut FactoryRun) -> Result<(), DispatchError> {
        let project = self.require_trusted_project(run.project_id)?;
        run.final_git_head = Some(self.git.head(&project.root)?);
        run.changed_files = self
            .git
            .changes_since(&project.root, &run.starting_git_head)?
            .into_iter()
            .map(|change| ChangedFile {
                path: change.path,
                change: match change.kind {
                    WorkingTreeChangeKind::Added => ChangedFileKind::Added,
                    WorkingTreeChangeKind::Modified => ChangedFileKind::Modified,
                    WorkingTreeChangeKind::Deleted => ChangedFileKind::Deleted,
                },
                before_hash: None,
                after_hash: None,
                diff: None,
            })
            .collect();
        Ok(())
    }

    fn workspace_terminal_create(
        &mut self,
        params: Value,
    ) -> Result<DispatchResult, DispatchError> {
        let params: CreateWorkspaceTerminalParams = decode_params(params)?;
        let snapshot = self.store.snapshot()?;
        let context = snapshot
            .target_workspace
            .work_contexts
            .iter()
            .find(|context| context.id == params.work_context_id)
            .ok_or_else(|| DispatchError::InvalidParams("unknown work context".into()))?;
        let binding = snapshot
            .target_workspace
            .target_groups
            .iter()
            .flat_map(|group| &group.workspace_bindings)
            .find(|binding| binding.id == context.workspace_binding_id)
            .ok_or_else(|| DispatchError::InvalidParams("unknown workspace binding".into()))?;
        let project = self
            .store
            .list_projects()?
            .into_iter()
            .find(|project| project.id == binding.project_id);
        let Some(project) = project else {
            return Err(DispatchError::InvalidParams(
                "unknown project for workspace binding".into(),
            ));
        };
        if !project.trusted {
            return Err(DispatchError::InvalidParams(
                "Trust this workspace before opening a terminal.".into(),
            ));
        }
        let cwd = self
            .files()?
            .authorize(&binding.primary_root)
            .map_err(|error| {
                DispatchError::InvalidParams(format!(
                    "cannot open terminal in workspace root {}: {error}",
                    binding.primary_root.display()
                ))
            })?;
        let executable = user_shell();
        let created = self.terminals.create(CreateTerminal {
            args: shell_args(&executable),
            executable,
            cwd,
            environment: shell_environment(&self.search_paths),
            cols: params.cols,
            rows: params.rows,
        })?;
        let terminal_number = snapshot
            .target_workspace
            .terminals
            .iter()
            .filter(|terminal| terminal.work_context_id == params.work_context_id)
            .count()
            + 1;
        self.store.register_workspace_terminal(
            created.terminal_id,
            params.work_context_id,
            &format!("Terminal {terminal_number}"),
        )?;
        self.store.set_work_context_dock(
            params.work_context_id,
            WorkspaceDock::Terminal,
            context.dock_percent,
        )?;
        Ok(DispatchResult::response(serde_json::to_value(created)?))
    }

    fn terminal_write(&mut self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: WriteTerminalParams = decode_params(params)?;
        self.terminals
            .write(params.terminal_id, params.data.as_bytes())?;
        Ok(DispatchResult::response(serde_json::to_value(
            TerminalWriteResultDto {
                terminal_id: params.terminal_id,
            },
        )?))
    }

    fn terminal_resize(&mut self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: ResizeTerminalParams = decode_params(params)?;
        self.terminals
            .resize(params.terminal_id, params.cols, params.rows)?;
        Ok(DispatchResult::response(serde_json::to_value(
            TerminalResizeResultDto {
                terminal_id: params.terminal_id,
                cols: params.cols,
                rows: params.rows,
            },
        )?))
    }

    fn terminal_read(&mut self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: ReadTerminalParams = decode_params(params)?;
        let read = self
            .terminals
            .read(params.terminal_id, params.cursor, params.max_bytes)?;
        Ok(DispatchResult::response(serde_json::to_value(read)?))
    }

    fn workspace_terminal_kill(&mut self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: TerminalIdParams = decode_params(params)?;
        let exit_status = self.terminals.kill(params.terminal_id)?;
        self.store
            .mark_workspace_terminal_exited(params.terminal_id)?;
        Ok(DispatchResult::response(serde_json::to_value(
            TerminalKillResultDto {
                terminal_id: params.terminal_id,
                exit_status,
            },
        )?))
    }

    fn workspace_terminal_close(&mut self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: TerminalIdParams = decode_params(params)?;
        let _ = self.terminals.release(params.terminal_id);
        self.store.remove_workspace_terminal(params.terminal_id)?;
        Ok(DispatchResult::response(serde_json::json!({
            "terminalId": params.terminal_id,
        })))
    }

    fn file_list(&self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: ListFilesParams = decode_params(params)?;
        let page = self.files()?.list(
            Path::new(&params.path),
            params.cursor.as_deref(),
            params.page_size,
        )?;
        Ok(DispatchResult::response(serde_json::to_value(page)?))
    }

    fn file_read(&self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: ReadFileParams = decode_params(params)?;
        let read = self
            .files()?
            .read_text(Path::new(&params.path), params.max_bytes)?;
        Ok(DispatchResult::response(serde_json::to_value(read)?))
    }

    fn file_diff(&self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: DiffFilesParams = decode_params(params)?;
        let diff = self.files()?.diff(
            Path::new(&params.before_path),
            Path::new(&params.after_path),
            params.context_lines,
        )?;
        Ok(DispatchResult::response(serde_json::to_value(diff)?))
    }

    fn version_files_list(&self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: ListVersionFilesParams = decode_params(params)?;
        let (repository_root, git_commit) = self.version_source(params.version_id)?;
        let entries = self
            .git
            .list_commit_files(&repository_root, &git_commit)?
            .into_iter()
            .map(|entry| VersionFileEntryDto {
                path: entry.path,
                kind: match entry.kind {
                    CommitTreeEntryKind::File => VersionFileEntryKindDto::File,
                    CommitTreeEntryKind::Symlink => VersionFileEntryKindDto::Symlink,
                    CommitTreeEntryKind::Submodule => VersionFileEntryKindDto::Submodule,
                },
                size: entry.size,
            })
            .collect();
        Ok(DispatchResult::response(serde_json::to_value(
            VersionFilesListDto {
                version_id: params.version_id,
                git_commit,
                entries,
            },
        )?))
    }

    fn version_file_read(&self, params: Value) -> Result<DispatchResult, DispatchError> {
        let params: ReadVersionFileParams = decode_params(params)?;
        let (repository_root, git_commit) = self.version_source(params.version_id)?;
        let file = self
            .git
            .read_commit_file(&repository_root, &git_commit, &params.path)?;
        Ok(DispatchResult::response(serde_json::to_value(
            VersionFileReadDto {
                version_id: params.version_id,
                git_commit,
                path: file.path,
                size: file.size,
                kind: match file.kind {
                    CommitFileKind::Text => VersionFileReadKindDto::Text,
                    CommitFileKind::Binary => VersionFileReadKindDto::Binary,
                    CommitFileKind::TooLarge => VersionFileReadKindDto::TooLarge,
                    CommitFileKind::Unsupported => VersionFileReadKindDto::Unsupported,
                },
                content: file.content,
            },
        )?))
    }

    fn version_source(&self, version_id: Uuid) -> Result<(PathBuf, String), DispatchError> {
        let version = self.store.target_agent_version(version_id)?;
        let agent = self.store.target_agent(version.target_agent_id)?;
        let source_draft = self.store.agent_draft(version.source_draft_id)?;
        let source_binding = self
            .store
            .workspace_binding(source_draft.workspace_binding_id)?;
        let source_is_trusted = self
            .store
            .list_projects()?
            .into_iter()
            .any(|project| project.trusted && project.id == source_binding.project_id);
        if !source_is_trusted {
            return Err(DispatchError::InvalidParams(
                "The Version repository is not trusted".into(),
            ));
        }
        Ok((agent.repository_root, version.git_commit))
    }

    fn files(&self) -> Result<FileSystem, DispatchError> {
        // Skip missing roots so one deleted project does not break every
        // authorize() call (including workspace terminal create).
        let roots = self
            .store
            .list_projects()?
            .into_iter()
            .filter(|project| project.trusted)
            .map(|project| project.root)
            .filter(|root| root.is_absolute() && root.is_dir());
        Ok(FileSystem::new(roots)?)
    }

    /// Projects the skills and MCP servers an installed plugin offers. A missing
    /// installed plugin (state without files) yields empty offerings.
    fn plugin_offering(
        &self,
        name: &str,
    ) -> (
        Vec<InstalledSkillDto>,
        Vec<InstalledMcpServerDto>,
        Option<String>,
    ) {
        let loaded = match self.plugin_store.active_plugin(name) {
            Ok(loaded) => loaded,
            Err(PluginError::NotInstalled(_)) => {
                return (Vec::new(), Vec::new(), None);
            }
            Err(_) => return (Vec::new(), Vec::new(), None),
        };
        project_plugin_components(&loaded.skills, &loaded.mcp)
    }

    fn local_mcp_servers(&self) -> Result<Vec<LocalMcpServerDto>, DispatchError> {
        let mut projections = Vec::new();
        for (_, environment) in self.environments.iter() {
            let plan = match environment
                .descriptor
                .resolve_plugin_plan(&self.plugin_store)
            {
                Ok(plan) => plan,
                Err(_) => continue,
            };
            for server in &plan.mcp_servers {
                if matches!(server, ResolvedMcpServer::Stdio { .. }) {
                    projections
                        .push(self.local_mcp_projection(&environment.descriptor.id, server)?);
                }
            }
        }
        Ok(projections)
    }

    fn local_mcp_projection(
        &self,
        environment_id: &str,
        server: &ResolvedMcpServer,
    ) -> Result<LocalMcpServerDto, DispatchError> {
        let ResolvedMcpServer::Stdio {
            plugin_name,
            name,
            command,
            args,
            env,
            cwd,
            trust_class,
            ..
        } = server
        else {
            return Err(DispatchError::InvalidParams(
                "remote MCP server does not require local trust".into(),
            ));
        };
        let executable = resolve_plugin_executable(command, *trust_class, &self.search_paths)?;
        let fingerprint = local_mcp_fingerprint(
            environment_id,
            plugin_name,
            name,
            &executable,
            args,
            env,
            cwd,
        )?;
        let trusted = self
            .store
            .local_mcp_trust(environment_id, plugin_name, name)?
            .is_some_and(|record| record.fingerprint == fingerprint);
        Ok(LocalMcpServerDto {
            environment_id: environment_id.into(),
            plugin_name: plugin_name.clone(),
            server_name: name.clone(),
            command: executable
                .to_str()
                .ok_or_else(|| DispatchError::InvalidParams("MCP path is not UTF-8".into()))?
                .into(),
            args: args.clone(),
            cwd: cwd
                .to_str()
                .ok_or_else(|| DispatchError::InvalidParams("MCP cwd is not UTF-8".into()))?
                .into(),
            environment_keys: env.keys().cloned().collect(),
            trust_class: match trust_class {
                ExecutableTrustClass::BundledExecutable => "bundled_executable",
                ExecutableTrustClass::PathExecutable => "path_executable",
                ExecutableTrustClass::NoLocalExecution => "no_local_execution",
            }
            .into(),
            fingerprint,
            trusted,
        })
    }

    fn take_sequence(&mut self) -> u64 {
        let sequence = self.next_event_sequence;
        self.next_event_sequence = self.next_event_sequence.saturating_add(1);
        sequence
    }
}

// Herdr owns its Workspaces and agents and outlives this runtime. Shutdown never
// tears them down; the next process joins durable identities to a fresh snapshot.

/// Trim notification text to what a platform banner will actually show.
fn bounded_notification_text(text: &str, max_chars: usize) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }
    normalized
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>()
        + "…"
}

/// The branch a Draft's worktree lives on.
///
/// Named for whoever runs `git branch`, not for the database: the slug carries
/// the meaning and the short id keeps two same-named Drafts apart.
fn draft_branch(agent_id: Uuid, agent_name: &str, draft_id: Uuid, draft_name: &str) -> String {
    format!(
        "agent-factory/{}/drafts/{}",
        named_id(agent_name, agent_id),
        named_id(draft_name, draft_id),
    )
}

/// The tag an immutable Version is published under.
fn version_tag(agent_id: Uuid, agent_name: &str, version: &str) -> String {
    format!(
        "agent-factory/{}/v{version}",
        named_id(agent_name, agent_id)
    )
}

fn reconcile_draft_publications(
    store: &ProjectStore,
    git: &GitRuntime,
) -> Result<(), RuntimeInitError> {
    for (draft, version) in store.publishing_agent_drafts()? {
        let agent = store.target_agent(draft.target_agent_id)?;
        let tag = version_tag(draft.target_agent_id, &agent.name, &version);
        let tag_ref = format!("refs/tags/{tag}");
        let commit = if git.ref_exists(&agent.repository_root, &tag_ref)? {
            git.resolve_ref(&agent.repository_root, &tag_ref)?
        } else {
            let head = git.head(&draft.worktree_path)?;
            if head == draft.git_head {
                store.restore_agent_draft(draft.id)?;
                let manifest = TargetAgentManifest {
                    schema_version: 4,
                    target_agent_id: draft.target_agent_id,
                    name: draft.name.clone(),
                    objective: draft.objective.clone(),
                    acceptance_criteria: draft.acceptance_criteria.clone(),
                    lifecycle: TargetAgentManifestLifecycle::Draft {
                        draft_id: draft.id,
                        base_version: draft.base_version.clone(),
                    },
                };
                let directory = draft.worktree_path.join(".agent-factory");
                std::fs::create_dir_all(&directory)?;
                std::fs::write(
                    directory.join("target-agent.json"),
                    serde_json::to_vec_pretty(&manifest)?,
                )?;
                continue;
            }
            git.tag_commit(
                &agent.repository_root,
                &tag,
                &head,
                &format!("Create Agent Version v{version}"),
            )?;
            head
        };
        store.finish_agent_draft_publication(draft.id, &version, &commit, &tag)?;
        store.set_agent_draft_cleanup(
            draft.id,
            true,
            Some("Version creation completed; Herdr worktree cleanup is pending."),
        )?;
    }
    Ok(())
}

fn prior_draft_definition(
    store: &ProjectStore,
    target_agent_id: Uuid,
) -> Option<(String, Vec<String>)> {
    let group = store
        .snapshot()
        .ok()?
        .target_workspace
        .target_groups
        .into_iter()
        .find(|group| group.target_agent.id == target_agent_id)?;
    group
        .drafts
        .into_iter()
        .max_by_key(|draft| draft.updated_at_unix_ms)
        .map(|draft| (draft.objective, draft.acceptance_criteria))
}

fn draft_worktree_path(
    repository: &Path,
    agent_name: &str,
    draft_name: &str,
    draft_id: Uuid,
) -> Result<PathBuf, DispatchError> {
    let worktrees_directory =
        RepositoryConfig::load(repository)?.prepare_worktrees_directory(repository)?;
    // The directory already sits inside this repository, so repeating the
    // repository's name in every entry only makes them harder to tell apart.
    let path = worktrees_directory.join(worktree_directory_name(agent_name, draft_name, draft_id));
    reject_path_collision(&path)?;
    Ok(path)
}

/// `ipl-analyst-draft-p2q`: the Agent, what the Draft is called, and enough to
/// stay unique.
///
/// A Draft named after its Agent — "IPL Analyst (DRAFT)" — would otherwise say
/// it twice, so the Agent's slug is dropped from the Draft's when it is already
/// the prefix.
fn worktree_directory_name(agent_name: &str, draft_name: &str, draft_id: Uuid) -> String {
    let agent = slug(agent_name);
    let draft = slug(draft_name);
    let draft = draft
        .strip_prefix(&agent)
        .map(|rest| rest.trim_matches('-'))
        .unwrap_or(draft.as_str());
    let stem = match (agent.is_empty(), draft.is_empty()) {
        (true, true) => "draft".to_owned(),
        (true, false) => draft.to_owned(),
        (false, true) => agent,
        (false, false) => format!("{agent}-{draft}"),
    };
    format!("{stem}-{}", short_id(draft_id))
}

/// Where a Run's session goes on screen.
///
/// A standalone session has neither: it opens a tab named for itself, which is
/// what a session outside any Run should do.
#[derive(Default)]
struct SessionPlacement {
    /// Names the tab when one is opened. Also the fallback name when a column's
    /// intended neighbour has gone and it opens a tab instead.
    tab_label: Option<String>,
    /// The pane this session stands to the right of.
    column_beside: Option<String>,
    /// The iteration this session belongs to, for its own title. Zero outside
    /// any Run.
    iteration: u32,
}

/// The name on an iteration's tab. The objective belongs in the Run, not here:
/// a tab strip has room for a few words.
fn iteration_tab_label(agent_name: &str, iteration: u32) -> String {
    format!("{agent_name} · iteration {iteration}")
}

/// `name-abc`: what a person reads, plus enough to stay unique.
fn named_id(name: &str, id: Uuid) -> String {
    let slug = slug(name);
    let short = short_id(id);
    if slug.is_empty() {
        short
    } else {
        format!("{slug}-{short}")
    }
}

/// Glyphs that cannot be mistaken for one another in a terminal: no `0`/`o`,
/// no `1`/`l`/`i`, no `u`/`v` confusion from `u`.
const SHORT_ID_ALPHABET: &[u8] = b"23456789abcdefghjkmnpqrstwxyz";
const SHORT_ID_LENGTH: usize = 3;

/// A short identifier derived from a record's own id.
///
/// Derived rather than drawn fresh so a name is reproducible from the record it
/// belongs to: nothing extra is persisted, and the same Draft always produces
/// the same suffix wherever it is rendered.
pub(crate) fn short_id(id: Uuid) -> String {
    let mut value = u128::from_be_bytes(*id.as_bytes());
    let base = SHORT_ID_ALPHABET.len() as u128;
    let mut short = String::with_capacity(SHORT_ID_LENGTH);
    for _ in 0..SHORT_ID_LENGTH {
        short.push(SHORT_ID_ALPHABET[(value % base) as usize] as char);
        value /= base;
    }
    short
}

fn slug(value: &str) -> String {
    let mut slug = String::new();
    for character in value.trim().chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_matches('-').chars().take(48).collect::<String>()
}

fn normalize_criteria(criteria: &[String]) -> Result<Vec<String>, DispatchError> {
    let criteria = criteria
        .iter()
        .map(|criterion| criterion.trim())
        .filter(|criterion| !criterion.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if criteria.is_empty() {
        return Err(DispatchError::InvalidParams(
            "at least one Agent success criterion is required".into(),
        ));
    }
    if criteria.iter().any(|criterion| criterion.len() > 16 * 1024) {
        return Err(DispatchError::InvalidParams(
            "Agent success criterion is too long".into(),
        ));
    }
    Ok(criteria)
}

fn normalize_required(label: &str, value: &str, max_bytes: usize) -> Result<String, DispatchError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(DispatchError::InvalidParams(format!(
            "{label} must not be empty",
        )));
    }
    if value.len() > max_bytes {
        return Err(DispatchError::InvalidParams(
            format!("{label} is too long",),
        ));
    }
    Ok(value.into())
}

fn write_draft_manifest(draft: &AgentDraftProjection) -> Result<(), DispatchError> {
    write_agent_manifest(
        &draft.worktree_path,
        &TargetAgentManifest {
            schema_version: 4,
            target_agent_id: draft.target_agent_id,
            name: draft.name.clone(),
            objective: draft.objective.clone(),
            acceptance_criteria: draft.acceptance_criteria.clone(),
            lifecycle: TargetAgentManifestLifecycle::Draft {
                draft_id: draft.id,
                base_version: draft.base_version.clone(),
            },
        },
    )
}

fn write_version_manifest(
    draft: &AgentDraftProjection,
    version: &str,
) -> Result<(), DispatchError> {
    write_agent_manifest(
        &draft.worktree_path,
        &TargetAgentManifest {
            schema_version: 4,
            target_agent_id: draft.target_agent_id,
            name: draft.name.clone(),
            objective: draft.objective.clone(),
            acceptance_criteria: draft.acceptance_criteria.clone(),
            lifecycle: TargetAgentManifestLifecycle::Version {
                version: version.into(),
            },
        },
    )
}

fn write_agent_manifest(root: &Path, manifest: &TargetAgentManifest) -> Result<(), DispatchError> {
    let directory = root.join(".agent-factory");
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("target-agent.json");
    std::fs::write(path, serde_json::to_vec_pretty(manifest)?)?;
    Ok(())
}

fn next_version(
    base: Option<&str>,
    versions: &[app_core::TargetAgentVersionProjection],
    bump: VersionBump,
) -> Result<String, DispatchError> {
    let used = versions
        .iter()
        .filter_map(|version| semver::Version::parse(&version.version).ok())
        .collect::<BTreeSet<_>>();
    if used.is_empty() {
        return Ok("0.1.0".into());
    }
    let base = base
        .and_then(|base| semver::Version::parse(base).ok())
        .or_else(|| used.iter().next_back().cloned())
        .ok_or_else(|| DispatchError::InvalidParams("Version history is invalid".into()))?;
    let mut candidate = base;
    match bump {
        VersionBump::Patch => candidate.patch += 1,
        VersionBump::Minor => {
            candidate.minor += 1;
            candidate.patch = 0;
        }
        VersionBump::Major => {
            candidate.major += 1;
            candidate.minor = 0;
            candidate.patch = 0;
        }
    }
    while used.contains(&candidate) {
        match bump {
            VersionBump::Patch => candidate.patch += 1,
            VersionBump::Minor => candidate.minor += 1,
            VersionBump::Major => candidate.major += 1,
        }
    }
    Ok(candidate.to_string())
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn default_session_title(
    purpose: HarnessPurpose,
    target_agent_name: &str,
    binding_name: &str,
) -> String {
    let context = format!("{} · {}", target_agent_name, binding_name);
    match purpose {
        HarnessPurpose::Orchestration => format!("Orchestrator · {context}"),
        HarnessPurpose::Coding => format!("Coding Agent · {context}"),
        HarnessPurpose::Evaluation => format!("Evaluator · {context}"),
    }
}

fn require_prompt_text(text: &str) -> Result<(), DispatchError> {
    if text.trim().is_empty() {
        return Err(DispatchError::InvalidParams("prompt text is empty".into()));
    }
    if text.len() > MAX_FACTORY_PROMPT_BYTES {
        return Err(DispatchError::InvalidParams(format!(
            "prompt exceeds the {MAX_FACTORY_PROMPT_BYTES}-byte limit"
        )));
    }
    Ok(())
}

/// Where an evaluator is asked to write its verdict.
///
/// Agents that render on the terminal's alternate screen lose rows to Herdr's
/// host scrollback, so a transcript read cannot be trusted to contain a long
/// final answer. A file always can.
fn evaluator_verdict_path(workspace_root: &Path, session_id: Uuid) -> PathBuf {
    workspace_root.join(format!(".agent-factory/verdict-{session_id}.json"))
}

fn provisional_session_title(prompt: &str) -> Option<String> {
    prompt
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .and_then(normalize_session_title)
}

fn normalize_session_title(title: &str) -> Option<String> {
    let normalized = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return None;
    }
    let mut characters = normalized.chars();
    let prefix = characters
        .by_ref()
        .take(MAX_SESSION_TITLE_CHARS)
        .collect::<String>();
    if characters.next().is_none() {
        return Some(prefix);
    }
    let mut truncated = prefix
        .chars()
        .take(MAX_SESSION_TITLE_CHARS.saturating_sub(1))
        .collect::<String>();
    truncated.push('…');
    Some(truncated)
}

/// Where the `agent-factory` command lives. It is installed beside the runtime,
/// so the runtime's own location is the answer in development and when packaged.
fn control_cli_directory() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    Some(executable.parent()?.to_path_buf())
}

/// The run state as the Orchestrator sees it, and as the CLI prints it.
fn run_state_label(state: FactoryRunState) -> &'static str {
    match state {
        FactoryRunState::Draft => "draft",
        FactoryRunState::Orchestrating => "orchestrating",
        FactoryRunState::Coding => "coding",
        FactoryRunState::Evaluating => "evaluating",
        FactoryRunState::Escalated => "escalated",
        FactoryRunState::Passed => "passed",
        FactoryRunState::Failed => "failed",
        FactoryRunState::NeedsReview => "needs_review",
        FactoryRunState::Cancelled => "cancelled",
    }
}

fn lifecycle_label(lifecycle: AgentLifecycle) -> &'static str {
    match lifecycle {
        AgentLifecycle::Idle => "idle",
        AgentLifecycle::Working => "working",
        AgentLifecycle::Blocked => "blocked",
        AgentLifecycle::Done => "done",
        AgentLifecycle::Unknown => "unknown",
    }
}

/// What the Orchestrator gets back: the agent to prompt next, and enough state
/// to decide without reading any terminal.
fn run_view(
    run: &FactoryRun,
    session: Option<AgentSessionProjection>,
    iteration: u32,
    message: String,
) -> agent_control::RunView {
    agent_control::RunView {
        state: run_state_label(run.state).to_owned(),
        iteration,
        objective: run.objective.clone(),
        acceptance_criteria: run.acceptance_criteria.clone(),
        changed_file_count: run.changed_files.len() as u32,
        agent: session.and_then(|session| {
            let placement = session.placement?;
            Some(agent_control::AgentHandle {
                name: placement.agent_name,
                pane_id: placement.pane_id,
                harness_id: session.harness_id,
            })
        }),
        evaluation: run
            .evaluation
            .as_ref()
            .map(|evaluation| agent_control::EvaluationView {
                verdict: format!("{:?}", evaluation.verdict).to_ascii_lowercase(),
                summary: evaluation.summary.clone(),
            }),
        message,
    }
}

/// Whether this session still owes its agent the opening brief.
///
/// Delivery is its own durable fact rather than a lifecycle value. Herdr reports
/// an agent sitting on its startup screen as idle, so a lifecycle-encoded marker
/// has two writers — the brief state machine and reconciliation against Herdr —
/// and whichever writes last decides whether the brief is ever retried.
fn awaiting_initial_prompt(session: &AgentSessionProjection) -> bool {
    !session.brief_delivered
        && session.outcome.is_none()
        && session.availability == SessionAvailability::Live
        && session.lifecycle.is_some_and(|lifecycle| {
            lifecycle.accepts_prompt() || lifecycle == AgentLifecycle::Unknown
        })
        && session.placement.is_some()
        && session.initial_prompt.is_some()
}

fn sanitize_public_diagnostic(input: &str) -> String {
    let mut output = String::new();
    for line in input.lines() {
        let normalized = line.to_ascii_lowercase();
        let sensitive = [
            "authorization",
            "bearer ",
            "api_key",
            "api-key",
            "apikey",
            "password",
            "secret",
            "access_token",
            "refresh_token",
            "cookie",
        ]
        .iter()
        .any(|marker| normalized.contains(marker));
        let public_line = if sensitive {
            "[redacted sensitive diagnostic line]"
        } else {
            line
        };
        for character in public_line.chars() {
            if output.len() + character.len_utf8() > MAX_PUBLIC_DIAGNOSTIC_BYTES {
                break;
            }
            if !character.is_control() || character == '\t' {
                output.push(character);
            }
        }
        if output.len() >= MAX_PUBLIC_DIAGNOSTIC_BYTES {
            break;
        }
        output.push('\n');
    }
    if output.len() >= MAX_PUBLIC_DIAGNOSTIC_BYTES {
        while !output.is_char_boundary(MAX_PUBLIC_DIAGNOSTIC_BYTES) {
            output.pop();
        }
        output.truncate(MAX_PUBLIC_DIAGNOSTIC_BYTES);
    }
    let output = output.trim().to_owned();
    if output.is_empty() {
        "The agent failed without a public diagnostic.".into()
    } else {
        output
    }
}

fn user_shell() -> PathBuf {
    std::env::var_os("SHELL")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute() && path.is_file())
        .unwrap_or_else(|| PathBuf::from("/bin/zsh"))
}

fn plugin_registry_dto(registry: PluginRegistryRecord) -> PluginRegistryDto {
    PluginRegistryDto {
        id: registry.id,
        catalog_url: registry.catalog_url,
        signature_url: registry.signature_url,
        public_key_base64: registry.public_key_base64,
    }
}

fn ensure_default_plugin_registry(store: &ProjectStore) -> Result<(), StoreError> {
    let registry = PluginRegistryRecord {
        id: DEFAULT_PLUGIN_REGISTRY_ID.into(),
        catalog_url: DEFAULT_PLUGIN_REGISTRY_CATALOG_URL.into(),
        signature_url: DEFAULT_PLUGIN_REGISTRY_SIGNATURE_URL.into(),
        public_key_base64: DEFAULT_PLUGIN_REGISTRY_PUBLIC_KEY.into(),
    };
    if store.list_plugin_registries()?.contains(&registry) {
        return Ok(());
    }
    store.put_plugin_registry(&registry)
}

fn registry_plugin_source_url(registry: &PluginRegistryRecord, plugin_id: &str) -> String {
    if registry.catalog_url == DEFAULT_PLUGIN_REGISTRY_CATALOG_URL {
        return format!("{DEFAULT_PLUGIN_REGISTRY_SOURCE_URL}/tree/main/plugins/{plugin_id}");
    }
    let Ok(url) = Url::parse(&registry.catalog_url) else {
        return registry.catalog_url.clone();
    };
    if url.host_str() != Some("raw.githubusercontent.com") {
        return registry.catalog_url.clone();
    }
    let Some(segments) = url.path_segments() else {
        return registry.catalog_url.clone();
    };
    let segments = segments.collect::<Vec<_>>();
    let [owner, repository, reference, ..] = segments.as_slice() else {
        return registry.catalog_url.clone();
    };
    format!("https://github.com/{owner}/{repository}/tree/{reference}/plugins/{plugin_id}")
}

fn project_plugin_components(
    skills: &[plugin_runtime::SkillDefinition],
    mcp: &McpComponent,
) -> (
    Vec<InstalledSkillDto>,
    Vec<InstalledMcpServerDto>,
    Option<String>,
) {
    let skills = skills
        .iter()
        .map(|skill| InstalledSkillDto {
            name: skill.name.clone(),
            description: skill.description.clone(),
        })
        .collect();
    let (mcp_servers, mcp_disabled_reason) = match mcp {
        McpComponent::Absent => (Vec::new(), None),
        McpComponent::Disabled { reason } => (Vec::new(), Some(reason.clone())),
        McpComponent::Loaded(servers) => {
            let projected = servers
                .iter()
                .map(|server| InstalledMcpServerDto {
                    name: server.name().to_owned(),
                    kind: match server {
                        McpServerDefinition::Stdio { .. } => InstalledMcpServerKindDto::Stdio,
                        McpServerDefinition::StreamableHttp { .. } => {
                            InstalledMcpServerKindDto::StreamableHttp
                        }
                        McpServerDefinition::Sse { .. } => InstalledMcpServerKindDto::Sse,
                    },
                })
                .collect();
            (projected, None)
        }
    };
    (skills, mcp_servers, mcp_disabled_reason)
}

fn decode_registry_public_key(value: &str) -> Result<[u8; 32], DispatchError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|_| DispatchError::InvalidParams("invalid registry public key".into()))?;
    bytes
        .try_into()
        .map_err(|_| DispatchError::InvalidParams("registry public key must be 32 bytes".into()))
}

fn resolve_plugin_executable(
    command: &Path,
    trust_class: ExecutableTrustClass,
    search_paths: &[PathBuf],
) -> Result<PathBuf, DispatchError> {
    if trust_class == ExecutableTrustClass::NoLocalExecution {
        return Err(DispatchError::InvalidParams(
            "remote MCP server has no local executable".into(),
        ));
    }
    let candidate = if command.is_absolute() {
        command.to_path_buf()
    } else {
        if command.components().count() != 1 {
            return Err(DispatchError::InvalidParams(
                "PATH MCP executable must be a bare command name".into(),
            ));
        }
        search_paths
            .iter()
            .map(|root| root.join(command))
            .find(|path| path.is_file())
            .ok_or_else(|| DispatchError::InvalidParams("MCP executable is unavailable".into()))?
    };
    let canonical = std::fs::canonicalize(&candidate)?;
    let metadata = canonical.metadata()?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > 256 * 1024 * 1024 {
        return Err(DispatchError::InvalidParams(
            "MCP executable is invalid or exceeds 256 MiB".into(),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(DispatchError::InvalidParams(
                "MCP executable is not executable".into(),
            ));
        }
    }
    Ok(canonical)
}

fn local_mcp_fingerprint(
    environment_id: &str,
    plugin_name: &str,
    server_name: &str,
    executable: &Path,
    args: &[String],
    env: &BTreeMap<String, String>,
    cwd: &Path,
) -> Result<String, DispatchError> {
    let mut digest = Sha256::new();
    hash_field(&mut digest, environment_id.as_bytes());
    hash_field(&mut digest, plugin_name.as_bytes());
    hash_field(&mut digest, server_name.as_bytes());
    hash_field(&mut digest, executable.as_os_str().as_encoded_bytes());
    let mut file = std::fs::File::open(executable)?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(256 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 256 * 1024 * 1024 {
        return Err(DispatchError::InvalidParams(
            "MCP executable exceeds 256 MiB".into(),
        ));
    }
    hash_field(&mut digest, &Sha256::digest(&bytes));
    for argument in args {
        hash_field(&mut digest, argument.as_bytes());
    }
    for (name, value) in env {
        hash_field(&mut digest, name.as_bytes());
        hash_field(&mut digest, value.as_bytes());
    }
    hash_field(&mut digest, cwd.as_os_str().as_encoded_bytes());
    Ok(hex::encode(digest.finalize()))
}

fn hash_field(digest: &mut Sha256, value: &[u8]) {
    digest.update((value.len() as u64).to_be_bytes());
    digest.update(value);
}

fn packaged_update_install_paths(
    executable: &Path,
    extraction_parent: PathBuf,
) -> Result<UpdateInstallPaths, &'static str> {
    if !executable.is_absolute()
        || executable
            .file_name()
            .is_none_or(|name| name != "agent-factory-runtime")
        || executable
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("runtime is not in a packaged update layout");
    }
    let resources = executable
        .parent()
        .filter(|path| path.file_name().is_some_and(|name| name == "Resources"))
        .ok_or("runtime is not directly inside Contents/Resources")?;
    let contents = resources
        .parent()
        .filter(|path| path.file_name().is_some_and(|name| name == "Contents"))
        .ok_or("runtime is not directly inside Contents/Resources")?;
    let bundle = contents
        .parent()
        .filter(|path| path.extension().is_some_and(|extension| extension == "app"))
        .ok_or("runtime is not inside an application bundle")?;
    let helper = resources.join("updater-helper");
    let metadata = std::fs::symlink_metadata(&helper).map_err(|_| "updater helper is missing")?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("updater helper is not a regular file");
    }
    Ok(UpdateInstallPaths {
        current_bundle: bundle.to_path_buf(),
        helper,
        extraction_parent,
    })
}

fn current_macos_version() -> Result<String, DispatchError> {
    if !cfg!(target_os = "macos") {
        return Err(UpdateError::UnsupportedPlatform.into());
    }
    let output = Command::new("/usr/bin/sw_vers")
        .arg("-productVersion")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()?;
    if !output.status.success() || output.stdout.len() > 64 {
        return Err(DispatchError::UpdateHelper(
            "unable to determine the macOS version".into(),
        ));
    }
    let version = std::str::from_utf8(&output.stdout)
        .map_err(|_| DispatchError::UpdateHelper("macOS version is not UTF-8".into()))?
        .trim();
    if version.is_empty() {
        return Err(DispatchError::UpdateHelper("macOS version is empty".into()));
    }
    Ok(version.into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpdateHelperResponse {
    ok: bool,
    /// Present on success; this caller only needs the failure reason.
    #[allow(dead_code)]
    result: Option<Value>,
    error: Option<Value>,
}

fn invoke_update_helper(helper: &Path, request: Value) -> Result<(), DispatchError> {
    let metadata = std::fs::symlink_metadata(helper)?;
    if !helper.is_absolute() || metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(DispatchError::UpdateHelper(
            "updater helper path is invalid".into(),
        ));
    }
    let input = serde_json::to_vec(&request)?;
    if input.len() > 64 * 1024 {
        return Err(DispatchError::UpdateHelper(
            "updater helper request exceeds 64 KiB".into(),
        ));
    }
    let mut child = Command::new(helper)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or_else(|| DispatchError::UpdateHelper("updater stdin unavailable".into()))?
        .write_all(&input)?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() > Duration::from_secs(120) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(DispatchError::UpdateHelper(
                "updater helper timed out".into(),
            ));
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let mut output = Vec::new();
    child
        .stdout
        .take()
        .ok_or_else(|| DispatchError::UpdateHelper("updater stdout unavailable".into()))?
        .take(64 * 1024 + 1)
        .read_to_end(&mut output)?;
    if output.len() > 64 * 1024 {
        return Err(DispatchError::UpdateHelper(
            "updater helper response exceeds 64 KiB".into(),
        ));
    }
    let response: UpdateHelperResponse = serde_json::from_slice(&output)?;
    if !status.success() || !response.ok {
        return Err(DispatchError::UpdateHelper(format!(
            "updater helper failed: {}",
            response
                .error
                .map(|error| error.to_string())
                .unwrap_or_else(|| "unknown error".into())
        )));
    }
    Ok(())
}

fn update_state_projection(state: &UpdateState) -> (&'static str, Option<String>, Option<String>) {
    match state {
        UpdateState::Idle => ("idle", None, None),
        UpdateState::Checking => ("checking", None, None),
        UpdateState::Available { version } => ("available", Some(version.clone()), None),
        UpdateState::AwaitingConfirmation { version } => {
            ("awaiting_confirmation", Some(version.clone()), None)
        }
        UpdateState::Confirmed { version } => ("confirmed", Some(version.clone()), None),
        UpdateState::Downloading { version } => ("downloading", Some(version.clone()), None),
        UpdateState::Verifying { version } => ("verifying", Some(version.clone()), None),
        UpdateState::Staged { version, .. } => ("staged", Some(version.clone()), None),
        UpdateState::Installing { version } => ("installing", Some(version.clone()), None),
        UpdateState::ReadyToRestart { version } => {
            ("ready_to_restart", Some(version.clone()), None)
        }
        UpdateState::Failed { message } => ("failed", None, Some(message.clone())),
    }
}

fn environment_variable_values(
    entries: Vec<EnvironmentVariableProjection>,
) -> Result<BTreeMap<String, environment_runtime::EnvironmentValue>, DispatchError> {
    let mut environment = BTreeMap::new();
    for entry in entries {
        let value = match entry.source {
            EnvironmentVariableSource::Literal => environment_runtime::EnvironmentValue::Literal(
                environment_runtime::LiteralEnvironmentValue {
                    literal: entry.value,
                },
            ),
            EnvironmentVariableSource::Secret => {
                let secret_ref =
                    entry
                        .value
                        .parse::<SecretRef>()
                        .map_err(|error: SecretError| {
                            DispatchError::InvalidParams(error.to_string())
                        })?;
                environment_runtime::EnvironmentValue::Secret(
                    environment_runtime::SecretEnvironmentValue { secret_ref },
                )
            }
        };
        if environment.insert(entry.name.clone(), value).is_some() {
            return Err(DispatchError::InvalidParams(format!(
                "duplicate environment variable `{}`",
                entry.name
            )));
        }
    }
    Ok(environment)
}

fn environment_llm_provider(
    value: ResolvedLlmProviderDto,
) -> Result<LlmProviderConfiguration, DispatchError> {
    let provider = LlmProviderConfiguration {
        provider_type: provider_kind(value.provider_type),
        endpoint: value.endpoint,
        credential_ref: value
            .credential_ref
            .map(|reference| reference.parse::<SecretRef>())
            .transpose()
            .map_err(|error| DispatchError::InvalidParams(error.to_string()))?,
        allowed_models: value.allowed_models,
    };
    provider
        .validate()
        .map_err(|error| DispatchError::InvalidParams(error.to_string()))?;
    Ok(provider)
}

fn llm_provider_connection(
    value: runtime_contract::LlmProviderConnectionDto,
) -> Result<LlmProviderConfiguration, DispatchError> {
    Ok(LlmProviderConfiguration {
        provider_type: provider_kind(value.provider_type),
        endpoint: value.endpoint,
        credential_ref: value
            .credential_ref
            .map(|reference| reference.parse::<SecretRef>())
            .transpose()
            .map_err(|error| DispatchError::InvalidParams(error.to_string()))?,
        allowed_models: Vec::new(),
    })
}

const fn provider_kind(value: ProjectedLlmProviderType) -> LlmProviderKind {
    match value {
        ProjectedLlmProviderType::Ollama => LlmProviderKind::Ollama,
        ProjectedLlmProviderType::Litellm => LlmProviderKind::Litellm,
        ProjectedLlmProviderType::Meta => LlmProviderKind::Meta,
        ProjectedLlmProviderType::OpenAi => LlmProviderKind::OpenAi,
    }
}

const fn project_provider_kind(value: LlmProviderKind) -> ProjectedLlmProviderType {
    match value {
        LlmProviderKind::Ollama => ProjectedLlmProviderType::Ollama,
        LlmProviderKind::Litellm => ProjectedLlmProviderType::Litellm,
        LlmProviderKind::Meta => ProjectedLlmProviderType::Meta,
        LlmProviderKind::OpenAi => ProjectedLlmProviderType::OpenAi,
    }
}

fn project_llm_provider(
    id: Uuid,
    value: LlmProviderConfigurationDto,
    available_secret_refs: &BTreeSet<String>,
) -> Result<LlmProviderDto, DispatchError> {
    validate_provider_name(&value.name)
        .map_err(|error| DispatchError::InvalidParams(error.to_string()))?;
    let provider = LlmProviderConfiguration {
        provider_type: provider_kind(value.provider_type),
        endpoint: value.endpoint,
        credential_ref: value
            .credential_ref
            .map(|reference| reference.parse::<SecretRef>())
            .transpose()
            .map_err(|error| DispatchError::InvalidParams(error.to_string()))?,
        allowed_models: value.allowed_models,
    };
    provider
        .validate()
        .map_err(|error| DispatchError::InvalidParams(error.to_string()))?;
    let readiness = llm_provider_readiness(&provider, available_secret_refs);
    Ok(LlmProviderDto {
        id,
        name: value.name,
        provider_type: project_provider_kind(provider.provider_type),
        endpoint: provider.endpoint,
        credential_ref: provider.credential_ref.as_ref().map(ToString::to_string),
        allowed_models: provider.allowed_models,
        readiness,
    })
}

fn ensure_unique_provider_name(
    providers: &[LlmProviderDto],
    current_id: Option<Uuid>,
    name: &str,
) -> Result<(), DispatchError> {
    if providers
        .iter()
        .any(|provider| Some(provider.id) != current_id && provider.name == name)
    {
        return Err(DispatchError::InvalidParams(format!(
            "an Intelligence Provider named {name:?} already exists"
        )));
    }
    Ok(())
}

fn provider_execution_fields_changed(previous: &LlmProviderDto, next: &LlmProviderDto) -> bool {
    previous.provider_type != next.provider_type
        || previous.endpoint != next.endpoint
        || previous.credential_ref != next.credential_ref
        || previous.allowed_models != next.allowed_models
}

fn llm_provider_readiness(
    provider: &LlmProviderConfiguration,
    available_secret_refs: &BTreeSet<String>,
) -> LlmProviderReadinessProjection {
    let mut issues = Vec::new();
    if let Some(reference) = &provider.credential_ref
        && !available_secret_refs.contains(&reference.to_string())
    {
        issues.push("The Intelligence Provider secret is unavailable".to_owned());
    }
    LlmProviderReadinessProjection {
        state: if issues.is_empty() {
            EnvironmentReadinessState::Ready
        } else {
            EnvironmentReadinessState::NeedsSetup
        },
        issues,
    }
}

fn llm_provider_projection_readiness(
    provider: &LlmProviderDto,
    available_secret_refs: &BTreeSet<String>,
) -> LlmProviderReadinessProjection {
    let mut issues = Vec::new();
    if let Some(reference) = &provider.credential_ref
        && !available_secret_refs.contains(reference)
    {
        issues.push("The Intelligence Provider secret is unavailable".to_owned());
    }
    LlmProviderReadinessProjection {
        state: if issues.is_empty() {
            EnvironmentReadinessState::Ready
        } else {
            EnvironmentReadinessState::NeedsSetup
        },
        issues,
    }
}

/// Converts a wire draft into the catalog's shape, validating the parts that
/// carry references into other stores (secret refs) as it goes.
fn environment_draft(
    configuration: EnvironmentConfigurationDraft,
) -> Result<environment_runtime::EnvironmentDraft, DispatchError> {
    Ok(environment_runtime::EnvironmentDraft {
        name: configuration.name,
        environment_variables: environment_variable_values(configuration.environment_variables)?,
        llm: configuration
            .llm
            .map(|llm| environment_runtime::EnvironmentLlmPolicy {
                provider_id: llm.provider_id,
                allowed_models: llm.allowed_models,
                default_model: llm.default_model,
            }),
        plugins: configuration
            .plugins
            .into_iter()
            .map(|plugin| environment_runtime::EnvironmentPlugin {
                name: plugin.name,
                enabled_mcp_servers: plugin.enabled_mcp_servers,
                default_skills: plugin.default_skills,
            })
            .collect(),
        registries: configuration.registries,
        permissions: descriptor_permissions(configuration.permissions.unwrap_or_default()),
    })
}

/// Carry the requested policy into the descriptor. Omitted stays cautious:
/// reads allowed, writes and terminal use ask.
fn descriptor_permissions(
    permissions: EnvironmentPermissionProjection,
) -> environment_runtime::EnvironmentPermissions {
    environment_runtime::EnvironmentPermissions {
        trusted_read: descriptor_permission(permissions.trusted_read),
        trusted_write: descriptor_permission(permissions.trusted_write),
        terminal: descriptor_permission(permissions.terminal),
    }
}

const fn descriptor_permission(policy: EnvironmentPermissionPolicy) -> PermissionPolicy {
    match policy {
        EnvironmentPermissionPolicy::Allow => PermissionPolicy::Allow,
        EnvironmentPermissionPolicy::Ask => PermissionPolicy::Ask,
        EnvironmentPermissionPolicy::Deny => PermissionPolicy::Deny,
    }
}

fn environment_draft_from_descriptor(
    descriptor: &environment_runtime::EnvironmentDescriptor,
) -> environment_runtime::EnvironmentDraft {
    environment_runtime::EnvironmentDraft {
        name: descriptor.name.clone(),
        environment_variables: descriptor.environment_variables.clone(),
        llm: descriptor.llm.clone(),
        plugins: descriptor.plugins.clone(),
        registries: descriptor.registries.clone(),
        // Editing an Environment must not quietly re-narrow what it may do.
        permissions: descriptor.permissions,
    }
}

fn environment_projection(
    descriptor: &environment_runtime::EnvironmentDescriptor,
    available_secret_refs: &BTreeSet<String>,
    providers: &[LlmProviderDto],
    llm_needs_setup: bool,
    plugin_store: &PluginStore,
) -> EnvironmentProjection {
    let mut readiness = environment_readiness(
        descriptor,
        available_secret_refs,
        providers,
        llm_needs_setup,
    );
    if let Err(error) = descriptor.resolve_plugin_plan(plugin_store) {
        readiness.state = EnvironmentReadinessState::NeedsSetup;
        readiness
            .issues
            .push(format!("Skills & Tools cannot be resolved: {error}"));
    }
    let resolved_llm = descriptor
        .llm
        .as_ref()
        .and_then(|policy| resolve_environment_llm(policy, providers).ok());
    EnvironmentProjection {
        id: descriptor.id.clone(),
        name: descriptor.name.clone(),
        coding_harness_id: descriptor.harnesses.coding.clone(),
        evaluation_harness_id: descriptor.harnesses.evaluation.clone(),
        plugins: descriptor
            .plugins
            .iter()
            .map(|plugin| EnvironmentPluginProjection {
                name: plugin.name.clone(),
                enabled_mcp_servers: plugin.enabled_mcp_servers.clone(),
                default_skills: plugin.default_skills.clone(),
            })
            .collect(),
        permissions: EnvironmentPermissionProjection {
            trusted_read: project_permission(descriptor.permissions.trusted_read),
            trusted_write: project_permission(descriptor.permissions.trusted_write),
            terminal: project_permission(descriptor.permissions.terminal),
        },
        registry_ids: descriptor.registries.clone(),
        environment_variables: descriptor
            .environment_variables
            .iter()
            .map(|(name, value)| {
                let (source, projected_value) = match value {
                    environment_runtime::EnvironmentValue::Literal(value) => {
                        (EnvironmentVariableSource::Literal, value.literal.clone())
                    }
                    environment_runtime::EnvironmentValue::Secret(value) => (
                        EnvironmentVariableSource::Secret,
                        value.secret_ref.to_string(),
                    ),
                };
                app_core::EnvironmentVariableProjection {
                    name: name.clone(),
                    source,
                    value: projected_value,
                }
            })
            .collect(),
        llm: descriptor.llm.as_ref().map(|llm| EnvironmentLlmPolicyDto {
            provider_id: llm.provider_id,
            allowed_models: llm.allowed_models.clone(),
            default_model: llm.default_model.clone(),
        }),
        resolved_llm,
        llm_needs_setup,
        readiness,
    }
}

fn resolve_environment_llm(
    policy: &environment_runtime::EnvironmentLlmPolicy,
    providers: &[LlmProviderDto],
) -> Result<ResolvedLlmProviderDto, String> {
    let provider = providers
        .iter()
        .find(|provider| provider.id == policy.provider_id)
        .ok_or_else(|| "The selected Intelligence Provider is unavailable".to_owned())?;
    if provider.readiness.state != EnvironmentReadinessState::Ready {
        return Err(provider.readiness.issues.join("; "));
    }
    let allowed_models = policy.allowed_models.clone();
    if let Some(model) = allowed_models
        .iter()
        .find(|model| !provider.allowed_models.contains(model))
    {
        return Err(format!(
            "Environment model {model:?} is not allowed by the Intelligence Provider"
        ));
    }
    let default_model = policy.default_model.clone();
    if !allowed_models.contains(&default_model) {
        return Err(format!(
            "Environment default model {default_model:?} is not among its available models"
        ));
    }
    Ok(ResolvedLlmProviderDto {
        provider_id: provider.id,
        provider_name: provider.name.clone(),
        provider_type: provider.provider_type,
        endpoint: provider.endpoint.clone(),
        credential_ref: provider.credential_ref.clone(),
        allowed_models,
        default_model,
    })
}

fn resolve_model(
    provider: Option<&ResolvedLlmProviderDto>,
    model: Option<&str>,
) -> Result<String, DispatchError> {
    let provider = provider.ok_or_else(|| {
        DispatchError::InvalidParams(
            "the Environment has no Intelligence Provider; Harness sessions cannot be started"
                .into(),
        )
    })?;
    let model = model.unwrap_or(&provider.default_model);
    if !provider
        .allowed_models
        .iter()
        .any(|allowed| allowed == model)
    {
        return Err(DispatchError::InvalidParams(format!(
            "model {model:?} is not allowed by the Environment's Intelligence Provider"
        )));
    }
    Ok(model.to_owned())
}

fn environment_readiness(
    descriptor: &environment_runtime::EnvironmentDescriptor,
    available_secret_refs: &BTreeSet<String>,
    providers: &[LlmProviderDto],
    llm_needs_setup: bool,
) -> EnvironmentReadinessProjection {
    environment_readiness_from_configuration(
        &descriptor.environment_variables,
        descriptor.llm.as_ref(),
        available_secret_refs,
        providers,
        llm_needs_setup,
    )
}

fn environment_readiness_from_configuration(
    environment_variables: &BTreeMap<String, environment_runtime::EnvironmentValue>,
    llm: Option<&environment_runtime::EnvironmentLlmPolicy>,
    available_secret_refs: &BTreeSet<String>,
    providers: &[LlmProviderDto],
    llm_needs_setup: bool,
) -> EnvironmentReadinessProjection {
    let mut issues = Vec::new();
    if llm_needs_setup {
        issues.push("Provider changed—review this Environment".to_owned());
    }
    match llm {
        None => issues.push("Configure an Intelligence Provider".to_owned()),
        Some(policy) => {
            if let Err(error) = resolve_environment_llm(policy, providers) {
                issues.push(error);
            }
        }
    }
    for (name, value) in environment_variables {
        if let environment_runtime::EnvironmentValue::Secret(value) = value
            && !available_secret_refs.contains(&value.secret_ref.to_string())
        {
            issues.push(format!(
                "Environment variable {name} references an unavailable secret"
            ));
        }
    }
    EnvironmentReadinessProjection {
        state: if issues.is_empty() {
            EnvironmentReadinessState::Ready
        } else {
            EnvironmentReadinessState::NeedsSetup
        },
        issues,
    }
}

const fn project_permission(policy: PermissionPolicy) -> EnvironmentPermissionPolicy {
    match policy {
        PermissionPolicy::Allow => EnvironmentPermissionPolicy::Allow,
        PermissionPolicy::Ask => EnvironmentPermissionPolicy::Ask,
        PermissionPolicy::Deny => EnvironmentPermissionPolicy::Deny,
    }
}

fn shell_args(executable: &Path) -> Vec<String> {
    match executable.file_name().and_then(|name| name.to_str()) {
        Some("zsh" | "bash" | "fish") => vec!["-l".into()],
        _ => vec![],
    }
}

fn shell_environment(search_paths: &[PathBuf]) -> BTreeMap<String, String> {
    let mut environment = std::env::vars()
        .filter(|(name, _)| {
            matches!(name.as_str(), "HOME" | "PATH" | "LANG" | "TERM") || name.starts_with("LC_")
        })
        .collect::<BTreeMap<_, _>>();
    if let Some(path) = joined_search_path(search_paths) {
        environment.insert("PATH".into(), path);
    }
    environment
}

/// Overlay the loopback gateway without setting `ANTHROPIC_API_KEY`.
///
/// Claude Code treats a non-empty `ANTHROPIC_API_KEY` as a custom key and
/// blocks the TUI until the user confirms it. The gateway already injects
/// `ANTHROPIC_AUTH_TOKEN` as the sentinel. An empty API key wipes any
/// inherited user credential without triggering that prompt.
fn herdr_provider_overrides() -> [(&'static str, &'static str); 4] {
    [
        ("ANTHROPIC_API_KEY", ""),
        ("CLAUDE_CODE_USE_BEDROCK", "0"),
        ("CLAUDE_CODE_USE_VERTEX", "0"),
        ("CLAUDE_CODE_DISABLE_UNKNOWN_MODEL_WINDOW_ENFORCEMENT", "1"),
    ]
}

/// How the Environment's permission policy is expressed to the harness.
///
/// A Factory Run is meant to advance without a person watching it, so an agent
/// that stops at an approval dialog has stalled the whole loop. The Environment
/// already states how much the user is willing to let an agent do unattended;
/// this is where that statement becomes the flag that honours it.
///
/// Nothing is widened here. An Environment that says `ask` still asks, and the
/// agent will sit blocked until someone answers — which is the correct outcome
/// for an Environment configured that way, not a bug to route around.
fn harness_permission_args(
    harness_id: &str,
    permissions: EnvironmentPermissionProjection,
) -> Vec<String> {
    if harness_id != "claude" {
        return Vec::new();
    }
    let writes = permissions.trusted_write == EnvironmentPermissionPolicy::Allow;
    let terminal = permissions.terminal == EnvironmentPermissionPolicy::Allow;
    match (writes, terminal) {
        // Everything the Environment permits is already granted, so there is
        // nothing left for a person to approve.
        (true, true) => vec!["--permission-mode".into(), "bypassPermissions".into()],
        // Edits proceed; running commands still stops for an answer.
        (true, false) => vec!["--permission-mode".into(), "acceptEdits".into()],
        _ => Vec::new(),
    }
}

/// Claude reads `ANTHROPIC_MODEL` from the inherited shell, so start it with
/// an explicit `--model` that matches the Environment default.
fn harness_start_args(
    harness_id: &str,
    model: &str,
    permissions: EnvironmentPermissionProjection,
) -> Vec<String> {
    let mut args = match harness_id {
        "claude" if !model.is_empty() => vec!["--model".into(), model.into()],
        _ => Vec::new(),
    };
    args.extend(harness_permission_args(harness_id, permissions));
    args
}

fn joined_search_path(search_paths: &[PathBuf]) -> Option<String> {
    std::env::join_paths(search_paths)
        .ok()
        .and_then(|path| path.into_string().ok())
}

/// The Orchestrator's one and only prompt.
///
/// It runs inside a Herdr pane, so it can already drive other agents itself.
/// What it cannot do is apply this Run's Environment to a new pane, which is
/// what the `agent-factory` commands are for. Everything else — deciding,
/// waiting, reading, judging — is its own work, and is deliberately not
/// something Rust infers on its behalf.
fn orchestrator_brief_prompt(run: &FactoryRun) -> String {
    bound_prompt(format!(
        "You are the Orchestrator for this Factory Run. You coordinate a Coding agent and an \
         Evaluation agent. Do not implement the target agent yourself.\n\n\
         Objective:\n{objective}\n\n\
         Acceptance criteria:\n{criteria}\n\n\
         Use the Herdr skill for every agent you request during this Run. Agent Factory \
         authorizes and creates each Environment-bound agent as its own pane beside yours, \
         one tab per iteration; the Herdr skill is how you inspect, prompt, wait for, read, \
         focus, and otherwise manage every Coding, Evaluation, or other returned agent. \
         Never spawn an agent \
         outside the commands below, because it would not be associated with this Run or \
         cancelled with it.\n\n\
         You are running inside a Herdr pane, so the Herdr skill can drive returned agents directly:\n\
         \x20 herdr agent prompt <name> \"<text>\" --wait --timeout 600000\n\
         \x20 herdr agent read <name> --source recent-unwrapped --lines 200\n\n\
         Agent Factory starts agents for you, because only it can apply this Run's \
         Environment. These commands act on this Run; there is no Run to name:\n\
         \x20 agent-factory status\n\
         \x20 agent-factory start coding --brief \"<what to build>\"\n\
         \x20 agent-factory start evaluation\n\
         \x20 agent-factory escalate --question \"<what you need decided>\"\n\
         \x20 agent-factory finish --verdict pass|needs-review --summary \"<why>\"\n\n\
         Each start command prints JSON containing agent.name. Prompt that name with herdr.\n\n\
         Work this loop:\n\
         1. Start Coding with a brief derived from the objective. Prompt the agent it \
         returns, wait for it, then read it.\n\
         2. Judge the work against the acceptance criteria.\n\
         3. Start Evaluation, prompt it, wait, and read its verdict.\n\
         4. Either start Coding again for another iteration, passing what must change, \
         or finish the Run.\n\n\
         Run this loop unattended. Nobody is watching, so do not wait for anyone: judge \
         the work yourself and keep going. If an agent stops for an approval you cannot \
         answer, or a decision is genuinely not yours to make, escalate with the exact \
         question — that reaches a person, who replies here in your pane, and your next \
         command continues the Run.\n\n\
         Finish exactly once. A command that is refused explains itself; correct it and \
         retry rather than abandoning the Run.",
        objective = run.objective,
        criteria = numbered(&run.acceptance_criteria),
    ))
}

/// The evaluator writes its verdict to a file as well as printing it.
///
/// Agents that draw on the terminal's alternate screen lose rows to Herdr's host
/// scrollback, so a long verdict may not survive a transcript read. The file is
/// the reliable channel; the printed copy is the fallback.
fn evaluation_prompt(run: &FactoryRun, verdict_path: &Path) -> String {
    let changes = bounded_evidence(&run.changed_files, 96 * 1024);
    let tests = bounded_evidence(&run.test_evidence, 32 * 1024);
    bound_prompt(format!(
        "Evaluate the Factory Run without modifying the workspace, apart from the verdict file named below. Produce exactly one JSON object with this schema: {{\"schemaVersion\":1,\"verdict\":\"pass|fail|needs_review\",\"summary\":\"nonempty summary\",\"findings\":[{{\"severity\":\"critical|major|minor|note\",\"title\":\"nonempty title\",\"evidence\":\"nonempty evidence\",\"file\":\"optional path\",\"line\":1}}]}}.\n\nWrite that JSON object, and nothing else, to `{}` (create parent directories if needed). Then print the same object as your final message, without Markdown fences or prose.\n\nObjective:\n{}\n\nAcceptance criteria:\n{}\n\nCaptured changed files and diffs:\n{}\n\nCaptured test evidence:\n{}",
        verdict_path.display(),
        run.objective,
        numbered(&run.acceptance_criteria),
        changes,
        tests,
    ))
}

fn numbered(values: &[String]) -> String {
    values
        .iter()
        .enumerate()
        .map(|(index, value)| format!("{}. {value}", index + 1))
        .collect::<Vec<_>>()
        .join("\n")
}

fn bounded_evidence<T: Serialize>(items: &[T], budget: usize) -> String {
    let mut output = String::new();
    for (index, item) in items.iter().enumerate() {
        let encoded = serde_json::to_string(item)
            .unwrap_or_else(|error| format!("{{\"encodingError\":{}}}", json!(error.to_string())));
        let line = format!("{}. {encoded}\n", index + 1);
        if output.len().saturating_add(line.len()) > budget {
            output.push_str(&format!(
                "[TRUNCATED: omitted {} of {} evidence items due to the {budget}-byte section budget]\n",
                items.len() - index,
                items.len(),
            ));
            break;
        }
        output.push_str(&line);
    }
    if items.is_empty() {
        output.push_str("[No evidence captured]\n");
    }
    output
}

fn bound_prompt(mut prompt: String) -> String {
    if prompt.len() <= MAX_FACTORY_PROMPT_BYTES {
        return prompt;
    }
    let omitted = prompt.len() - MAX_FACTORY_PROMPT_BYTES;
    let marker = format!(
        "\n\n[TRUNCATED: at least {omitted} bytes omitted by the {MAX_FACTORY_PROMPT_BYTES}-byte Factory prompt budget]"
    );
    let mut end = MAX_FACTORY_PROMPT_BYTES.saturating_sub(marker.len());
    while !prompt.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    prompt.truncate(end);
    prompt.push_str(&marker);
    prompt
}

fn decode_params<T: for<'de> Deserialize<'de>>(params: Value) -> Result<T, DispatchError> {
    serde_json::from_value(params).map_err(|error| DispatchError::InvalidParams(error.to_string()))
}

struct DispatchResult {
    result: Value,
    event: Option<(String, u64, Value)>,
}

impl DispatchResult {
    fn response(result: Value) -> Self {
        Self {
            result,
            event: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum DispatchError {
    #[error("invalid parameters: {0}")]
    InvalidParams(String),
    #[error("{0}")]
    Unauthorized(String),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    File(#[from] FileError),
    #[error(transparent)]
    Terminal(#[from] TerminalError),
    #[error("Herdr could not complete the request: {0}")]
    Herdr(String),
    #[error("session {0} is not ready for a prompt")]
    SessionBusy(Uuid),
    #[error(transparent)]
    FactoryRun(#[from] FactoryRunError),
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    RepositoryConfig(#[from] RepositoryConfigError),
    #[error(transparent)]
    Environment(#[from] EnvironmentError),
    #[error(transparent)]
    Plugin(#[from] PluginError),
    #[error(transparent)]
    Secret(#[from] SecretError),
    #[error(transparent)]
    Update(#[from] UpdateError),
    #[error("updates are disabled: {0}")]
    UpdatesDisabled(String),
    #[error("update helper failed: {0}")]
    UpdateHelper(String),
    #[error("default skill `{0}` exceeds the 64 KiB limit")]
    DefaultSkillTooLarge(String),
    #[error("default skill prefix exceeds the 256 KiB aggregate limit")]
    DefaultSkillPrefixTooLarge,
    #[error("failed to read default skill content: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to encode runtime result: {0}")]
    Serialize(#[from] serde_json::Error),
}

impl DispatchError {
    /// A short, stable code an Orchestrator can branch on. The message explains;
    /// this is what a script matches.
    fn control_code(&self) -> &'static str {
        match self {
            Self::Unauthorized(_) => "unauthorized",
            Self::FactoryRun(_) => "illegal_transition",
            Self::Herdr(_) => "herdr_unavailable",
            Self::InvalidParams(_) | Self::Store(StoreError::InvalidInput(_)) => "invalid_request",
            _ => "runtime_error",
        }
    }

    fn code(&self) -> ErrorCode {
        match self {
            Self::InvalidParams(_)
            | Self::Unauthorized(_)
            | Self::Store(StoreError::InvalidInput(_) | StoreError::InvalidLayout) => {
                ErrorCode::InvalidParams
            }
            Self::Store(StoreError::Conflict(_)) => ErrorCode::Conflict,
            Self::Store(StoreError::NotFound(_)) => ErrorCode::InvalidParams,
            Self::Secret(
                SecretError::InvalidReference
                | SecretError::InvalidLabel
                | SecretError::InvalidValue
                | SecretError::NotFound,
            ) => ErrorCode::InvalidParams,
            Self::Plugin(
                PluginError::HttpsRequired
                | PluginError::NotInstalled(_)
                | PluginError::InvalidEnvironmentSelection(_)
                | PluginError::InvalidRegistrySource(_),
            ) => ErrorCode::InvalidParams,
            Self::UpdatesDisabled(_) => ErrorCode::InvalidParams,
            Self::Update(UpdateError::InvalidTransition) => ErrorCode::Conflict,
            Self::File(
                FileError::InvalidRoot(_)
                | FileError::PathMustBeAbsolute
                | FileError::OutsideTrustedRoot(_)
                | FileError::NotDirectory(_)
                | FileError::NotFile(_)
                | FileError::InvalidCursor(_)
                | FileError::InvalidPageSize(_)
                | FileError::InvalidReadLimit(_)
                | FileError::InvalidWriteLimit(_)
                | FileError::InvalidWritePath(_)
                | FileError::SymlinkWriteRejected(_)
                | FileError::InvalidContextLines(_)
                | FileError::BinaryDiffUnsupported
                | FileError::DiffTooLarge,
            ) => ErrorCode::InvalidParams,
            Self::Terminal(
                TerminalError::InvalidExecutable(_)
                | TerminalError::InvalidWorkingDirectory(_)
                | TerminalError::InvalidSize { .. }
                | TerminalError::InvalidReadLimit(_)
                | TerminalError::Unknown(_)
                | TerminalError::Exited(_),
            ) => ErrorCode::InvalidParams,
            Self::Terminal(TerminalError::CapacityReached) => ErrorCode::Conflict,
            Self::SessionBusy(_) => ErrorCode::Conflict,
            Self::Herdr(_) => ErrorCode::Conflict,
            Self::Git(GitError::DirtyWorktree(_) | GitError::ReferenceCollision(_)) => {
                ErrorCode::Conflict
            }
            Self::Git(
                GitError::InvalidPath(_)
                | GitError::InvalidVersionPath(_)
                | GitError::UnsupportedRepository(_)
                | GitError::MissingIdentity
                | GitError::MissingGit,
            ) => ErrorCode::InvalidParams,
            Self::RepositoryConfig(_) => ErrorCode::InvalidParams,
            Self::FactoryRun(_) => ErrorCode::Conflict,
            Self::Environment(EnvironmentError::NotFound(_)) => ErrorCode::Conflict,
            Self::DefaultSkillTooLarge(_) | Self::DefaultSkillPrefixTooLarge => ErrorCode::Conflict,
            Self::Store(_)
            | Self::File(FileError::Io(_))
            | Self::Terminal(_)
            | Self::Git(_)
            | Self::Environment(_)
            | Self::Plugin(_)
            | Self::Secret(_)
            | Self::Update(_)
            | Self::UpdateHelper(_)
            | Self::Io(_)
            | Self::Serialize(_) => ErrorCode::Internal,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeInitError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Serialize(#[from] serde_json::Error),
    #[error(transparent)]
    Environment(#[from] EnvironmentError),
    #[error(transparent)]
    Plugin(#[from] PluginError),
    #[error(transparent)]
    Secret(#[from] SecretError),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimePaths {
    pub database: PathBuf,
}

/// Where durable state lives. `AGENT_FACTORY_DATA_DIR` overrides it, which is
/// how a test or a seeding run stays away from the developer's own data.
pub fn application_data_directory() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(directory) = std::env::var_os("AGENT_FACTORY_DATA_DIR") {
        return Ok(PathBuf::from(directory));
    }
    let directories = directories::ProjectDirs::from("dev", "Native", "Agent Factory")
        .ok_or("unable to resolve the platform application-data directory")?;
    Ok(directories.data_local_dir().to_path_buf())
}

/// The socket an Orchestrator calls to drive its own Factory Run.
///
/// Bound by the process, drained by the dispatch loop. Held separately from the
/// [`Runtime`] so accepting connections never borrows the state every command
/// has to mutate.
pub struct AgentControlService {
    listener: control_socket::ControlListener,
}

impl AgentControlService {
    pub fn bind(data_directory: &Path) -> std::io::Result<Self> {
        Ok(Self {
            listener: control_socket::ControlListener::bind(&agent_control::socket_path(
                data_directory,
            ))?,
        })
    }

    pub fn endpoint(&self) -> &Path {
        self.listener.path()
    }
}

impl Runtime {
    /// Answer every Orchestrator command that arrived since the last tick.
    ///
    /// Returns the projection events those commands produced, so the UI sees a
    /// Run advance whether a person or its Orchestrator moved it.
    pub fn drain_agent_control(&mut self, service: &AgentControlService) -> Vec<Frame> {
        let mut frames = Vec::new();
        for call in service.listener.drain() {
            let (response, produced) = self.handle_control(call.request.clone());
            frames.extend(produced);
            call.answer(response);
        }
        frames
    }
}

#[cfg(test)]
mod factory_loop_tests;

#[cfg(test)]
mod tests {
    use std::fs;
    use std::ops::{Deref, DerefMut};
    use std::process::Command;

    use ipc_contract::ResponseOutcome;
    use tempfile::TempDir;

    use super::*;

    /// A Draft's Environment is a durable choice, so a name that resolves to
    /// nothing must be refused rather than stored and discovered at launch.
    #[test]
    fn a_draft_cannot_be_pointed_at_an_environment_that_does_not_exist() {
        let user = TempDir::new().unwrap();
        let plugins = TempDir::new().unwrap();
        let mut runtime = Runtime::with_environment_services(
            ProjectStore::open_in_memory().unwrap(),
            vec![],
            EnvironmentServicePaths {
                user_environments: user.path().to_path_buf(),
                plugins: plugins.path().to_path_buf(),
            },
            Arc::new(InMemorySecretStore::default()),
        )
        .unwrap();

        let refused = runtime.handle_request(Request::new(
            "agentDraft.environment.set",
            json!({
                "agentDraftId": Uuid::new_v4(),
                "environmentId": "no-such-environment",
            }),
        ));
        match refused.first() {
            Some(Frame::Response(Response {
                outcome: ResponseOutcome::Error { error },
                ..
            })) => assert!(
                error.message.contains("no-such-environment"),
                "unexpected refusal: {}",
                error.message
            ),
            other => panic!("an unknown Environment was not refused: {other:?}"),
        }
    }

    /// `git branch` is read by people, so a Draft's branch says which Agent and
    /// which Draft rather than repeating two opaque identifiers.
    #[test]
    fn a_draft_branch_reads_as_names_not_identifiers() {
        let agent = Uuid::parse_str("09b21b83-01ea-4bfb-843c-af6ac8a8d032").unwrap();
        let draft = Uuid::parse_str("1c75866f-9e9d-43d7-b4fd-824cbb139e56").unwrap();
        let branch = draft_branch(agent, "IPL Analyst", draft, "First Draft");
        assert_eq!(
            branch,
            "agent-factory/ipl-analyst-ezc/drafts/first-draft-nht"
        );
        assert!(!branch.contains(&agent.to_string()));
        assert!(!branch.contains(&draft.to_string()));
    }

    #[test]
    fn a_version_tag_matches_the_branch_it_was_cut_from() {
        let agent = Uuid::parse_str("09b21b83-01ea-4bfb-843c-af6ac8a8d032").unwrap();
        assert_eq!(
            version_tag(agent, "IPL Analyst", "0.2.0"),
            "agent-factory/ipl-analyst-ezc/v0.2.0"
        );
    }

    /// A Draft named after its Agent would otherwise say it twice, and the
    /// repository's own name adds nothing to a directory already inside it.
    #[test]
    fn a_worktree_directory_says_each_name_once() {
        let draft = Uuid::parse_str("1c75866f-9e9d-43d7-b4fd-824cbb139e56").unwrap();
        assert_eq!(
            worktree_directory_name("IPL Analyst", "IPL Analyst (DRAFT)", draft),
            "ipl-analyst-draft-nht"
        );
        assert_eq!(
            worktree_directory_name("IPL Analyst", "Experiment", draft),
            "ipl-analyst-experiment-nht"
        );
    }

    #[test]
    fn a_short_id_is_stable_for_a_record_and_easy_to_read_back() {
        let id = Uuid::new_v4();
        assert_eq!(short_id(id), short_id(id));
        assert_eq!(short_id(id).len(), SHORT_ID_LENGTH);
        assert!(
            short_id(id)
                .bytes()
                .all(|byte| SHORT_ID_ALPHABET.contains(&byte)),
            "{}",
            short_id(id)
        );
        // Glyphs that read as one another in a terminal are not in play.
        assert!(!SHORT_ID_ALPHABET.iter().any(|byte| b"01lio".contains(byte)));
    }

    /// The tab strip has room for a few words, not a Run's whole objective.
    #[test]
    fn an_iteration_tab_is_named_for_its_agent_and_iteration() {
        assert_eq!(
            iteration_tab_label("IPL Analyst", 2),
            "IPL Analyst · iteration 2"
        );
    }

    struct RuntimeWithHerdr {
        runtime: Runtime,
        _herdr: super::factory_loop_tests::ScriptedHerdr,
    }

    impl Deref for RuntimeWithHerdr {
        type Target = Runtime;

        fn deref(&self) -> &Self::Target {
            &self.runtime
        }
    }

    impl DerefMut for RuntimeWithHerdr {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.runtime
        }
    }

    fn runtime_with_herdr() -> RuntimeWithHerdr {
        let herdr = super::factory_loop_tests::ScriptedHerdr::basic();
        let runtime =
            Runtime::connected_to_herdr(ProjectStore::open_in_memory().unwrap(), herdr.socket());
        RuntimeWithHerdr {
            runtime,
            _herdr: herdr,
        }
    }

    fn response_result(frames: &[Frame]) -> &Value {
        match &frames[0] {
            Frame::Response(Response {
                outcome: ResponseOutcome::Success { result },
                ..
            }) => result,
            frame => panic!("expected successful response, got {frame:?}"),
        }
    }

    fn initialize_repository(root: &Path) {
        let run = |args: &[&str]| {
            let status = Command::new("git")
                .args(args)
                .current_dir(root)
                .status()
                .unwrap();
            assert!(status.success(), "git {args:?} failed");
        };
        run(&["init"]);
        run(&["config", "user.name", "Agent Factory Tests"]);
        run(&["config", "user.email", "tests@example.invalid"]);
        if !root.join("README.md").exists() && !root.join(".agent-factory").exists() {
            fs::write(root.join("README.md"), "test repository\n").unwrap();
        }
        run(&["add", "--all"]);
        run(&["commit", "-m", "initial"]);
    }

    fn commit_repository_changes(root: &Path, message: &str) {
        let status = Command::new("git")
            .args(["add", "--all"])
            .current_dir(root)
            .status()
            .unwrap();
        assert!(status.success(), "git add failed");
        let status = Command::new("git")
            .args(["commit", "-m", message])
            .current_dir(root)
            .status()
            .unwrap();
        assert!(status.success(), "git commit failed");
    }

    fn test_repository() -> (TempDir, PathBuf) {
        let container = TempDir::new().unwrap();
        let root = container.path().join("repository");
        fs::create_dir(&root).unwrap();
        initialize_repository(&root);
        (container, root)
    }

    fn create_test_provider(runtime: &mut Runtime, credential_ref: Option<&str>) -> String {
        let ordinal = runtime.store.list_llm_providers().unwrap().len() + 1;
        let created = runtime.handle_request(Request::new(
            "llmProvider.create",
            json!({
                "configuration": {
                    "name": format!("Test Provider {ordinal}"),
                    "type": "ollama",
                    "endpoint": "http://127.0.0.1:11434",
                    "credentialRef": credential_ref,
                    "allowedModels": ["glm-5.2:cloud"]
                }
            }),
        ));
        response_result(&created)["providerId"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    #[test]
    fn hello_reports_the_contract_version() {
        let mut runtime = Runtime::new(ProjectStore::open_in_memory().unwrap(), vec![]);
        let result = runtime.handle_request(Request::new("runtime.hello", json!({})));

        assert_eq!(response_result(&result)["protocolVersion"], 1);
        assert_eq!(response_result(&result)["runtimeName"], RUNTIME_NAME);
    }

    #[test]
    fn project_creation_returns_a_response_then_a_revisioned_event() {
        let root = TempDir::new().unwrap();
        let mut runtime = Runtime::new(ProjectStore::open_in_memory().unwrap(), vec![]);
        let frames = runtime.handle_request(Request::new(
            "project.create",
            json!({
                "name": "Agent",
                "root": root.path(),
                "trusted": true,
            }),
        ));

        assert_eq!(frames.len(), 2);
        assert_eq!(response_result(&frames)["project"]["name"], "Agent");
        assert!(matches!(
            &frames[1],
            Frame::Event(Event {
                sequence: 1,
                revision: 1,
                topic,
                ..
            }) if topic == "project.changed"
        ));
    }

    #[test]
    fn new_target_agent_projects_are_trusted_by_default() {
        let (_container, root) = test_repository();
        let mut runtime = runtime_with_herdr();
        let created = runtime.handle_request(Request::new(
            "targetAgent.create",
            json!({
                "name":"Commerce Copilot",
                "objective":"Resolve commerce support requests",
                "acceptanceCriteria":["Refund requests are classified correctly"],
                "repositoryRoot":root,
                "draftName":"main",
            }),
        ));
        assert!(response_result(&created)["workspaceBinding"]["projectId"].is_string());
        let worktree = PathBuf::from(
            response_result(&created)["draft"]["worktreePath"]
                .as_str()
                .unwrap(),
        );
        assert!(
            worktree.starts_with(
                fs::canonicalize(&root)
                    .unwrap()
                    .join(".agent-factory/worktrees")
            )
        );
        assert!(worktree.join(".agent-factory/target-agent.json").is_file());
        let snapshot = runtime.handle_request(Request::new("snapshot.get", json!({})));
        assert_eq!(response_result(&snapshot)["projects"][0]["trusted"], true);
    }

    #[test]
    fn target_agent_creation_accepts_herdr_workspace_integration() {
        let (_container, root) = test_repository();
        fs::write(root.join(".DS_Store"), "Finder metadata").unwrap();
        fs::create_dir_all(root.join(".agents/skills/herdr")).unwrap();
        fs::write(
            root.join(".agents/skills/herdr/SKILL.md"),
            "Herdr integration",
        )
        .unwrap();
        fs::create_dir_all(root.join(".claude/skills/herdr")).unwrap();
        fs::write(root.join(".claude/.DS_Store"), "Finder metadata").unwrap();
        fs::write(
            root.join(".claude/skills/herdr/SKILL.md"),
            "Herdr integration",
        )
        .unwrap();

        let herdr = super::factory_loop_tests::ScriptedHerdr::basic();
        herdr.set_install_worktree_integration(true);
        let mut runtime =
            Runtime::connected_to_herdr(ProjectStore::open_in_memory().unwrap(), herdr.socket());
        let created = runtime.handle_request(Request::new(
            "targetAgent.create",
            json!({
                "name": "Runtime Integration Agent",
                "objective": "Accept Herdr-owned workspace integration",
                "acceptanceCriteria": ["The Draft is created without masking authored data"],
                "repositoryRoot": root,
                "draftName": "main",
                "trusted": true,
            }),
        ));
        let draft_id = response_result(&created)["draft"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let worktree_path = PathBuf::from(
            response_result(&created)["draft"]["worktreePath"]
                .as_str()
                .unwrap(),
        );
        assert!(
            worktree_path
                .join(".agents/skills/herdr/SKILL.md")
                .is_file()
        );

        let discarded = runtime.handle_request(Request::new(
            "agentDraft.discard",
            json!({"agentDraftId": draft_id}),
        ));
        let _ = response_result(&discarded);
        assert!(!worktree_path.exists());
    }

    #[test]
    fn a_missing_draft_checkout_is_actionable_and_can_be_discarded() {
        let (_container, root) = test_repository();
        let mut runtime = runtime_with_herdr();
        let created = runtime.handle_request(Request::new(
            "targetAgent.create",
            json!({
                "name": "Missing Checkout Agent",
                "objective": "Recover from a deleted checkout",
                "acceptanceCriteria": ["The stale Draft can be discarded"],
                "repositoryRoot": root,
                "draftName": "main",
                "trusted": true,
            }),
        ));
        let draft_id = response_result(&created)["draft"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let draft_uuid = Uuid::parse_str(&draft_id).unwrap();
        let worktree_path = PathBuf::from(
            response_result(&created)["draft"]["worktreePath"]
                .as_str()
                .unwrap(),
        );
        fs::remove_dir_all(&worktree_path).unwrap();

        let rejected = runtime.handle_request(Request::new(
            "agentDraft.openWorkspace",
            json!({"agentDraftId": draft_id}),
        ));
        assert!(matches!(
            &rejected[0],
            Frame::Response(Response {
                outcome: ResponseOutcome::Error { error },
                ..
            }) if error.code == ErrorCode::InvalidParams
                && error.message.contains("Draft checkout no longer exists")
                && error.message.contains("Discard this Draft")
        ));

        let discarded = runtime.handle_request(Request::new(
            "agentDraft.discard",
            json!({"agentDraftId": draft_id}),
        ));
        let _ = response_result(&discarded);
        assert_eq!(
            runtime.store.agent_draft(draft_uuid).unwrap().lifecycle,
            AgentDraftLifecycle::Archived
        );
    }

    #[test]
    fn repository_config_applies_to_new_drafts_without_relocating_existing_paths() {
        let container = TempDir::new().unwrap();
        let root = container.path().join("repository");
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join(".agent-factory")).unwrap();
        fs::write(
            root.join(".agent-factory/config.json"),
            serde_json::to_vec_pretty(&json!({
                "schemaVersion": 1,
                "worktreesDirectory": "generated/first"
            }))
            .unwrap(),
        )
        .unwrap();
        initialize_repository(&root);

        let mut runtime = runtime_with_herdr();
        let created = runtime.handle_request(Request::new(
            "targetAgent.create",
            json!({
                "name": "Configured Agent",
                "objective": "Use repository-local worktrees",
                "acceptanceCriteria": ["Both creation paths use the current configuration"],
                "repositoryRoot": root,
                "draftName": "initial",
                "trusted": true
            }),
        ));
        let agent_id = response_result(&created)["targetAgent"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let initial_path = PathBuf::from(
            response_result(&created)["draft"]["worktreePath"]
                .as_str()
                .unwrap(),
        );
        let canonical_root = fs::canonicalize(&root).unwrap();
        assert!(initial_path.starts_with(canonical_root.join(".agent-factory/generated/first")));
        assert!(
            initial_path
                .join(".agent-factory/target-agent.json")
                .is_file()
        );

        fs::write(
            root.join(".agent-factory/config.json"),
            serde_json::to_vec_pretty(&json!({
                "schemaVersion": 1,
                "worktreesDirectory": "generated/second"
            }))
            .unwrap(),
        )
        .unwrap();
        commit_repository_changes(&root, "change Agent Factory worktree directory");

        let snapshot = runtime.handle_request(Request::new("snapshot.get", json!({})));
        assert_eq!(
            response_result(&snapshot)["targetWorkspace"]["targetGroups"][0]["drafts"][0]["worktreePath"],
            initial_path.to_string_lossy().as_ref()
        );

        let created = runtime.handle_request(Request::new(
            "agentDraft.create",
            json!({
                "targetAgentId": agent_id,
                "draftName": "next"
            }),
        ));
        let next_path = PathBuf::from(
            response_result(&created)["draft"]["worktreePath"]
                .as_str()
                .unwrap(),
        );
        assert!(next_path.starts_with(canonical_root.join(".agent-factory/generated/second")));
        assert_ne!(next_path, initial_path);
        assert!(next_path.join(".agent-factory/target-agent.json").is_file());
        assert!(initial_path.is_dir());
    }

    #[test]
    fn invalid_repository_config_returns_an_actionable_error() {
        let container = TempDir::new().unwrap();
        let root = container.path().join("repository");
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join(".agent-factory")).unwrap();
        fs::write(
            root.join(".agent-factory/config.json"),
            br#"{"schemaVersion":1,"worktreesDirectory":"../outside"}"#,
        )
        .unwrap();
        initialize_repository(&root);

        let mut runtime = Runtime::new(ProjectStore::open_in_memory().unwrap(), vec![]);
        let rejected = runtime.handle_request(Request::new(
            "targetAgent.create",
            json!({
                "name": "Invalid Config Agent",
                "objective": "Reject an unsafe worktree directory",
                "acceptanceCriteria": ["The request explains the invalid setting"],
                "repositoryRoot": root,
                "draftName": "initial",
                "trusted": true
            }),
        ));

        assert!(matches!(
            &rejected[0],
            Frame::Response(Response {
                outcome: ResponseOutcome::Error { error },
                ..
            }) if error.code == ErrorCode::InvalidParams
                && error.message.contains("worktreesDirectory")
        ));
    }

    #[test]
    fn target_workspace_intents_keep_target_binding_and_pane_identity_together() {
        let (_container, root) = test_repository();
        let mut runtime = runtime_with_herdr();
        let created = runtime.handle_request(Request::new(
            "targetAgent.create",
            json!({
                "name": "Commerce Copilot",
                "objective": "Resolve commerce support requests",
                "acceptanceCriteria": ["Refund requests are classified correctly"],
                "repositoryRoot": root,
                "draftName": "main",
                "trusted": true,
            }),
        ));
        let target_id = response_result(&created)["targetAgent"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let snapshot = runtime.handle_request(Request::new("snapshot.get", json!({})));
        let workspace = &response_result(&snapshot)["targetWorkspace"];
        let draft_id = response_result(&created)["draft"]["id"].as_str().unwrap();
        assert_eq!(workspace["targetGroups"][0]["workItems"][0]["id"], draft_id);
        assert_eq!(
            workspace["targetGroups"][0]["workItems"][0]["targetAgentId"],
            target_id
        );
        assert_eq!(workspace["panes"].as_array().unwrap().len(), 1);
        assert_eq!(workspace["focusedPaneId"], workspace["panes"][0]["id"]);

        let pane_id = workspace["panes"][0]["id"].as_str().unwrap();
        let closed = runtime.handle_request(Request::new(
            "workspacePane.close",
            json!({"workspacePaneId": pane_id}),
        ));
        assert!(response_result(&closed)["revision"].as_u64().is_some());
        let after = runtime.handle_request(Request::new("snapshot.get", json!({})));
        assert!(
            response_result(&after)["targetWorkspace"]["panes"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            response_result(&after)["targetWorkspace"]["targetGroups"][0]["workItems"][0]["id"],
            draft_id
        );
    }

    #[test]
    fn target_agent_create_resets_an_obsolete_portable_manifest() {
        let container = TempDir::new().unwrap();
        let root = container.path().join("repository");
        fs::create_dir(&root).unwrap();
        let manifest_directory = root.join(".agent-factory");
        fs::create_dir(&manifest_directory).unwrap();
        let target_agent_id = uuid::Uuid::new_v4();
        fs::write(
            manifest_directory.join("target-agent.json"),
            serde_json::to_vec_pretty(&json!({
                "schemaVersion": 3,
                "targetAgentId": target_agent_id,
                "name": "Custumer Support",
                "currentVersion": "0.1.0",
                "objective": "Resolve customer support requests",
                "acceptanceCriteria": ["Answers cite the relevant support policy"]
            }))
            .unwrap(),
        )
        .unwrap();
        initialize_repository(&root);
        let mut runtime = runtime_with_herdr();

        let created = runtime.handle_request(Request::new(
            "targetAgent.create",
            json!({
                "name": "Customer Support",
                "objective": "Replace the obsolete definition",
                "acceptanceCriteria": ["The Draft uses manifest schema v4"],
                "repositoryRoot": root,
                "draftName": "main",
                "trusted": true,
            }),
        ));

        assert_ne!(
            response_result(&created)["targetAgent"]["id"],
            target_agent_id.to_string()
        );
        assert_eq!(
            response_result(&created)["draft"]["objective"],
            "Replace the obsolete definition"
        );
        let manifest: TargetAgentManifest = serde_json::from_slice(
            &fs::read(
                response_result(&created)["draft"]["worktreePath"]
                    .as_str()
                    .unwrap()
                    .to_owned()
                    + "/.agent-factory/target-agent.json",
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(manifest.schema_version, 4);
    }

    #[test]
    fn remove_hides_the_agent_and_leaves_repository_files() {
        let (_container, root) = test_repository();
        let readme = PathBuf::from(&root).join("README.md");
        assert!(readme.is_file());
        let mut runtime = runtime_with_herdr();
        let created = runtime.handle_request(Request::new(
            "targetAgent.create",
            json!({
                "name": "Removable Agent",
                "objective": "Stay on disk after Remove",
                "acceptanceCriteria": ["Factory records leave"],
                "repositoryRoot": root,
                "draftName": "initial",
                "trusted": true
            }),
        ));
        let agent_id = response_result(&created)["targetAgent"]["id"]
            .as_str()
            .unwrap();
        let worktree = PathBuf::from(
            response_result(&created)["draft"]["worktreePath"]
                .as_str()
                .unwrap(),
        );
        let removed = runtime.handle_request(Request::new(
            "targetAgent.remove",
            json!({ "targetAgentId": agent_id }),
        ));
        assert!(response_result(&removed)["revision"].is_number());
        let snapshot = runtime.handle_request(Request::new("snapshot.get", json!({})));
        assert!(
            response_result(&snapshot)["targetWorkspace"]["targetGroups"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(readme.is_file());
        assert!(PathBuf::from(&root).is_dir());
        assert!(worktree.is_dir());
    }

    #[test]
    fn creates_a_draft_from_repository_head_without_a_version() {
        let (_container, root) = test_repository();
        let mut runtime = runtime_with_herdr();
        let created = runtime.handle_request(Request::new(
            "targetAgent.create",
            json!({
                "name": "Head Draft Agent",
                "objective": "Create again from HEAD",
                "acceptanceCriteria": ["No Version is required"],
                "repositoryRoot": root,
                "draftName": "initial",
                "trusted": true
            }),
        ));
        let agent_id = response_result(&created)["targetAgent"]["id"]
            .as_str()
            .unwrap();
        let draft_id = response_result(&created)["draft"]["id"].as_str().unwrap();
        runtime.handle_request(Request::new(
            "agentDraft.discard",
            json!({ "agentDraftId": draft_id }),
        ));
        let created = runtime.handle_request(Request::new(
            "agentDraft.create",
            json!({
                "targetAgentId": agent_id,
                "draftName": "fresh"
            }),
        ));
        let draft = &response_result(&created)["draft"];
        assert_eq!(draft["baseVersion"], Value::Null);
        assert_eq!(draft["objective"], "Create again from HEAD");
        assert!(Path::new(draft["worktreePath"].as_str().unwrap()).is_dir());
    }

    #[test]
    fn publishes_a_draft_then_creates_parallel_drafts_from_the_version() {
        let (_container, root) = test_repository();
        let mut runtime = runtime_with_herdr();
        let created = runtime.handle_request(Request::new(
            "targetAgent.create",
            json!({
                "name": "Release Agent",
                "objective": "Produce a release-ready result",
                "acceptanceCriteria": ["The release checks pass"],
                "repositoryRoot": root,
                "draftName": "initial",
                "trusted": true
            }),
        ));
        let agent_id = response_result(&created)["targetAgent"]["id"]
            .as_str()
            .unwrap();
        let draft_id = response_result(&created)["draft"]["id"].as_str().unwrap();
        let published = runtime.handle_request(Request::new(
            "agentDraft.publish",
            json!({
                "agentDraftId": draft_id,
                "bump": "patch",
                "confirmWithoutPassingRun": true
            }),
        ));
        assert_eq!(response_result(&published)["version"]["version"], "0.1.0");
        assert_eq!(response_result(&published)["cleanupRequired"], false);
        let version_id = response_result(&published)["version"]["id"]
            .as_str()
            .unwrap();

        for name in ["maintenance", "experiment"] {
            let created = runtime.handle_request(Request::new(
                "agentDraft.create",
                json!({
                    "targetAgentId": agent_id,
                    "baseVersionId": version_id,
                    "draftName": name
                }),
            ));
            assert_eq!(response_result(&created)["draft"]["baseVersion"], "0.1.0");
            assert!(
                Path::new(
                    response_result(&created)["draft"]["worktreePath"]
                        .as_str()
                        .unwrap()
                )
                .is_dir()
            );
        }
        let snapshot = runtime.handle_request(Request::new("snapshot.get", json!({})));
        let drafts = response_result(&snapshot)["targetWorkspace"]["targetGroups"][0]["drafts"]
            .as_array()
            .unwrap();
        assert_eq!(
            drafts
                .iter()
                .filter(|draft| draft["lifecycle"] == "active")
                .count(),
            2
        );
    }

    #[test]
    fn version_files_are_commit_bound_and_fail_safely() {
        let (_container, root) = test_repository();
        let mut runtime = runtime_with_herdr();
        let created = runtime.handle_request(Request::new(
            "targetAgent.create",
            json!({
                "name": "Inspector Agent",
                "objective": "Inspect immutable source",
                "acceptanceCriteria": ["Only Version files are shown"],
                "repositoryRoot": root,
                "draftName": "initial",
                "trusted": true
            }),
        ));
        let draft_id = response_result(&created)["draft"]["id"].as_str().unwrap();
        let worktree = PathBuf::from(
            response_result(&created)["draft"]["worktreePath"]
                .as_str()
                .unwrap(),
        );
        fs::create_dir_all(worktree.join("src")).unwrap();
        fs::write(worktree.join("src/selected.txt"), "Version content\n").unwrap();
        fs::write(worktree.join("src/binary.bin"), [0, 159, 146, 150]).unwrap();
        fs::write(
            worktree.join("src/oversized.txt"),
            vec![b'x'; git_runtime::MAX_VERSION_FILE_BYTES + 1],
        )
        .unwrap();

        let published = runtime.handle_request(Request::new(
            "agentDraft.publish",
            json!({
                "agentDraftId": draft_id,
                "bump": "patch",
                "confirmWithoutPassingRun": true
            }),
        ));
        let version_id = response_result(&published)["version"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let git_commit = response_result(&published)["version"]["gitCommit"]
            .as_str()
            .unwrap()
            .to_owned();

        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/selected.txt"), "Current working tree\n").unwrap();
        let listed = runtime.handle_request(Request::new(
            "version.files.list",
            json!({"versionId": version_id}),
        ));
        assert_eq!(response_result(&listed)["gitCommit"], git_commit);
        let paths = response_result(&listed)["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["path"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert!(paths.contains(&"src/selected.txt"));
        assert!(paths.contains(&"src/binary.bin"));
        assert!(paths.contains(&"src/oversized.txt"));

        let selected = runtime.handle_request(Request::new(
            "version.file.read",
            json!({"versionId": version_id, "path": "src/selected.txt"}),
        ));
        assert_eq!(response_result(&selected)["gitCommit"], git_commit);
        assert_eq!(response_result(&selected)["kind"], "text");
        assert_eq!(response_result(&selected)["content"], "Version content\n");

        let binary = runtime.handle_request(Request::new(
            "version.file.read",
            json!({"versionId": version_id, "path": "src/binary.bin"}),
        ));
        assert_eq!(response_result(&binary)["kind"], "binary");
        assert_eq!(response_result(&binary)["content"], Value::Null);

        let oversized = runtime.handle_request(Request::new(
            "version.file.read",
            json!({"versionId": version_id, "path": "src/oversized.txt"}),
        ));
        assert_eq!(response_result(&oversized)["kind"], "too_large");
        assert_eq!(response_result(&oversized)["content"], Value::Null);

        for params in [
            json!({"versionId": version_id, "path": "../README.md"}),
            json!({
                "versionId": version_id,
                "path": "src/selected.txt",
                "repositoryRoot": root,
                "gitRef": "HEAD"
            }),
        ] {
            let rejected = runtime.handle_request(Request::new("version.file.read", params));
            assert!(matches!(
                &rejected[0],
                Frame::Response(Response {
                    outcome: ResponseOutcome::Error { error },
                    ..
                }) if error.code == ErrorCode::InvalidParams
            ));
        }

        let object = root
            .join(".git/objects")
            .join(&git_commit[..2])
            .join(&git_commit[2..]);
        fs::remove_file(object).unwrap();
        let missing = runtime.handle_request(Request::new(
            "version.files.list",
            json!({"versionId": version_id}),
        ));
        assert!(matches!(
            &missing[0],
            Frame::Response(Response {
                outcome: ResponseOutcome::Error { error },
                ..
            }) if error.code == ErrorCode::Internal
        ));
    }

    #[test]
    fn semantic_version_selection_preserves_maintenance_lines() {
        let agent_id = Uuid::new_v4();
        let draft_id = Uuid::new_v4();
        let versions = ["0.1.0", "0.1.1", "0.2.0", "1.0.0"]
            .into_iter()
            .map(|version| app_core::TargetAgentVersionProjection {
                id: Uuid::new_v4(),
                target_agent_id: agent_id,
                version: version.into(),
                name: "Agent".into(),
                objective: "Objective".into(),
                acceptance_criteria: vec!["Criterion".into()],
                source_draft_id: draft_id,
                git_commit: "0123456789abcdef".into(),
                git_tag: format!("agent-factory/{agent_id}/v{version}"),
                created_at_unix_ms: 1,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            next_version(Some("0.1.0"), &versions, VersionBump::Patch).unwrap(),
            "0.1.2",
        );
        assert_eq!(
            next_version(Some("0.1.0"), &versions, VersionBump::Minor).unwrap(),
            "0.3.0",
        );
        assert_eq!(
            next_version(Some("0.2.0"), &versions, VersionBump::Major).unwrap(),
            "2.0.0",
        );
    }

    #[test]
    fn invalid_project_params_are_structured_errors() {
        let mut runtime = Runtime::new(ProjectStore::open_in_memory().unwrap(), vec![]);
        let frames = runtime.handle_request(Request::new(
            "project.create",
            json!({ "name": "missing root" }),
        ));

        assert!(matches!(
            &frames[0],
            Frame::Response(Response {
                outcome: ResponseOutcome::Error { error },
                ..
            }) if error.code == ErrorCode::InvalidParams
        ));
    }

    #[test]
    fn snapshot_is_complete_and_revisioned() {
        let mut runtime = Runtime::new(ProjectStore::open_in_memory().unwrap(), vec![]);
        let frames = runtime.handle_request(Request::new("snapshot.get", json!({})));
        let snapshot = response_result(&frames);

        assert_eq!(snapshot["revision"], 0);
        assert_eq!(snapshot["projects"], json!([]));
        assert_eq!(snapshot["agentSessions"], json!([]));
        assert_eq!(snapshot["liveAgents"], json!([]));
        assert_eq!(snapshot["harnesses"], json!([]));
        assert_eq!(snapshot["factoryRuns"], json!([]));
        assert_eq!(snapshot["targetWorkspace"]["targetGroups"], json!([]));
        // First boot has an empty Environment catalog.
        assert_eq!(snapshot["environments"], json!([]));
        assert_eq!(snapshot["activeProjectId"], Value::Null);
        assert_eq!(snapshot["activeAgentSessionId"], Value::Null);
        assert_eq!(snapshot["activeRunId"], Value::Null);
        assert_eq!(
            snapshot["settings"],
            json!({
                "theme":"system",
                "nativeNotifications":true,
                "layout":{"inspectorPercent":28,"terminalPercent":24}
            })
        );
    }

    #[test]
    fn environment_configuration_updates_atomically_and_never_returns_secret_values() {
        let user = TempDir::new().unwrap();
        let environment_dir = user.path().join("ollama");
        fs::create_dir_all(&environment_dir).unwrap();
        fs::write(
            environment_dir.join("environment.json"),
            serde_json::to_vec_pretty(&json!({
                "$schema": "../schema.json",
                "schemaVersion": 1,
                "id": "ollama",
                "name": "Ollama",
                "harnesses": {"coding":"claude", "evaluation":"claude"},
                "plugins": [],
                "permissions": {"trustedRead":"allow", "trustedWrite":"ask", "terminal":"ask"},
                "environmentVariables": {},
                "registries": []
            }))
            .unwrap(),
        )
        .unwrap();
        let plugins = TempDir::new().unwrap();
        let mut runtime = Runtime::with_environment_services(
            ProjectStore::open_in_memory().unwrap(),
            vec![],
            EnvironmentServicePaths {
                user_environments: user.path().to_path_buf(),
                plugins: plugins.path().to_path_buf(),
            },
            Arc::new(InMemorySecretStore::default()),
        )
        .unwrap();

        let secret = runtime.handle_request(Request::new(
            "secret.create",
            json!({"label":"Team LiteLLM", "value":"top-secret"}),
        ));
        let secret_ref = response_result(&secret)["secrets"][0]["secretRef"]
            .as_str()
            .unwrap()
            .to_owned();
        let provider_id = create_test_provider(&mut runtime, Some(&secret_ref));
        let frames = runtime.handle_request(Request::new(
            "environment.configuration.set",
            json!({
                "environmentId": "ollama",
                "configuration": {
                    "name": "Ollama",
                    "environmentVariables": [
                        {"name":"CUSTOM_PROVIDER_LABEL", "source":"literal", "value":"local"},
                        {"name":"CUSTOM_MODEL_LABEL", "source":"literal", "value":"gemma4:cloud"}
                    ],
                    "llm": {"providerId": provider_id.clone(), "allowedModels": ["glm-5.2:cloud"], "defaultModel": "glm-5.2:cloud"},
                    "plugins": [],
                    "registries": []
                }
            }),
        ));
        let environment = |frames: &[Frame]| response_result(frames)["environments"][0].clone();
        assert_eq!(
            environment(&frames)["resolvedLlm"]["credentialRef"],
            secret_ref
        );
        assert_eq!(
            environment(&frames)["environmentVariables"],
            json!([
                {"name":"CUSTOM_MODEL_LABEL", "source":"literal", "value":"gemma4:cloud"},
                {"name":"CUSTOM_PROVIDER_LABEL", "source":"literal", "value":"local"}
            ])
        );
        assert!(matches!(
            &frames[1],
            Frame::Event(Event { topic, .. }) if topic == "environment.changed"
        ));
        let persisted = serde_json::from_slice::<Value>(
            &fs::read(environment_dir.join("environment.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(persisted["llm"]["providerId"], provider_id);
        assert!(!persisted.to_string().contains("credentialRef"));
        assert_eq!(
            persisted["environmentVariables"]["CUSTOM_MODEL_LABEL"],
            json!({"literal":"gemma4:cloud"})
        );
        assert!(!persisted.to_string().contains("top-secret"));

        let listed = runtime.handle_request(Request::new("secret.list", json!({})));
        assert_eq!(
            response_result(&listed)["secrets"][0]["referencedBy"][0]["kind"],
            "llm_provider"
        );
        let blocked_delete = runtime.handle_request(Request::new(
            "secret.delete",
            json!({"secretRef":secret_ref}),
        ));
        assert!(matches!(
            &blocked_delete[0],
            Frame::Response(Response {
                outcome: ResponseOutcome::Error { error },
                ..
            }) if error.message.contains("referenced")
        ));

        // Saving everything at once cannot leave a ready Environment in a
        // configuration that could not be used for a future launch.
        let before_invalid_update = fs::read(environment_dir.join("environment.json")).unwrap();
        let invalid_update = runtime.handle_request(Request::new(
            "environment.configuration.set",
            json!({
                "environmentId":"ollama",
                "configuration": {
                    "name":"Ollama",
                    "environmentVariables":[],
                    "llm":null,
                    "plugins":[],
                    "registries":[]
                }
            }),
        ));
        assert!(matches!(
            &invalid_update[0],
            Frame::Response(Response {
                outcome: ResponseOutcome::Error { error },
                ..
            }) if error.message.contains("Environment configuration is not ready")
        ));
        assert_eq!(
            fs::read(environment_dir.join("environment.json")).unwrap(),
            before_invalid_update
        );

        let unknown = runtime.handle_request(Request::new(
            "environment.configuration.set",
            json!({
                "environmentId":"nonexistent",
                "configuration": {
                    "name":"Nope",
                    "environmentVariables":[],
                    "llm":null,
                    "plugins":[],
                    "registries":[]
                }
            }),
        ));
        assert!(matches!(
            &unknown[0],
            Frame::Response(Response {
                outcome: ResponseOutcome::Error { .. },
                ..
            })
        ));
    }

    #[test]
    fn environment_create_derives_unique_ids_without_global_selection() {
        let user = TempDir::new().unwrap();
        let plugins = TempDir::new().unwrap();
        let mut runtime = Runtime::with_environment_services(
            ProjectStore::open_in_memory().unwrap(),
            vec![],
            EnvironmentServicePaths {
                user_environments: user.path().to_path_buf(),
                plugins: plugins.path().to_path_buf(),
            },
            Arc::new(InMemorySecretStore::default()),
        )
        .unwrap();
        let provider_id = create_test_provider(&mut runtime, None);

        let ready = |name: &str| {
            json!({
                "configuration": {
                    "name": name,
                    "environmentVariables": [],
                    "llm": {"providerId": provider_id, "allowedModels": ["glm-5.2:cloud"], "defaultModel": "glm-5.2:cloud"},
                    "plugins": [],
                    "registries": []
                }
            })
        };

        // The user types a name; the runtime derives the id.
        let first =
            runtime.handle_request(Request::new("environment.create", ready("Local Ollama")));
        assert_eq!(response_result(&first)["environmentId"], "local-ollama");
        // A second Environment with the same name gets a distinct id.
        let second =
            runtime.handle_request(Request::new("environment.create", ready("Local Ollama")));
        assert_eq!(response_result(&second)["environmentId"], "local-ollama-2");

        // An Environment may exist before it is ready, but cannot be used at launch.
        let incomplete = runtime.handle_request(Request::new(
            "environment.create",
            json!({
                "configuration": {
                    "name": "Incomplete",
                    "environmentVariables": [],
                    "llm": null,
                    "plugins": [],
                    "registries": []
                }
            }),
        ));
        assert_eq!(response_result(&incomplete)["environmentId"], "incomplete");
    }

    #[test]
    fn deleting_an_environment_never_reuses_its_id() {
        let user = TempDir::new().unwrap();
        let plugins = TempDir::new().unwrap();
        let mut runtime = Runtime::with_environment_services(
            ProjectStore::open_in_memory().unwrap(),
            vec![],
            EnvironmentServicePaths {
                user_environments: user.path().to_path_buf(),
                plugins: plugins.path().to_path_buf(),
            },
            Arc::new(InMemorySecretStore::default()),
        )
        .unwrap();
        let provider_id = create_test_provider(&mut runtime, None);

        let configuration = json!({
            "configuration": {
                "name": "Local Ollama",
                "environmentVariables": [],
                "llm": {"providerId": provider_id, "allowedModels": ["glm-5.2:cloud"], "defaultModel": "glm-5.2:cloud"},
                "plugins": [],
                "registries": []
            }
        });
        runtime.handle_request(Request::new("environment.create", configuration.clone()));

        let deleted = runtime.handle_request(Request::new(
            "environment.delete",
            json!({"environmentId":"local-ollama"}),
        ));
        assert_eq!(response_result(&deleted)["environments"], json!([]));
        assert!(!user.path().join("local-ollama").exists());
        assert!(matches!(
            &deleted[1],
            Frame::Event(Event { topic, .. }) if topic == "environment.changed"
        ));

        // The id stays reserved: recreating the same name must not adopt the
        // deleted Environment's sessions and harness profiles.
        let recreated = runtime.handle_request(Request::new("environment.create", configuration));
        assert_eq!(
            response_result(&recreated)["environmentId"],
            "local-ollama-2"
        );

        let unknown = runtime.handle_request(Request::new(
            "environment.delete",
            json!({"environmentId":"gone"}),
        ));
        assert!(matches!(
            &unknown[0],
            Frame::Response(Response {
                outcome: ResponseOutcome::Error { .. },
                ..
            })
        ));
    }

    #[test]
    fn shared_provider_changes_require_review_only_for_conflicted_environments() {
        let mut runtime = Runtime::new(ProjectStore::open_in_memory().unwrap(), vec![]);
        let provider_id = create_test_provider(&mut runtime, None);
        response_result(&runtime.handle_request(Request::new(
            "llmProvider.configuration.set",
            json!({
                "providerId": provider_id,
                "configuration": {
                    "name": "Test Provider 1",
                    "type": "ollama",
                    "endpoint": "http://127.0.0.1:11434",
                    "credentialRef": null,
                    "allowedModels": ["glm-5.2:cloud", "qwen3-coder"]
                }
            }),
        )));
        for (name, model) in [("Coding", "glm-5.2:cloud"), ("Evaluation", "qwen3-coder")] {
            let created = runtime.handle_request(Request::new(
                "environment.create",
                json!({
                    "configuration": {
                        "name": name,
                        "environmentVariables": [],
                        "llm": {
                            "providerId": provider_id,
                            "allowedModels": [model],
                            "defaultModel": model
                        },
                        "plugins": [],
                        "registries": []
                    }
                }),
            ));
            assert!(matches!(created[0], Frame::Response(_)));
        }

        let changed = runtime.handle_request(Request::new(
            "llmProvider.configuration.set",
            json!({
                "providerId": provider_id,
                "configuration": {
                    "name": "Test Provider 1",
                    "type": "ollama",
                    "endpoint": "http://localhost:11434",
                    "credentialRef": null,
                    "allowedModels": ["glm-5.2:cloud"]
                }
            }),
        ));
        let environments = response_result(&changed)["environments"]
            .as_array()
            .unwrap();
        assert_eq!(environments.len(), 2);
        assert_eq!(environments[0]["llmNeedsSetup"], false);
        assert_eq!(environments[1]["llmNeedsSetup"], true);
        assert_eq!(
            environments[0]["llm"]["allowedModels"],
            json!(["glm-5.2:cloud"])
        );
        assert_eq!(
            environments[1]["llm"]["allowedModels"],
            json!(["qwen3-coder"])
        );

        let reviewed = runtime.handle_request(Request::new(
            "environment.configuration.set",
            json!({
                "environmentId": "coding",
                "configuration": {
                    "name": "Coding",
                    "environmentVariables": [],
                    "llm": {
                        "providerId": provider_id,
                        "allowedModels": ["glm-5.2:cloud"],
                        "defaultModel": "glm-5.2:cloud"
                    },
                    "plugins": [],
                    "registries": []
                }
            }),
        ));
        let environments = response_result(&reviewed)["environments"]
            .as_array()
            .unwrap();
        assert_eq!(
            environments
                .iter()
                .find(|environment| environment["id"] == "coding")
                .unwrap()["llmNeedsSetup"],
            false,
        );
        assert_eq!(
            environments
                .iter()
                .find(|environment| environment["id"] == "evaluation")
                .unwrap()["llmNeedsSetup"],
            true,
        );

        let renamed = runtime.handle_request(Request::new(
            "llmProvider.configuration.set",
            json!({
                "providerId": provider_id,
                "configuration": {
                    "name": "Renamed Provider",
                    "type": "ollama",
                    "endpoint": "http://localhost:11434",
                    "credentialRef": null,
                    "allowedModels": ["glm-5.2:cloud"]
                }
            }),
        ));
        let environments = response_result(&renamed)["environments"]
            .as_array()
            .unwrap();
        assert_eq!(
            environments
                .iter()
                .find(|environment| environment["id"] == "coding")
                .unwrap()["llmNeedsSetup"],
            false,
        );
    }

    #[test]
    fn adding_a_provider_model_keeps_a_compatible_environment_ready() {
        let mut runtime = Runtime::new(ProjectStore::open_in_memory().unwrap(), vec![]);
        let provider_id = create_test_provider(&mut runtime, None);
        response_result(&runtime.handle_request(Request::new(
            "environment.create",
            json!({
                "configuration": {
                    "name": "Coding",
                    "environmentVariables": [],
                    "llm": {"providerId": provider_id, "allowedModels": ["glm-5.2:cloud"], "defaultModel": "glm-5.2:cloud"},
                    "plugins": [],
                    "registries": []
                }
            }),
        )));
        let changed = runtime.handle_request(Request::new(
            "llmProvider.configuration.set",
            json!({
                "providerId": provider_id,
                "configuration": {
                    "name": "Test Provider 1",
                    "type": "ollama",
                    "endpoint": "http://127.0.0.1:11434",
                    "credentialRef": null,
                    "allowedModels": ["glm-5.2:cloud", "qwen3-coder"]
                }
            }),
        ));
        let environment = &response_result(&changed)["environments"][0];
        assert_eq!(environment["llmNeedsSetup"], false);
        assert_eq!(environment["readiness"]["state"], "ready");
    }

    #[test]
    fn changing_an_environment_provider_reseeds_available_models() {
        let mut runtime = Runtime::new(ProjectStore::open_in_memory().unwrap(), vec![]);
        let first_provider_id = create_test_provider(&mut runtime, None);
        let second_provider_id = create_test_provider(&mut runtime, None);
        response_result(&runtime.handle_request(Request::new(
            "environment.create",
            json!({
                "configuration": {
                    "name": "Coding",
                    "environmentVariables": [],
                    "llm": {
                        "providerId": first_provider_id,
                        "allowedModels": ["glm-5.2:cloud"],
                        "defaultModel": "glm-5.2:cloud"
                    },
                    "plugins": [],
                    "registries": []
                }
            }),
        )));

        // A provider swap carries a stale model selection. The runtime re-seeds
        // available models and the default from the new provider's pool rather
        // than trusting the carried (and possibly invalid) selection.
        let changed = runtime.handle_request(Request::new(
            "environment.configuration.set",
            json!({
                "environmentId": "coding",
                "configuration": {
                    "name": "Coding",
                    "environmentVariables": [],
                    "llm": {
                        "providerId": second_provider_id,
                        "allowedModels": ["stale-model"],
                        "defaultModel": "stale-model"
                    },
                    "plugins": [],
                    "registries": []
                }
            }),
        ));
        assert_eq!(
            response_result(&changed)["environments"][0]["llm"],
            json!({
                "providerId": second_provider_id,
                "allowedModels": ["glm-5.2:cloud"],
                "defaultModel": "glm-5.2:cloud"
            })
        );
    }

    #[test]
    fn provider_delete_unlinks_all_environments_and_retains_the_secret() {
        let mut runtime = Runtime::new(ProjectStore::open_in_memory().unwrap(), vec![]);
        let secret = runtime.handle_request(Request::new(
            "secret.create",
            json!({"label": "Shared key", "value": "top-secret"}),
        ));
        let secret_ref = response_result(&secret)["secrets"][0]["secretRef"]
            .as_str()
            .unwrap()
            .to_owned();
        let provider_id = create_test_provider(&mut runtime, Some(&secret_ref));
        for name in ["Coding", "Evaluation"] {
            response_result(&runtime.handle_request(Request::new(
                "environment.create",
                json!({
                    "configuration": {
                        "name": name,
                        "environmentVariables": [],
                        "llm": {"providerId": provider_id, "allowedModels": ["glm-5.2:cloud"], "defaultModel": "glm-5.2:cloud"},
                        "plugins": [],
                        "registries": []
                    }
                }),
            )));
        }

        let deleted = runtime.handle_request(Request::new(
            "llmProvider.delete",
            json!({"providerId": provider_id}),
        ));
        assert_eq!(response_result(&deleted)["providers"], json!([]));
        assert!(
            response_result(&deleted)["environments"]
                .as_array()
                .unwrap()
                .iter()
                .all(|environment| {
                    environment["llm"].is_null()
                        && environment["llmNeedsSetup"] == true
                        && environment["readiness"]["state"] == "needs_setup"
                })
        );
        let secrets = runtime.handle_request(Request::new("secret.list", json!({})));
        assert_eq!(
            response_result(&secrets)["secrets"][0]["secretRef"],
            secret_ref
        );
    }

    #[test]
    fn settings_methods_are_strict_revisioned_and_evented() {
        let mut runtime = Runtime::new(ProjectStore::open_in_memory().unwrap(), vec![]);
        let theme =
            runtime.handle_request(Request::new("settings.setTheme", json!({"theme":"dark"})));
        assert_eq!(response_result(&theme)["settings"]["theme"], "dark");
        assert_eq!(response_result(&theme)["revision"], 1);
        assert!(matches!(
            &theme[1],
            Frame::Event(Event { topic, revision: 1, payload, .. })
                if topic == "settings.changed" && payload["theme"] == "dark"
        ));

        let notifications = runtime.handle_request(Request::new(
            "settings.setNotifications",
            json!({"enabled":false}),
        ));
        assert_eq!(
            response_result(&notifications)["settings"]["nativeNotifications"],
            false
        );
        assert_eq!(response_result(&notifications)["revision"], 2);

        let layout = runtime.handle_request(Request::new(
            "settings.setLayout",
            json!({"inspectorPercent":32,"terminalPercent":27}),
        ));
        assert_eq!(
            response_result(&layout)["settings"]["layout"],
            json!({"inspectorPercent":32,"terminalPercent":27})
        );
        assert_eq!(response_result(&layout)["revision"], 3);

        for invalid in [
            Request::new("settings.setTheme", json!({"theme":"sepia"})),
            Request::new(
                "settings.setNotifications",
                json!({"enabled":true,"unexpected":true}),
            ),
            Request::new(
                "settings.setLayout",
                json!({"inspectorPercent":5,"terminalPercent":90}),
            ),
        ] {
            let frames = runtime.handle_request(invalid);
            assert!(matches!(
                &frames[0],
                Frame::Response(Response {
                    outcome: ResponseOutcome::Error { error },
                    ..
                }) if error.code == ErrorCode::InvalidParams
            ));
        }
    }

    #[test]
    fn secret_crud_returns_only_keychain_metadata() {
        let mut runtime = Runtime::new(ProjectStore::open_in_memory().unwrap(), vec![]);
        let created = runtime.handle_request(Request::new(
            "secret.create",
            json!({"label":"Anthropic API key","value":"do-not-return"}),
        ));
        let created_json = serde_json::to_string(&created).unwrap();
        assert!(!created_json.contains("do-not-return"));
        let reference = response_result(&created)["secrets"][0]["secretRef"]
            .as_str()
            .unwrap()
            .to_owned();

        let replaced = runtime.handle_request(Request::new(
            "secret.replace",
            json!({"secretRef":reference.clone(),"value":"replacement-secret"}),
        ));
        assert!(
            !serde_json::to_string(&replaced)
                .unwrap()
                .contains("replacement-secret")
        );
        assert_eq!(
            response_result(&replaced)["secrets"][0]["label"],
            "Anthropic API key"
        );

        let deleted = runtime.handle_request(Request::new(
            "secret.delete",
            json!({"secretRef":reference}),
        ));
        assert_eq!(response_result(&deleted)["secrets"], json!([]));

        let invalid = runtime.handle_request(Request::new(
            "secret.create",
            json!({"label":" padded ","value":"x"}),
        ));
        assert!(matches!(
            &invalid[0],
            Frame::Response(Response {
                outcome: ResponseOutcome::Error { error },
                ..
            }) if error.code == ErrorCode::InvalidParams
        ));
    }

    #[test]
    fn canonicalizes_the_default_marketplace_registry_without_network_access() {
        let store = ProjectStore::open_in_memory().unwrap();
        store
            .put_plugin_registry(&PluginRegistryRecord {
                id: DEFAULT_PLUGIN_REGISTRY_ID.into(),
                catalog_url: "https://raw.githubusercontent.com/example/plugins/HEAD/catalog.json"
                    .into(),
                signature_url: "https://raw.githubusercontent.com/example/plugins/HEAD/catalog.sig"
                    .into(),
                public_key_base64: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into(),
            })
            .unwrap();
        let mut runtime = Runtime::new(store, vec![]);

        let listed = runtime.handle_request(Request::new("registry.list", json!({})));
        let registries = response_result(&listed)["registries"].as_array().unwrap();

        assert_eq!(registries.len(), 1);
        assert_eq!(registries[0]["id"], DEFAULT_PLUGIN_REGISTRY_ID);
        assert_eq!(
            registries[0]["catalogUrl"],
            DEFAULT_PLUGIN_REGISTRY_CATALOG_URL,
        );
        assert_eq!(
            registries[0]["signatureUrl"],
            DEFAULT_PLUGIN_REGISTRY_SIGNATURE_URL,
        );
        assert_eq!(
            registries[0]["publicKeyBase64"],
            DEFAULT_PLUGIN_REGISTRY_PUBLIC_KEY,
        );
    }

    #[test]
    fn lists_plugins_without_any_environment() {
        let mut runtime = Runtime::new(ProjectStore::open_in_memory().unwrap(), vec![]);

        let listed = runtime.handle_request(Request::new("plugin.list", json!({})));

        assert_eq!(response_result(&listed)["installed"], json!([]));
        assert_eq!(response_result(&listed)["localMcpServers"], json!([]));
    }

    #[test]
    fn registry_trust_anchors_are_strict_and_listed_without_network_access() {
        let mut runtime = Runtime::new(ProjectStore::open_in_memory().unwrap(), vec![]);
        let invalid = runtime.handle_request(Request::new(
            "registry.put",
            json!({
                "id":"official",
                "catalogUrl":"http://plugins.example/catalog.json",
                "signatureUrl":"https://plugins.example/catalog.sig",
                "publicKeyBase64":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
            }),
        ));
        assert!(matches!(
            &invalid[0],
            Frame::Response(Response {
                outcome: ResponseOutcome::Error { error },
                ..
            }) if error.code == ErrorCode::InvalidParams
        ));

        let stored = runtime.handle_request(Request::new(
            "registry.put",
            json!({
                "id":"official",
                "catalogUrl":"https://plugins.example/catalog.json",
                "signatureUrl":"https://plugins.example/catalog.sig",
                "publicKeyBase64":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
            }),
        ));
        assert_eq!(response_result(&stored)["registries"][0]["id"], "official");
        assert_eq!(
            response_result(&stored)["registries"][0]["catalogUrl"],
            "https://plugins.example/catalog.json"
        );
    }

    #[test]
    fn one_save_carries_plugins_and_registries_with_the_rest_of_the_configuration() {
        let user = TempDir::new().unwrap();
        let plugins = TempDir::new().unwrap();
        let version_key = hex::encode(Sha256::digest(b"1.0.0"));
        let plugin_root = plugins
            .path()
            .join("plugins")
            .join("agent-factory-fixture")
            .join("versions")
            .join(version_key);
        fs::create_dir_all(plugin_root.join("skills").join("verify")).unwrap();
        fs::write(
            plugin_root.join("plugin.json"),
            json!({
                "$schema":"https://agent-plugins.org/schemas/1.0.0/plugin.schema.json",
                "name":"agent-factory-fixture",
                "version":"1.0.0"
            })
            .to_string(),
        )
        .unwrap();
        fs::write(
            plugin_root.join("skills").join("verify").join("SKILL.md"),
            "---\nname: verify\ndescription: Verify the requested behavior.\n---\n\nVerify it.\n",
        )
        .unwrap();
        fs::write(
            plugin_root.join("mcp.json"),
            json!({
                "$schema":"https://agent-plugins.org/schemas/1.0.0/mcp.schema.json",
                "mcpServers": {
                    "httpbin": {"type":"streamable-http","url":"http://127.0.0.1/mcp"}
                }
            })
            .to_string(),
        )
        .unwrap();
        fs::write(
            plugins
                .path()
                .join("plugins")
                .join("agent-factory-fixture")
                .join("state.json"),
            json!({"activeVersion":"1.0.0","previousVersion":null}).to_string(),
        )
        .unwrap();
        let mut runtime = Runtime::with_environment_services(
            ProjectStore::open_in_memory().unwrap(),
            vec![],
            EnvironmentServicePaths {
                user_environments: user.path().to_path_buf(),
                plugins: plugins.path().to_path_buf(),
            },
            Arc::new(InMemorySecretStore::default()),
        )
        .unwrap();
        let environment_dir = user.path().join("acme");

        let secret = runtime.handle_request(Request::new(
            "secret.create",
            json!({"label":"Acme key","value":"top-secret"}),
        ));
        let secret_ref = response_result(&secret)["secrets"][0]["secretRef"]
            .as_str()
            .unwrap()
            .to_owned();
        let provider_id = create_test_provider(&mut runtime, Some(&secret_ref));

        // A reviewed Environment can only select currently resolvable components.
        let selection = json!([{
            "name": "agent-factory-fixture",
            "enabledMcpServers": ["httpbin"],
            "defaultSkills": ["verify"]
        }]);
        let configuration = |plugins: Value, registries: Value| {
            json!({
                "name": "Acme",
                "environmentVariables": [],
                "llm": {"providerId": provider_id, "allowedModels": ["glm-5.2:cloud"], "defaultModel": "glm-5.2:cloud"},
                "plugins": plugins,
                "registries": registries
            })
        };

        let created = runtime.handle_request(Request::new(
            "environment.create",
            json!({"configuration": configuration(selection.clone(), json!([]))}),
        ));
        assert_eq!(response_result(&created)["environmentId"], "acme");
        let persisted = serde_json::from_slice::<Value>(
            &fs::read(environment_dir.join("environment.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(persisted["plugins"], selection);

        // Adding the trust anchor succeeds without any network access.
        runtime.handle_request(Request::new(
            "registry.put",
            json!({
                "id":"official",
                "catalogUrl":"https://127.0.0.1:1/catalog.json",
                "signatureUrl":"https://127.0.0.1:1/catalog.sig",
                "publicKeyBase64":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
            }),
        ));

        // Refresh is allowed as soon as the registry is stored; it reaches the
        // network immediately and fails with a transport error, not a selection gate.
        let reached_network = runtime.handle_request(Request::new(
            "registry.refresh",
            json!({"registryId":"official"}),
        ));
        assert!(matches!(
            &reached_network[0],
            Frame::Response(Response {
                outcome: ResponseOutcome::Error { error },
                ..
            }) if !error.message.is_empty()
        ));

        let selected = runtime.handle_request(Request::new(
            "environment.configuration.set",
            json!({
                "environmentId":"acme",
                "configuration": configuration(selection.clone(), json!(["official"]))
            }),
        ));
        assert_eq!(
            response_result(&selected)["environments"][0]["registryIds"],
            json!(["official"])
        );
        assert_eq!(
            response_result(&selected)["environments"][0]["plugins"],
            selection,
            "saving registries must not disturb the plugin selection",
        );
        let persisted = serde_json::from_slice::<Value>(
            &fs::read(environment_dir.join("environment.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(persisted["registries"], json!(["official"]));

        // Selection no longer changes whether refresh is allowed; it still
        // reaches the network and fails with a transport error.
        let still_reaches_network = runtime.handle_request(Request::new(
            "registry.refresh",
            json!({"registryId":"official"}),
        ));
        assert!(matches!(
            &still_reaches_network[0],
            Frame::Response(Response {
                outcome: ResponseOutcome::Error { error },
                ..
            }) if !error.message.is_empty()
        ));

        // Deselecting both persists the empty sets.
        let removed = runtime.handle_request(Request::new(
            "environment.configuration.set",
            json!({
                "environmentId":"acme",
                "configuration": configuration(json!([]), json!([]))
            }),
        ));
        assert_eq!(
            response_result(&removed)["environments"][0]["registryIds"],
            json!([])
        );
        assert_eq!(
            response_result(&removed)["environments"][0]["plugins"],
            json!([])
        );

        // The Environment validator still rejects malformed selections.
        for invalid in [
            configuration(json!([]), json!(["OFFICIAL"])),
            configuration(
                json!([
                    {"name":"agent-factory-fixture","enabledMcpServers":[],"defaultSkills":[]},
                    {"name":"agent-factory-fixture","enabledMcpServers":[],"defaultSkills":[]}
                ]),
                json!([]),
            ),
        ] {
            let rejected = runtime.handle_request(Request::new(
                "environment.configuration.set",
                json!({"environmentId":"acme", "configuration": invalid}),
            ));
            assert!(matches!(
                &rejected[0],
                Frame::Response(Response {
                    outcome: ResponseOutcome::Error { .. },
                    ..
                })
            ));
        }
    }

    #[test]
    fn plugin_list_projects_available_skills_and_mcp() {
        let user = TempDir::new().unwrap();
        let plugins = TempDir::new().unwrap();
        let store_root = plugins.path().to_path_buf();
        let version_key = hex::encode(Sha256::digest(b"1.0.0"));
        let plugin_root = store_root
            .join("plugins")
            .join("agent-factory-fixture")
            .join("versions")
            .join(version_key);
        fs::create_dir_all(plugin_root.join("skills").join("verify")).unwrap();
        fs::write(
            plugin_root.join("plugin.json"),
            json!({
                "$schema":"https://agent-plugins.org/schemas/1.0.0/plugin.schema.json",
                "name":"agent-factory-fixture",
                "version":"1.0.0"
            })
            .to_string(),
        )
        .unwrap();
        fs::write(
            plugin_root.join("skills").join("verify").join("SKILL.md"),
            "---\nname: verify\ndescription: Verify the requested behavior.\n---\n\nVerify it.\n",
        )
        .unwrap();
        fs::write(
            plugin_root.join("mcp.json"),
            json!({
                "$schema":"https://agent-plugins.org/schemas/1.0.0/mcp.schema.json",
                "mcpServers": {
                    "httpbin": {"type":"streamable-http","url":"http://127.0.0.1/mcp"}
                }
            })
            .to_string(),
        )
        .unwrap();
        fs::write(
            store_root
                .join("plugins")
                .join("agent-factory-fixture")
                .join("state.json"),
            json!({"activeVersion":"1.0.0","previousVersion":null}).to_string(),
        )
        .unwrap();

        let mut runtime = Runtime::with_environment_services(
            ProjectStore::open_in_memory().unwrap(),
            vec![],
            EnvironmentServicePaths {
                user_environments: user.path().to_path_buf(),
                plugins: store_root,
            },
            Arc::new(InMemorySecretStore::default()),
        )
        .unwrap();
        let provider_id = create_test_provider(&mut runtime, None);

        // Local MCP servers are projected from independently configured
        // Environments, without requiring a global selection.
        runtime.handle_request(Request::new(
            "environment.create",
            json!({
                "configuration": {
                    "name": "Fixture Environment",
                    "environmentVariables": [],
                    "llm": {"providerId": provider_id, "allowedModels": ["glm-5.2:cloud"], "defaultModel": "glm-5.2:cloud"},
                    "plugins": [],
                    "registries": []
                }
            }),
        ));

        let listed = runtime.handle_request(Request::new("plugin.list", json!({})));
        let installed = response_result(&listed)["installed"].as_array().unwrap();
        let fixture = installed
            .iter()
            .find(|plugin| plugin["name"] == "agent-factory-fixture")
            .expect("fixture plugin is installed");
        assert_eq!(fixture["activeVersion"], "1.0.0");
        let skill = &fixture["skills"][0];
        assert_eq!(skill["name"], "verify");
        assert_eq!(skill["description"], "Verify the requested behavior.");
        let mcp = &fixture["mcpServers"][0];
        assert_eq!(mcp["name"], "httpbin");
        assert_eq!(mcp["kind"], "streamableHttp");
        assert_eq!(fixture["mcpDisabledReason"], Value::Null);

        response_result(&runtime.handle_request(Request::new(
            "environment.configuration.set",
            json!({
                "environmentId": "fixture-environment",
                "configuration": {
                    "name": "Fixture Environment",
                    "environmentVariables": [],
                    "llm": {"providerId": provider_id, "allowedModels": ["glm-5.2:cloud"], "defaultModel": "glm-5.2:cloud"},
                    "plugins": [{
                        "name": "agent-factory-fixture",
                        "enabledMcpServers": [],
                        "defaultSkills": ["verify"]
                    }],
                    "registries": []
                }
            }),
        )));
        let uninstalled = runtime.handle_request(Request::new(
            "plugin.uninstall",
            json!({"pluginName": "agent-factory-fixture"}),
        ));
        assert!(
            response_result(&uninstalled)["installed"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            runtime.store.snapshot().unwrap().environments[0]
                .plugins
                .is_empty()
        );
        assert!(!plugin_root.exists());
    }

    #[test]
    fn local_mcp_trust_fingerprint_binds_executable_and_launch_configuration() {
        let directory = TempDir::new().unwrap();
        let executable = directory.path().join("plugin-mcp");
        fs::write(&executable, b"#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&executable).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&executable, permissions).unwrap();
        }
        let resolved = resolve_plugin_executable(
            Path::new("plugin-mcp"),
            ExecutableTrustClass::PathExecutable,
            &[directory.path().to_path_buf()],
        )
        .unwrap();
        let environment = BTreeMap::from([("PLUGIN_DATA".into(), "/private/data".into())]);
        let original = local_mcp_fingerprint(
            "default",
            "quality-tools",
            "lint",
            &resolved,
            &["--stdio".into()],
            &environment,
            directory.path(),
        )
        .unwrap();
        let changed_argument = local_mcp_fingerprint(
            "default",
            "quality-tools",
            "lint",
            &resolved,
            &["--json".into()],
            &environment,
            directory.path(),
        )
        .unwrap();
        assert_ne!(original, changed_argument);

        fs::write(&resolved, b"#!/bin/sh\nexit 1\n").unwrap();
        let changed_binary = local_mcp_fingerprint(
            "default",
            "quality-tools",
            "lint",
            &resolved,
            &["--stdio".into()],
            &environment,
            directory.path(),
        )
        .unwrap();
        assert_ne!(original, changed_binary);
    }

    #[test]
    fn unpackaged_updates_fail_closed_with_an_explicit_status() {
        let mut runtime = Runtime::new(ProjectStore::open_in_memory().unwrap(), vec![]);
        let status = runtime.handle_request(Request::new("update.status", json!({})));
        assert_eq!(response_result(&status)["enabled"], false);
        assert_eq!(response_result(&status)["currentVersion"], RUNTIME_VERSION);
        assert_eq!(response_result(&status)["state"], "idle");
        assert!(response_result(&status)["message"].is_string());

        let check = runtime.handle_request(Request::new("update.check", json!({})));
        assert!(matches!(
            &check[0],
            Frame::Response(Response {
                outcome: ResponseOutcome::Error { error },
                ..
            }) if error.code == ErrorCode::InvalidParams
        ));
    }

    #[test]
    fn public_session_diagnostics_are_bounded_and_redacted() {
        let raw = format!(
            "connection failed\nAuthorization: Bearer top-secret\nAPI_KEY=hidden\n{}",
            "x".repeat(MAX_PUBLIC_DIAGNOSTIC_BYTES * 2),
        );
        let sanitized = sanitize_public_diagnostic(&raw);
        assert!(sanitized.contains("connection failed"));
        assert!(!sanitized.contains("top-secret"));
        assert!(!sanitized.contains("hidden"));
        assert!(sanitized.len() <= MAX_PUBLIC_DIAGNOSTIC_BYTES);
    }

    #[test]
    fn file_methods_are_confined_to_trusted_projects() {
        let root = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        fs::write(root.path().join("inside.txt"), "inside").unwrap();
        fs::write(outside.path().join("outside.txt"), "outside").unwrap();
        let mut runtime = Runtime::new(ProjectStore::open_in_memory().unwrap(), vec![]);
        runtime.handle_request(Request::new(
            "project.create",
            json!({ "name": "Trusted", "root": root.path(), "trusted": true }),
        ));

        let read = runtime.handle_request(Request::new(
            "file.read",
            json!({ "path": root.path().join("inside.txt") }),
        ));
        assert_eq!(response_result(&read)["content"], "inside");

        let denied = runtime.handle_request(Request::new(
            "file.read",
            json!({ "path": outside.path().join("outside.txt") }),
        ));
        assert!(matches!(
            &denied[0],
            Frame::Response(Response {
                outcome: ResponseOutcome::Error { error },
                ..
            }) if error.code == ErrorCode::InvalidParams
        ));
    }

    #[test]
    fn prompt_and_evaluator_output_budgets_are_explicit_and_bounded() {
        let prompt = bound_prompt("🌍".repeat(MAX_FACTORY_PROMPT_BYTES));
        assert!(prompt.len() <= MAX_FACTORY_PROMPT_BYTES);
        assert!(prompt.contains("[TRUNCATED:"));

        let evidence = bounded_evidence(&vec!["x".repeat(10_000); 20], 16_000);
        assert!(evidence.len() < 20_000);
        assert!(evidence.contains("omitted"));
    }

    #[test]
    fn herdr_provider_overrides_do_not_set_a_custom_anthropic_api_key() {
        let overrides = herdr_provider_overrides();
        assert_eq!(overrides[0], ("ANTHROPIC_API_KEY", ""));
        assert!(
            !overrides
                .iter()
                .any(|(_, value)| value.contains("agent-factory-loopback"))
        );
    }

    fn permissions(
        trusted_write: EnvironmentPermissionPolicy,
        terminal: EnvironmentPermissionPolicy,
    ) -> EnvironmentPermissionProjection {
        EnvironmentPermissionProjection {
            trusted_read: EnvironmentPermissionPolicy::Allow,
            trusted_write,
            terminal,
        }
    }

    #[test]
    fn claude_starts_with_the_environment_model() {
        let asking = permissions(
            EnvironmentPermissionPolicy::Ask,
            EnvironmentPermissionPolicy::Ask,
        );
        assert_eq!(
            harness_start_args("claude", "deepseek-v4-flash:0731:cloud", asking),
            vec!["--model", "deepseek-v4-flash:0731:cloud"]
        );
        assert!(harness_start_args("codex", "deepseek-v4-flash:0731:cloud", asking).is_empty());
        assert!(harness_start_args("claude", "", asking).is_empty());
    }

    /// A Factory Run advances unattended, so an Environment that grants a
    /// permission must not leave the agent waiting for someone to grant it
    /// again at a dialog.
    #[test]
    fn an_environment_that_grants_permission_does_not_stop_for_approval() {
        let full = permissions(
            EnvironmentPermissionPolicy::Allow,
            EnvironmentPermissionPolicy::Allow,
        );
        assert_eq!(
            harness_start_args("claude", "model", full),
            vec!["--model", "model", "--permission-mode", "bypassPermissions"]
        );

        let edits_only = permissions(
            EnvironmentPermissionPolicy::Allow,
            EnvironmentPermissionPolicy::Ask,
        );
        assert_eq!(
            harness_permission_args("claude", edits_only),
            vec!["--permission-mode", "acceptEdits"]
        );

        // An Environment that asks keeps asking; autonomy is the user's call.
        assert!(
            harness_permission_args(
                "claude",
                permissions(
                    EnvironmentPermissionPolicy::Ask,
                    EnvironmentPermissionPolicy::Allow,
                ),
            )
            .is_empty()
        );
        assert!(harness_permission_args("codex", full).is_empty());
    }
}
