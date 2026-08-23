//! Transactional local persistence for projects, environments, sessions, and runs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use app_core::{
    AgentDraftLifecycle, AgentDraftProjection, AgentSessionProjection, ApplicationProjection,
    ChangedFile, EnvironmentLlmPolicyDto, EnvironmentPermissionProjection,
    EnvironmentPluginProjection, EnvironmentProjection, EnvironmentReadinessProjection,
    EnvironmentReadinessState, EnvironmentVariableProjection, EvaluationResult, FactoryRun,
    FactoryRunState, HarnessPurpose, HerdrStatusProjection, LayoutProjection, LlmProviderDto,
    ManagedSessionOutcome, ProjectProjection, ResolvedLlmProviderDto, SessionAvailability,
    SettingsProjection, TargetAgentProjection, TargetAgentVersionProjection,
    TargetAgentWorkGroupProjection, TargetWorkItemKind, TargetWorkItemProjection,
    TargetWorkspaceProjection, TestEvidence, ThemePreference, WorkContextProjection,
    WorkspaceBindingProjection, WorkspaceDock, WorkspacePaneProjection,
    WorkspaceTerminalProjection, WorkspaceTerminalState,
};
use rusqlite::{Connection, ErrorCode, OptionalExtension, params};
use thiserror::Error;
use uuid::Uuid;

