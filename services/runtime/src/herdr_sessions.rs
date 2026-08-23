//! Herdr orchestration for Coding Sessions and Evaluation Sessions.
//!
//! Herdr owns the terminal topology and the agents inside it. This module is the
//! only place that talks to Herdr: it places a session in Herdr's topology,
//! starts the agent kind the Environment selects, and translates Herdr's
//! lifecycle states into Agent Factory's. It stores no agent state of its own —
//! Herdr is the authority, and the runtime reflects it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use app_core::{
    AgentLifecycle, AuthorityFreshness, HarnessActionProjection, HarnessProjection,
    HarnessReadinessState, HerdrPlacement, HerdrStatusProjection,
};
use herdr_client::{
    AgentStatus, HerdrClient, HerdrError, HerdrEvents, PaneSpec, ReadSource, SessionSnapshot,
    SplitDirection, TerminalAttach, TerminalSession, WorktreeCreated, WorktreeOpened,
    WorktreeRemoved, default_terminal_size, resolve_herdr_bin,
};

/// Herdr workspace labels are human-readable and stable across restarts. The
/// short binding suffix disambiguates same-named bindings without exposing a
/// full opaque identifier in Herdr's UI.
/// Transcript rows requested per read. Enough for a full agent response without
/// asking Herdr to walk its whole scrollback each time.
const TRANSCRIPT_LINES: u32 = 400;
/// How long to wait before the first attempt to reach Herdr again.
const RECONNECT_MIN_BACKOFF: Duration = Duration::from_secs(1);
/// The ceiling the backoff grows to while Herdr stays away.
const RECONNECT_MAX_BACKOFF: Duration = Duration::from_secs(30);
/// Deadline for the probe that decides whether a full refresh is worth trying.
///
/// The dispatch loop calls this between IPC frames, so a probe against a socket
/// whose server is wedged must fail fast rather than hold the default timeout.
const RECONNECT_PROBE_TIMEOUT: Duration = Duration::from_secs(2);
/// A healthy event stream remains the primary invalidation path. This bounded
/// snapshot poll catches server-side changes when a notification is lost or a
/// Herdr build does not emit the relevant topic.
#[cfg(not(test))]
const SNAPSHOT_POLL_INTERVAL: Duration = Duration::from_secs(2);
#[cfg(test)]
const SNAPSHOT_POLL_INTERVAL: Duration = Duration::from_millis(40);
/// A pane close is acknowledged before every Herdr projection necessarily
/// observes the teardown. Cancellation waits for this bounded interval so a
/// successful response means the agent and its pane are actually gone.
#[cfg(not(test))]
const STOP_CONFIRM_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(test)]
const STOP_CONFIRM_TIMEOUT: Duration = Duration::from_millis(200);
const STOP_CONFIRM_POLL_INTERVAL: Duration = Duration::from_millis(50);
/// Tests drive the reconnect through a stand-in socket and should not wait out
/// the production backoff to observe it.
#[cfg(test)]
const TEST_RECONNECT_MIN_BACKOFF: Duration = Duration::from_millis(20);

pub(crate) struct AgentLaunchSpec {
    pub(crate) agent_name: String,
    pub(crate) harness_id: String,
    pub(crate) workspace_label: String,
    pub(crate) tab_label: String,
    pub(crate) cwd: PathBuf,
    pub(crate) environment: BTreeMap<String, String>,
    /// Native arguments forwarded to the harness executable (`claude --model …`).
    pub(crate) agent_args: Vec<String>,
    /// The pane this session stands beside, when it joins an iteration already
    /// on screen. `None` opens a tab of its own.
    pub(crate) column_beside: Option<String>,
}

/// What `start` actually opened, so a failure can undo exactly that.
struct OpenedSurface {
    tab_id: String,
    pane_id: String,
    /// True when a whole tab was created for this session, false when it became
    /// a column inside somebody else's tab.
    owns_tab: bool,
}

/// Result of placing a session and asking Herdr to start its agent.
pub(crate) struct StartedAgent {
    pub(crate) placement: HerdrPlacement,
    /// False when Herdr registered the name but the agent is not prompt-ready
    /// yet (blocked at startup). The pane stays so the user can unblock it.
    pub(crate) ready: bool,
}

/// What Herdr reported after one prompt submission.
pub(crate) struct PromptAttempt {
    pub(crate) lifecycle: AgentLifecycle,
}

/// The only process the native terminal may launch for a Run workspace.
/// Herdr still owns every Workspace, pane, process, and terminal inside the
/// resulting TUI; Agent Factory supplies only the client executable and the
/// configured Herdr session selector.
pub(crate) struct WorkspaceTerminalLaunch {
    pub(crate) executable: String,
    pub(crate) arguments: Vec<String>,
}

struct PromptedAgent {
    info: herdr_client::AgentInfo,
}

pub(crate) struct HerdrRuntime {
    client: HerdrClient,
    session: Option<String>,
    events: Option<HerdrEvents>,
    snapshot: Option<SessionSnapshot>,
    status: HerdrStatusProjection,
    harnesses: Vec<HarnessProjection>,
    start_timeout: Duration,
    transient_retry_timeout: Duration,
    /// Live `terminal session control` attaches, keyed by pane id.
    terminals: BTreeMap<String, TerminalSession>,
    terminal_sizes: BTreeMap<String, (u16, u16)>,
    herdr_bin: Option<PathBuf>,
    allow_terminal_control: bool,
    /// When to try Herdr again. `None` never retries, which is what a detached
    /// runtime wants; otherwise Herdr outliving a lost subscription would leave
    /// Agent Factory permanently deaf.
    reconnect_at: Option<Instant>,
    reconnect_backoff: Duration,
    /// Where the backoff restarts after each loss of the subscription.
    reconnect_min_backoff: Duration,
    /// Fallback reconciliation deadline while the event stream is healthy.
    snapshot_poll_at: Option<Instant>,
}

