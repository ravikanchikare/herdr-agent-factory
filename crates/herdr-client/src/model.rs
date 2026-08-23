//! Wire types for the Herdr socket API.
//!
//! These mirror the shapes Herdr publishes through `herdr api schema`. Optional
//! fields stay optional and unknown fields are ignored so a newer Herdr server
//! never breaks the client.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Lifecycle state Herdr reports for the agent occupying a pane.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    /// Ready for input and the tab has been seen in the focused Herdr UI.
    Idle,
    /// Actively producing output.
    Working,
    /// Herdr recognized an approval or question surface inside the agent.
    Blocked,
    /// Unseen background work finished; the same underlying idle state.
    Done,
    /// An agent is present but Herdr cannot classify it confidently.
    #[default]
    Unknown,
}

impl AgentStatus {
    /// Whether Herdr considers the agent settled and ready for a new prompt.
    pub fn is_settled(self) -> bool {
        matches!(self, Self::Idle | Self::Done | Self::Blocked)
    }
}

/// Which pane snapshot a read should return.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReadSource {
    /// The currently rendered viewport.
    Visible,
    /// Recent rendered output including soft wraps.
    Recent,
    /// Recent output with soft wraps joined. Preferred for transcripts.
    #[default]
    RecentUnwrapped,
    /// The plain-text bottom-buffer snapshot used for agent detection.
    Detection,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReadFormat {
    #[default]
    Text,
    Ansi,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SplitDirection {
    Right,
    Down,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerCapabilities {
    #[serde(default)]
    pub live_handoff: bool,
    #[serde(default)]
    pub detached_server_daemon: bool,
}

/// Result of `ping`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerInfo {
    pub version: String,
    pub protocol: u32,
    #[serde(default)]
    pub capabilities: ServerCapabilities,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub workspace_id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub active_tab_id: String,
    #[serde(default)]
    pub focused: bool,
    #[serde(default)]
    pub agent_status: AgentStatus,
    #[serde(default)]
    pub worktree: Option<WorkspaceWorktreeInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceWorktreeInfo {
    pub repo_key: String,
    pub repo_name: String,
    pub repo_root: String,
    pub checkout_path: String,
    pub is_linked_worktree: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabInfo {
    pub tab_id: String,
    pub workspace_id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub focused: bool,
    #[serde(default)]
    pub agent_status: AgentStatus,
}

/// Reference an agent published about its own native session, when it has one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSessionRef {
    pub source: String,
    pub agent: String,
    pub kind: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneInfo {
    pub pane_id: String,
    pub workspace_id: String,
    pub tab_id: String,
    #[serde(default)]
    pub terminal_id: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub foreground_cwd: Option<String>,
    #[serde(default)]
    pub focused: bool,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub display_agent: Option<String>,
    #[serde(default)]
    pub agent_status: AgentStatus,
    #[serde(default)]
    pub agent_session: Option<AgentSessionRef>,
    #[serde(default)]
    pub revision: u64,
}

/// A live agent as Herdr resolves it. `name` is the unique handle callers use.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentInfo {
    pub pane_id: String,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub tab_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub display_agent: Option<String>,
    #[serde(default)]
    pub agent_status: AgentStatus,
    #[serde(default)]
    pub agent_session: Option<AgentSessionRef>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub interactive_ready: bool,
    #[serde(default)]
    pub launch_pending: bool,
    #[serde(default)]
    pub state_labels: BTreeMap<String, String>,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub state_change_seq: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneRead {
    pub pane_id: String,
    pub workspace_id: String,
    pub tab_id: String,
    pub source: ReadSource,
    pub format: ReadFormat,
    pub text: String,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub truncated: bool,
}

/// An agent kind Herdr knows how to launch and recognize.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentManifest {
    pub agent: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub source_kind: String,
    #[serde(default)]
    pub active_version: Option<String>,
    #[serde(default)]
    pub warning: Option<String>,
}

/// Result of `workspace.create` and `tab.create`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceCreated {
    pub workspace: WorkspaceInfo,
    pub tab: TabInfo,
    pub root_pane: PaneInfo,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabCreated {
    pub tab: TabInfo,
    pub root_pane: PaneInfo,
}

/// Complete live topology returned by `session.snapshot`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub version: String,
    pub protocol: u32,
    #[serde(default)]
    pub focused_workspace_id: Option<String>,
    #[serde(default)]
    pub focused_tab_id: Option<String>,
    #[serde(default)]
    pub focused_pane_id: Option<String>,
    #[serde(default)]
    pub workspaces: Vec<WorkspaceInfo>,
    #[serde(default)]
    pub tabs: Vec<TabInfo>,
    #[serde(default)]
    pub panes: Vec<PaneInfo>,
    #[serde(default)]
    pub agents: Vec<AgentInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeSourceInfo {
    pub repo_key: String,
    pub repo_name: String,
    pub repo_root: String,
    pub source_checkout_path: String,
    #[serde(default)]
    pub source_workspace_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeInfo {
    pub path: String,
    #[serde(default)]
    pub branch: Option<String>,
    pub is_bare: bool,
    pub is_detached: bool,
    pub is_prunable: bool,
    pub is_linked_worktree: bool,
    #[serde(default)]
    pub open_workspace_id: Option<String>,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeList {
    pub source: WorktreeSourceInfo,
    pub worktrees: Vec<WorktreeInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeCreated {
    pub workspace: WorkspaceInfo,
    pub tab: TabInfo,
    pub root_pane: PaneInfo,
    pub worktree: WorktreeInfo,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeOpened {
    pub workspace: WorkspaceInfo,
    pub tab: TabInfo,
    pub root_pane: PaneInfo,
    pub worktree: WorktreeInfo,
    pub already_open: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeRemoved {
    pub workspace_id: String,
    pub path: String,
    pub forced: bool,
}

/// A pane to create, with the environment boundary applied at creation time.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct PaneSpec {
    pub cwd: Option<String>,
    pub env: BTreeMap<String, String>,
    pub label: Option<String>,
}

/// Global event topics Agent Factory uses as snapshot invalidations.
///
/// `pane.agent_status_changed` is intentionally absent: Herdr requires one
/// subscription per pane for that event, so the event client adds those from
/// the latest snapshot's live Agent identities.
pub const GLOBAL_SUBSCRIPTIONS: [&str; 23] = [
    "workspace.created",
    "workspace.updated",
    "workspace.metadata_updated",
    "workspace.closed",
    "workspace.renamed",
    "workspace.moved",
    "workspace.reordered",
    "worktree.created",
    "worktree.opened",
    "worktree.removed",
    "tab.created",
    "tab.closed",
    "tab.renamed",
    "tab.moved",
    "pane.agent_detected",
    "pane.exited",
    "pane.closed",
    "pane.updated",
    "pane.created",
    "pane.focused",
    "pane.moved",
    "tab.focused",
    "layout.updated",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pane_info_tolerates_unknown_fields() {
        let pane: PaneInfo = serde_json::from_str(
            r#"{"pane_id":"w1:p1","workspace_id":"w1","tab_id":"w1:t1","scroll":{"offset_from_bottom":0},"future_field":true}"#,
        )
        .unwrap();
        assert_eq!(pane.pane_id, "w1:p1");
        assert_eq!(pane.agent_status, AgentStatus::Unknown);
    }

    #[test]
    fn settled_states_exclude_working() {
        assert!(AgentStatus::Idle.is_settled());
        assert!(AgentStatus::Done.is_settled());
        assert!(AgentStatus::Blocked.is_settled());
        assert!(!AgentStatus::Working.is_settled());
        assert!(!AgentStatus::Unknown.is_settled());
    }
}