const SCHEMA_VERSION: u32 = 27;
const WORKSPACE_WIDTH_BASIS_POINTS: u16 = 10_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginRegistryRecord {
    pub id: String,
    pub catalog_url: String,
    pub signature_url: String,
    pub public_key_base64: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalMcpTrustRecord {
    pub environment_id: String,
    pub plugin_name: String,
    pub server_name: String,
    pub fingerprint: String,
}

pub struct ProjectStore {
    connection: Mutex<Connection>,
    path: Option<PathBuf>,
}

impl ProjectStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path.as_ref())?;
        Self::from_connection(connection, Some(path.as_ref().to_path_buf()))
    }

    pub fn open_in_memory() -> Result<Self, StoreError> {
        Self::from_connection(Connection::open_in_memory()?, None)
    }

    /// Where this store lives on disk, if anywhere. Callers that keep sibling
    /// state next to the database use it to stay co-located with it.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    fn from_connection(connection: Connection, path: Option<PathBuf>) -> Result<Self, StoreError> {
        let existing_version =
            connection.query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))?;
        if existing_version > SCHEMA_VERSION {
            return Err(StoreError::IncompatibleSchema {
                found: existing_version,
                supported: SCHEMA_VERSION,
            });
        }
        if existing_version != 0 && existing_version != SCHEMA_VERSION {
            reset_greenfield_schema(&connection)?;
        }
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA busy_timeout = 5000;",
        )?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS app_state (
               singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
               revision INTEGER NOT NULL,
               focused_pane_id TEXT,
               theme TEXT NOT NULL DEFAULT 'system'
                 CHECK (theme IN ('system', 'light', 'dark')),
               native_notifications INTEGER NOT NULL DEFAULT 1
                 CHECK (native_notifications IN (0, 1)),
               inspector_percent INTEGER NOT NULL DEFAULT 28
                 CHECK (inspector_percent BETWEEN 20 AND 50),
               terminal_percent INTEGER NOT NULL DEFAULT 24
                 CHECK (terminal_percent BETWEEN 14 AND 50)
             );
             INSERT OR IGNORE INTO app_state(singleton, revision)
               VALUES (1, 0);

             CREATE TABLE IF NOT EXISTS projects (
               id TEXT PRIMARY KEY,
               name TEXT NOT NULL CHECK (length(trim(name)) > 0),
               root TEXT NOT NULL UNIQUE,
               trusted INTEGER NOT NULL CHECK (trusted IN (0, 1))
             );

             CREATE TABLE IF NOT EXISTS target_agents (
               id TEXT PRIMARY KEY,
               name TEXT NOT NULL CHECK (length(trim(name)) > 0),
               repository_root TEXT NOT NULL,
               archived INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1)),
               last_activity_at_unix_ms INTEGER NOT NULL
             );

             CREATE TABLE IF NOT EXISTS target_agent_drafts (
               id TEXT PRIMARY KEY,
               target_agent_id TEXT NOT NULL REFERENCES target_agents(id) ON DELETE CASCADE,
               workspace_binding_id TEXT UNIQUE,
               name TEXT NOT NULL CHECK (length(trim(name)) > 0),
               objective TEXT NOT NULL CHECK (length(trim(objective)) > 0),
               acceptance_criteria_json TEXT NOT NULL,
               base_version TEXT,
               branch_ref TEXT NOT NULL UNIQUE,
               worktree_path TEXT NOT NULL UNIQUE,
               git_head TEXT NOT NULL,
               lifecycle TEXT NOT NULL CHECK (lifecycle IN (
                 'active', 'publishing', 'archived', 'cleanup_required'
               )),
               reserved_version TEXT,
               cleanup_guidance TEXT,
               -- The Environment the user chose for this Draft's Runs. Not a
               -- foreign key: Environments live in their own descriptors and
               -- may be removed, so a stale choice has to read as no choice
               -- rather than block the Draft.
               environment_id TEXT,
               created_at_unix_ms INTEGER NOT NULL,
               updated_at_unix_ms INTEGER NOT NULL
             );

             CREATE TABLE IF NOT EXISTS target_agent_versions (
               id TEXT PRIMARY KEY,
               target_agent_id TEXT NOT NULL REFERENCES target_agents(id) ON DELETE CASCADE,
               version TEXT NOT NULL,
               name TEXT NOT NULL CHECK (length(trim(name)) > 0),
               objective TEXT NOT NULL CHECK (length(trim(objective)) > 0),
               acceptance_criteria_json TEXT NOT NULL,
               source_draft_id TEXT NOT NULL REFERENCES target_agent_drafts(id),
               git_commit TEXT NOT NULL,
               git_tag TEXT NOT NULL UNIQUE,
               created_at_unix_ms INTEGER NOT NULL,
               UNIQUE(target_agent_id, version)
             );

             CREATE TABLE IF NOT EXISTS workspace_bindings (
               id TEXT PRIMARY KEY,
               target_agent_id TEXT NOT NULL REFERENCES target_agents(id),
               project_id TEXT NOT NULL REFERENCES projects(id),
               name TEXT NOT NULL CHECK (length(trim(name)) > 0),
               primary_root TEXT NOT NULL,
               additional_roots_json TEXT NOT NULL DEFAULT '[]',
               source_ref_label TEXT,
               archived INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1)),
               last_used_at_unix_ms INTEGER NOT NULL,
               UNIQUE(target_agent_id, project_id, primary_root)
             );

             CREATE TABLE IF NOT EXISTS llm_providers (
               id TEXT PRIMARY KEY,
               provider_json TEXT NOT NULL
             );

             CREATE TABLE IF NOT EXISTS environments (
               id TEXT PRIMARY KEY,
               name TEXT NOT NULL CHECK (length(trim(name)) > 0),
               coding_harness_id TEXT NOT NULL,
               evaluation_harness_id TEXT NOT NULL,
               plugins_json TEXT NOT NULL,
               permissions_json TEXT NOT NULL,
               registry_ids_json TEXT NOT NULL,
               environment_variables_json TEXT NOT NULL,
               llm_policy_json TEXT,
               resolved_llm_json TEXT,
               llm_needs_setup INTEGER NOT NULL DEFAULT 0 CHECK (llm_needs_setup IN (0, 1)),
               readiness_json TEXT NOT NULL,
               -- Rows are never deleted: agent_sessions.environment_id references
               -- them, so an Environment that leaves the catalog is tombstoned instead.
               available INTEGER NOT NULL DEFAULT 1 CHECK (available IN (0, 1))
             );

            -- Durable lineage for a Factory-managed Herdr session. Live
            -- lifecycle, topology, attention, and process state are joined from
            -- fresh Herdr snapshots and are never written here.
            CREATE TABLE IF NOT EXISTS agent_sessions (
               id TEXT PRIMARY KEY,
               workspace_binding_id TEXT NOT NULL REFERENCES workspace_bindings(id),
               factory_run_id TEXT REFERENCES factory_runs(id) ON DELETE CASCADE,
               parent_session_id TEXT REFERENCES agent_sessions(id),
               environment_id TEXT NOT NULL REFERENCES environments(id),
               harness_id TEXT NOT NULL CHECK (length(trim(harness_id)) > 0),
               purpose TEXT NOT NULL CHECK (purpose IN ('orchestration', 'coding', 'evaluation')),
               herdr_agent_name TEXT NOT NULL UNIQUE,
               title TEXT NOT NULL,
               created_at_unix_ms INTEGER NOT NULL,
               last_activity_at_unix_ms INTEGER NOT NULL,
               llm_provider_snapshot_json TEXT,
               effective_model TEXT,
               initial_prompt TEXT,
               brief_delivered INTEGER NOT NULL DEFAULT 0 CHECK (brief_delivered IN (0, 1)),
               outcome_json TEXT
             );

             CREATE TABLE IF NOT EXISTS factory_runs (
               id TEXT PRIMARY KEY,
               workspace_binding_id TEXT NOT NULL REFERENCES workspace_bindings(id),
               agent_draft_id TEXT NOT NULL REFERENCES target_agent_drafts(id),
               environment_id TEXT NOT NULL REFERENCES environments(id),
               objective TEXT NOT NULL,
               acceptance_criteria_json TEXT NOT NULL,
               starting_git_head TEXT NOT NULL,
               final_git_head TEXT,
               changed_files_json TEXT NOT NULL DEFAULT '[]',
               test_evidence_json TEXT NOT NULL DEFAULT '[]',
               evaluation_json TEXT,
               state TEXT NOT NULL,
               escalation TEXT,
               last_activity_at_unix_ms INTEGER NOT NULL,
               completed_at_unix_ms INTEGER
             );
             CREATE UNIQUE INDEX IF NOT EXISTS factory_runs_one_live_per_draft
               ON factory_runs(agent_draft_id)
               WHERE state NOT IN ('passed', 'failed', 'needs_review', 'cancelled');
             -- Authorizes one Factory Run's Orchestrator to drive its own loop.
             -- Deliberately not part of the run projection: it is a secret, and
             -- projections reach IPC payloads and the UI.
             CREATE TABLE IF NOT EXISTS run_control_tokens (
               token TEXT PRIMARY KEY,
               factory_run_id TEXT NOT NULL REFERENCES factory_runs(id) ON DELETE CASCADE,
               created_at_unix_ms INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS run_control_tokens_by_run
               ON run_control_tokens(factory_run_id);
             CREATE TABLE IF NOT EXISTS work_contexts (
               id TEXT PRIMARY KEY,
               workspace_binding_id TEXT NOT NULL REFERENCES workspace_bindings(id),
               agent_draft_id TEXT UNIQUE REFERENCES target_agent_drafts(id) ON DELETE CASCADE,
               agent_session_id TEXT UNIQUE REFERENCES agent_sessions(id) ON DELETE CASCADE,
               factory_run_id TEXT UNIQUE REFERENCES factory_runs(id) ON DELETE CASCADE,
               dock TEXT NOT NULL DEFAULT 'closed'
                 CHECK (dock IN ('closed', 'terminal')),
               dock_percent INTEGER NOT NULL DEFAULT 32
                 CHECK (dock_percent BETWEEN 20 AND 60),
               last_viewed_at_unix_ms INTEGER NOT NULL,
               CHECK (
                 (agent_draft_id IS NOT NULL) +
                 (agent_session_id IS NOT NULL) +
                 (factory_run_id IS NOT NULL) <= 1
               )
             );
             CREATE UNIQUE INDEX IF NOT EXISTS work_contexts_one_agent_draft
               ON work_contexts(workspace_binding_id)
               WHERE agent_draft_id IS NULL
                 AND agent_session_id IS NULL
                 AND factory_run_id IS NULL;
             CREATE TABLE IF NOT EXISTS workspace_panes (
               id TEXT PRIMARY KEY,
               work_context_id TEXT NOT NULL UNIQUE REFERENCES work_contexts(id) ON DELETE CASCADE,
               position INTEGER NOT NULL UNIQUE CHECK (position BETWEEN 0 AND 2),
               width_basis_points INTEGER NOT NULL
                 CHECK (width_basis_points BETWEEN 1 AND 10000)
             );
             CREATE TABLE IF NOT EXISTS workspace_terminals (
               id TEXT PRIMARY KEY,
               work_context_id TEXT NOT NULL REFERENCES work_contexts(id) ON DELETE CASCADE,
               title TEXT NOT NULL,
               state TEXT NOT NULL CHECK (state IN ('running', 'exited')),
               created_at_unix_ms INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS plugin_registries (
               id TEXT PRIMARY KEY,
               catalog_url TEXT NOT NULL,
               signature_url TEXT NOT NULL,
               public_key_base64 TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS local_mcp_trust (
               environment_id TEXT NOT NULL REFERENCES environments(id) ON DELETE CASCADE,
               plugin_name TEXT NOT NULL,
               server_name TEXT NOT NULL,
               fingerprint TEXT NOT NULL,
               PRIMARY KEY(environment_id, plugin_name, server_name)
             );
             PRAGMA user_version = 27;",
        )?;
        connection.execute(
            "UPDATE workspace_terminals SET state = 'exited' WHERE state = 'running'",
            [],
        )?;
        Ok(Self {
            connection: Mutex::new(connection),
            path,
        })
    }

    pub fn create_project(
        &self,
        name: &str,
        root: &Path,
        trusted: bool,
    ) -> Result<ProjectProjection, StoreError> {
        if name.trim().is_empty() {
            return Err(StoreError::InvalidInput(
                "project name must not be empty".into(),
            ));
        }
        if !root.is_absolute() {
            return Err(StoreError::InvalidInput(
                "project root must be absolute".into(),
            ));
        }
        if !root.is_dir() {
            return Err(StoreError::InvalidInput(
                "project root must be an existing directory".into(),
            ));
        }
        let root = std::fs::canonicalize(root)?;
        let project = ProjectProjection {
            id: Uuid::new_v4(),
            name: name.trim().to_owned(),
            root,
            trusted,
        };

        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        let result = transaction.execute(
            "INSERT INTO projects(id, name, root, trusted) VALUES (?1, ?2, ?3, ?4)",
            params![
                project.id.to_string(),
                project.name,
                project.root.to_string_lossy(),
                project.trusted,
            ],
        );
        if let Err(error) = result {
            if matches!(
                &error,
                rusqlite::Error::SqliteFailure(details, _)
                    if details.code == ErrorCode::ConstraintViolation
            ) {
                return Err(StoreError::Conflict(
                    "a project with this root already exists".into(),
                ));
            }
            return Err(error.into());
        }
        bump_revision(&transaction)?;
        transaction.commit()?;
        Ok(project)
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectProjection>, StoreError> {
        let connection = self.connection.lock().expect("store lock poisoned");
        let mut statement =
            connection.prepare("SELECT id, name, root, trusted FROM projects ORDER BY name, id")?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, bool>(3)?,
            ))
        })?;
        rows.map(|row| {
            let (id, name, root, trusted) = row?;
            Ok(ProjectProjection {
                id: parse_uuid(&id)?,
                name,
                root: PathBuf::from(root),
                trusted,
            })
        })
        .collect()
    }

    pub fn set_project_trust(
        &self,
        project_id: Uuid,
        trusted: bool,
    ) -> Result<ProjectProjection, StoreError> {
        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        let changed = transaction.execute(
            "UPDATE projects SET trusted = ?1 WHERE id = ?2 AND trusted != ?1",
            params![trusted, project_id.to_string()],
        )?;
        let project = transaction
            .query_row(
                "SELECT id, name, root, trusted FROM projects WHERE id = ?1",
                [project_id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, bool>(3)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| StoreError::NotFound(format!("project `{project_id}`")))?;
        if changed > 0 {
            bump_revision(&transaction)?;
        }
        transaction.commit()?;
        Ok(ProjectProjection {
            id: parse_uuid(&project.0)?,
            name: project.1,
            root: PathBuf::from(project.2),
            trusted: project.3,
        })
    }

    pub fn create_target_agent(
        &self,
        id: Uuid,
        name: &str,
        repository_root: &Path,
    ) -> Result<TargetAgentProjection, StoreError> {
        require_text("target agent name", name.trim(), 200)?;
        let repository_root = canonical_directory(repository_root, "Agent repository root")?;
        let target = TargetAgentProjection {
            id,
            name: name.trim().to_owned(),
            repository_root,
            archived: false,
            last_activity_at_unix_ms: now_unix_ms(),
        };
        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO target_agents(
               id, name, repository_root, archived, last_activity_at_unix_ms
             ) VALUES (?1, ?2, ?3, 0, ?4)",
            params![
                target.id.to_string(),
                target.name,
                target.repository_root.to_string_lossy(),
                target.last_activity_at_unix_ms,
            ],
        )?;
        bump_revision(&transaction)?;
        transaction.commit()?;
        Ok(target)
    }

    pub fn create_agent_draft(
        &self,
        draft: &AgentDraftProjection,
    ) -> Result<AgentDraftProjection, StoreError> {
        validate_draft_definition(&draft.name, &draft.objective, &draft.acceptance_criteria)?;
        if !draft.worktree_path.is_absolute() {
            return Err(StoreError::InvalidInput(
                "Draft worktree path must be absolute".into(),
            ));
        }
        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        require_uuid_entity(&transaction, "target_agents", draft.target_agent_id)?;
        require_uuid_entity(
            &transaction,
            "workspace_bindings",
            draft.workspace_binding_id,
        )?;
        transaction.execute(
            "INSERT INTO target_agent_drafts(
               id, target_agent_id, workspace_binding_id, name, objective,
               acceptance_criteria_json, base_version, branch_ref, worktree_path,
               git_head, lifecycle, cleanup_guidance, environment_id,
               created_at_unix_ms, updated_at_unix_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                draft.id.to_string(),
                draft.target_agent_id.to_string(),
                draft.workspace_binding_id.to_string(),
                draft.name,
                draft.objective,
                serde_json::to_string(&draft.acceptance_criteria)?,
                draft.base_version,
                draft.branch_ref,
                draft.worktree_path.to_string_lossy(),
                draft.git_head,
                draft_lifecycle_name(draft.lifecycle),
                draft.cleanup_guidance,
                draft.environment_id,
                draft.created_at_unix_ms,
                draft.updated_at_unix_ms,
            ],
        )?;
        bump_revision(&transaction)?;
        transaction.commit()?;
        Ok(draft.clone())
    }

    pub fn create_workspace_binding(
        &self,
        target_agent_id: Uuid,
        project_id: Uuid,
        name: &str,
        primary_root: &Path,
        additional_roots: &[PathBuf],
        source_ref_label: Option<&str>,
    ) -> Result<WorkspaceBindingProjection, StoreError> {
        self.create_workspace_binding_with_id(
            Uuid::new_v4(),
            target_agent_id,
            project_id,
            name,
            primary_root,
            additional_roots,
            source_ref_label,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_workspace_binding_with_id(
        &self,
        binding_id: Uuid,
        target_agent_id: Uuid,
        project_id: Uuid,
        name: &str,
        primary_root: &Path,
        additional_roots: &[PathBuf],
        source_ref_label: Option<&str>,
    ) -> Result<WorkspaceBindingProjection, StoreError> {
        require_text("workspace binding name", name.trim(), 200)?;
        let primary_root = canonical_directory(primary_root, "primary workspace root")?;
        let mut normalized_additional = Vec::with_capacity(additional_roots.len());
        for root in additional_roots {
            let root = canonical_directory(root, "additional workspace root")?;
            if root == primary_root || normalized_additional.contains(&root) {
                return Err(StoreError::InvalidInput(
                    "workspace roots must be unique".into(),
                ));
            }
            normalized_additional.push(root);
        }
        if let Some(label) = source_ref_label {
            require_text("source reference label", label, 200)?;
        }
        let binding = WorkspaceBindingProjection {
            id: binding_id,
            target_agent_id,
            project_id,
            name: name.trim().to_owned(),
            primary_root,
            additional_roots: normalized_additional,
            source_ref_label: source_ref_label.map(str::to_owned),
            archived: false,
            last_used_at_unix_ms: now_unix_ms(),
        };
        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        require_uuid_entity(&transaction, "target_agents", target_agent_id)?;
        require_entity(&transaction, "projects", project_id)?;
        transaction.execute(
            "INSERT INTO workspace_bindings(
               id, target_agent_id, project_id, name, primary_root,
               additional_roots_json, source_ref_label, archived, last_used_at_unix_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8)",
            params![
                binding.id.to_string(),
                binding.target_agent_id.to_string(),
                binding.project_id.to_string(),
                binding.name,
                binding.primary_root.to_string_lossy(),
                serde_json::to_string(&binding.additional_roots)?,
                binding.source_ref_label,
                binding.last_used_at_unix_ms,
            ],
        )?;
        bump_revision(&transaction)?;
        transaction.commit()?;
        Ok(binding)
    }

    /// Record which Environment this Draft's Runs should use.
    ///
    /// Only the choice is stored. Whether that Environment still exists and is
    /// ready is read at launch, so a Draft is never blocked by a choice that
    /// has gone stale.
    pub fn set_agent_draft_environment(
        &self,
        draft_id: Uuid,
        environment_id: Option<&str>,
    ) -> Result<AgentDraftProjection, StoreError> {
        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        let changed = transaction.execute(
            "UPDATE target_agent_drafts
             SET environment_id = ?1, updated_at_unix_ms = ?2
             WHERE id = ?3 AND lifecycle = 'active'",
            params![environment_id, now_unix_ms(), draft_id.to_string()],
        )?;
        if changed == 0 {
            return Err(StoreError::InvalidInput(
                "only an active Draft can choose an Environment".into(),
            ));
        }
        bump_revision(&transaction)?;
        let draft = query_agent_draft_row(&transaction, draft_id)?;
        transaction.commit()?;
        Ok(draft)
    }

    pub fn update_agent_draft(
        &self,
        draft_id: Uuid,
        name: &str,
        objective: &str,
        acceptance_criteria: &[String],
        git_head: &str,
    ) -> Result<AgentDraftProjection, StoreError> {
        let acceptance_criteria = validate_draft_definition(name, objective, acceptance_criteria)?;
        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        let timestamp = now_unix_ms();
        let changed = transaction.execute(
            "UPDATE target_agent_drafts
             SET name = ?1, objective = ?2, acceptance_criteria_json = ?3,
                 git_head = ?4, updated_at_unix_ms = ?5
             WHERE id = ?6 AND lifecycle = 'active'",
            params![
                name.trim(),
                objective.trim(),
                serde_json::to_string(&acceptance_criteria)?,
                git_head,
                timestamp,
                draft_id.to_string()
            ],
        )?;
        if changed == 0 {
            return Err(StoreError::InvalidInput(
                "only an active Draft can be edited".into(),
            ));
        }
        transaction.execute(
            "UPDATE target_agents SET
               name = CASE WHEN NOT EXISTS (
                 SELECT 1 FROM target_agent_versions v
                 WHERE v.target_agent_id = target_agents.id
               ) AND EXISTS (
                 SELECT 1 FROM target_agent_drafts d WHERE d.id = ?2
                   AND d.target_agent_id = target_agents.id AND d.base_version IS NULL
               ) THEN ?3 ELSE name END,
               last_activity_at_unix_ms = ?1
             WHERE id = (SELECT target_agent_id FROM target_agent_drafts WHERE id = ?2)",
            params![timestamp, draft_id.to_string(), name.trim()],
        )?;
        bump_revision(&transaction)?;
        transaction.commit()?;
        drop(connection);
        self.agent_draft(draft_id)
    }

    pub fn agent_draft(&self, draft_id: Uuid) -> Result<AgentDraftProjection, StoreError> {
        let connection = self.connection.lock().expect("store lock poisoned");
        query_agent_drafts(&connection)?
            .into_iter()
            .find(|draft| draft.id == draft_id)
            .ok_or_else(|| StoreError::NotFound(format!("Agent Draft `{draft_id}`")))
    }

    pub fn reserve_agent_draft_version(
        &self,
        draft_id: Uuid,
        version: &str,
    ) -> Result<(), StoreError> {
        validate_semver(version)?;
        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        let changed = transaction.execute(
            "UPDATE target_agent_drafts SET lifecycle = 'publishing', reserved_version = ?1,
             updated_at_unix_ms = ?2 WHERE id = ?3 AND lifecycle = 'active'",
            params![version, now_unix_ms(), draft_id.to_string()],
        )?;
        if changed == 0 {
            return Err(StoreError::InvalidInput(
                "Draft cannot be published in its current state".into(),
            ));
        }
        bump_revision(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn restore_agent_draft(&self, draft_id: Uuid) -> Result<(), StoreError> {
        let connection = self.connection.lock().expect("store lock poisoned");
        connection.execute(
            "UPDATE target_agent_drafts SET lifecycle = 'active', reserved_version = NULL,
             updated_at_unix_ms = ?1 WHERE id = ?2 AND lifecycle = 'publishing'",
            params![now_unix_ms(), draft_id.to_string()],
        )?;
        bump_revision(&connection)?;
        Ok(())
    }

    pub fn publishing_agent_drafts(
        &self,
    ) -> Result<Vec<(AgentDraftProjection, String)>, StoreError> {
        let connection = self.connection.lock().expect("store lock poisoned");
        let drafts = query_agent_drafts(&connection)?;
        drafts
            .into_iter()
            .filter(|draft| draft.lifecycle == AgentDraftLifecycle::Publishing)
            .map(|draft| {
                let reserved = connection
                    .query_row(
                        "SELECT reserved_version FROM target_agent_drafts WHERE id = ?1",
                        [draft.id.to_string()],
                        |row| row.get::<_, Option<String>>(0),
                    )?
                    .ok_or_else(|| {
                        StoreError::Corrupt(format!(
                            "publishing Draft `{}` has no reserved Version",
                            draft.id
                        ))
                    })?;
                Ok((draft, reserved))
            })
            .collect()
    }

    pub fn agent_drafts_requiring_cleanup(&self) -> Result<Vec<AgentDraftProjection>, StoreError> {
        let connection = self.connection.lock().expect("store lock poisoned");
        Ok(query_agent_drafts(&connection)?
            .into_iter()
            .filter(|draft| draft.lifecycle == AgentDraftLifecycle::CleanupRequired)
            .collect())
    }

    pub fn finish_agent_draft_publication(
        &self,
        draft_id: Uuid,
        version: &str,
        git_commit: &str,
        git_tag: &str,
    ) -> Result<TargetAgentVersionProjection, StoreError> {
        validate_semver(version)?;
        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        let draft = query_agent_draft_row(&transaction, draft_id)?;
        if draft.lifecycle != AgentDraftLifecycle::Publishing {
            return Err(StoreError::InvalidInput(
                "Draft publication was not reserved".into(),
            ));
        }
        let created_at = now_unix_ms();
        let published = TargetAgentVersionProjection {
            id: Uuid::new_v4(),
            target_agent_id: draft.target_agent_id,
            version: version.into(),
            name: draft.name.clone(),
            objective: draft.objective.clone(),
            acceptance_criteria: draft.acceptance_criteria.clone(),
            source_draft_id: draft.id,
            git_commit: git_commit.into(),
            git_tag: git_tag.into(),
            created_at_unix_ms: created_at,
        };
        transaction.execute(
            "INSERT INTO target_agent_versions(
               id, target_agent_id, version, name, objective, acceptance_criteria_json,
               source_draft_id, git_commit, git_tag, created_at_unix_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                published.id.to_string(),
                published.target_agent_id.to_string(),
                published.version,
                published.name,
                published.objective,
                serde_json::to_string(&published.acceptance_criteria)?,
                published.source_draft_id.to_string(),
                published.git_commit,
                published.git_tag,
                published.created_at_unix_ms
            ],
        )?;
        transaction.execute(
            "UPDATE target_agent_drafts SET lifecycle = 'archived', reserved_version = NULL,
             updated_at_unix_ms = ?1 WHERE id = ?2",
            params![created_at, draft_id.to_string()],
        )?;
        let canonical_name = query_target_agent_versions(&transaction)?
            .into_iter()
            .filter(|candidate| candidate.target_agent_id == draft.target_agent_id)
            .max_by_key(|candidate| semver::Version::parse(&candidate.version).ok())
            .map(|candidate| candidate.name)
            .unwrap_or_else(|| published.name.clone());
        transaction.execute(
            "UPDATE target_agents SET name = ?1, last_activity_at_unix_ms = ?2 WHERE id = ?3",
            params![
                canonical_name,
                created_at,
                draft.target_agent_id.to_string()
            ],
        )?;
        bump_revision(&transaction)?;
        transaction.commit()?;
        Ok(published)
    }

    pub fn set_agent_draft_cleanup(
        &self,
        draft_id: Uuid,
        required: bool,
        guidance: Option<&str>,
    ) -> Result<(), StoreError> {
        let connection = self.connection.lock().expect("store lock poisoned");
        connection.execute(
            "UPDATE target_agent_drafts SET lifecycle = ?1, cleanup_guidance = ?2,
             updated_at_unix_ms = ?3 WHERE id = ?4",
            params![
                if required {
                    "cleanup_required"
                } else {
                    "archived"
                },
                guidance,
                now_unix_ms(),
                draft_id.to_string()
            ],
        )?;
        if !required {
            connection.execute(
                "UPDATE workspace_bindings SET archived = 1
                 WHERE id = (SELECT workspace_binding_id FROM target_agent_drafts WHERE id = ?1)",
                [draft_id.to_string()],
            )?;
        }
        bump_revision(&connection)?;
        Ok(())
    }

    pub fn update_workspace_binding_root(
        &self,
        workspace_binding_id: Uuid,
        root: &Path,
        trusted: bool,
    ) -> Result<(WorkspaceBindingProjection, ProjectProjection), StoreError> {
        let root = canonical_directory(root, "primary workspace root")?;
        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        let project_id = transaction
            .query_row(
                "SELECT project_id FROM workspace_bindings WHERE id = ?1",
                [workspace_binding_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| {
                StoreError::NotFound(format!("workspace binding `{workspace_binding_id}`"))
            })?;
        transaction.execute(
            "UPDATE projects SET root = ?1, trusted = ?2 WHERE id = ?3",
            params![root.to_string_lossy(), trusted, project_id],
        )?;
        transaction.execute(
            "UPDATE workspace_bindings
             SET primary_root = ?1, last_used_at_unix_ms = ?2
             WHERE id = ?3",
            params![
                root.to_string_lossy(),
                now_unix_ms(),
                workspace_binding_id.to_string(),
            ],
        )?;
        bump_revision(&transaction)?;
        transaction.commit()?;
        drop(connection);
        Ok((
            self.workspace_binding(workspace_binding_id)?,
            self.list_projects()?
                .into_iter()
                .find(|project| project.id.to_string() == project_id)
                .ok_or_else(|| StoreError::Corrupt("updated project is missing".into()))?,
        ))
    }

    pub fn target_agent_version(
        &self,
        target_agent_version_id: Uuid,
    ) -> Result<TargetAgentVersionProjection, StoreError> {
        let connection = self.connection.lock().expect("store lock poisoned");
        query_target_agent_versions(&connection)?
            .into_iter()
            .find(|version| version.id == target_agent_version_id)
            .ok_or_else(|| {
                StoreError::NotFound(format!("target agent version `{target_agent_version_id}`"))
            })
    }

    pub fn target_agent_versions(
        &self,
        target_agent_id: Uuid,
    ) -> Result<Vec<TargetAgentVersionProjection>, StoreError> {
        let connection = self.connection.lock().expect("store lock poisoned");
        let mut versions = query_target_agent_versions(&connection)?
            .into_iter()
            .filter(|version| version.target_agent_id == target_agent_id)
            .collect::<Vec<_>>();
        versions.sort_by(|left, right| {
            semver::Version::parse(&right.version)
                .ok()
                .cmp(&semver::Version::parse(&left.version).ok())
        });
        Ok(versions)
    }

    pub fn target_agent(&self, target_agent_id: Uuid) -> Result<TargetAgentProjection, StoreError> {
        let connection = self.connection.lock().expect("store lock poisoned");
        query_targets(&connection)?
            .into_iter()
            .find(|agent| agent.id == target_agent_id)
            .ok_or_else(|| StoreError::NotFound(format!("target agent `{target_agent_id}`")))
    }

    /// Hide an Agent and its bindings from Agent Factory. Disk files stay.
    pub fn archive_target_agent(&self, target_agent_id: Uuid) -> Result<(), StoreError> {
        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        require_uuid_entity(&transaction, "target_agents", target_agent_id)?;
        let pane_ids = {
            let mut statement = transaction.prepare(
                "SELECT wp.id FROM workspace_panes wp
                 JOIN work_contexts wc ON wc.id = wp.work_context_id
                 JOIN workspace_bindings wb ON wb.id = wc.workspace_binding_id
                 WHERE wb.target_agent_id = ?1
                 ORDER BY wp.position",
            )?;
            let rows = statement
                .query_map([target_agent_id.to_string()], |row| row.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        for pane_id in &pane_ids {
            transaction.execute("DELETE FROM workspace_panes WHERE id = ?1", [pane_id])?;
        }
        let remaining = query_workspace_pane_layout(&transaction)?;
        if !remaining.is_empty() {
            let resized = scale_workspace_widths(&remaining, WORKSPACE_WIDTH_BASIS_POINTS);
            update_workspace_widths(&transaction, &resized)?;
            for (index, (id, _)) in remaining.iter().enumerate() {
                transaction.execute(
                    "UPDATE workspace_panes SET position = ?1 WHERE id = ?2",
                    params![index as u8, id.to_string()],
                )?;
            }
        }
        let next_focus = remaining.first().map(|(id, _)| id.to_string());
        transaction.execute(
            "UPDATE app_state SET focused_pane_id = ?1 WHERE singleton = 1",
            [next_focus],
        )?;
        transaction.execute(
            "UPDATE workspace_bindings SET archived = 1 WHERE target_agent_id = ?1",
            [target_agent_id.to_string()],
        )?;
        transaction.execute(
            "UPDATE target_agents SET archived = 1 WHERE id = ?1",
            [target_agent_id.to_string()],
        )?;
        bump_revision(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn agent_draft_has_live_run(&self, draft_id: Uuid) -> Result<bool, StoreError> {
        let connection = self.connection.lock().expect("store lock poisoned");
        Ok(connection
            .query_row(
                "SELECT 1 FROM factory_runs WHERE agent_draft_id = ?1
             AND state NOT IN ('passed', 'failed', 'cancelled') LIMIT 1",
                [draft_id.to_string()],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    pub fn archive_agent_draft(&self, draft_id: Uuid) -> Result<(), StoreError> {
        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        let changed = transaction.execute(
            "UPDATE target_agent_drafts SET lifecycle = 'archived', updated_at_unix_ms = ?1
             WHERE id = ?2 AND lifecycle IN ('active', 'cleanup_required')",
            params![now_unix_ms(), draft_id.to_string()],
        )?;
        if changed == 0 {
            return Err(StoreError::InvalidInput(
                "Draft cannot be discarded in its current state".into(),
            ));
        }
        transaction.execute(
            "UPDATE workspace_bindings SET archived = 1
             WHERE id = (SELECT workspace_binding_id FROM target_agent_drafts WHERE id = ?1)",
            [draft_id.to_string()],
        )?;
        bump_revision(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn workspace_binding(&self, id: Uuid) -> Result<WorkspaceBindingProjection, StoreError> {
        let connection = self.connection.lock().expect("store lock poisoned");
        query_workspace_binding(&connection, id)?
            .ok_or_else(|| StoreError::NotFound(format!("workspace binding `{id}`")))
    }

    pub fn agent_session_workspace(
        &self,
        agent_session_id: Uuid,
    ) -> Result<WorkspaceBindingProjection, StoreError> {
        let connection = self.connection.lock().expect("store lock poisoned");
        let binding_id = connection
            .query_row(
                "SELECT workspace_binding_id FROM agent_sessions WHERE id = ?1",
                [agent_session_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| {
                StoreError::NotFound(format!("workspace for agent session `{agent_session_id}`"))
            })?;
        query_workspace_binding(&connection, parse_uuid(&binding_id)?)?.ok_or_else(|| {
            StoreError::Corrupt(format!(
                "agent session `{agent_session_id}` references a missing workspace binding"
            ))
        })
    }

    pub fn factory_run_workspace(
        &self,
        factory_run_id: Uuid,
    ) -> Result<WorkspaceBindingProjection, StoreError> {
        let connection = self.connection.lock().expect("store lock poisoned");
        let binding_id = connection
            .query_row(
                "SELECT workspace_binding_id FROM factory_runs WHERE id = ?1",
                [factory_run_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| StoreError::NotFound(format!("workspace for Run `{factory_run_id}`")))?;
        query_workspace_binding(&connection, parse_uuid(&binding_id)?)?.ok_or_else(|| {
            StoreError::Corrupt(format!(
                "Run `{factory_run_id}` references a missing workspace binding"
            ))
        })
    }

    pub fn open_work_item(
        &self,
        target_agent_id: Uuid,
        workspace_binding_id: Uuid,
        work_item_id: Option<Uuid>,
        work_item_kind: Option<TargetWorkItemKind>,
        open_to_side: bool,
    ) -> Result<Uuid, StoreError> {
        if work_item_id.is_some() != work_item_kind.is_some() {
            return Err(StoreError::InvalidInput(
                "work item id and kind must be provided together".into(),
            ));
        }
        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        require_uuid_entity(&transaction, "target_agents", target_agent_id)?;
        require_uuid_entity(&transaction, "workspace_bindings", workspace_binding_id)?;
        let binding_target = transaction.query_row(
            "SELECT target_agent_id FROM workspace_bindings WHERE id = ?1",
            [workspace_binding_id.to_string()],
            |row| row.get::<_, String>(0),
        )?;
        if parse_uuid(&binding_target)? != target_agent_id {
            return Err(StoreError::InvalidInput(
                "workspace binding does not belong to the target agent".into(),
            ));
        }
        validate_work_item_context(
            &transaction,
            target_agent_id,
            workspace_binding_id,
            work_item_id,
            work_item_kind,
        )?;
        let (agent_draft_id, agent_session_id, factory_run_id) = match work_item_kind {
            Some(TargetWorkItemKind::AgentDraft) => (work_item_id, None, None),
            Some(
                TargetWorkItemKind::OrchestrationThread
                | TargetWorkItemKind::CodingThread
                | TargetWorkItemKind::EvaluationThread,
            ) => (None, work_item_id, None),
            Some(TargetWorkItemKind::FactoryRun) => (None, None, work_item_id),
            None => (None, None, None),
        };
        let existing_context = transaction
            .query_row(
                "SELECT id FROM work_contexts
                 WHERE workspace_binding_id = ?1
                   AND agent_draft_id IS ?2
                   AND agent_session_id IS ?3 AND factory_run_id IS ?4",
                params![
                    workspace_binding_id.to_string(),
                    agent_draft_id.map(|id| id.to_string()),
                    agent_session_id.map(|id| id.to_string()),
                    factory_run_id.map(|id| id.to_string()),
                ],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let context_id = existing_context
            .map(|id| parse_uuid(&id))
            .transpose()?
            .unwrap_or_else(Uuid::new_v4);
        let timestamp = now_unix_ms();
        transaction.execute(
            "INSERT INTO work_contexts(
               id, workspace_binding_id, agent_draft_id,
               agent_session_id, factory_run_id,
               dock, dock_percent, last_viewed_at_unix_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, 'closed', 32, ?6)
             ON CONFLICT(id) DO UPDATE SET
               last_viewed_at_unix_ms = excluded.last_viewed_at_unix_ms",
            params![
                context_id.to_string(),
                workspace_binding_id.to_string(),
                agent_draft_id.map(|id| id.to_string()),
                agent_session_id.map(|id| id.to_string()),
                factory_run_id.map(|id| id.to_string()),
                timestamp,
            ],
        )?;
        if let Some(pane_id) = transaction
            .query_row(
                "SELECT id FROM workspace_panes WHERE work_context_id = ?1",
                [context_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            transaction.execute(
                "UPDATE app_state SET focused_pane_id = ?1 WHERE singleton = 1",
                [pane_id],
            )?;
        } else {
            let panes = query_workspace_pane_layout(&transaction)?;
            let pane_id = if !open_to_side && !panes.is_empty() {
                let primary_pane_id = panes[0].0;
                transaction.execute(
                    "UPDATE workspace_panes SET work_context_id = ?1 WHERE id = ?2",
                    params![context_id.to_string(), primary_pane_id.to_string()],
                )?;
                primary_pane_id
            } else {
                if panes.len() >= 3 {
                    return Err(StoreError::Conflict(
                        "at most three workspace panes may be visible".into(),
                    ));
                }
                let pane_id = Uuid::new_v4();
                let position = panes.len() as u8;
                if panes.is_empty() {
                    transaction.execute(
                        "INSERT INTO workspace_panes(
                           id, work_context_id, position, width_basis_points
                         ) VALUES (?1, ?2, 0, ?3)",
                        params![
                            pane_id.to_string(),
                            context_id.to_string(),
                            WORKSPACE_WIDTH_BASIS_POINTS,
                        ],
                    )?;
                } else {
                    let new_pane_width = WORKSPACE_WIDTH_BASIS_POINTS / (panes.len() as u16 + 1);
                    let resized = scale_workspace_widths(
                        &panes,
                        WORKSPACE_WIDTH_BASIS_POINTS - new_pane_width,
                    );
                    update_workspace_widths(&transaction, &resized)?;
                    transaction.execute(
                        "INSERT INTO workspace_panes(
                           id, work_context_id, position, width_basis_points
                         ) VALUES (?1, ?2, ?3, ?4)",
                        params![
                            pane_id.to_string(),
                            context_id.to_string(),
                            position,
                            new_pane_width,
                        ],
                    )?;
                }
                pane_id
            };
            transaction.execute(
                "UPDATE app_state SET focused_pane_id = ?1 WHERE singleton = 1",
                [pane_id.to_string()],
            )?;
        }
        bump_revision(&transaction)?;
        transaction.commit()?;
        Ok(context_id)
    }

    pub fn focus_workspace_pane(&self, pane_id: Uuid) -> Result<(), StoreError> {
        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        require_pane(&transaction, pane_id)?;
        transaction.execute(
            "UPDATE app_state SET focused_pane_id = ?1 WHERE singleton = 1",
            [pane_id.to_string()],
        )?;
        bump_revision(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn close_workspace_pane(&self, pane_id: Uuid) -> Result<(), StoreError> {
        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        let panes = query_workspace_pane_layout(&transaction)?;
        let position = panes
            .iter()
            .position(|(id, _)| *id == pane_id)
            .ok_or_else(|| StoreError::NotFound(format!("workspace pane `{pane_id}`")))?;
        transaction.execute(
            "DELETE FROM workspace_panes WHERE id = ?1",
            [pane_id.to_string()],
        )?;
        transaction.execute(
            "UPDATE workspace_panes SET position = position - 1 WHERE position > ?1",
            [position as u8],
        )?;
        let survivors = panes
            .into_iter()
            .filter(|(id, _)| *id != pane_id)
            .collect::<Vec<_>>();
        if !survivors.is_empty() {
            let resized = scale_workspace_widths(&survivors, WORKSPACE_WIDTH_BASIS_POINTS);
            update_workspace_widths(&transaction, &resized)?;
        }
        let next_focus = transaction
            .query_row(
                "SELECT id FROM workspace_panes ORDER BY ABS(position - ?1), position LIMIT 1",
                [position as u8],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        transaction.execute(
            "UPDATE app_state SET focused_pane_id = ?1 WHERE singleton = 1",
            [next_focus],
        )?;
        bump_revision(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn resize_workspace_panes(&self, layout: &[(Uuid, u16)]) -> Result<(), StoreError> {
        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        let existing = query_workspace_pane_layout(&transaction)?;
        if layout.is_empty() || layout.len() != existing.len() {
            return Err(StoreError::InvalidInput(
                "workspace resize must contain every visible pane".into(),
            ));
        }
        if layout
            .iter()
            .map(|(_, width)| u32::from(*width))
            .sum::<u32>()
            != u32::from(WORKSPACE_WIDTH_BASIS_POINTS)
            || layout.iter().any(|(_, width)| *width == 0)
        {
            return Err(StoreError::InvalidInput(
                "workspace pane widths must be positive and total 10000 basis points".into(),
            ));
        }
        if layout.iter().map(|(id, _)| *id).collect::<Vec<_>>()
            != existing.iter().map(|(id, _)| *id).collect::<Vec<_>>()
        {
            return Err(StoreError::InvalidInput(
                "workspace resize pane order must match the visible layout".into(),
            ));
        }
        update_workspace_widths(&transaction, layout)?;
        bump_revision(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn move_workspace_pane(&self, pane_id: Uuid, destination: u8) -> Result<(), StoreError> {
        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        let panes = query_workspace_pane_layout(&transaction)?;
        if usize::from(destination) >= panes.len() {
            return Err(StoreError::InvalidInput(
                "workspace pane destination is outside the visible layout".into(),
            ));
        }
        let source = panes
            .iter()
            .position(|(id, _)| *id == pane_id)
            .ok_or_else(|| StoreError::NotFound(format!("workspace pane `{pane_id}`")))?;
        let (work_context_id, width_basis_points) = transaction.query_row(
            "SELECT work_context_id, width_basis_points
             FROM workspace_panes WHERE id = ?1",
            [pane_id.to_string()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, u16>(1)?)),
        )?;
        transaction.execute(
            "DELETE FROM workspace_panes WHERE id = ?1",
            [pane_id.to_string()],
        )?;
        if source < usize::from(destination) {
            for position in source + 1..=usize::from(destination) {
                transaction.execute(
                    "UPDATE workspace_panes SET position = ?1 WHERE position = ?2",
                    params![position - 1, position],
                )?;
            }
        } else {
            for position in (usize::from(destination)..source).rev() {
                transaction.execute(
                    "UPDATE workspace_panes SET position = ?1 WHERE position = ?2",
                    params![position + 1, position],
                )?;
            }
        }
        transaction.execute(
            "INSERT INTO workspace_panes(
               id, work_context_id, position, width_basis_points
             ) VALUES (?1, ?2, ?3, ?4)",
            params![
                pane_id.to_string(),
                work_context_id,
                destination,
                width_basis_points,
            ],
        )?;
        transaction.execute(
            "UPDATE app_state SET focused_pane_id = ?1 WHERE singleton = 1",
            [pane_id.to_string()],
        )?;
        bump_revision(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn set_work_context_dock(
        &self,
        work_context_id: Uuid,
        dock: WorkspaceDock,
        dock_percent: u8,
    ) -> Result<(), StoreError> {
        if !(20..=60).contains(&dock_percent) {
            return Err(StoreError::InvalidInput(
                "dock percent must be between 20 and 60".into(),
            ));
        }
        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        let changed = transaction.execute(
            "UPDATE work_contexts
             SET dock = ?1, dock_percent = ?2
             WHERE id = ?3",
            params![
                workspace_dock_name(dock),
                dock_percent,
                work_context_id.to_string(),
            ],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!(
                "work context `{work_context_id}`"
            )));
        }
        bump_revision(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn register_workspace_terminal(
        &self,
        terminal_id: Uuid,
        work_context_id: Uuid,
        title: &str,
    ) -> Result<(), StoreError> {
        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        transaction
            .query_row(
                "SELECT 1 FROM work_contexts WHERE id = ?1",
                [work_context_id.to_string()],
                |_| Ok(()),
            )
            .optional()?
            .ok_or_else(|| StoreError::NotFound(format!("work context `{work_context_id}`")))?;
        transaction.execute(
            "INSERT INTO workspace_terminals(
               id, work_context_id, title, state, created_at_unix_ms
             ) VALUES (?1, ?2, ?3, 'running', ?4)",
            params![
                terminal_id.to_string(),
                work_context_id.to_string(),
                title.trim(),
                now_unix_ms(),
            ],
        )?;
        bump_revision(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn mark_workspace_terminal_exited(&self, terminal_id: Uuid) -> Result<(), StoreError> {
        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        let changed = transaction.execute(
            "UPDATE workspace_terminals SET state = 'exited' WHERE id = ?1",
            [terminal_id.to_string()],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!(
                "workspace terminal `{terminal_id}`"
            )));
        }
        bump_revision(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn remove_workspace_terminal(&self, terminal_id: Uuid) -> Result<(), StoreError> {
        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        let changed = transaction.execute(
            "DELETE FROM workspace_terminals WHERE id = ?1",
            [terminal_id.to_string()],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!(
                "workspace terminal `{terminal_id}`"
            )));
        }
        bump_revision(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn put_environment(&self, id: &str, name: &str) -> Result<(), StoreError> {
        if id.trim().is_empty() || name.trim().is_empty() {
            return Err(StoreError::InvalidInput(
                "environment id and name must not be empty".into(),
            ));
        }
        self.sync_environments(&[EnvironmentProjection {
            id: id.to_owned(),
            name: name.trim().to_owned(),
            coding_harness_id: "claude".into(),
            evaluation_harness_id: "claude".into(),
            plugins: Vec::new(),
            permissions: EnvironmentPermissionProjection::default(),
            registry_ids: Vec::new(),
            environment_variables: Vec::new(),
            llm: None,
            resolved_llm: None,
            llm_needs_setup: false,
            readiness: EnvironmentReadinessProjection {
                state: EnvironmentReadinessState::NeedsSetup,
                issues: vec!["Configure an Intelligence Provider".into()],
            },
        }])?;
        Ok(())
    }

    /// Upserts validated catalog summaries without touching availability, for
    /// callers that are saving a subset of the catalog.
    pub fn sync_environments(
        &self,
        environments: &[EnvironmentProjection],
    ) -> Result<u64, StoreError> {
        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        let changed = upsert_environments(&transaction, environments)?;
        if changed > 0 {
            bump_revision(&transaction)?;
        }
        transaction.commit()?;
        Ok(changed as u64)
    }

    /// Makes the stored catalog match `environments` exactly: every listed Environment is
    /// upserted and marked available, and every other row is tombstoned.
    ///
    /// Rows are never deleted because `agent_sessions.environment_id` references
    /// them — tombstoning is what lets a deleted Environment keep its history
    /// while disappearing from projections.
    /// Reconciling on every mutation and on boot means the store converges no
    /// matter where a crash lands between the filesystem and SQLite.
    pub fn reconcile_environments(
        &self,
        environments: &[EnvironmentProjection],
    ) -> Result<u64, StoreError> {
        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        let mut changed = upsert_environments(&transaction, environments)?;

        let present = environments
            .iter()
            .map(|environment| environment.id.as_str())
            .collect::<Vec<_>>();
        let placeholders = repeat_placeholders(present.len());
        let bindings = rusqlite::params_from_iter(present.iter());

        changed += transaction.execute(
            &format!(
                "UPDATE environments SET available = 0 WHERE available = 1 AND id NOT IN ({placeholders})"
            ),
            rusqlite::params_from_iter(present.iter()),
        )?;
        // Local MCP trust is a live grant, not history: it must not outlive the
        // Environment that granted it. The `ON DELETE CASCADE` never fires because the
        // `environments` row survives, so the purge has to be explicit.
        changed += transaction.execute(
            &format!("DELETE FROM local_mcp_trust WHERE environment_id NOT IN ({placeholders})"),
            bindings,
        )?;

        if changed > 0 {
            bump_revision(&transaction)?;
        }
        transaction.commit()?;
        Ok(changed as u64)
    }

    /// Reports whether an Environment id was ever used, including tombstoned rows.
    /// Slug allocation consults this so a deleted id is never handed out again
    /// and cannot silently re-parent the old Environment's sessions.
    pub fn environment_id_exists(&self, id: &str) -> Result<bool, StoreError> {
        let connection = self.connection.lock().expect("store lock poisoned");
        Ok(connection
            .query_row("SELECT 1 FROM environments WHERE id = ?1", [id], |_| Ok(()))
            .optional()?
            .is_some())
    }

    /// Upsert durable managed-session lineage. Live Herdr state is deliberately
    /// ignored even though it is present on the joined projection.
    pub fn save_agent_session(&self, session: &AgentSessionProjection) -> Result<(), StoreError> {
        require_text("agent session title", &session.title, 1_000)?;
        require_text("harness id", &session.harness_id, 64)?;
        require_text("Herdr agent name", &session.herdr_agent_name, 64)?;
        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        validate_workspace_binding_identity(
            &transaction,
            session.workspace_binding_id,
            session.target_agent_id,
            session.project_id,
        )?;
        transaction.execute(
            "INSERT INTO agent_sessions(
               id, workspace_binding_id, factory_run_id, parent_session_id,
               environment_id, harness_id, purpose, herdr_agent_name, title,
               created_at_unix_ms, last_activity_at_unix_ms,
               llm_provider_snapshot_json, effective_model, initial_prompt,
               brief_delivered, outcome_json
             ) VALUES (
               ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
               ?14, ?15, ?16
             )
             ON CONFLICT(id) DO UPDATE SET
               environment_id = excluded.environment_id,
               harness_id = excluded.harness_id,
               title = excluded.title,
               last_activity_at_unix_ms = excluded.last_activity_at_unix_ms,
               llm_provider_snapshot_json = excluded.llm_provider_snapshot_json,
               effective_model = excluded.effective_model,
               initial_prompt = COALESCE(
                 agent_sessions.initial_prompt,
                 excluded.initial_prompt
               ),
               brief_delivered = excluded.brief_delivered,
               outcome_json = COALESCE(excluded.outcome_json, agent_sessions.outcome_json)",
            params![
                session.id.to_string(),
                session.workspace_binding_id.to_string(),
                session.factory_run_id.map(|id| id.to_string()),
                session.parent_session_id.map(|id| id.to_string()),
                session.environment_id,
                session.harness_id,
                purpose_name(session.purpose),
                session.herdr_agent_name,
                session.title,
                session.created_at_unix_ms,
                session.last_activity_at_unix_ms,
                optional_json(&session.llm_provider_snapshot)?,
                session.effective_model,
                session.initial_prompt,
                session.brief_delivered,
                optional_json(&session.outcome)?,
            ],
        )?;
        transaction.execute(
            "UPDATE target_agents SET last_activity_at_unix_ms = ?1 WHERE id = ?2",
            params![
                session.last_activity_at_unix_ms,
                session.target_agent_id.to_string()
            ],
        )?;
        bump_revision(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    /// Return an unfinished interactive managed session for this binding,
    /// purpose, and Environment, creating one when none exists.
    pub fn reserve_draft_agent_session(
        &self,
        proposed: &AgentSessionProjection,
    ) -> Result<(AgentSessionProjection, bool), StoreError> {
        if proposed.factory_run_id.is_some() {
            return Err(StoreError::InvalidInput(
                "interactive session reservation cannot belong to a Run".into(),
            ));
        }
        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        validate_workspace_binding_identity(
            &transaction,
            proposed.workspace_binding_id,
            proposed.target_agent_id,
            proposed.project_id,
        )?;
        let existing = transaction
            .query_row(
                &format!(
                    "SELECT {AGENT_SESSION_COLUMNS}
                     FROM agent_sessions s
                     JOIN workspace_bindings b ON b.id = s.workspace_binding_id
                     WHERE s.workspace_binding_id = ?1
                       AND s.purpose = ?2 AND s.environment_id = ?3
                       AND s.factory_run_id IS NULL AND s.outcome_json IS NULL
                     ORDER BY s.created_at_unix_ms DESC LIMIT 1"
                ),
                params![
                    proposed.workspace_binding_id.to_string(),
                    purpose_name(proposed.purpose),
                    proposed.environment_id,
                ],
                agent_session_row,
            )
            .optional()?;
        if let Some(existing) = existing {
            let existing = parse_agent_session_row(existing)?;
            transaction.commit()?;
            return Ok((existing, true));
        }
        drop(transaction);
        drop(connection);
        self.save_agent_session(proposed)?;
        Ok((proposed.clone(), false))
    }

    /// Drop a reservation whose opening prompt was never delivered.
    pub fn discard_unstarted_agent_session(
        &self,
        agent_session_id: Uuid,
    ) -> Result<bool, StoreError> {
        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        let discardable = transaction
            .query_row(
                "SELECT 1 FROM agent_sessions s
                 WHERE s.id = ?1 AND s.factory_run_id IS NULL
                   AND s.brief_delivered = 0 AND s.outcome_json IS NULL",
                [agent_session_id.to_string()],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !discardable {
            transaction.commit()?;
            return Ok(false);
        }
        transaction.execute(
            "DELETE FROM agent_sessions WHERE id = ?1",
            [agent_session_id.to_string()],
        )?;
        bump_revision(&transaction)?;
        transaction.commit()?;
        Ok(true)
    }

    /// Mint the secret that authorizes this run's Orchestrator to drive itself.
    ///
    /// One live token per run, reused across restarts so an Orchestrator that
    /// outlived Agent Factory keeps working. It is returned once here and then
    /// only ever resolved back to a run id; nothing projects it.
    pub fn mint_run_control_token(&self, run_id: Uuid) -> Result<String, StoreError> {
        let connection = self.connection.lock().expect("store lock poisoned");
        if let Some(existing) = connection
            .query_row(
                "SELECT token FROM run_control_tokens WHERE factory_run_id = ?1",
                [run_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            return Ok(existing);
        }
        // Two v4 UUIDs are 256 bits from the platform CSPRNG, which is what this
        // needs; a guessable token would let any local process drive a run.
        let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        connection.execute(
            "INSERT INTO run_control_tokens(token, factory_run_id, created_at_unix_ms)
             VALUES (?1, ?2, ?3)",
            params![token, run_id.to_string(), now_unix_ms()],
        )?;
        Ok(token)
    }

    /// Resolve a presented token back to the run it may act on.
    pub fn factory_run_for_control_token(&self, token: &str) -> Result<Option<Uuid>, StoreError> {
        let connection = self.connection.lock().expect("store lock poisoned");
        let found = connection
            .query_row(
                "SELECT factory_run_id FROM run_control_tokens WHERE token = ?1",
                [token],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        found.map(|id| parse_uuid(&id)).transpose()
    }

    /// Withdraw a run's authority once it can no longer legally be driven.
    pub fn revoke_run_control_tokens(&self, run_id: Uuid) -> Result<(), StoreError> {
        let connection = self.connection.lock().expect("store lock poisoned");
        connection.execute(
            "DELETE FROM run_control_tokens WHERE factory_run_id = ?1",
            [run_id.to_string()],
        )?;
        Ok(())
    }

    pub fn save_factory_run(&self, run: &FactoryRun) -> Result<(), StoreError> {
        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        let (target_agent_id, project_id) = transaction
            .query_row(
                "SELECT target_agent_id, project_id FROM workspace_bindings WHERE id = ?1",
                [run.workspace_binding_id.to_string()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?
            .ok_or_else(|| {
                StoreError::NotFound(format!("workspace binding `{}`", run.workspace_binding_id))
            })?;
        if target_agent_id != run.target_agent_id.to_string()
            || project_id != run.project_id.to_string()
        {
            return Err(StoreError::InvalidInput(
                "Run identity does not match its workspace binding".into(),
            ));
        }
        require_uuid_entity(&transaction, "target_agent_drafts", run.agent_draft_id)?;
        require_entity_text(&transaction, "environments", &run.environment_id)?;
        let draft_target = transaction.query_row(
            "SELECT target_agent_id FROM target_agent_drafts WHERE id = ?1",
            [run.agent_draft_id.to_string()],
            |row| row.get::<_, String>(0),
        )?;
        if draft_target != run.target_agent_id.to_string() {
            return Err(StoreError::InvalidInput(
                "Run Draft does not belong to its Agent".into(),
            ));
        }
        let timestamp = now_unix_ms();
        transaction.execute(
            "INSERT INTO factory_runs(
               id, workspace_binding_id, agent_draft_id, environment_id,
               objective, acceptance_criteria_json, starting_git_head,
               final_git_head, changed_files_json, test_evidence_json,
               evaluation_json, state, escalation, last_activity_at_unix_ms,
               completed_at_unix_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
             ON CONFLICT(id) DO UPDATE SET
               environment_id = excluded.environment_id,
               objective = excluded.objective,
               acceptance_criteria_json = excluded.acceptance_criteria_json,
               starting_git_head = excluded.starting_git_head,
               final_git_head = excluded.final_git_head,
               changed_files_json = excluded.changed_files_json,
               test_evidence_json = excluded.test_evidence_json,
               evaluation_json = excluded.evaluation_json,
               state = excluded.state,
               escalation = excluded.escalation,
               last_activity_at_unix_ms = excluded.last_activity_at_unix_ms,
               completed_at_unix_ms = excluded.completed_at_unix_ms",
            params![
                run.id.to_string(),
                run.workspace_binding_id.to_string(),
                run.agent_draft_id.to_string(),
                run.environment_id,
                run.objective,
                serde_json::to_string(&run.acceptance_criteria)?,
                run.starting_git_head,
                run.final_git_head,
                serde_json::to_string(&run.changed_files)?,
                serde_json::to_string(&run.test_evidence)?,
                optional_json(&run.evaluation)?,
                run_state_name(run.state),
                run.escalation,
                timestamp,
                run.completed_at_unix_ms,
            ],
        )?;
        transaction.execute(
            "UPDATE target_agents SET last_activity_at_unix_ms = ?1 WHERE id = ?2",
            params![timestamp, run.target_agent_id.to_string()],
        )?;
        bump_revision(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn snapshot(&self) -> Result<ApplicationProjection, StoreError> {
        let connection = self.connection.lock().expect("store lock poisoned");
        let revision = connection.query_row(
            "SELECT revision FROM app_state WHERE singleton = 1",
            [],
            |row| row.get::<_, u64>(0),
        )?;
        let (theme, notifications, inspector_percent, terminal_percent) = connection.query_row(
            "SELECT theme, native_notifications,
                    inspector_percent, terminal_percent
             FROM app_state WHERE singleton = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, bool>(1)?,
                    row.get::<_, u8>(2)?,
                    row.get::<_, u8>(3)?,
                ))
            },
        )?;

        let projects = query_projects(&connection)?;
        let llm_providers = query_llm_providers(&connection)?;
        let environments = query_environments(&connection)?;
        let agent_sessions = query_agent_sessions(&connection)?;
        let factory_runs = query_factory_runs(&connection)?;
        let target_workspace =
            query_target_workspace(&connection, &projects, &agent_sessions, &factory_runs)?;
        let (active_project_id, active_agent_session_id, active_run_id) =
            focused_workspace_selection(&target_workspace);
        Ok(ApplicationProjection {
            revision,
            settings: SettingsProjection {
                theme: parse_theme(&theme)?,
                native_notifications: notifications,
                layout: LayoutProjection {
                    inspector_percent,
                    terminal_percent,
                },
            },
            active_project_id,
            active_agent_session_id,
            active_run_id,
            projects,
            llm_providers,
            environments,
            herdr: HerdrStatusProjection::default(),
            harnesses: Vec::new(),
            agent_sessions,
            live_agents: Vec::new(),
            factory_runs,
            target_workspace,
        })
    }

    pub fn set_theme(&self, theme: ThemePreference) -> Result<SettingsProjection, StoreError> {
        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        transaction.execute(
            "UPDATE app_state SET theme = ?1 WHERE singleton = 1",
            [theme_name(theme)],
        )?;
        bump_revision(&transaction)?;
        transaction.commit()?;
        drop(connection);
        Ok(self.snapshot()?.settings)
    }

    pub fn set_native_notifications(
        &self,
        enabled: bool,
    ) -> Result<SettingsProjection, StoreError> {
        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        transaction.execute(
            "UPDATE app_state SET native_notifications = ?1 WHERE singleton = 1",
            [enabled],
        )?;
        bump_revision(&transaction)?;
        transaction.commit()?;
        drop(connection);
        Ok(self.snapshot()?.settings)
    }

    pub fn set_layout(
        &self,
        inspector_percent: u8,
        terminal_percent: u8,
    ) -> Result<SettingsProjection, StoreError> {
        if !(20..=50).contains(&inspector_percent) || !(14..=50).contains(&terminal_percent) {
            return Err(StoreError::InvalidLayout);
        }
        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        transaction.execute(
            "UPDATE app_state SET inspector_percent = ?1, terminal_percent = ?2
             WHERE singleton = 1",
            params![inspector_percent, terminal_percent],
        )?;
        bump_revision(&transaction)?;
        transaction.commit()?;
        drop(connection);
        Ok(self.snapshot()?.settings)
    }

    pub fn list_plugin_registries(&self) -> Result<Vec<PluginRegistryRecord>, StoreError> {
        let connection = self.connection.lock().expect("store lock poisoned");
        let mut statement = connection.prepare(
            "SELECT id, catalog_url, signature_url, public_key_base64
             FROM plugin_registries ORDER BY id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(PluginRegistryRecord {
                id: row.get(0)?,
                catalog_url: row.get(1)?,
                signature_url: row.get(2)?,
                public_key_base64: row.get(3)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn list_llm_providers(&self) -> Result<Vec<LlmProviderDto>, StoreError> {
        let connection = self.connection.lock().expect("store lock poisoned");
        query_llm_providers(&connection)
    }

    /// Saves one provider and marks every linked Environment for explicit setup
    /// in the same SQLite transaction. `affected_environment_ids` is empty for
    /// create and rename-only updates.
    pub fn put_llm_provider(
        &self,
        provider: &LlmProviderDto,
        affected_environment_ids: &[String],
    ) -> Result<(), StoreError> {
        let provider_json = serde_json::to_string(provider)?;
        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        let mut changed = transaction.execute(
            "INSERT INTO llm_providers(id, provider_json) VALUES (?1, ?2)
             ON CONFLICT(id) DO UPDATE SET provider_json = excluded.provider_json
             WHERE provider_json != excluded.provider_json",
            params![provider.id.to_string(), provider_json],
        )?;
        for environment_id in affected_environment_ids {
            changed += transaction.execute(
                "UPDATE environments SET llm_needs_setup = 1
                 WHERE id = ?1 AND llm_needs_setup = 0",
                [environment_id],
            )?;
        }
        if changed > 0 {
            bump_revision(&transaction)?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn delete_llm_provider(&self, provider_id: Uuid) -> Result<(), StoreError> {
        let mut connection = self.connection.lock().expect("store lock poisoned");
        let transaction = connection.transaction()?;
        let changed = transaction.execute(
            "DELETE FROM llm_providers WHERE id = ?1",
            [provider_id.to_string()],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!(
                "Intelligence Provider `{provider_id}`"
            )));
        }
        bump_revision(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn put_plugin_registry(&self, registry: &PluginRegistryRecord) -> Result<(), StoreError> {
        validate_registry_id(&registry.id)?;
        let connection = self.connection.lock().expect("store lock poisoned");
        connection.execute(
            "INSERT INTO plugin_registries(id, catalog_url, signature_url, public_key_base64)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET
               catalog_url = excluded.catalog_url,
               signature_url = excluded.signature_url,
               public_key_base64 = excluded.public_key_base64",
            params![
                registry.id,
                registry.catalog_url,
                registry.signature_url,
                registry.public_key_base64
            ],
        )?;
        Ok(())
    }

    pub fn delete_plugin_registry(&self, id: &str) -> Result<(), StoreError> {
        validate_registry_id(id)?;
        let connection = self.connection.lock().expect("store lock poisoned");
        let changed = connection.execute("DELETE FROM plugin_registries WHERE id = ?1", [id])?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("plugin registry {id}")));
        }
        Ok(())
    }

    pub fn local_mcp_trust(
        &self,
        environment_id: &str,
        plugin_name: &str,
        server_name: &str,
    ) -> Result<Option<LocalMcpTrustRecord>, StoreError> {
        let connection = self.connection.lock().expect("store lock poisoned");
        connection
            .query_row(
                "SELECT environment_id, plugin_name, server_name, fingerprint
                 FROM local_mcp_trust
                 WHERE environment_id = ?1 AND plugin_name = ?2 AND server_name = ?3",
                params![environment_id, plugin_name, server_name],
                |row| {
                    Ok(LocalMcpTrustRecord {
                        environment_id: row.get(0)?,
                        plugin_name: row.get(1)?,
                        server_name: row.get(2)?,
                        fingerprint: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn trust_local_mcp(&self, trust: &LocalMcpTrustRecord) -> Result<(), StoreError> {
        let connection = self.connection.lock().expect("store lock poisoned");
        require_text("plugin name", &trust.plugin_name, 64)?;
        require_text("MCP server name", &trust.server_name, 128)?;
        if trust.fingerprint.len() != 64
            || !trust
                .fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(StoreError::InvalidInput("invalid MCP fingerprint".into()));
        }
        require_entity_text(&connection, "environments", &trust.environment_id)?;
        connection.execute(
            "INSERT INTO local_mcp_trust(environment_id, plugin_name, server_name, fingerprint)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(environment_id, plugin_name, server_name)
             DO UPDATE SET fingerprint = excluded.fingerprint",
            params![
                trust.environment_id,
                trust.plugin_name,
                trust.server_name,
                trust.fingerprint
            ],
        )?;
        Ok(())
    }

    pub fn revoke_local_mcp_trust(
        &self,
        environment_id: &str,
        plugin_name: &str,
        server_name: &str,
    ) -> Result<(), StoreError> {
        let connection = self.connection.lock().expect("store lock poisoned");
        connection.execute(
            "DELETE FROM local_mcp_trust
             WHERE environment_id = ?1 AND plugin_name = ?2 AND server_name = ?3",
            params![environment_id, plugin_name, server_name],
        )?;
        Ok(())
    }
}

/// Drop every table so the current schema can be created from scratch.
///
/// This is a reset, not a migration: the shape on disk may be any older one, so
/// the tables are enumerated from the database rather than from a hand-written
/// list that drifts out of date the moment the schema changes. Foreign keys are
/// suspended for the duration because drop order across an unknown shape cannot
/// be known in advance.
fn reset_greenfield_schema(connection: &Connection) -> Result<(), StoreError> {
    let tables = {
        let mut statement = connection.prepare(
            "SELECT name FROM sqlite_master
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
        )?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    connection.execute_batch("PRAGMA foreign_keys = OFF;")?;
    let result = (|| -> Result<(), StoreError> {
        for table in &tables {
            // Table names come from sqlite_master, so they are already valid
            // identifiers; quoting keeps any unusual one safe.
            connection.execute_batch(&format!("DROP TABLE IF EXISTS \"{table}\";"))?;
        }
        connection.execute_batch("PRAGMA user_version = 0;")?;
        Ok(())
    })();
    connection.execute_batch("PRAGMA foreign_keys = ON;")?;
    result
}

/// Renders `?,?,?` for an `IN` clause. An empty list renders `NULL`, which no
/// value equals, so `id NOT IN (NULL)`… would match nothing — callers that need
/// "everything" semantics must therefore render a form that is never satisfied.
fn repeat_placeholders(count: usize) -> String {
    if count == 0 {
        // `x NOT IN (SELECT NULL WHERE 0)` is an empty set, so every row matches.
        return "SELECT NULL WHERE 0".into();
    }
    std::iter::repeat_n("?", count)
        .collect::<Vec<_>>()
        .join(", ")
}

fn upsert_environments(
    transaction: &Connection,
    environments: &[EnvironmentProjection],
) -> Result<usize, StoreError> {
    let mut changed = 0;
    for environment in environments {
        if environment.id.trim().is_empty() || environment.name.trim().is_empty() {
            return Err(StoreError::InvalidInput(
                "environment id and name must not be empty".into(),
            ));
        }
        let plugins = serde_json::to_string(&environment.plugins)?;
        let permissions = serde_json::to_string(&environment.permissions)?;
        let registries = serde_json::to_string(&environment.registry_ids)?;
        let environment_variables = serde_json::to_string(&environment.environment_variables)?;
        let llm_policy = environment
            .llm
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let resolved_llm = environment
            .resolved_llm
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let readiness = serde_json::to_string(&environment.readiness)?;
        changed += transaction.execute(
            "INSERT INTO environments(
               id, name, coding_harness_id, evaluation_harness_id,
               plugins_json, permissions_json, registry_ids_json,
               environment_variables_json, llm_policy_json, resolved_llm_json,
               llm_needs_setup, readiness_json, available
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 1)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               coding_harness_id = excluded.coding_harness_id,
               evaluation_harness_id = excluded.evaluation_harness_id,
               plugins_json = excluded.plugins_json,
               permissions_json = excluded.permissions_json,
               registry_ids_json = excluded.registry_ids_json,
               environment_variables_json = excluded.environment_variables_json,
               llm_policy_json = excluded.llm_policy_json,
               resolved_llm_json = excluded.resolved_llm_json,
               llm_needs_setup = excluded.llm_needs_setup,
               readiness_json = excluded.readiness_json,
               available = 1
             WHERE name != excluded.name
                OR coding_harness_id != excluded.coding_harness_id
                OR evaluation_harness_id != excluded.evaluation_harness_id
                OR plugins_json != excluded.plugins_json
                OR permissions_json != excluded.permissions_json
                OR registry_ids_json != excluded.registry_ids_json
                OR environment_variables_json != excluded.environment_variables_json
                OR llm_policy_json IS NOT excluded.llm_policy_json
                OR resolved_llm_json IS NOT excluded.resolved_llm_json
                OR llm_needs_setup != excluded.llm_needs_setup
                OR readiness_json != excluded.readiness_json
                OR available != 1",
            params![
                environment.id,
                environment.name,
                environment.coding_harness_id,
                environment.evaluation_harness_id,
                plugins,
                permissions,
                registries,
                environment_variables,
                llm_policy,
                resolved_llm,
                environment.llm_needs_setup,
                readiness,
            ],
        )?;
    }
    Ok(changed)
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn canonical_directory(path: &Path, label: &str) -> Result<PathBuf, StoreError> {
    if !path.is_absolute() || !path.is_dir() {
        return Err(StoreError::InvalidInput(format!(
            "{label} must be an existing absolute directory"
        )));
    }
    std::fs::canonicalize(path).map_err(StoreError::from)
}

fn require_uuid_entity(
    connection: &Connection,
    table: &'static str,
    id: Uuid,
) -> Result<(), StoreError> {
    let sql = match table {
        "target_agents" => "SELECT 1 FROM target_agents WHERE id = ?1 AND archived = 0",
        "target_agent_versions" => "SELECT 1 FROM target_agent_versions WHERE id = ?1",
        "target_agent_drafts" => "SELECT 1 FROM target_agent_drafts WHERE id = ?1",
        "workspace_bindings" => "SELECT 1 FROM workspace_bindings WHERE id = ?1 AND archived = 0",
        "agent_sessions" => "SELECT 1 FROM agent_sessions WHERE id = ?1",
        _ => return Err(StoreError::InvalidInput("invalid entity table".into())),
    };
    connection
        .query_row(sql, [id.to_string()], |_| Ok(()))
        .optional()?
        .ok_or_else(|| StoreError::NotFound(format!("{table} `{id}`")))
}

fn project_name(projects: &[ProjectProjection], id: Uuid) -> String {
    projects
        .iter()
        .find(|project| project.id == id)
        .map(|project| project.name.clone())
        .unwrap_or_else(|| "Unknown project".into())
}

const fn work_item_rank(kind: TargetWorkItemKind) -> u8 {
    match kind {
        TargetWorkItemKind::AgentDraft => 0,
        TargetWorkItemKind::OrchestrationThread => 1,
        TargetWorkItemKind::CodingThread => 2,
        TargetWorkItemKind::EvaluationThread => 3,
        TargetWorkItemKind::FactoryRun => 4,
    }
}

const fn purpose_name(value: HarnessPurpose) -> &'static str {
    match value {
        HarnessPurpose::Orchestration => "orchestration",
        HarnessPurpose::Coding => "coding",
        HarnessPurpose::Evaluation => "evaluation",
    }
}

fn parse_purpose(value: &str) -> Result<HarnessPurpose, StoreError> {
    match value {
        "orchestration" => Ok(HarnessPurpose::Orchestration),
        "coding" => Ok(HarnessPurpose::Coding),
        "evaluation" => Ok(HarnessPurpose::Evaluation),
        _ => Err(StoreError::Corrupt(format!(
            "unknown harness purpose `{value}`"
        ))),
    }
}

fn parse_workspace_dock(value: &str) -> Result<WorkspaceDock, StoreError> {
    match value {
        "closed" => Ok(WorkspaceDock::Closed),
        "terminal" => Ok(WorkspaceDock::Terminal),
        _ => Err(StoreError::Corrupt(format!(
            "unknown workspace dock `{value}`"
        ))),
    }
}

fn validate_work_item_context(
    connection: &Connection,
    target_agent_id: Uuid,
    workspace_binding_id: Uuid,
    work_item_id: Option<Uuid>,
    work_item_kind: Option<TargetWorkItemKind>,
) -> Result<(), StoreError> {
    let (Some(work_item_id), Some(work_item_kind)) = (work_item_id, work_item_kind) else {
        return Ok(());
    };
    let exists = match work_item_kind {
        TargetWorkItemKind::AgentDraft => connection
            .query_row(
                "SELECT 1 FROM target_agent_drafts
                 WHERE id = ?1 AND target_agent_id = ?2 AND workspace_binding_id = ?3",
                params![
                    work_item_id.to_string(),
                    target_agent_id.to_string(),
                    workspace_binding_id.to_string()
                ],
                |_| Ok(()),
            )
            .optional()?
            .is_some(),
        TargetWorkItemKind::OrchestrationThread
        | TargetWorkItemKind::CodingThread
        | TargetWorkItemKind::EvaluationThread => connection
            .query_row(
                "SELECT 1 FROM agent_sessions s
                 JOIN workspace_bindings b ON b.id = s.workspace_binding_id
                 WHERE s.id = ?1 AND b.target_agent_id = ?2
                   AND s.workspace_binding_id = ?3 AND s.purpose = ?4",
                params![
                    work_item_id.to_string(),
                    target_agent_id.to_string(),
                    workspace_binding_id.to_string(),
                    match work_item_kind {
                        TargetWorkItemKind::OrchestrationThread => "orchestration",
                        TargetWorkItemKind::CodingThread => "coding",
                        _ => "evaluation",
                    },
                ],
                |_| Ok(()),
            )
            .optional()?
            .is_some(),
        TargetWorkItemKind::FactoryRun => connection
            .query_row(
                "SELECT 1 FROM factory_runs r
                 JOIN workspace_bindings b ON b.id = r.workspace_binding_id
                 WHERE r.id = ?1 AND b.target_agent_id = ?2
                   AND r.workspace_binding_id = ?3",
                params![
                    work_item_id.to_string(),
                    target_agent_id.to_string(),
                    workspace_binding_id.to_string(),
                ],
                |_| Ok(()),
            )
            .optional()?
            .is_some(),
    };
    if exists {
        Ok(())
    } else {
        Err(StoreError::InvalidInput(
            "work item does not belong to the target workspace".into(),
        ))
    }
}

fn validate_workspace_binding_identity(
    connection: &Connection,
    workspace_binding_id: Uuid,
    target_agent_id: Uuid,
    project_id: Uuid,
) -> Result<(), StoreError> {
    let identity = connection
        .query_row(
            "SELECT target_agent_id, project_id FROM workspace_bindings WHERE id = ?1",
            [workspace_binding_id.to_string()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
        .ok_or_else(|| {
            StoreError::NotFound(format!("workspace binding `{workspace_binding_id}`"))
        })?;
    if parse_uuid(&identity.0)? != target_agent_id || parse_uuid(&identity.1)? != project_id {
        return Err(StoreError::InvalidInput(
            "workspace binding does not match the target agent and project".into(),
        ));
    }
    Ok(())
}

fn require_pane(connection: &Connection, pane_id: Uuid) -> Result<(), StoreError> {
    connection
        .query_row(
            "SELECT 1 FROM workspace_panes WHERE id = ?1",
            [pane_id.to_string()],
            |_| Ok(()),
        )
        .optional()?
        .ok_or_else(|| StoreError::NotFound(format!("workspace pane `{pane_id}`")))
}

const fn workspace_dock_name(value: WorkspaceDock) -> &'static str {
    match value {
        WorkspaceDock::Closed => "closed",
        WorkspaceDock::Terminal => "terminal",
    }
}

fn validate_registry_id(value: &str) -> Result<(), StoreError> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(StoreError::InvalidInput(
            "invalid plugin registry id".into(),
        ));
    }
    Ok(())
}

fn require_text(label: &str, value: &str, max_bytes: usize) -> Result<(), StoreError> {
    if value.is_empty()
        || value.len() > max_bytes
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(StoreError::InvalidInput(format!("invalid {label}")));
    }
    Ok(())
}

fn validate_semver(value: &str) -> Result<(), StoreError> {
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() != 3
        || parts
            .iter()
            .any(|part| part.is_empty() || part.parse::<u32>().is_err())
    {
        return Err(StoreError::InvalidInput(
            "Agent version must use major.minor.patch".into(),
        ));
    }
    Ok(())
}

fn require_entity_text(connection: &Connection, table: &str, id: &str) -> Result<(), StoreError> {
    let query = match table {
        "environments" => "SELECT 1 FROM environments WHERE id = ?1",
        _ => return Err(StoreError::InvalidInput("invalid entity table".into())),
    };
    connection
        .query_row(query, [id], |_| Ok(()))
        .optional()?
        .ok_or_else(|| StoreError::NotFound(format!("{table} {id}")))
}

const fn theme_name(theme: ThemePreference) -> &'static str {
    match theme {
        ThemePreference::System => "system",
        ThemePreference::Light => "light",
        ThemePreference::Dark => "dark",
    }
}

fn parse_theme(value: &str) -> Result<ThemePreference, StoreError> {
    match value {
        "system" => Ok(ThemePreference::System),
        "light" => Ok(ThemePreference::Light),
        "dark" => Ok(ThemePreference::Dark),
        _ => Err(StoreError::Corrupt(format!(
            "unknown theme preference `{value}`"
        ))),
    }
}

fn require_entity(
    connection: &Connection,
    table: &'static str,
    id: Uuid,
) -> Result<(), StoreError> {
    let sql = match table {
        "projects" => "SELECT 1 FROM projects WHERE id = ?1",
        _ => unreachable!("table is fixed by caller"),
    };
    if connection
        .query_row(sql, [id.to_string()], |_| Ok(()))
        .optional()?
        .is_none()
    {
        return Err(StoreError::NotFound(format!("{table} `{id}`")));
    }
    Ok(())
}

fn bump_revision(connection: &Connection) -> Result<(), rusqlite::Error> {
    connection.execute(
        "UPDATE app_state SET revision = revision + 1 WHERE singleton = 1",
        [],
    )?;
    Ok(())
}

fn query_projects(connection: &Connection) -> Result<Vec<ProjectProjection>, StoreError> {
    let mut statement =
        connection.prepare("SELECT id, name, root, trusted FROM projects ORDER BY name, id")?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, bool>(3)?,
        ))
    })?;
    rows.map(|row| {
        let (id, name, root, trusted) = row?;
        Ok(ProjectProjection {
            id: parse_uuid(&id)?,
            name,
            root: PathBuf::from(root),
            trusted,
        })
    })
    .collect()
}

fn query_targets(connection: &Connection) -> Result<Vec<TargetAgentProjection>, StoreError> {
    let mut statement = connection.prepare(
        "SELECT id, name, repository_root, archived, last_activity_at_unix_ms
         FROM target_agents WHERE archived = 0
         ORDER BY last_activity_at_unix_ms DESC, name, id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, bool>(3)?,
            row.get::<_, u64>(4)?,
        ))
    })?;
    rows.map(|row| {
        let (id, name, repository_root, archived, last_activity_at_unix_ms) = row?;
        Ok(TargetAgentProjection {
            id: parse_uuid(&id)?,
            name,
            repository_root: PathBuf::from(repository_root),
            archived,
            last_activity_at_unix_ms,
        })
    })
    .collect()
}

fn query_agent_drafts(connection: &Connection) -> Result<Vec<AgentDraftProjection>, StoreError> {
    let mut statement = connection.prepare(
        "SELECT id, target_agent_id, workspace_binding_id, name, objective,
                acceptance_criteria_json, base_version, branch_ref, worktree_path,
                git_head, lifecycle, cleanup_guidance, environment_id,
                created_at_unix_ms, updated_at_unix_ms
         FROM target_agent_drafts ORDER BY updated_at_unix_ms DESC, id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
            row.get::<_, String>(9)?,
            row.get::<_, String>(10)?,
            row.get::<_, Option<String>>(11)?,
            row.get::<_, Option<String>>(12)?,
            row.get::<_, u64>(13)?,
            row.get::<_, u64>(14)?,
        ))
    })?;
    rows.map(|row| parse_agent_draft_row(row?)).collect()
}

type AgentDraftRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    u64,
    u64,
);

fn parse_agent_draft_row(row: AgentDraftRow) -> Result<AgentDraftProjection, StoreError> {
    Ok(AgentDraftProjection {
        id: parse_uuid(&row.0)?,
        target_agent_id: parse_uuid(&row.1)?,
        workspace_binding_id: parse_uuid(&row.2)?,
        name: row.3,
        objective: row.4,
        acceptance_criteria: serde_json::from_str(&row.5)?,
        base_version: row.6,
        branch_ref: row.7,
        worktree_path: PathBuf::from(row.8),
        git_head: row.9,
        lifecycle: parse_draft_lifecycle(&row.10)?,
        cleanup_guidance: row.11,
        environment_id: row.12,
        created_at_unix_ms: row.13,
        updated_at_unix_ms: row.14,
    })
}

fn query_agent_draft_row(
    connection: &Connection,
    draft_id: Uuid,
) -> Result<AgentDraftProjection, StoreError> {
    query_agent_drafts(connection)?
        .into_iter()
        .find(|draft| draft.id == draft_id)
        .ok_or_else(|| StoreError::NotFound(format!("Agent Draft `{draft_id}`")))
}

fn query_target_agent_versions(
    connection: &Connection,
) -> Result<Vec<TargetAgentVersionProjection>, StoreError> {
    let mut statement = connection.prepare(
        "SELECT id, target_agent_id, version, name, objective,
                acceptance_criteria_json, source_draft_id, git_commit, git_tag,
                created_at_unix_ms
         FROM target_agent_versions
         ORDER BY created_at_unix_ms DESC, id DESC",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
            row.get::<_, u64>(9)?,
        ))
    })?;
    rows.map(|row| {
        let (
            id,
            target_agent_id,
            version,
            name,
            objective,
            criteria,
            source_draft_id,
            git_commit,
            git_tag,
            created_at_unix_ms,
        ) = row?;
        Ok(TargetAgentVersionProjection {
            id: parse_uuid(&id)?,
            target_agent_id: parse_uuid(&target_agent_id)?,
            version,
            name,
            objective,
            acceptance_criteria: serde_json::from_str(&criteria)?,
            source_draft_id: parse_uuid(&source_draft_id)?,
            git_commit,
            git_tag,
            created_at_unix_ms,
        })
    })
    .collect()
}

fn query_workspace_binding(
    connection: &Connection,
    id: Uuid,
) -> Result<Option<WorkspaceBindingProjection>, StoreError> {
    connection
        .query_row(
            "SELECT id, target_agent_id, project_id, name, primary_root,
                    additional_roots_json, source_ref_label, archived,
                    last_used_at_unix_ms
             FROM workspace_bindings WHERE id = ?1",
            [id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, bool>(7)?,
                    row.get::<_, u64>(8)?,
                ))
            },
        )
        .optional()?
        .map(parse_workspace_binding_row)
        .transpose()
}

fn query_workspace_bindings(
    connection: &Connection,
) -> Result<Vec<WorkspaceBindingProjection>, StoreError> {
    let mut statement = connection.prepare(
        "SELECT id, target_agent_id, project_id, name, primary_root,
                additional_roots_json, source_ref_label, archived,
                last_used_at_unix_ms
         FROM workspace_bindings WHERE archived = 0
         ORDER BY last_used_at_unix_ms DESC, name, id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, bool>(7)?,
            row.get::<_, u64>(8)?,
        ))
    })?;
    rows.map(|row| parse_workspace_binding_row(row?)).collect()
}

type WorkspaceBindingRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    bool,
    u64,
);

fn parse_workspace_binding_row(
    row: WorkspaceBindingRow,
) -> Result<WorkspaceBindingProjection, StoreError> {
    let (
        id,
        target_agent_id,
        project_id,
        name,
        primary_root,
        additional_roots,
        source_ref_label,
        archived,
        last_used_at_unix_ms,
    ) = row;
    Ok(WorkspaceBindingProjection {
        id: parse_uuid(&id)?,
        target_agent_id: parse_uuid(&target_agent_id)?,
        project_id: parse_uuid(&project_id)?,
        name,
        primary_root: PathBuf::from(primary_root),
        additional_roots: from_json(additional_roots)?,
        source_ref_label,
        archived,
        last_used_at_unix_ms,
    })
}

/// Column list shared by every `agent_sessions` read.
const AGENT_SESSION_COLUMNS: &str = "s.id, b.target_agent_id, s.workspace_binding_id, b.project_id, \
     s.environment_id, s.harness_id, s.purpose, s.factory_run_id, s.parent_session_id, \
     s.herdr_agent_name, s.title, s.created_at_unix_ms, s.last_activity_at_unix_ms, \
     s.llm_provider_snapshot_json, s.effective_model, s.initial_prompt, \
     s.brief_delivered, s.outcome_json";

type AgentSessionRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
    u64,
    u64,
    Option<String>,
    Option<String>,
    Option<String>,
    bool,
    Option<String>,
);

fn agent_session_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentSessionRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
        row.get(10)?,
        row.get(11)?,
        row.get(12)?,
        row.get(13)?,
        row.get(14)?,
        row.get(15)?,
        row.get(16)?,
        row.get(17)?,
    ))
}

fn parse_agent_session_row(row: AgentSessionRow) -> Result<AgentSessionProjection, StoreError> {
    let (
        id,
        target_agent_id,
        workspace_binding_id,
        project_id,
        environment_id,
        harness_id,
        purpose,
        factory_run_id,
        parent_session_id,
        herdr_agent_name,
        title,
        created_at_unix_ms,
        last_activity_at_unix_ms,
        llm_provider_snapshot,
        effective_model,
        initial_prompt,
        brief_delivered,
        outcome,
    ) = row;
    Ok(AgentSessionProjection {
        id: parse_uuid(&id)?,
        target_agent_id: parse_uuid(&target_agent_id)?,
        workspace_binding_id: parse_uuid(&workspace_binding_id)?,
        project_id: parse_uuid(&project_id)?,
        environment_id,
        harness_id,
        purpose: parse_purpose(&purpose)?,
        factory_run_id: optional_uuid(factory_run_id)?,
        parent_session_id: optional_uuid(parent_session_id)?,
        herdr_agent_name,
        availability: SessionAvailability::Historical,
        lifecycle: None,
        placement: None,
        title,
        created_at_unix_ms,
        last_activity_at_unix_ms,
        llm_provider_snapshot: optional_from_json(llm_provider_snapshot)?,
        effective_model,
        attention: Vec::new(),
        initial_prompt,
        brief_delivered,
        outcome: optional_from_json::<ManagedSessionOutcome>(outcome)?,
    })
}

fn query_agent_sessions(
    connection: &Connection,
) -> Result<Vec<AgentSessionProjection>, StoreError> {
    let mut statement = connection.prepare(&format!(
        "SELECT {AGENT_SESSION_COLUMNS}
         FROM agent_sessions s
         JOIN workspace_bindings b ON b.id = s.workspace_binding_id
         ORDER BY s.created_at_unix_ms"
    ))?;
    let rows = statement.query_map([], agent_session_row)?;
    let mut projected = Vec::new();
    for row in rows {
        projected.push(parse_agent_session_row(row?)?);
    }
    Ok(projected)
}

fn query_target_workspace(
    connection: &Connection,
    projects: &[ProjectProjection],
    agent_sessions: &[AgentSessionProjection],
    factory_runs: &[FactoryRun],
) -> Result<TargetWorkspaceProjection, StoreError> {
    let targets = query_targets(connection)?;
    let drafts = query_agent_drafts(connection)?;
    let versions = query_target_agent_versions(connection)?;
    let bindings = query_workspace_bindings(connection)?;
    let mut run_activity =
        connection.prepare("SELECT id, last_activity_at_unix_ms FROM factory_runs ORDER BY id")?;
    let run_activity = run_activity
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
        })?
        .collect::<rusqlite::Result<BTreeMap<_, _>>>()?;
    let mut target_groups = Vec::with_capacity(targets.len());
    for target in targets {
        let target_bindings = bindings
            .iter()
            .filter(|binding| binding.target_agent_id == target.id)
            .cloned()
            .collect::<Vec<_>>();
        let mut work_items = Vec::new();
        for draft in drafts.iter().filter(|draft| {
            draft.target_agent_id == target.id
                && matches!(
                    draft.lifecycle,
                    AgentDraftLifecycle::Active
                        | AgentDraftLifecycle::Publishing
                        | AgentDraftLifecycle::CleanupRequired
                )
        }) {
            let binding = bindings
                .iter()
                .find(|binding| binding.id == draft.workspace_binding_id)
                .ok_or_else(|| {
                    StoreError::Corrupt(format!("missing binding for Draft `{}`", draft.id))
                })?;
            work_items.push(TargetWorkItemProjection {
                id: draft.id,
                kind: TargetWorkItemKind::AgentDraft,
                target_agent_id: target.id,
                workspace_binding_id: binding.id,
                project_id: binding.project_id,
                agent_draft_id: Some(draft.id),
                title: draft.name.clone(),
                status: draft_lifecycle_name(draft.lifecycle).into(),
                last_activity_at_unix_ms: draft.updated_at_unix_ms,
                project_label: project_name(projects, binding.project_id),
                workspace_label: binding.name.clone(),
                source_ref_label: Some(draft.branch_ref.clone()),
            });
        }
        for session in agent_sessions.iter().filter(|session| {
            session.target_agent_id == target.id && session.factory_run_id.is_none()
        }) {
            if let Some(binding) = bindings
                .iter()
                .find(|binding| binding.id == session.workspace_binding_id)
            {
                work_items.push(TargetWorkItemProjection {
                    id: session.id,
                    kind: match session.purpose {
                        HarnessPurpose::Orchestration => TargetWorkItemKind::OrchestrationThread,
                        HarnessPurpose::Coding => TargetWorkItemKind::CodingThread,
                        HarnessPurpose::Evaluation => TargetWorkItemKind::EvaluationThread,
                    },
                    target_agent_id: target.id,
                    workspace_binding_id: binding.id,
                    project_id: binding.project_id,
                    agent_draft_id: None,
                    title: session.title.clone(),
                    status: if session.outcome.is_some() {
                        "historical"
                    } else {
                        "managed"
                    }
                    .into(),
                    last_activity_at_unix_ms: session.last_activity_at_unix_ms,
                    project_label: project_name(projects, binding.project_id),
                    workspace_label: binding.name.clone(),
                    source_ref_label: binding.source_ref_label.clone(),
                });
            }
        }
        for run in factory_runs
            .iter()
            .filter(|run| run.target_agent_id == target.id)
        {
            let Some(binding) = bindings
                .iter()
                .find(|binding| binding.id == run.workspace_binding_id)
            else {
                continue;
            };
            work_items.push(TargetWorkItemProjection {
                id: run.id,
                kind: TargetWorkItemKind::FactoryRun,
                target_agent_id: target.id,
                workspace_binding_id: binding.id,
                project_id: binding.project_id,
                agent_draft_id: Some(run.agent_draft_id),
                title: run.objective.clone(),
                status: run_state_name(run.state).into(),
                last_activity_at_unix_ms: *run_activity.get(&run.id.to_string()).ok_or_else(
                    || StoreError::Corrupt(format!("missing activity for run `{}`", run.id)),
                )?,
                project_label: project_name(projects, binding.project_id),
                workspace_label: binding.name.clone(),
                source_ref_label: binding.source_ref_label.clone(),
            });
        }
        work_items.sort_by(|left, right| {
            right
                .last_activity_at_unix_ms
                .cmp(&left.last_activity_at_unix_ms)
                .then_with(|| work_item_rank(left.kind).cmp(&work_item_rank(right.kind)))
                .then_with(|| left.id.cmp(&right.id))
        });
        let mut target_versions = versions
            .iter()
            .filter(|version| version.target_agent_id == target.id)
            .cloned()
            .collect::<Vec<_>>();
        target_versions.sort_by(|left, right| {
            semver::Version::parse(&right.version)
                .ok()
                .cmp(&semver::Version::parse(&left.version).ok())
        });
        target_groups.push(TargetAgentWorkGroupProjection {
            drafts: drafts
                .iter()
                .filter(|draft| draft.target_agent_id == target.id)
                .cloned()
                .collect(),
            versions: target_versions,
            target_agent: target,
            workspace_bindings: target_bindings,
            work_items,
        });
    }
    let work_contexts = query_work_contexts(connection)?;
    let panes = query_workspace_panes(connection)?;
    let terminals = query_workspace_terminals(connection)?;
    let focused_pane_id = connection.query_row(
        "SELECT focused_pane_id FROM app_state WHERE singleton = 1",
        [],
        |row| row.get::<_, Option<String>>(0),
    )?;
    Ok(TargetWorkspaceProjection {
        target_groups,
        work_contexts,
        panes,
        terminals,
        focused_pane_id: optional_uuid(focused_pane_id)?,
    })
}

fn focused_workspace_selection(
    workspace: &TargetWorkspaceProjection,
) -> (Option<Uuid>, Option<Uuid>, Option<Uuid>) {
    let context = workspace
        .focused_pane_id
        .and_then(|focused| workspace.panes.iter().find(|pane| pane.id == focused))
        .and_then(|pane| {
            workspace
                .work_contexts
                .iter()
                .find(|context| context.id == pane.work_context_id)
        });
    let Some(context) = context else {
        return (None, None, None);
    };
    let project_id = workspace
        .target_groups
        .iter()
        .flat_map(|group| &group.workspace_bindings)
        .find(|binding| binding.id == context.workspace_binding_id)
        .map(|binding| binding.project_id);
    match (context.work_item_id, context.work_item_kind) {
        (
            Some(session_id),
            Some(TargetWorkItemKind::CodingThread | TargetWorkItemKind::EvaluationThread),
        ) => (project_id, Some(session_id), None),
        (Some(run_id), Some(TargetWorkItemKind::FactoryRun)) => (project_id, None, Some(run_id)),
        _ => (project_id, None, None),
    }
}

fn query_workspace_terminals(
    connection: &Connection,
) -> Result<Vec<WorkspaceTerminalProjection>, StoreError> {
    let mut statement = connection.prepare(
        "SELECT wt.id, wt.work_context_id, wc.workspace_binding_id, wt.title,
                wt.state, wt.created_at_unix_ms
         FROM workspace_terminals wt
         JOIN work_contexts wc ON wc.id = wt.work_context_id
         ORDER BY wt.created_at_unix_ms, wt.id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, u64>(5)?,
        ))
    })?;
    rows.map(|row| {
        let (id, context, binding, title, state, created) = row?;
        Ok(WorkspaceTerminalProjection {
            id: parse_uuid(&id)?,
            work_context_id: parse_uuid(&context)?,
            workspace_binding_id: parse_uuid(&binding)?,
            title,
            state: match state.as_str() {
                "running" => WorkspaceTerminalState::Running,
                "exited" => WorkspaceTerminalState::Exited,
                _ => {
                    return Err(StoreError::Corrupt(format!(
                        "invalid terminal state `{state}`"
                    )));
                }
            },
            created_at_unix_ms: created,
        })
    })
    .collect()
}