impl HerdrRuntime {
    /// Connect to Herdr, or record why it is unavailable. Agent Factory never
    /// installs or starts Herdr on the user's behalf; it explains and moves on.
    ///
    /// `AGENT_FACTORY_HERDR_SOCKET` names a socket directly, which is how tests
    /// point the runtime at a stand-in server instead of the developer's own
    /// Herdr.
    pub(crate) fn connect(session: Option<String>) -> Self {
        let client = match std::env::var_os("AGENT_FACTORY_HERDR_SOCKET") {
            Some(socket) => Ok(HerdrClient::new(PathBuf::from(socket))),
            None => HerdrClient::discover(session.as_deref()),
        };
        let client = match client {
            Ok(client) => client,
            Err(error) => {
                return Self::unavailable(
                    HerdrClient::new(PathBuf::from("herdr.sock")),
                    session,
                    error,
                );
            }
        };
        let (allow_terminal_control, herdr_bin) = live_terminal_control();
        let mut runtime = Self {
            client,
            session,
            events: None,
            snapshot: None,
            status: HerdrStatusProjection::default(),
            harnesses: Vec::new(),
            start_timeout: herdr_client::DEFAULT_START_TIMEOUT,
            transient_retry_timeout: herdr_client::TRANSIENT_RETRY_TIMEOUT,
            terminals: BTreeMap::new(),
            terminal_sizes: BTreeMap::new(),
            herdr_bin,
            allow_terminal_control,
            reconnect_at: None,
            reconnect_backoff: RECONNECT_MIN_BACKOFF,
            reconnect_min_backoff: RECONNECT_MIN_BACKOFF,
            snapshot_poll_at: Some(Instant::now() + SNAPSHOT_POLL_INTERVAL),
        };
        runtime.refresh();
        runtime.schedule_reconnect_if_unsubscribed();
        runtime
    }

    /// Connect to an explicit socket. Tests use this so they never touch the
    /// developer's live Herdr session.
    #[cfg(test)]
    pub(crate) fn connect_to(socket: PathBuf) -> Self {
        let mut runtime = Self {
            client: HerdrClient::new(socket).with_timeout(Duration::from_secs(5)),
            session: None,
            events: None,
            snapshot: None,
            status: HerdrStatusProjection::default(),
            harnesses: Vec::new(),
            start_timeout: Duration::from_secs(5),
            transient_retry_timeout: Duration::from_secs(2),
            terminals: BTreeMap::new(),
            terminal_sizes: BTreeMap::new(),
            herdr_bin: None,
            allow_terminal_control: false,
            reconnect_at: None,
            reconnect_backoff: TEST_RECONNECT_MIN_BACKOFF,
            reconnect_min_backoff: TEST_RECONNECT_MIN_BACKOFF,
            snapshot_poll_at: Some(Instant::now() + SNAPSHOT_POLL_INTERVAL),
        };
        runtime.refresh();
        runtime.schedule_reconnect_if_unsubscribed();
        runtime
    }

    /// A runtime with no Herdr behind it. Unit tests that never start an agent
    /// use this so they cannot reach the developer's own Herdr session.
    pub(crate) fn detached() -> Self {
        Self::unavailable(
            HerdrClient::new(PathBuf::from("/nonexistent/agent-factory/herdr.sock")),
            None,
            HerdrError::Unreachable {
                socket: PathBuf::from("/nonexistent/agent-factory/herdr.sock"),
                source: std::io::Error::other("no Herdr is configured"),
            },
        )
    }

    fn unavailable(client: HerdrClient, session: Option<String>, error: HerdrError) -> Self {
        Self {
            client,
            session: session.clone(),
            events: None,
            snapshot: None,
            status: HerdrStatusProjection {
                connected: false,
                freshness: AuthorityFreshness::LastObserved,
                observed_at_unix_ms: None,
                version: None,
                protocol: None,
                session,
                issues: vec![error.public_message()],
            },
            harnesses: Vec::new(),
            start_timeout: herdr_client::DEFAULT_START_TIMEOUT,
            transient_retry_timeout: herdr_client::TRANSIENT_RETRY_TIMEOUT,
            terminals: BTreeMap::new(),
            terminal_sizes: BTreeMap::new(),
            herdr_bin: None,
            allow_terminal_control: false,
            // No socket was ever resolved, so there is nothing to retry.
            reconnect_at: None,
            reconnect_backoff: RECONNECT_MIN_BACKOFF,
            reconnect_min_backoff: RECONNECT_MIN_BACKOFF,
            snapshot_poll_at: None,
        }
    }

    /// Re-probe Herdr, refresh the harness catalog, and (re)open the event
    /// subscription. Safe to call at any time; a live subscription is kept.
    pub(crate) fn refresh(&mut self) {
        match self.client.probe() {
            Ok(info) => {
                self.status = HerdrStatusProjection {
                    connected: true,
                    freshness: AuthorityFreshness::Reconnecting,
                    observed_at_unix_ms: self.status.observed_at_unix_ms,
                    version: Some(info.version),
                    protocol: Some(info.protocol),
                    session: self.session.clone(),
                    issues: Vec::new(),
                };
            }
            Err(error) => {
                self.status = HerdrStatusProjection {
                    connected: false,
                    freshness: if self.snapshot.is_some() {
                        AuthorityFreshness::LastObserved
                    } else {
                        AuthorityFreshness::Reconnecting
                    },
                    observed_at_unix_ms: self.status.observed_at_unix_ms,
                    version: None,
                    protocol: None,
                    session: self.session.clone(),
                    issues: vec![error.public_message()],
                };
                self.events = None;
                self.harnesses.clear();
                return;
            }
        }

        self.harnesses = match self.client.agent_manifests() {
            Ok(manifests) => manifests
                .into_iter()
                .filter_map(harness_projection)
                .collect(),
            Err(error) => {
                self.status.issues.push(error.public_message());
                Vec::new()
            }
        };

        let _ = self.refresh_live_state();
    }

