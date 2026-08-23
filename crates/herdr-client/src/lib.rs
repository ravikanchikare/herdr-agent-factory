//! Typed client for the Herdr control socket.
//!
//! Herdr owns terminal topology (workspaces, tabs, panes) and the coding agents
//! running inside panes. Agent Factory drives it as a socket client: it creates
//! panes with an environment boundary applied, starts a recognized agent kind in
//! one, submits prompts, reads transcripts, and reflects lifecycle state that
//! Herdr reports. Nothing here reconstructs structured protocol traffic — Herdr
//! publishes lifecycle states and terminal text, and that is what this exposes.

mod error;
mod events;
mod model;
mod terminal_session;
mod transport;

pub use error::{HerdrError, REQUIRED_PROTOCOL};
pub use events::HerdrEvents;
pub use model::*;
pub use terminal_session::{
    TerminalAttach, TerminalSession, default_terminal_size, resolve_herdr_bin,
};
pub use transport::{config_root, socket_path};

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{Value, json};

use transport::{Connection, into_result};

/// Default deadline for control calls that settle immediately.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);
/// Herdr's own `agent start` default; long enough for a TUI agent to boot.
pub const DEFAULT_START_TIMEOUT: Duration = Duration::from_secs(30);
/// How long to retry `agent_pane_busy` / `agent_not_ready` before giving up.
pub const TRANSIENT_RETRY_TIMEOUT: Duration = Duration::from_secs(10);
const TRANSIENT_RETRY_INTERVAL: Duration = Duration::from_millis(100);

/// Retry `op` while Herdr reports a transient start/prompt readiness error.
pub fn retry_transient<T>(
    timeout: Duration,
    mut op: impl FnMut() -> Result<T, HerdrError>,
) -> Result<T, HerdrError> {
    let deadline = Instant::now() + timeout;
    loop {
        match op() {
            Ok(value) => return Ok(value),
            Err(error) if error.is_transient() && Instant::now() < deadline => {
                std::thread::sleep(TRANSIENT_RETRY_INTERVAL);
            }
            Err(error) => return Err(error),
        }
    }
}

/// A connection factory for one Herdr session.
#[derive(Clone, Debug)]
pub struct HerdrClient {
    socket: PathBuf,
    timeout: Duration,
}