fn query_work_contexts(connection: &Connection) -> Result<Vec<WorkContextProjection>, StoreError> {
    let mut statement = connection.prepare(
        "SELECT wc.id, b.target_agent_id, wc.workspace_binding_id,
                wc.agent_draft_id, wc.agent_session_id, s.purpose,
                wc.factory_run_id,
                wc.dock, wc.dock_percent, wc.last_viewed_at_unix_ms
         FROM work_contexts wc
         JOIN workspace_bindings b ON b.id = wc.workspace_binding_id
         LEFT JOIN agent_sessions s ON s.id = wc.agent_session_id
         ORDER BY wc.last_viewed_at_unix_ms DESC, wc.id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, u8>(8)?,
            row.get::<_, u64>(9)?,
        ))
    })?;
    rows.map(|row| {
        let (id, target, binding, draft, session, purpose, run, dock, dock_percent, viewed) = row?;
        let (work_item_id, work_item_kind) = match (&draft, session, purpose, run) {
            (Some(draft), None, None, None) => (
                Some(parse_uuid(draft)?),
                Some(TargetWorkItemKind::AgentDraft),
            ),
            (None, Some(session), Some(purpose), None) => (
                Some(parse_uuid(&session)?),
                Some(match parse_purpose(&purpose)? {
                    HarnessPurpose::Orchestration => TargetWorkItemKind::OrchestrationThread,
                    HarnessPurpose::Coding => TargetWorkItemKind::CodingThread,
                    HarnessPurpose::Evaluation => TargetWorkItemKind::EvaluationThread,
                }),
            ),
            (None, None, None, Some(run)) => (
                Some(parse_uuid(&run)?),
                Some(TargetWorkItemKind::FactoryRun),
            ),
            (None, None, None, None) => (None, None),
            _ => {
                return Err(StoreError::Corrupt(format!(
                    "work context `{id}` has inconsistent work-item references"
                )));
            }
        };
        Ok(WorkContextProjection {
            id: parse_uuid(&id)?,
            target_agent_id: parse_uuid(&target)?,
            workspace_binding_id: parse_uuid(&binding)?,
            agent_draft_id: draft.map(|id| parse_uuid(&id)).transpose()?,
            work_item_id,
            work_item_kind,
            dock: parse_workspace_dock(&dock)?,
            dock_percent,
            last_viewed_at_unix_ms: viewed,
        })
    })
    .collect()
}