    /// Re-establish the subscription when Herdr has come back.
    ///
    /// Herdr outlives Agent Factory and can restart under it — a server restart,
    /// `herdr update --handoff`, or Herdr simply not running yet when the app
    /// launches. Without this the lost subscription is never reopened: lifecycle
    /// events stop arriving and `is_connected` keeps refusing new sessions until
    /// the user restarts Agent Factory.
    ///
    /// Returns `true` only on the transition back to a live subscription, so the
    /// caller can resynchronize once rather than on every tick.
    pub(crate) fn reconnect_if_due(&mut self) -> bool {
        if self.is_subscribed() {
            return false;
        }
        let Some(due) = self.reconnect_at else {
            return false;
        };
        if Instant::now() < due {
            return false;
        }

        // Probe on a short deadline first. This runs between IPC frames, so a
        // socket whose server is wedged must not hold the dispatch loop for the
        // full control-call timeout.
        let reachable = self
            .client
            .clone()
            .with_timeout(RECONNECT_PROBE_TIMEOUT)
            .probe()
            .is_ok();
        if reachable {
            self.refresh();
        }
        if self.is_subscribed() {
            self.reconnect_at = None;
            self.reconnect_backoff = self.reconnect_min_backoff;
            return true;
        }
        self.reconnect_backoff = (self.reconnect_backoff * 2).min(RECONNECT_MAX_BACKOFF);
        self.reconnect_at = Some(Instant::now() + self.reconnect_backoff);
        false
    }

    fn is_subscribed(&self) -> bool {
        self.status.connected && self.events.as_ref().is_some_and(HerdrEvents::is_connected)
    }

    /// Arm the retry when a socket exists but no subscription does.
    fn schedule_reconnect_if_unsubscribed(&mut self) {
        if self.is_subscribed() {
            self.reconnect_at = None;
            self.reconnect_backoff = self.reconnect_min_backoff;
            return;
        }
        self.reconnect_at = Some(Instant::now() + self.reconnect_backoff);
    }

    pub(crate) fn status(&self) -> &HerdrStatusProjection {
        &self.status
    }

    pub(crate) fn snapshot(&self) -> Option<&SessionSnapshot> {
        self.snapshot.as_ref()
    }

    pub(crate) fn harnesses(&self) -> &[HarnessProjection] {
        &self.harnesses
    }

    pub(crate) fn is_connected(&self) -> bool {
        self.status.connected && self.status.freshness == AuthorityFreshness::Live
    }

    pub(crate) fn workspace_terminal_launch(&self) -> Result<WorkspaceTerminalLaunch, HerdrError> {
        if !self.allow_terminal_control {
            return Err(HerdrError::Protocol(
                "the native Herdr terminal is unavailable in this runtime".into(),
            ));
        }
        let executable = self.herdr_bin.as_ref().ok_or_else(|| {
            HerdrError::Protocol(
                "herdr CLI is not available to open the native workspace terminal".into(),
            )
        })?;
        let executable = std::fs::canonicalize(executable).map_err(HerdrError::Io)?;
        let executable = executable
            .into_os_string()
            .into_string()
            .map_err(|_| HerdrError::Protocol("herdr executable path is not valid UTF-8".into()))?;
        Ok(WorkspaceTerminalLaunch {
            executable,
            arguments: workspace_terminal_arguments(self.session.as_deref()),
        })
    }

    /// Replace the complete live cache. A failed read leaves the prior snapshot
    /// as presentation-only data and cannot authorize commands.
    fn try_refresh_snapshot(&mut self) -> Result<(), HerdrError> {
        let result = match self.client.session_snapshot() {
            Ok(snapshot) => {
                let observed_at = now_unix_ms();
                self.status.connected = true;
                self.status.freshness = AuthorityFreshness::Live;
                self.status.observed_at_unix_ms = Some(observed_at);
                self.snapshot = Some(snapshot);
                Ok(())
            }
            Err(error) => {
                self.status.freshness = if self.snapshot.is_some() {
                    AuthorityFreshness::LastObserved
                } else {
                    AuthorityFreshness::Reconnecting
                };
                let message = error.public_message();
                if !self.status.issues.iter().any(|issue| issue == &message) {
                    self.status.issues.push(message);
                }
                Err(error)
            }
        };
        self.snapshot_poll_at = Some(Instant::now() + SNAPSHOT_POLL_INTERVAL);
        result
    }

    fn refresh_snapshot(&mut self) -> bool {
        self.try_refresh_snapshot().is_ok()
    }

    /// Obtain the current server snapshot before a side effect. Last-observed
    /// topology remains useful for presentation, but never authorizes control.
    pub(crate) fn require_fresh_state(&mut self) -> Result<(), HerdrError> {
        self.try_refresh_snapshot()?;
        let _ = self.synchronize_subscription();
        Ok(())
    }

    /// Keep one global invalidation stream plus the pane-scoped lifecycle
    /// subscriptions Herdr requires for every live Agent in the snapshot.
    fn synchronize_subscription(&mut self) -> bool {
        let agent_pane_ids = self
            .snapshot
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .agents
                    .iter()
                    .map(|agent| agent.pane_id.clone())
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default();
        if self.events.as_ref().is_some_and(|events| {
            events.is_connected() && events.covers_agent_panes(&agent_pane_ids)
        }) {
            return true;
        }

        match self.client.subscribe(agent_pane_ids.iter().cloned()) {
            Ok(events) => {
                self.events = Some(events);
                true
            }
            Err(error) => {
                self.events = None;
                self.status.freshness = if self.snapshot.is_some() {
                    AuthorityFreshness::LastObserved
                } else {
                    AuthorityFreshness::Reconnecting
                };
                let message = error.public_message();
                if !self.status.issues.iter().any(|issue| issue == &message) {
                    self.status.issues.push(message);
                }
                self.reconnect_backoff = self.reconnect_min_backoff;
                self.schedule_reconnect_if_unsubscribed();
                false
            }
        }
    }