impl HerdrClient {
    pub fn new(socket: PathBuf) -> Self {
        Self {
            socket,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Resolve the socket for the default session, or a named session when the
    /// deployment isolates Agent Factory from the user's own Herdr session.
    pub fn discover(session: Option<&str>) -> Result<Self, HerdrError> {
        Ok(Self::new(socket_path(&config_root()?, session)))
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn socket(&self) -> &Path {
        &self.socket
    }

    /// One request, one connection. Herdr closes the socket after responding.
    fn call<P: Serialize>(
        &self,
        method: &str,
        params: P,
        timeout: Duration,
    ) -> Result<Value, HerdrError> {
        let mut connection = Connection::open(&self.socket, timeout)?;
        connection.send(method, params)?;
        let frame = connection
            .read_frame()?
            .ok_or_else(|| HerdrError::Protocol(format!("{method} returned no response")))?;
        into_result(frame)
    }

    fn field<T: serde::de::DeserializeOwned>(
        result: Value,
        key: &str,
        method: &str,
    ) -> Result<T, HerdrError> {
        let value = result
            .get(key)
            .cloned()
            .ok_or_else(|| HerdrError::Protocol(format!("{method} response is missing `{key}`")))?;
        Ok(serde_json::from_value(value)?)
    }

    /// Confirm Herdr is running and speaks a protocol Agent Factory supports.
    pub fn probe(&self) -> Result<ServerInfo, HerdrError> {
        let info: ServerInfo =
            serde_json::from_value(self.call("ping", json!({}), self.timeout)?)?;
        if info.protocol < REQUIRED_PROTOCOL {
            return Err(HerdrError::IncompatibleProtocol {
                server: info.protocol,
            });
        }
        Ok(info)
    }

    /// The agent kinds this Herdr installation can launch and recognize.
    pub fn agent_manifests(&self) -> Result<Vec<AgentManifest>, HerdrError> {
        let result = self.call("server.agent_manifests", json!({}), self.timeout)?;
        Self::field(result, "manifests", "server.agent_manifests")
    }

    pub fn create_workspace(
        &self,
        label: Option<&str>,
        spec: &PaneSpec,
    ) -> Result<WorkspaceCreated, HerdrError> {
        let result = self.call(
            "workspace.create",
            omit_nulls(json!({
                "label": label,
                "cwd": spec.cwd,
                "env": spec.env,
                "focus": false,
            })),
            self.timeout,
        )?;
        Ok(serde_json::from_value(result)?)
    }

    pub fn workspaces(&self) -> Result<Vec<WorkspaceInfo>, HerdrError> {
        let result = self.call("workspace.list", json!({}), self.timeout)?;
        Self::field(result, "workspaces", "workspace.list")
    }

    /// Read one complete live topology snapshot. Callers use events only as
    /// invalidations and replace their cache from this response.
    pub fn session_snapshot(&self) -> Result<SessionSnapshot, HerdrError> {
        let result = self.call("session.snapshot", json!({}), self.timeout)?;
        Self::field(result, "snapshot", "session.snapshot")
    }

    /// Ask Herdr to create a linked Git worktree and open its Workspace.
    #[allow(clippy::too_many_arguments)]
    pub fn create_worktree(
        &self,
        source_cwd: &Path,
        branch: &str,
        base: &str,
        path: &Path,
        label: &str,
        focus: bool,
    ) -> Result<WorktreeCreated, HerdrError> {
        let result = self.call(
            "worktree.create",
            json!({
                "cwd": source_cwd,
                "branch": branch,
                "base": base,
                "path": path,
                "label": label,
                "focus": focus,
            }),
            self.timeout,
        )?;
        Ok(serde_json::from_value(result)?)
    }

    /// Ask Herdr to open an existing checkout as a Workspace.
    pub fn open_worktree(
        &self,
        source_cwd: &Path,
        path: &Path,
        label: &str,
        focus: bool,
    ) -> Result<WorktreeOpened, HerdrError> {
        let result = self.call(
            "worktree.open",
            json!({
                "cwd": source_cwd,
                "path": path,
                "label": label,
                "focus": focus,
            }),
            self.timeout,
        )?;
        Ok(serde_json::from_value(result)?)
    }

    pub fn worktrees(&self, cwd: &Path) -> Result<WorktreeList, HerdrError> {
        let result = self.call("worktree.list", json!({"cwd": cwd}), self.timeout)?;
        Ok(serde_json::from_value(result)?)
    }

    /// Remove a linked checkout through Herdr. Herdr never deletes its branch.
    pub fn remove_worktree(
        &self,
        workspace_id: &str,
        force: bool,
    ) -> Result<WorktreeRemoved, HerdrError> {
        let result = self.call(
            "worktree.remove",
            json!({"workspace_id": workspace_id, "force": force}),
            self.timeout,
        )?;
        Ok(serde_json::from_value(result)?)
    }

    pub fn close_workspace(&self, workspace_id: &str) -> Result<(), HerdrError> {
        self.call(
            "workspace.close",
            json!({"workspace_id": workspace_id}),
            self.timeout,
        )?;
        Ok(())
    }

    pub fn create_tab(
        &self,
        workspace_id: &str,
        label: Option<&str>,
        spec: &PaneSpec,
    ) -> Result<TabCreated, HerdrError> {
        let result = self.call(
            "tab.create",
            omit_nulls(json!({
                "workspace_id": workspace_id,
                "label": label,
                "cwd": spec.cwd,
                "env": spec.env,
                "focus": false,
            })),
            self.timeout,
        )?;
        Ok(serde_json::from_value(result)?)
    }

    pub fn close_tab(&self, tab_id: &str) -> Result<(), HerdrError> {
        self.call("tab.close", json!({"tab_id": tab_id}), self.timeout)?;
        Ok(())
    }

    /// Split an existing pane. The new pane carries its own cwd and environment.
    pub fn split_pane(
        &self,
        target_pane_id: &str,
        direction: SplitDirection,
        spec: &PaneSpec,
    ) -> Result<PaneInfo, HerdrError> {
        let result = self.call(
            "pane.split",
            omit_nulls(json!({
                "target_pane_id": target_pane_id,
                "direction": direction,
                "cwd": spec.cwd,
                "env": spec.env,
                "focus": false,
            })),
            self.timeout,
        )?;
        Self::field(result, "pane", "pane.split")
    }

    pub fn pane(&self, pane_id: &str) -> Result<PaneInfo, HerdrError> {
        let result = self.call("pane.get", json!({"pane_id": pane_id}), self.timeout)?;
        Self::field(result, "pane", "pane.get")
    }

    pub fn close_pane(&self, pane_id: &str) -> Result<(), HerdrError> {
        self.call("pane.close", json!({"pane_id": pane_id}), self.timeout)?;
        Ok(())
    }

    pub fn read_pane(
        &self,
        pane_id: &str,
        source: ReadSource,
        lines: Option<u32>,
    ) -> Result<PaneRead, HerdrError> {
        let result = self.call(
            "pane.read",
            omit_nulls(json!({
                "pane_id": pane_id,
                "source": source,
                "lines": lines,
                "format": ReadFormat::Text,
                "strip_ansi": true,
            })),
            self.timeout,
        )?;
        Self::field(result, "read", "pane.read")
    }

    /// Write raw terminal bytes into a pane PTY.
    ///
    /// This is the interactive path: it does not wrap the payload in bracketed
    /// paste. Use [`Self::send_pane_input`] when the caller has logical keys.
    pub fn send_pane_text(&self, pane_id: &str, text: &str) -> Result<(), HerdrError> {
        self.call(
            "pane.send_text",
            json!({"pane_id": pane_id, "text": text}),
            self.timeout,
        )?;
        Ok(())
    }

    /// Send literal text plus logical keys to a raw pane.
    ///
    /// Herdr's `text` field is a string, not `Option`. Omit empty fields instead
    /// of sending JSON null, which Herdr rejects as `invalid type: null`.
    /// Text sent this way is paste-wrapped when the pane has bracketed paste
    /// enabled, so typed Ghostty bytes must use [`Self::send_pane_text`].
    pub fn send_pane_input(
        &self,
        pane_id: &str,
        text: Option<&str>,
        keys: &[&str],
    ) -> Result<(), HerdrError> {
        self.call(
            "pane.send_input",
            pane_send_input_params(pane_id, text, keys),
            self.timeout,
        )?;
        Ok(())
    }

    /// Start a recognized agent kind in an available shell pane.
    ///
    /// Herdr returns once it has detected the agent in that pane and considers
    /// it ready for interactive input.
    pub fn start_agent(
        &self,
        name: &str,
        kind: &str,
        pane_id: &str,
        args: &[String],
        timeout: Duration,
    ) -> Result<AgentInfo, HerdrError> {
        let result = self.call(
            "agent.start",
            json!({
                "name": name,
                "kind": kind,
                "pane_id": pane_id,
                "args": args,
                "timeout_ms": timeout.as_millis() as u64,
            }),
            timeout + self.timeout,
        )?;
        Self::field(result, "agent", "agent.start")
    }

    /// Submit a prompt. With `wait`, Herdr returns on the first settled state.
    pub fn prompt_agent(
        &self,
        target: &str,
        text: &str,
        wait: Option<Duration>,
    ) -> Result<AgentInfo, HerdrError> {
        let params = match wait {
            Some(timeout) => json!({
                "target": target,
                "text": text,
                "wait": {"timeout_ms": timeout.as_millis() as u64},
            }),
            None => json!({"target": target, "text": text}),
        };
        let deadline = wait.unwrap_or(Duration::ZERO) + self.timeout;
        let result = self.call("agent.prompt", params, deadline)?;
        Self::field(result, "agent", "agent.prompt")
    }

    /// Wait for a settled state, or for specific states when given.
    pub fn wait_agent(
        &self,
        target: &str,
        until: &[AgentStatus],
        timeout: Duration,
    ) -> Result<AgentInfo, HerdrError> {
        let result = self.call(
            "agent.wait",
            json!({
                "target": target,
                "until": until,
                "timeout_ms": timeout.as_millis() as u64,
            }),
            timeout + self.timeout,
        )?;
        Self::field(result, "agent", "agent.wait")
    }

    pub fn agent(&self, target: &str) -> Result<AgentInfo, HerdrError> {
        let result = self.call("agent.get", json!({"target": target}), self.timeout)?;
        Self::field(result, "agent", "agent.get")
    }

    pub fn agents(&self) -> Result<Vec<AgentInfo>, HerdrError> {
        let result = self.call("agent.list", json!({}), self.timeout)?;
        Self::field(result, "agents", "agent.list")
    }

    pub fn read_agent(
        &self,
        target: &str,
        source: ReadSource,
        lines: Option<u32>,
    ) -> Result<PaneRead, HerdrError> {
        self.read_agent_formatted(target, source, lines, ReadFormat::Text, true)
    }

    /// Visible ANSI snapshot for rendering the agent's own TUI.
    pub fn read_agent_screen(&self, target: &str) -> Result<PaneRead, HerdrError> {
        self.read_agent_formatted(target, ReadSource::Visible, None, ReadFormat::Ansi, false)
    }

    fn read_agent_formatted(
        &self,
        target: &str,
        source: ReadSource,
        lines: Option<u32>,
        format: ReadFormat,
        strip_ansi: bool,
    ) -> Result<PaneRead, HerdrError> {
        let result = self.call(
            "agent.read",
            omit_nulls(json!({
                "target": target,
                "source": source,
                "lines": lines,
                "format": format,
                "strip_ansi": strip_ansi,
            })),
            self.timeout,
        )?;
        Self::field(result, "read", "agent.read")
    }

    /// Send logical keys to an agent's interactive UI, such as `esc` or `ctrl+c`.
    pub fn send_agent_keys(&self, target: &str, keys: &[String]) -> Result<(), HerdrError> {
        self.call(
            "agent.send_keys",
            json!({"target": target, "keys": keys}),
            self.timeout,
        )?;
        Ok(())
    }

    /// Bring the agent's pane to the front of the Herdr UI.
    pub fn focus_agent(&self, target: &str) -> Result<(), HerdrError> {
        self.call("agent.focus", json!({"target": target}), self.timeout)?;
        Ok(())
    }

    /// Bind a live pane occupant to a unique agent name.
    pub fn rename_agent(&self, target: &str, name: &str) -> Result<(), HerdrError> {
        self.call(
            "agent.rename",
            json!({"target": target, "name": name}),
            self.timeout,
        )?;
        Ok(())
    }

    /// Release Herdr's agent binding for a pane without closing the pane.
    pub fn release_agent(&self, pane_id: &str, agent: &str) -> Result<(), HerdrError> {
        self.call(
            "pane.release_agent",
            json!({"pane_id": pane_id, "agent": agent, "source": "agent-factory"}),
            self.timeout,
        )?;
        Ok(())
    }

    /// Open the long-lived subscription stream Agent Factory reflects state from.
    pub fn subscribe(
        &self,
        agent_pane_ids: impl IntoIterator<Item = String>,
    ) -> Result<HerdrEvents, HerdrEventsError> {
        HerdrEvents::open(&self.socket, self.timeout, agent_pane_ids)
    }
}

fn pane_send_input_params(pane_id: &str, text: Option<&str>, keys: &[&str]) -> Value {
    let mut params = serde_json::Map::new();
    params.insert("pane_id".into(), json!(pane_id));
    if let Some(text) = text.filter(|value| !value.is_empty()) {
        params.insert("text".into(), json!(text));
    }
    if !keys.is_empty() {
        params.insert("keys".into(), json!(keys));
    }
    Value::Object(params)
}

fn omit_nulls(value: Value) -> Value {
    match value {
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .filter(|(_, candidate)| !candidate.is_null())
                .map(|(key, candidate)| (key, omit_nulls(candidate)))
                .collect(),
        ),
        other => other,
    }
}

/// Alias kept for readability at the call site.
pub type HerdrEventsError = HerdrError;

/// Environment variables and working directory for a pane, as one boundary.
impl PaneSpec {
    pub fn new(cwd: Option<String>, env: BTreeMap<String, String>) -> Self {
        Self {
            cwd,
            env,
            label: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_socket_reports_herdr_as_unreachable() {
        let client = HerdrClient::new(PathBuf::from("/nonexistent/herdr.sock"))
            .with_timeout(Duration::from_millis(200));
        let error = client.probe().unwrap_err();
        assert!(error.is_unreachable());
    }

    #[test]
    fn pane_send_input_omits_null_text() {
        let keys_only = pane_send_input_params("w1:p2", None, &["enter"]);
        assert_eq!(keys_only["pane_id"], "w1:p2");
        assert_eq!(keys_only["keys"], json!(["enter"]));
        assert!(keys_only.get("text").is_none());

        let text_only = pane_send_input_params("w1:p2", Some("hello"), &[]);
        assert_eq!(text_only["text"], "hello");
        assert!(text_only.get("keys").is_none());
    }
}