fn query_workspace_panes(
    connection: &Connection,
) -> Result<Vec<WorkspacePaneProjection>, StoreError> {
    let mut statement = connection.prepare(
        "SELECT id, work_context_id, position, width_basis_points
         FROM workspace_panes ORDER BY position",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, u8>(2)?,
            row.get::<_, u16>(3)?,
        ))
    })?;
    rows.map(|row| {
        let (id, context, position, width_basis_points) = row?;
        Ok(WorkspacePaneProjection {
            id: parse_uuid(&id)?,
            work_context_id: parse_uuid(&context)?,
            position,
            width_basis_points,
        })
    })
    .collect()
}

fn query_workspace_pane_layout(connection: &Connection) -> Result<Vec<(Uuid, u16)>, StoreError> {
    let mut statement = connection
        .prepare("SELECT id, width_basis_points FROM workspace_panes ORDER BY position")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, u16>(1)?))
    })?;
    rows.map(|row| {
        let (id, width) = row?;
        Ok((parse_uuid(&id)?, width))
    })
    .collect()
}

fn scale_workspace_widths(layout: &[(Uuid, u16)], target_total: u16) -> Vec<(Uuid, u16)> {
    let source_total = layout
        .iter()
        .map(|(_, width)| u32::from(*width))
        .sum::<u32>();
    let mut assigned = 0u16;
    layout
        .iter()
        .enumerate()
        .map(|(index, (id, width))| {
            let scaled = if index + 1 == layout.len() {
                target_total - assigned
            } else {
                ((u32::from(*width) * u32::from(target_total)) / source_total) as u16
            };
            assigned += scaled;
            (*id, scaled)
        })
        .collect()
}