    fn refresh_live_state(&mut self) -> bool {
        let refreshed = self.refresh_snapshot();
        if refreshed {
            let _ = self.synchronize_subscription();
        }
        refreshed
    }

    /// Events only invalidate the cache. The payload is never applied as a
    /// transition over a newer snapshot.
    pub(crate) fn refresh_if_invalidated(&mut self) -> bool {
        let Some(events) = self.events.as_ref() else {
            return false;
        };
        let invalidated = events.drain() > 0;
        if !events.is_connected() {
            self.events = None;
            self.status.connected = false;
            self.status.freshness = if self.snapshot.is_some() {
                AuthorityFreshness::LastObserved
            } else {
                AuthorityFreshness::Reconnecting
            };
            self.status
                .issues
                .push("The Herdr event stream closed.".to_owned());
            self.reconnect_backoff = self.reconnect_min_backoff;
            self.schedule_reconnect_if_unsubscribed();
            return true;
        }
        if invalidated {
            let _ = self.refresh_live_state();
        }
        invalidated
    }

    /// Periodically replace the complete cache even when no event arrived.
    /// This is a fallback only: disconnected streams use the reconnect path,
    /// and every poll still reads authoritative Herdr state in one snapshot.
    pub(crate) fn refresh_if_poll_due(&mut self) -> bool {
        if !self.is_subscribed() {
            return false;
        }
        let Some(due) = self.snapshot_poll_at else {
            self.snapshot_poll_at = Some(Instant::now() + SNAPSHOT_POLL_INTERVAL);
            return false;
        };
        if Instant::now() < due {
            return false;
        }
        let _ = self.refresh_live_state();
        true
    }

    pub(crate) fn create_worktree(
        &mut self,
        source_cwd: &std::path::Path,
        branch: &str,
        base: &str,
        path: &std::path::Path,
        workspace_label: &str,
    ) -> Result<WorktreeCreated, HerdrError> {
        let created =
            self.client
                .create_worktree(source_cwd, branch, base, path, workspace_label, false)?;
        let _ = self.refresh_live_state();
        Ok(created)
    }

    pub(crate) fn remove_worktree(
        &mut self,
        workspace_id: &str,
        force: bool,
    ) -> Result<WorktreeRemoved, HerdrError> {
        let removed = self.client.remove_worktree(workspace_id, force)?;
        let _ = self.refresh_live_state();
        Ok(removed)
    }

    pub(crate) fn open_worktree(
        &mut self,
        source_cwd: &std::path::Path,
        path: &std::path::Path,
        workspace_label: &str,
        focus: bool,
    ) -> Result<WorktreeOpened, HerdrError> {
        let opened = self
            .client
            .open_worktree(source_cwd, path, workspace_label, focus)?;
        let _ = self.refresh_live_state();
        Ok(opened)
    }

    /// Whether Herdr can launch this agent kind right now.
    pub(crate) fn harness(&self, harness_id: &str) -> Option<&HarnessProjection> {
        self.harnesses
            .iter()
            .find(|harness| harness.id == harness_id)
    }

    /// Place a session in Herdr and start its agent.
    ///
    /// The Environment boundary is applied when the pane is created, so the
    /// agent's process inherits exactly the variables the Environment resolves.
    /// A split carries the same `PaneSpec` as a new tab, so standing an agent
    /// beside another costs nothing in boundary terms.
    /// `agent_not_ready` keeps the pane: the name exists, and the TUI is how
    /// the user unblocks a startup dialog. `agent_pane_busy` still retries
    /// until the shell can accept `agent start`.
    pub(crate) fn start(&mut self, spec: &AgentLaunchSpec) -> Result<StartedAgent, HerdrError> {
        let pane_spec = PaneSpec::new(
            Some(spec.cwd.to_string_lossy().into_owned()),
            spec.environment.clone(),
        );
        let workspace_id = self.workspace_for(&spec.workspace_label, &pane_spec)?;
        let opened = self.open_surface(spec, &workspace_id, &pane_spec)?;
        let placement = HerdrPlacement {
            workspace_id,
            tab_id: opened.tab_id.clone(),
            pane_id: opened.pane_id.clone(),
            agent_name: spec.agent_name.clone(),
        };
        // Shell rc files can overwrite spawn-time ANTHROPIC_* values. Re-export
        // the gateway and model after the pane exists so `claude` inherits them.
        self.seed_shell_provider_env(&opened.pane_id, &spec.environment);

        match herdr_client::retry_transient(self.transient_retry_timeout, || {
            match self.client.start_agent(
                &spec.agent_name,
                &spec.harness_id,
                &opened.pane_id,
                &spec.agent_args,
                self.start_timeout,
            ) {
                Ok(_) => Ok(true),
                Err(error) if error.is_agent_not_ready() => Ok(false),
                Err(error) => Err(error),
            }
        }) {
            Ok(ready) => {
                let _ = self
                    .client
                    .rename_agent(&placement.pane_id, &placement.agent_name);
                let _ = self.refresh_live_state();
                Ok(StartedAgent { placement, ready })
            }
            Err(error) => {
                // Undo exactly what was opened. Closing the tab around a column
                // would take the Orchestrator down with the agent that failed.
                if opened.owns_tab {
                    let _ = self.client.close_tab(&opened.tab_id);
                } else {
                    let _ = self.client.close_pane(&opened.pane_id);
                }
                Err(error)
            }
        }
    }