fn update_workspace_widths(
    transaction: &rusqlite::Transaction<'_>,
    layout: &[(Uuid, u16)],
) -> Result<(), StoreError> {
    for (id, width) in layout {
        transaction.execute(
            "UPDATE workspace_panes SET width_basis_points = ?1 WHERE id = ?2",
            params![width, id.to_string()],
        )?;
    }
    Ok(())
}

fn query_environments(connection: &Connection) -> Result<Vec<EnvironmentProjection>, StoreError> {
    let mut statement = connection.prepare(
        "SELECT id, name, coding_harness_id, evaluation_harness_id,
                plugins_json, permissions_json, registry_ids_json,
                environment_variables_json, llm_policy_json, resolved_llm_json,
                llm_needs_setup, readiness_json
         FROM environments WHERE available = 1 ORDER BY name, id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, Option<String>>(8)?,
            row.get::<_, Option<String>>(9)?,
            row.get::<_, bool>(10)?,
            row.get::<_, String>(11)?,
        ))
    })?;
    rows.map(|row| {
        let (
            id,
            name,
            coding_harness_id,
            evaluation_harness_id,
            plugins,
            permissions,
            registries,
            environment,
            llm_policy,
            resolved_llm,
            llm_needs_setup,
            readiness,
        ) = row?;
        Ok(EnvironmentProjection {
            id,
            name,
            coding_harness_id,
            evaluation_harness_id,
            plugins: from_json::<Vec<EnvironmentPluginProjection>>(plugins)?,
            permissions: from_json::<EnvironmentPermissionProjection>(permissions)?,
            registry_ids: from_json::<Vec<String>>(registries)?,
            environment_variables: from_json::<Vec<EnvironmentVariableProjection>>(environment)?,
            llm: llm_policy
                .map(from_json::<EnvironmentLlmPolicyDto>)
                .transpose()?,
            resolved_llm: resolved_llm
                .map(from_json::<ResolvedLlmProviderDto>)
                .transpose()?,
            llm_needs_setup,
            readiness: from_json::<EnvironmentReadinessProjection>(readiness)?,
        })
    })
    .collect()
}

fn query_llm_providers(connection: &Connection) -> Result<Vec<LlmProviderDto>, StoreError> {
    let mut statement =
        connection.prepare("SELECT provider_json FROM llm_providers ORDER BY provider_json")?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    let mut providers = rows
        .map(|row| from_json::<LlmProviderDto>(row?))
        .collect::<Result<Vec<_>, _>>()?;
    providers.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(providers)
}

fn query_factory_runs(connection: &Connection) -> Result<Vec<FactoryRun>, StoreError> {
    let mut statement = connection.prepare(
        "SELECT r.id, b.target_agent_id, r.agent_draft_id,
                r.workspace_binding_id, b.project_id, r.environment_id,
                r.objective, r.acceptance_criteria_json,
                r.starting_git_head, r.final_git_head, changed_files_json,
                test_evidence_json, evaluation_json, state, escalation,
                completed_at_unix_ms
         FROM factory_runs r
         JOIN workspace_bindings b ON b.id = r.workspace_binding_id
         ORDER BY r.id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
            row.get::<_, Option<String>>(9)?,
            row.get::<_, String>(10)?,
            row.get::<_, String>(11)?,
            row.get::<_, Option<String>>(12)?,
            row.get::<_, String>(13)?,
            row.get::<_, Option<String>>(14)?,
            row.get::<_, Option<u64>>(15)?,
        ))
    })?;
    rows.map(|row| {
        let (
            id,
            target_agent_id,
            agent_draft_id,
            workspace_binding_id,
            project_id,
            environment_id,
            objective,
            acceptance_criteria,
            starting_git_head,
            final_git_head,
            changed_files,
            test_evidence,
            evaluation,
            state,
            escalation,
            completed_at_unix_ms,
        ) = row?;
        Ok(FactoryRun {
            id: parse_uuid(&id)?,
            target_agent_id: parse_uuid(&target_agent_id)?,
            agent_draft_id: parse_uuid(&agent_draft_id)?,
            workspace_binding_id: parse_uuid(&workspace_binding_id)?,
            project_id: parse_uuid(&project_id)?,
            environment_id,
            objective,
            acceptance_criteria: serde_json::from_str(&acceptance_criteria)?,
            starting_git_head,
            final_git_head,
            changed_files: serde_json::from_str::<Vec<ChangedFile>>(&changed_files)?,
            test_evidence: serde_json::from_str::<Vec<TestEvidence>>(&test_evidence)?,
            evaluation: optional_from_json::<EvaluationResult>(evaluation)?,
            state: parse_run_state(&state)?,
            escalation,
            completed_at_unix_ms,
        })
    })
    .collect()
}