    /// Open the pane this session will run in.
    ///
    /// One iteration is one tab: the Orchestrator opens it, and every agent it
    /// starts for that iteration becomes the next column to the right.
    fn open_surface(
        &self,
        spec: &AgentLaunchSpec,
        workspace_id: &str,
        pane_spec: &PaneSpec,
    ) -> Result<OpenedSurface, HerdrError> {
        if let Some(target) = spec
            .column_beside
            .as_deref()
            .and_then(|pane_id| self.live_pane_in(pane_id, workspace_id))
        {
            let pane = self
                .client
                .split_pane(&target, SplitDirection::Right, pane_spec)?;
            return Ok(OpenedSurface {
                tab_id: pane.tab_id,
                pane_id: pane.pane_id,
                owns_tab: false,
            });
        }
        let created = self
            .client
            .create_tab(workspace_id, Some(&spec.tab_label), pane_spec)?;
        Ok(OpenedSurface {
            tab_id: created.tab.tab_id,
            pane_id: created.root_pane.pane_id,
            owns_tab: true,
        })
    }

    /// A stored pane id is a locator, never a promise. Confirm Herdr still has
    /// the pane, and still has it here, before splitting it; a session whose
    /// neighbour has gone gets a tab of its own rather than a failed Run.
    fn live_pane_in(&self, pane_id: &str, workspace_id: &str) -> Option<String> {
        let pane = self.client.pane(pane_id).ok()?;
        (pane.workspace_id == workspace_id).then_some(pane.pane_id)
    }

    /// Herdr keeps workspaces across Agent Factory restarts, so the workspace
    /// for a binding is found by its label rather than persisted here.
    fn workspace_for(&self, label: &str, spec: &PaneSpec) -> Result<String, HerdrError> {
        if let Some(existing) = self
            .client
            .workspaces()?
            .into_iter()
            .find(|workspace| workspace.label == label)
        {
            return Ok(existing.workspace_id);
        }
        Ok(self
            .client
            .create_workspace(Some(label), spec)?
            .workspace
            .workspace_id)
    }

    pub(crate) fn workspace_for_label(&self, label: &str) -> Option<&str> {
        self.snapshot
            .as_ref()?
            .workspaces
            .iter()
            .find(|workspace| workspace.label == label)
            .map(|workspace| workspace.workspace_id.as_str())
    }

    /// Submit a prompt without blocking the dispatch thread. Herdr reports the
    /// resulting lifecycle through the subscription.
    pub(crate) fn prompt(
        &mut self,
        placement: &HerdrPlacement,
        text: &str,
    ) -> Result<PromptAttempt, HerdrError> {
        let prompted = herdr_client::retry_transient(self.transient_retry_timeout, || {
            self.prompt_named_or_pane(placement, text)
        })?;
        let attempt = prompt_attempt(prompted);
        let _ = self.refresh_snapshot();
        Ok(attempt)
    }

    /// One prompt attempt. Used when a session is already visible and a retry
    /// loop would freeze the event thread.
    pub(crate) fn try_prompt(
        &mut self,
        placement: &HerdrPlacement,
        text: &str,
    ) -> Result<PromptAttempt, HerdrError> {
        let prompted = self.prompt_named_or_pane(placement, text)?;
        let attempt = prompt_attempt(prompted);
        let _ = self.refresh_snapshot();
        Ok(attempt)
    }

    /// Ask Herdr to reconcile a managed launch that outlived the original
    /// bounded `agent.start` call. `agent.get` performs that reconciliation;
    /// starting again would conflict with the name already reserved by the
    /// same pending pane.
    pub(crate) fn try_reconcile_ready(
        &self,
        placement: &HerdrPlacement,
    ) -> Result<bool, HerdrError> {
        let info = self.client.agent(&placement.pane_id)?;
        Ok(info.interactive_ready && !info.launch_pending)
    }

    fn prompt_named_or_pane(
        &self,
        placement: &HerdrPlacement,
        text: &str,
    ) -> Result<PromptedAgent, HerdrError> {
        match self.client.prompt_agent(&placement.agent_name, text, None) {
            Ok(info) => Ok(PromptedAgent { info }),
            Err(error) if error.is_unbound_agent() => {
                let info = self.client.prompt_agent(&placement.pane_id, text, None)?;
                let _ = self
                    .client
                    .rename_agent(&placement.pane_id, &placement.agent_name);
                Ok(PromptedAgent { info })
            }
            Err(error) => Err(error),
        }
    }

    fn seed_shell_provider_env(&self, pane_id: &str, env: &BTreeMap<String, String>) {
        const KEYS: &[&str] = &[
            "ANTHROPIC_BASE_URL",
            "ANTHROPIC_AUTH_TOKEN",
            "ANTHROPIC_MODEL",
            "ANTHROPIC_API_KEY",
            "CLAUDE_CODE_USE_BEDROCK",
            "CLAUDE_CODE_USE_VERTEX",
            "CLAUDE_CODE_DISABLE_UNKNOWN_MODEL_WINDOW_ENFORCEMENT",
        ];
        let mut assignments = Vec::new();
        for key in KEYS {
            if let Some(value) = env.get(*key) {
                assignments.push(format!("{key}={}", posix_single_quote(value)));
            }
        }
        if assignments.is_empty() {
            return;
        }
        let _ = self
            .client
            .send_pane_text(pane_id, &format!("export {}\n", assignments.join(" ")));
    }

    /// Interrupt whatever the agent is doing, using its own interactive control.
    pub(crate) fn interrupt(&self, placement: &HerdrPlacement) -> Result<(), HerdrError> {
        self.send_keys(placement, &["escape".into()])
    }

    /// Forward logical keys to a blocked agent's own approval surface.
    pub(crate) fn send_keys(
        &self,
        placement: &HerdrPlacement,
        keys: &[String],
    ) -> Result<(), HerdrError> {
        match self.client.send_agent_keys(&placement.agent_name, keys) {
            Ok(()) => Ok(()),
            Err(error) if error.is_unbound_agent() => {
                self.client.send_agent_keys(&placement.pane_id, keys)
            }
            Err(error) => Err(error),
        }
    }