fn optional_json<T: serde::Serialize>(value: &Option<T>) -> Result<Option<String>, StoreError> {
    value
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(StoreError::from)
}

fn from_json<T: serde::de::DeserializeOwned>(value: String) -> Result<T, StoreError> {
    serde_json::from_str(&value).map_err(StoreError::from)
}

fn optional_from_json<T: serde::de::DeserializeOwned>(
    value: Option<String>,
) -> Result<Option<T>, StoreError> {
    value
        .map(|value| serde_json::from_str(&value))
        .transpose()
        .map_err(StoreError::from)
}

fn optional_uuid(value: Option<String>) -> Result<Option<Uuid>, StoreError> {
    value.map(|value| parse_uuid(&value)).transpose()
}

fn parse_uuid(value: &str) -> Result<Uuid, StoreError> {
    Uuid::parse_str(value)
        .map_err(|error| StoreError::Corrupt(format!("invalid UUID `{value}`: {error}")))
}

const fn draft_lifecycle_name(value: AgentDraftLifecycle) -> &'static str {
    match value {
        AgentDraftLifecycle::Active => "active",
        AgentDraftLifecycle::Publishing => "publishing",
        AgentDraftLifecycle::Archived => "archived",
        AgentDraftLifecycle::CleanupRequired => "cleanup_required",
    }
}