    pub(crate) fn transcript(
        &self,
        placement: &HerdrPlacement,
        lines: Option<u32>,
    ) -> Result<(String, u64, bool), HerdrError> {
        let target = match self.client.read_agent(
            &placement.agent_name,
            ReadSource::RecentUnwrapped,
            Some(lines.unwrap_or(TRANSCRIPT_LINES)),
        ) {
            Ok(read) => return Ok((read.text, read.revision, read.truncated)),
            Err(error) if error.is_unbound_agent() => placement.pane_id.as_str(),
            Err(error) => return Err(error),
        };
        let read = self.client.read_agent(
            target,
            ReadSource::RecentUnwrapped,
            Some(lines.unwrap_or(TRANSCRIPT_LINES)),
        )?;
        Ok((read.text, read.revision, read.truncated))
    }

    /// Visible ANSI of the agent's pane, for a Ghostty/wterm surface.
    ///
    /// After a live terminal session attach, this is the streamed frame buffer.
    /// Before attach, it falls back to `agent.read` visible so the TUI still
    /// paints while the controller is coming up.
    pub(crate) fn screen(
        &mut self,
        placement: &HerdrPlacement,
    ) -> Result<(String, u64, bool), HerdrError> {
        self.drop_dead_terminal(&placement.pane_id);
        if let Some(session) = self.terminals.get(&placement.pane_id) {
            return Ok(session.snapshot());
        }
        let read = self.client.read_agent_screen(&placement.agent_name)?;
        Ok((read.text, read.revision, read.truncated))
    }

    /// Forward typed bytes, logical keys, or a resize into the Herdr pane.
    ///
    /// Interactive typing attaches `herdr terminal session control --takeover`
    /// so input and live frames share one owner. Tests and a missing `herdr`
    /// binary keep the JSON API fallback (`pane.send_text` / `pane.send_input`).
    pub(crate) fn write(
        &mut self,
        placement: &HerdrPlacement,
        text: Option<&str>,
        keys: &[String],
        cols: Option<u16>,
        rows: Option<u16>,
    ) -> Result<(), HerdrError> {
        if let (Some(cols), Some(rows)) = (cols, rows)
            && cols > 0
            && rows > 0
        {
            self.terminal_sizes
                .insert(placement.pane_id.clone(), (cols, rows));
        }
        if self.ensure_terminal(placement).is_some() {
            let session = self
                .terminals
                .get(&placement.pane_id)
                .expect("ensure_terminal inserted the session");
            if let (Some(cols), Some(rows)) = (cols, rows) {
                session.resize(cols, rows)?;
            }
            if let Some(text) = text {
                session.send_text(text)?;
            }
            if !keys.is_empty() {
                self.client.send_agent_keys(&placement.agent_name, keys)?;
            }
            return Ok(());
        }
        if let Some(text) = text {
            self.client.send_pane_text(&placement.pane_id, text)?;
        }
        if !keys.is_empty() {
            let key_refs: Vec<&str> = keys.iter().map(String::as_str).collect();
            self.client
                .send_pane_input(&placement.pane_id, None, &key_refs)?;
        }
        Ok(())
    }

    pub(crate) fn focus(&self, placement: &HerdrPlacement) -> Result<(), HerdrError> {
        self.client.focus_agent(&placement.agent_name)
    }

    /// Close the session's pane. Herdr tears the agent down with it, and this
    /// method does not return success until a fresh snapshot confirms the
    /// managed agent and the closed pane are absent.
    pub(crate) fn stop(&mut self, placement: &HerdrPlacement) -> Result<(), HerdrError> {
        self.stop_managed_agent(&placement.agent_name, Some(placement))?;
        Ok(())
    }

    /// Stop a Factory-managed Herdr agent using its stable name. Persisted pane
    /// and tab ids are only revalidated locators; a fresh named-agent match wins.
    /// Returns whether this call closed a live pane rather than confirming that
    /// the managed agent was already absent.
    ///
    /// The pane is what gets closed, never the tab around it. An iteration's
    /// agents share a tab, so closing the tab would stop the Orchestrator and
    /// every sibling along with the one agent that was asked for. Herdr drops a
    /// tab once its last pane is gone, so a lone agent still takes its tab with
    /// it.
    pub(crate) fn stop_managed_agent(
        &mut self,
        agent_name: &str,
        placement: Option<&HerdrPlacement>,
    ) -> Result<bool, HerdrError> {
        self.require_fresh_state()?;

        let live_pane_id = self.snapshot.as_ref().and_then(|snapshot| {
            snapshot
                .agents
                .iter()
                .find(|agent| agent.name.as_deref() == Some(agent_name))
                .map(|agent| agent.pane_id.clone())
        });
        let revalidated_placement_pane = placement.and_then(|placement| {
            let snapshot = self.snapshot.as_ref()?;
            let pane_exists = snapshot.panes.iter().any(|pane| {
                pane.pane_id == placement.pane_id && pane.workspace_id == placement.workspace_id
            }) || snapshot.agents.iter().any(|agent| {
                agent.pane_id == placement.pane_id
                    && agent.workspace_id.as_deref() == Some(&placement.workspace_id)
            });
            pane_exists.then(|| placement.pane_id.clone())
        });
        let Some(pane_id) = live_pane_id.or(revalidated_placement_pane) else {
            return Ok(false);
        };

        self.terminals.remove(&pane_id);
        self.terminal_sizes.remove(&pane_id);
        if let Some(placement) = placement {
            self.terminals.remove(&placement.pane_id);
            self.terminal_sizes.remove(&placement.pane_id);
        }
        self.client.close_pane(&pane_id)?;

        let deadline = Instant::now() + STOP_CONFIRM_TIMEOUT;
        loop {
            if self.try_refresh_snapshot().is_ok() {
                let snapshot = self
                    .snapshot
                    .as_ref()
                    .expect("a successful refresh stores a snapshot");
                let agent_exists = snapshot
                    .agents
                    .iter()
                    .any(|agent| agent.name.as_deref() == Some(agent_name));
                let pane_exists = snapshot.panes.iter().any(|pane| pane.pane_id == pane_id)
                    || snapshot.agents.iter().any(|agent| agent.pane_id == pane_id);
                if !agent_exists && !pane_exists {
                    let _ = self.synchronize_subscription();
                    return Ok(true);
                }
            }
            if Instant::now() >= deadline {
                return Err(HerdrError::Protocol(format!(
                    "Herdr still reports managed agent `{agent_name}` or pane `{pane_id}` after the close request"
                )));
            }
            std::thread::sleep(STOP_CONFIRM_POLL_INTERVAL);
        }
    }

    pub(crate) fn present_managed_agents<'a>(
        &self,
        agent_names: impl IntoIterator<Item = &'a str>,
    ) -> Vec<String> {
        let names = agent_names.into_iter().collect::<BTreeSet<_>>();
        self.snapshot
            .as_ref()
            .into_iter()
            .flat_map(|snapshot| snapshot.agents.iter())
            .filter_map(|agent| agent.name.as_deref())
            .filter(|name| names.contains(name))
            .map(str::to_owned)
            .collect()
    }

    fn ensure_terminal(&mut self, placement: &HerdrPlacement) -> Option<()> {
        self.drop_dead_terminal(&placement.pane_id);
        if self.terminals.contains_key(&placement.pane_id) {
            return Some(());
        }
        if !self.allow_terminal_control {
            return None;
        }
        let herdr_bin = self.herdr_bin.clone()?;
        let (default_cols, default_rows) = default_terminal_size();
        let (cols, rows) = self
            .terminal_sizes
            .get(&placement.pane_id)
            .copied()
            .unwrap_or((default_cols, default_rows));
        match TerminalSession::attach(TerminalAttach {
            herdr_bin,
            api_socket: self.client.socket().to_path_buf(),
            session: self.session.clone(),
            target: placement.pane_id.clone(),
            cols,
            rows,
            takeover: true,
        }) {
            Ok(session) => {
                self.terminals.insert(placement.pane_id.clone(), session);
                Some(())
            }
            Err(error) => {
                let message = error.public_message();
                if !self.status.issues.iter().any(|issue| issue == &message) {
                    self.status.issues.push(message);
                }
                None
            }
        }
    }

    fn drop_dead_terminal(&mut self, pane_id: &str) {
        let dead = self
            .terminals
            .get_mut(pane_id)
            .is_some_and(|session| !session.is_alive());
        if dead {
            self.terminals.remove(pane_id);
        }
    }
}

fn live_terminal_control() -> (bool, Option<PathBuf>) {
    if std::env::var_os("AGENT_FACTORY_HERDR_SOCKET").is_some() {
        return (false, None);
    }
    (true, resolve_herdr_bin().ok())
}

fn workspace_terminal_arguments(session: Option<&str>) -> Vec<String> {
    match session {
        Some(session) => vec!["--session".into(), session.into()],
        None => Vec::new(),
    }
}