fn parse_draft_lifecycle(value: &str) -> Result<AgentDraftLifecycle, StoreError> {
    match value {
        "active" => Ok(AgentDraftLifecycle::Active),
        "publishing" => Ok(AgentDraftLifecycle::Publishing),
        "archived" => Ok(AgentDraftLifecycle::Archived),
        "cleanup_required" => Ok(AgentDraftLifecycle::CleanupRequired),
        _ => Err(StoreError::Corrupt(format!(
            "unknown Draft lifecycle `{value}`"
        ))),
    }
}

fn validate_draft_definition(
    name: &str,
    objective: &str,
    acceptance_criteria: &[String],
) -> Result<Vec<String>, StoreError> {
    require_text("Agent name", name.trim(), 200)?;
    require_text("Agent objective", objective.trim(), 16 * 1024)?;
    if acceptance_criteria.is_empty() {
        return Err(StoreError::InvalidInput(
            "at least one Agent success criterion is required".into(),
        ));
    }
    acceptance_criteria
        .iter()
        .map(|criterion| {
            let criterion = criterion.trim();
            require_text("Agent success criterion", criterion, 16 * 1024)?;
            Ok(criterion.to_owned())
        })
        .collect()
}

fn run_state_name(value: FactoryRunState) -> &'static str {
    match value {
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

fn parse_run_state(value: &str) -> Result<FactoryRunState, StoreError> {
    match value {
        "draft" => Ok(FactoryRunState::Draft),
        "orchestrating" => Ok(FactoryRunState::Orchestrating),
        "coding" => Ok(FactoryRunState::Coding),
        "evaluating" => Ok(FactoryRunState::Evaluating),
        "escalated" => Ok(FactoryRunState::Escalated),
        "passed" => Ok(FactoryRunState::Passed),
        "failed" => Ok(FactoryRunState::Failed),
        "needs_review" => Ok(FactoryRunState::NeedsReview),
        "cancelled" => Ok(FactoryRunState::Cancelled),
        _ => Err(StoreError::Corrupt(format!("unknown Run state `{value}`"))),
    }
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("filesystem error: {0}")]
    FileSystem(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("layout percentages are outside supported bounds")]
    InvalidLayout,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("corrupt local data: {0}")]
    Corrupt(String),
    #[error("database schema version {found} is newer than supported version {supported}")]
    IncompatibleSchema { found: u32, supported: u32 },
}

#[cfg(test)]
mod draft_lifecycle_tests {
    use super::*;
    use app_core::{FactoryRunInput, ManagedSessionOutcomeKind};
    use tempfile::TempDir;

    struct Fixture {
        _temp: TempDir,
        store: ProjectStore,
        agent: TargetAgentProjection,
        draft: AgentDraftProjection,
    }

    fn fixture() -> Fixture {
        let temp = TempDir::new().unwrap();
        let repository = temp.path().join("repository");
        let worktree = temp.path().join("repository-agent-main-12345678");
        std::fs::create_dir(&repository).unwrap();
        std::fs::create_dir(&worktree).unwrap();
        let store = ProjectStore::open_in_memory().unwrap();
        let agent = store
            .create_target_agent(Uuid::new_v4(), "Agent", &repository)
            .unwrap();
        let project = store.create_project("Agent main", &worktree, true).unwrap();
        let binding = store
            .create_workspace_binding(
                agent.id,
                project.id,
                "main",
                &worktree,
                &[],
                Some("agent-factory/agent/drafts/main"),
            )
            .unwrap();
        let timestamp = now_unix_ms();
        let draft = AgentDraftProjection {
            id: Uuid::new_v4(),
            target_agent_id: agent.id,
            workspace_binding_id: binding.id,
            name: "Agent".into(),
            objective: "Ship the requested behavior".into(),
            acceptance_criteria: vec!["Focused tests pass".into()],
            base_version: None,
            branch_ref: format!("agent-factory/{}/drafts/main", agent.id),
            worktree_path: worktree,
            git_head: "0123456789abcdef".into(),
            lifecycle: AgentDraftLifecycle::Active,
            cleanup_guidance: None,
            environment_id: None,
            created_at_unix_ms: timestamp,
            updated_at_unix_ms: timestamp,
        };
        let draft = store.create_agent_draft(&draft).unwrap();
        Fixture {
            _temp: temp,
            store,
            agent,
            draft,
        }
    }

    #[test]
    fn archive_target_agent_hides_the_agent_and_leaves_files() {
        let fixture = fixture();
        let marker = fixture.agent.repository_root.join("keep-me.txt");
        std::fs::write(&marker, "stays").unwrap();
        fixture
            .store
            .open_work_item(
                fixture.agent.id,
                fixture.draft.workspace_binding_id,
                Some(fixture.draft.id),
                Some(TargetWorkItemKind::AgentDraft),
                false,
            )
            .unwrap();
        assert_eq!(
            fixture
                .store
                .snapshot()
                .unwrap()
                .target_workspace
                .panes
                .len(),
            1
        );

        fixture
            .store
            .archive_target_agent(fixture.agent.id)
            .unwrap();

        let snapshot = fixture.store.snapshot().unwrap();
        assert!(snapshot.target_workspace.target_groups.is_empty());
        assert!(snapshot.target_workspace.panes.is_empty());
        assert!(marker.is_file());
        assert!(fixture.agent.repository_root.is_dir());
    }

    #[test]
    fn creating_an_agent_creates_no_version() {
        let fixture = fixture();
        assert!(
            fixture
                .store
                .target_agent_versions(fixture.agent.id)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            fixture
                .store
                .agent_draft(fixture.draft.id)
                .unwrap()
                .lifecycle,
            AgentDraftLifecycle::Active
        );
    }

    #[test]
    fn edits_only_the_active_draft_and_publishes_an_immutable_snapshot() {
        let fixture = fixture();
        let updated = fixture
            .store
            .update_agent_draft(
                fixture.draft.id,
                "Renamed Agent",
                "A better objective",
                &["A better criterion".into()],
                "fedcba9876543210",
            )
            .unwrap();
        assert_eq!(updated.name, "Renamed Agent");
        assert!(
            fixture
                .store
                .target_agent_versions(fixture.agent.id)
                .unwrap()
                .is_empty()
        );

        fixture
            .store
            .reserve_agent_draft_version(updated.id, "0.1.0")
            .unwrap();
        let version = fixture
            .store
            .finish_agent_draft_publication(
                updated.id,
                "0.1.0",
                "aaaaaaaaaaaaaaaa",
                "agent-factory/agent/v0.1.0",
            )
            .unwrap();
        assert_eq!(version.name, "Renamed Agent");
        assert_eq!(version.source_draft_id, updated.id);
        assert_eq!(
            fixture.store.target_agent(fixture.agent.id).unwrap().name,
            "Renamed Agent"
        );
        assert!(
            fixture
                .store
                .update_agent_draft(
                    updated.id,
                    "Changed",
                    "Changed",
                    &["Changed".into()],
                    "bbbb",
                )
                .is_err()
        );
    }

    /// A Draft remembers which Environment its Runs use, so the choice outlives
    /// the window that made it.
    #[test]
    fn a_draft_remembers_the_environment_it_was_given() {
        let fixture = fixture();
        assert_eq!(fixture.draft.environment_id, None);

        let chosen = fixture
            .store
            .set_agent_draft_environment(fixture.draft.id, Some("ipl-test-using-meta-muse"))
            .unwrap();
        assert_eq!(
            chosen.environment_id.as_deref(),
            Some("ipl-test-using-meta-muse")
        );
        let reloaded = fixture.store.agent_draft(fixture.draft.id).unwrap();
        assert_eq!(
            reloaded.environment_id.as_deref(),
            Some("ipl-test-using-meta-muse")
        );

        // Removing the chosen Environment leaves no choice rather than a
        // dangling one.
        let cleared = fixture
            .store
            .set_agent_draft_environment(fixture.draft.id, None)
            .unwrap();
        assert_eq!(cleared.environment_id, None);
    }

    #[test]
    fn session_history_preserves_durable_run_lineage_without_runtime_state() {
        let fixture = fixture();
        fixture
            .store
            .put_environment("history-env", "History")
            .unwrap();
        let project = fixture
            .store
            .snapshot()
            .unwrap()
            .projects
            .into_iter()
            .next()
            .unwrap();
        let run = FactoryRun::new(FactoryRunInput {
            target_agent_id: fixture.agent.id,
            agent_draft_id: fixture.draft.id,
            workspace_binding_id: fixture.draft.workspace_binding_id,
            project_id: project.id,
            environment_id: "history-env".into(),
            objective: fixture.draft.objective.clone(),
            acceptance_criteria: fixture.draft.acceptance_criteria.clone(),
            starting_git_head: fixture.draft.git_head.clone(),
        })
        .unwrap();
        fixture.store.save_factory_run(&run).unwrap();
        let mut session = AgentSessionProjection {
            id: Uuid::new_v4(),
            target_agent_id: fixture.agent.id,
            workspace_binding_id: fixture.draft.workspace_binding_id,
            project_id: project.id,
            environment_id: "history-env".into(),
            harness_id: "claude".into(),
            purpose: HarnessPurpose::Coding,
            factory_run_id: Some(run.id),
            parent_session_id: None,
            herdr_agent_name: format!("coding-{}", Uuid::new_v4()),
            availability: SessionAvailability::Historical,
            lifecycle: None,
            placement: None,
            title: "coding: Ship it".into(),
            created_at_unix_ms: 1,
            last_activity_at_unix_ms: 1,
            llm_provider_snapshot: None,
            effective_model: None,
            attention: Vec::new(),
            initial_prompt: Some("Implement the objective".into()),
            brief_delivered: false,
            outcome: Some(ManagedSessionOutcome {
                kind: ManagedSessionOutcomeKind::Completed,
                summary: Some("Work handed back to the Orchestrator".into()),
                recorded_at_unix_ms: 2,
            }),
        };
        fixture.store.save_agent_session(&session).unwrap();

        session.initial_prompt = Some("later prompt must not replace the first".into());
        fixture.store.save_agent_session(&session).unwrap();

        let snapshot = fixture.store.snapshot().unwrap();
        let stored_session = snapshot
            .agent_sessions
            .iter()
            .find(|candidate| candidate.id == session.id)
            .unwrap();
        assert_eq!(
            stored_session.initial_prompt.as_deref(),
            Some("Implement the objective")
        );
        assert_eq!(stored_session.factory_run_id, Some(run.id));
        assert_eq!(stored_session.availability, SessionAvailability::Historical);
        assert!(stored_session.lifecycle.is_none());
        assert!(stored_session.placement.is_none());
        assert_eq!(stored_session.outcome, session.outcome);
    }

    #[test]
    fn schema_contains_only_the_current_durable_ledger_shape() {
        let store = ProjectStore::open_in_memory().unwrap();
        let connection = store.connection.lock().unwrap();
        let mut table_statement = connection
            .prepare(
                "SELECT name FROM sqlite_master \
                 WHERE type = 'table' AND name NOT LIKE 'sqlite_%' \
                 ORDER BY name",
            )
            .unwrap();
        let tables = table_statement
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            tables,
            [
                "agent_sessions",
                "app_state",
                "environments",
                "factory_runs",
                "llm_providers",
                "local_mcp_trust",
                "plugin_registries",
                "projects",
                "run_control_tokens",
                "target_agent_drafts",
                "target_agent_versions",
                "target_agents",
                "work_contexts",
                "workspace_bindings",
                "workspace_panes",
                "workspace_terminals",
            ]
        );

        let table_columns = |table: &str| {
            let mut statement = connection
                .prepare(&format!("PRAGMA table_info({table})"))
                .unwrap();
            statement
                .query_map([], |row| row.get::<_, String>(1))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert_eq!(
            table_columns("agent_sessions"),
            [
                "id",
                "workspace_binding_id",
                "factory_run_id",
                "parent_session_id",
                "environment_id",
                "harness_id",
                "purpose",
                "herdr_agent_name",
                "title",
                "created_at_unix_ms",
                "last_activity_at_unix_ms",
                "llm_provider_snapshot_json",
                "effective_model",
                "initial_prompt",
                "brief_delivered",
                "outcome_json",
            ]
        );
        assert_eq!(
            table_columns("factory_runs"),
            [
                "id",
                "workspace_binding_id",
                "agent_draft_id",
                "environment_id",
                "objective",
                "acceptance_criteria_json",
                "starting_git_head",
                "final_git_head",
                "changed_files_json",
                "test_evidence_json",
                "evaluation_json",
                "state",
                "escalation",
                "last_activity_at_unix_ms",
                "completed_at_unix_ms",
            ]
        );

        let live_run_index = connection
            .query_row(
                "SELECT sql FROM sqlite_master \
                 WHERE type = 'index' AND name = 'factory_runs_one_live_per_draft'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        assert!(live_run_index.contains("'needs_review'"));
    }

    #[test]
    fn reset_replaces_an_obsolete_schema() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("state.sqlite3");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch("CREATE TABLE obsolete(id INTEGER); PRAGMA user_version = 21;")
            .unwrap();
        drop(connection);
        let store = ProjectStore::open(&path).unwrap();
        assert_eq!(store.snapshot().unwrap().revision, 0);
        let connection = Connection::open(path).unwrap();
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
                .unwrap(),
            SCHEMA_VERSION
        );
    }
}