pub(crate) fn lifecycle_from_status(status: AgentStatus) -> AgentLifecycle {
    match status {
        AgentStatus::Idle => AgentLifecycle::Idle,
        AgentStatus::Working => AgentLifecycle::Working,
        AgentStatus::Blocked => AgentLifecycle::Blocked,
        AgentStatus::Done => AgentLifecycle::Done,
        AgentStatus::Unknown => AgentLifecycle::Unknown,
    }
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Herdr agent names must match `[a-z][a-z0-9_-]{0,31}` and be unique among live
/// agents, so a session's name is derived from its purpose and its own id.
pub(crate) fn agent_name(prefix: &str, context: &str, session_id: uuid::Uuid) -> String {
    let suffix = crate::short_id(session_id);
    // Budget only what the prefix, suffix, and two dashes actually cost.
    // Counting the context against itself left no room for it at all, so every
    // name collapsed to `orch--<id>` and told the reader nothing.
    let reserved = prefix.len() + suffix.len() + 2;
    let context = slug(context);
    let context = context
        .get(..context.len().min(32usize.saturating_sub(reserved)))
        .unwrap_or_default()
        .trim_end_matches('-');
    if context.is_empty() {
        return format!("{prefix}-{suffix}");
    }
    format!("{prefix}-{context}-{suffix}")
}

pub(crate) fn workspace_label(
    target_agent_name: &str,
    binding_name: &str,
    binding_id: uuid::Uuid,
) -> String {
    format!(
        "{} / {} ({})",
        human_label(target_agent_name),
        human_label(binding_name),
        crate::short_id(binding_id),
    )
}

fn human_label(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn slug(value: &str) -> String {
    let mut result = String::new();
    for character in value.chars() {
        let character = character.to_ascii_lowercase();
        if character.is_ascii_alphanumeric() {
            result.push(character);
        } else if !result.ends_with('-') {
            result.push('-');
        }
    }
    result.trim_matches('-').to_owned()
}

#[derive(Clone, Copy)]
struct SupportedHarness {
    id: &'static str,
    name: &'static str,
    install_command: &'static str,
    setup_command: &'static str,
}

/// Agent Factory's supported-manifest seed. Adding a row is a product
/// configuration decision; Herdr's manifest response still decides whether the
/// row exists and what readiness Herdr reports.
const SUPPORTED_HARNESS_MANIFEST: &[SupportedHarness] = &[
    SupportedHarness {
        id: "claude",
        name: "Claude Code",
        install_command: "npm install -g @anthropic-ai/claude-code",
        setup_command: "claude",
    },
    SupportedHarness {
        id: "codex",
        name: "Codex",
        install_command: "npm install -g @openai/codex",
        setup_command: "codex",
    },
];

fn harness_projection(manifest: herdr_client::AgentManifest) -> Option<HarnessProjection> {
    let supported = SUPPORTED_HARNESS_MANIFEST
        .iter()
        .find(|supported| supported.id == manifest.agent)?;
    let (readiness, guidance, action) = match manifest.warning.as_deref() {
        None => (
            HarnessReadinessState::Ready,
            "Ready to launch with Herdr.".to_owned(),
            None,
        ),
        Some(warning) if warning_indicates_missing_installation(warning) => (
            HarnessReadinessState::InstallationRequired,
            format!("Install {}, then restart Herdr.", supported.name),
            Some(HarnessActionProjection {
                label: "Copy install command".to_owned(),
                command: supported.install_command.to_owned(),
            }),
        ),
        Some(_) => (
            HarnessReadinessState::SetupRequired,
            format!(
                "Run {} once to finish setup, then restart Herdr.",
                supported.name
            ),
            Some(HarnessActionProjection {
                label: "Copy setup command".to_owned(),
                command: supported.setup_command.to_owned(),
            }),
        ),
    };
    Some(HarnessProjection {
        id: supported.id.to_owned(),
        name: supported.name.to_owned(),
        readiness,
        guidance,
        action,
    })
}

fn posix_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn prompt_attempt(prompted: PromptedAgent) -> PromptAttempt {
    PromptAttempt {
        lifecycle: lifecycle_from_status(prompted.info.agent_status),
    }
}

fn warning_indicates_missing_installation(warning: &str) -> bool {
    let warning = warning.to_ascii_lowercase();
    warning.contains("not installed")
        || (["command", "executable", "binary", "cli"]
            .iter()
            .any(|subject| warning.contains(subject))
            && ["not found", "missing", "unavailable", "could not run"]
                .iter()
                .any(|state| warning.contains(state)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(agent: &str, warning: Option<&str>) -> herdr_client::AgentManifest {
        herdr_client::AgentManifest {
            agent: agent.to_owned(),
            source: "builtin".to_owned(),
            source_kind: "builtin".to_owned(),
            active_version: None,
            warning: warning.map(str::to_owned),
        }
    }

    #[test]
    fn supported_manifest_seed_filters_the_herdr_catalog() {
        assert!(harness_projection(manifest("claude", None)).is_some());
        assert!(harness_projection(manifest("codex", None)).is_some());
        assert!(harness_projection(manifest("gemini", None)).is_none());
    }

    #[test]
    fn manifest_warnings_become_human_readiness_and_commands() {
        let ready = harness_projection(manifest("codex", None)).unwrap();
        assert_eq!(ready.readiness, HarnessReadinessState::Ready);
        assert!(ready.action.is_none());

        let installation = harness_projection(manifest(
            "codex",
            Some("Codex executable was not found on the Herdr PATH"),
        ))
        .unwrap();
        assert_eq!(
            installation.readiness,
            HarnessReadinessState::InstallationRequired
        );
        assert_eq!(
            installation.action.unwrap().command,
            "npm install -g @openai/codex"
        );

        let setup = harness_projection(manifest(
            "claude",
            Some("local override is older than the bundled manifest"),
        ))
        .unwrap();
        assert_eq!(setup.readiness, HarnessReadinessState::SetupRequired);
        assert_eq!(setup.action.unwrap().command, "claude");
    }

    #[test]
    fn posix_single_quote_wraps_and_escapes() {
        assert_eq!(posix_single_quote("glm-5.2:cloud"), "'glm-5.2:cloud'");
        assert_eq!(posix_single_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn agent_names_fit_herdrs_naming_rule() {
        let name = agent_name("coding", "Weather Reporter / Run 3", uuid::Uuid::new_v4());
        assert!(name.len() <= 32, "{name}");
        assert!(name.starts_with("coding-"));
        assert!(name.bytes().all(|byte| byte.is_ascii_lowercase()
            || byte.is_ascii_digit()
            || matches!(byte, b'-' | b'_')));
    }

    #[test]
    fn distinct_sessions_get_distinct_agent_names() {
        assert_ne!(
            agent_name("coding", "Weather Reporter", uuid::Uuid::new_v4()),
            agent_name("coding", "Weather Reporter", uuid::Uuid::new_v4())
        );
    }

    /// The point of the context is to say which Agent this is. Budgeting it
    /// against itself left no room for any of it, so every name read
    /// `coding--<id>` and the list told the reader nothing.
    #[test]
    fn an_agent_name_keeps_the_agent_it_belongs_to() {
        let name = agent_name("coding", "Weather Reporter", uuid::Uuid::new_v4());
        assert!(name.starts_with("coding-weather-reporter-"), "{name}");
        assert!(!name.contains("--"), "{name}");
    }

    /// A context too long to fit is dropped whole rather than leaving a stub or
    /// a dangling separator.
    #[test]
    fn an_overlong_context_leaves_a_clean_name() {
        let name = agent_name(
            "evaluation",
            &"Weather Reporter ".repeat(20),
            uuid::Uuid::new_v4(),
        );
        assert!(name.len() <= 32, "{name}");
        assert!(!name.contains("--"), "{name}");
        assert!(!name.ends_with('-'), "{name}");
        assert!(name.starts_with("evaluation-"), "{name}");
    }

    #[test]
    fn workspace_labels_are_human_readable_and_disambiguated() {
        let id = uuid::Uuid::parse_str("12345678-1234-4234-8234-123456789abc").unwrap();
        let label = workspace_label("Weather Reporter", "Run 3", id);
        assert_eq!(label, "Weather Reporter / Run 3 (33c)");
        // The names people read carry the meaning; the suffix only has to keep
        // two same-named bindings apart, so it stays short enough to say aloud.
        assert_eq!(
            workspace_label("Weather Reporter", "Run 3", uuid::Uuid::new_v4()).len(),
            label.len()
        );
    }

    #[test]
    fn workspace_terminal_selects_only_the_configured_herdr_session() {
        assert!(workspace_terminal_arguments(None).is_empty());
        assert_eq!(
            workspace_terminal_arguments(Some("agent-factory-dev")),
            ["--session", "agent-factory-dev"]
        );
    }

    #[test]
    fn an_unclassified_agent_is_not_treated_as_ready() {
        assert_eq!(
            lifecycle_from_status(AgentStatus::Unknown),
            AgentLifecycle::Unknown
        );
        assert!(!AgentLifecycle::Unknown.accepts_prompt());
    }
}
