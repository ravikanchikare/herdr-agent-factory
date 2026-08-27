//! Factory Run start/orchestration against a scripted Herdr stand-in.
//!
//! These tests drive the shipped Runtime dispatch + `poll_events` path. They
//! never connect to the developer's live Herdr session.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use ipc_contract::{Frame, Request, Response, ResponseOutcome};
use project_store::ProjectStore;
use serde_json::{Value, json};
use tempfile::TempDir;

use super::Runtime;

const AGENT_NAME: &str = r"^[a-z][a-z0-9_-]{0,31}$";

pub(super) struct ScriptedHerdr {
    socket: PathBuf,
    requests: Arc<Mutex<Vec<Value>>>,
    events: Arc<Mutex<Option<Sender<Value>>>>,
    state: Arc<Mutex<HerdrState>>,
    _directory: TempDir,
}

struct HerdrState {
    workspaces: Vec<(String, String)>,
    worktrees: BTreeMap<String, (String, String)>,
    next_ordinal: u32,
    start_failures: u32,
    prompt_failures: u32,
    start_not_ready: bool,
    prompt_reject_names: bool,
    prompt_names_not_ready: bool,
    activation_required: bool,
    activation_attempted: bool,
    prompt_idle_remaining: u32,
    install_worktree_integration: bool,
    /// What `agent.list` reports, as `(name, pane_id, status)`.
    agents: Vec<(String, String, String)>,
    agent_list_fails: bool,
    close_keeps_agent: bool,
    /// Every live pane as `(pane_id, tab_id)`. A split shares its target's tab,
    /// so the mapping has to be recorded rather than derived from the id.
    panes: Vec<(String, String)>,
}

impl ScriptedHerdr {
    pub(super) fn basic() -> Self {
        Self::start_with(0, 0, false, false, false, false)
    }

    fn start_with(
        start_failures: u32,
        prompt_failures: u32,
        start_not_ready: bool,
        prompt_reject_names: bool,
        prompt_names_not_ready: bool,
        activation_required: bool,
    ) -> Self {
        let directory = TempDir::new().unwrap();
        let socket = directory.path().join("herdr.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let events: Arc<Mutex<Option<Sender<Value>>>> = Arc::new(Mutex::new(None));
        let state = Arc::new(Mutex::new(HerdrState {
            workspaces: Vec::new(),
            worktrees: BTreeMap::new(),
            next_ordinal: 1,
            start_failures,
            prompt_failures,
            start_not_ready,
            prompt_reject_names,
            prompt_names_not_ready,
            activation_required,
            activation_attempted: false,
            prompt_idle_remaining: 0,
            install_worktree_integration: false,
            agents: Vec::new(),
            agent_list_fails: false,
            close_keeps_agent: false,
            panes: Vec::new(),
        }));
        let recorded = Arc::clone(&requests);
        let event_slot = Arc::clone(&events);
        let shared_state = Arc::clone(&state);
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                let recorded = Arc::clone(&recorded);
                let event_slot = Arc::clone(&event_slot);
                let state = Arc::clone(&shared_state);
                thread::spawn(move || serve(stream, recorded, event_slot, state));
            }
        });
        Self {
            socket,
            requests,
            events,
            state,
            _directory: directory,
        }
    }

    fn set_start_not_ready(&self, start_not_ready: bool) {
        self.state.lock().expect("state").start_not_ready = start_not_ready;
    }

    fn set_prompt_failures(&self, prompt_failures: u32) {
        self.state.lock().expect("state").prompt_failures = prompt_failures;
    }

    fn set_prompt_idle_remaining(&self, prompt_idle_remaining: u32) {
        self.state.lock().expect("state").prompt_idle_remaining = prompt_idle_remaining;
    }

    pub(super) fn set_install_worktree_integration(&self, install: bool) {
        self.state
            .lock()
            .expect("state")
            .install_worktree_integration = install;
    }

    /// What `agent.list` reports, as `(name, pane_id, status)`.
    fn set_agents(&self, agents: &[(&str, &str, &str)]) {
        self.state.lock().expect("state").agents = agents
            .iter()
            .map(|(name, pane_id, status)| {
                (
                    (*name).to_owned(),
                    (*pane_id).to_owned(),
                    (*status).to_owned(),
                )
            })
            .collect();
    }

    fn set_agent_list_fails(&self, agent_list_fails: bool) {
        self.state.lock().expect("state").agent_list_fails = agent_list_fails;
    }

    fn set_close_keeps_agent(&self, keeps_agent: bool) {
        self.state.lock().expect("state").close_keeps_agent = keeps_agent;
    }

    fn add_agent(&self, name: &str, pane_id: &str, status: &str) {
        self.state.lock().expect("state").agents.push((
            name.to_owned(),
            pane_id.to_owned(),
            status.to_owned(),
        ));
    }

    /// Forget a pane without touching its agent, the way a pane closed outside
    /// Agent Factory disappears from Herdr between one call and the next.
    fn forget_pane(&self, pane_id: &str) {
        self.state
            .lock()
            .expect("state")
            .panes
            .retain(|(pane, _)| pane != pane_id);
    }

    /// Drop the subscriber, which ends the streaming connection the same way a
    /// Herdr server restart does.
    fn close_event_stream(&self) {
        *self.events.lock().expect("events") = None;
    }

    fn is_subscribed(&self) -> bool {
        self.events.lock().expect("events").is_some()
    }

    pub(super) fn socket(&self) -> PathBuf {
        self.socket.clone()
    }

    fn recorded(&self) -> Vec<Value> {
        self.requests.lock().expect("requests").clone()
    }

    fn emit_status(&self, pane_id: &str, status: &str) {
        // Herdr reports one truth through both the subscription and
        // `agent.list`, so the stand-in keeps them consistent.
        {
            let mut state = self.state.lock().expect("state");
            for (_, pane, existing) in state.agents.iter_mut() {
                if pane == pane_id {
                    *existing = status.to_owned();
                }
            }
        }
        let event = json!({
            "event": "pane.agent_status_changed",
            "data": {
                "pane_id": pane_id,
                "workspace_id": "w1",
                "agent_status": status
            }
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(sender) = self.events.lock().expect("events").as_ref()
                && sender.send(event.clone()).is_ok()
            {
                return;
            }
            if Instant::now() > deadline {
                panic!("Herdr event subscription was not ready to accept {status}");
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn emit_invalidation(&self) {
        let pane_id = self
            .state
            .lock()
            .expect("state")
            .agents
            .first()
            .map(|(_, pane_id, _)| pane_id.clone())
            .unwrap_or_else(|| "w1:p1".into());
        self.emit_status(&pane_id, "idle");
    }
}

fn serve(
    stream: UnixStream,
    recorded: Arc<Mutex<Vec<Value>>>,
    event_slot: Arc<Mutex<Option<Sender<Value>>>>,
    state: Arc<Mutex<HerdrState>>,
) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
    let mut writer = stream;
    let mut line = String::new();
    if reader.read_line(&mut line).unwrap_or(0) == 0 {
        return;
    }
    let request: Value = serde_json::from_str(&line).expect("request json");
    recorded.lock().expect("requests").push(request.clone());
    let method = request["method"].as_str().unwrap_or_default();

    if method == "events.subscribe" {
        let has_unscoped_agent_status =
            request["params"]["subscriptions"]
                .as_array()
                .is_some_and(|subscriptions| {
                    subscriptions.iter().any(|subscription| {
                        subscription["type"].as_str() == Some("pane.agent_status_changed")
                            && subscription["pane_id"].as_str().is_none()
                    })
                });
        if has_unscoped_agent_status {
            writeln!(
                writer,
                "{}",
                json!({
                    "id": request["id"],
                    "error": {
                        "code": "invalid_request",
                        "message": "missing field `pane_id`"
                    }
                })
            )
            .ok();
            writer.flush().ok();
            return;
        }
        writeln!(
            writer,
            "{}",
            json!({"id": request["id"], "result": {"type": "subscription_started"}})
        )
        .ok();
        writer.flush().ok();
        let (sender, receiver) = mpsc::channel();
        *event_slot.lock().expect("events") = Some(sender);
        for event in receiver {
            if writeln!(writer, "{event}").is_err() {
                break;
            }
            if writer.flush().is_err() {
                break;
            }
        }
        return;
    }

    if let Some(error) = method_error(method, &request, &state) {
        writeln!(writer, "{}", json!({"id": request["id"], "error": error})).ok();
        writer.flush().ok();
        return;
    }

    let result = method_result(method, &request, &state);
    writeln!(writer, "{}", json!({"id": request["id"], "result": result})).ok();
    writer.flush().ok();
}

fn method_error(method: &str, request: &Value, state: &Mutex<HerdrState>) -> Option<Value> {
    let mut state = state.lock().expect("state");
    match method {
        "session.snapshot" if state.agent_list_fails => Some(json!({
            "code": "internal",
            "message": "agent listing is unavailable"
        })),
        "agent.start" if state.activation_required && !state.activation_attempted => {
            state.activation_attempted = true;
            record_pending_agent(&mut state, request);
            Some(json!({
                "code": "agent_not_ready",
                "message": "agent launch is still pending"
            }))
        }
        "agent.start" if state.start_not_ready => {
            let name = request["params"]["name"].as_str().unwrap_or("unknown");
            record_pending_agent(&mut state, request);
            Some(json!({
                "code": "agent_not_ready",
                "message": format!("agent {name} is not an active named agent")
            }))
        }
        "agent.start" if state.start_failures > 0 => {
            state.start_failures -= 1;
            Some(json!({"code": "agent_pane_busy", "message": "pane is not ready"}))
        }
        "agent.prompt" if state.activation_required => Some(json!({
            "code": "agent_not_ready",
            "message": "agent launch is still pending"
        })),
        "agent.prompt"
            if (state.prompt_reject_names || state.prompt_names_not_ready)
                && !request["params"]["target"]
                    .as_str()
                    .unwrap_or_default()
                    .contains(':') =>
        {
            let target = request["params"]["target"].as_str().unwrap_or("unknown");
            Some(json!({
                "code": if state.prompt_names_not_ready {
                    "agent_not_ready"
                } else {
                    "agent_not_found"
                },
                "message": format!("agent target {target} is not prompt-ready")
            }))
        }
        "agent.prompt" if state.prompt_failures > 0 => {
            state.prompt_failures -= 1;
            let target = request["params"]["target"].as_str().unwrap_or("unknown");
            Some(json!({
                "code": "agent_not_ready",
                "message": format!("agent {target} is not an active named agent")
            }))
        }
        _ => None,
    }
}

fn method_result(method: &str, request: &Value, state: &Mutex<HerdrState>) -> Value {
    let mut state = state.lock().expect("state");
    match method {
        "ping" => {
            json!({"type": "pong", "version": "0.8.0", "protocol": herdr_client::REQUIRED_PROTOCOL})
        }
        "server.agent_manifests" => json!({
            "type": "agent_manifest_status",
            "manifests": [
                {"agent": "claude", "source": "builtin", "source_kind": "builtin"},
                {"agent": "codex", "source": "builtin", "source_kind": "builtin"}
            ]
        }),
        "workspace.list" => json!({
            "workspaces": state.workspaces.iter().map(|(id, label)| {
                json!({"workspace_id": id, "label": label, "active_tab_id": format!("{id}:t1")})
            }).collect::<Vec<_>>()
        }),
        "session.snapshot" => json!({
            "snapshot": {
                "version": "0.8.0",
                "protocol": herdr_client::REQUIRED_PROTOCOL,
                "workspaces": state.workspaces.iter().map(|(id, label)| {
                    json!({
                        "workspace_id": id,
                        "label": label,
                        "active_tab_id": format!("{id}:t1")
                    })
                }).collect::<Vec<_>>(),
                "tabs": [],
                // Panes carry their real tab: a column shares the tab of the
                // pane it was split from, so the tab cannot be derived from the
                // pane's own ordinal.
                "panes": state.panes.iter().map(|(pane_id, tab_id)| {
                    json!({
                        "pane_id": pane_id,
                        "workspace_id": pane_id.split(':').next().unwrap_or("w1"),
                        "tab_id": tab_id
                    })
                }).collect::<Vec<_>>(),
                "agents": state.agents.iter().map(|(name, pane_id, status)| {
                    let workspace_id = pane_id.split(':').next().unwrap_or("w1");
                    let tab_id = tab_id_for_pane(&state, pane_id)
                        .unwrap_or_else(|| format!("{workspace_id}:t1"));
                    json!({
                        "pane_id": pane_id,
                        "workspace_id": workspace_id,
                        "tab_id": tab_id,
                        "name": name,
                        "agent": "claude",
                        "display_agent": "Claude",
                        "agent_status": status,
                        "interactive_ready": true,
                        "launch_pending": false,
                        "revision": 1
                    })
                }).collect::<Vec<_>>()
            }
        }),
        "worktree.create" => {
            let cwd = request["params"]["cwd"].as_str().expect("worktree cwd");
            let path = request["params"]["path"].as_str().expect("worktree path");
            let branch = request["params"]["branch"]
                .as_str()
                .expect("worktree branch");
            let base = request["params"]["base"].as_str().expect("worktree base");
            let label = request["params"]["label"].as_str().expect("worktree label");
            let status = Command::new("git")
                .args(["worktree", "add", "-b", branch, path, base])
                .current_dir(cwd)
                .status()
                .expect("git worktree add");
            assert!(status.success(), "scripted Herdr could not create worktree");
            if state.install_worktree_integration {
                let worktree = Path::new(path);
                std::fs::write(worktree.join(".DS_Store"), "Finder metadata")
                    .expect("write desktop metadata");
                std::fs::create_dir_all(worktree.join(".agents/skills/herdr"))
                    .expect("create agent skill directory");
                std::fs::write(
                    worktree.join(".agents/skills/herdr/SKILL.md"),
                    "Herdr integration",
                )
                .expect("write agent skill");
                std::fs::create_dir_all(worktree.join(".claude/skills/herdr"))
                    .expect("create Claude skill directory");
                std::fs::write(worktree.join(".claude/.DS_Store"), "Finder metadata")
                    .expect("write Claude desktop metadata");
                std::fs::write(
                    worktree.join(".claude/skills/herdr/SKILL.md"),
                    "Herdr integration",
                )
                .expect("write Claude skill");
            }
            let workspace_id = format!("w{}", state.workspaces.len() + 1);
            state
                .workspaces
                .push((workspace_id.clone(), label.to_owned()));
            state
                .worktrees
                .insert(workspace_id.clone(), (cwd.to_owned(), path.to_owned()));
            let pane = next_pane(&mut state);
            json!({
                "workspace": {
                    "workspace_id": workspace_id,
                    "label": label,
                    "active_tab_id": pane.tab_id
                },
                "tab": {"tab_id": pane.tab_id, "workspace_id": workspace_id},
                "root_pane": pane.info(),
                "worktree": {
                    "path": path,
                    "branch": branch,
                    "is_bare": false,
                    "is_detached": false,
                    "is_prunable": false,
                    "is_linked_worktree": true,
                    "open_workspace_id": workspace_id,
                    "label": label
                }
            })
        }
        "worktree.open" => {
            let path = request["params"]["path"].as_str().expect("worktree path");
            let cwd = request["params"]["cwd"].as_str().expect("worktree cwd");
            let label = request["params"]["label"].as_str().expect("worktree label");
            let workspace_id = format!("w{}", state.workspaces.len() + 1);
            state
                .workspaces
                .push((workspace_id.clone(), label.to_owned()));
            state
                .worktrees
                .insert(workspace_id.clone(), (cwd.to_owned(), path.to_owned()));
            let pane = next_pane(&mut state);
            json!({
                "workspace": {"workspace_id": workspace_id, "label": label, "active_tab_id": pane.tab_id},
                "tab": {"tab_id": pane.tab_id, "workspace_id": workspace_id},
                "root_pane": pane.info(),
                "worktree": {
                    "path": path,
                    "branch": null,
                    "is_bare": false,
                    "is_detached": false,
                    "is_prunable": false,
                    "is_linked_worktree": true,
                    "open_workspace_id": workspace_id,
                    "label": label
                },
                "already_open": false
            })
        }
        "worktree.remove" => {
            let workspace_id = request["params"]["workspace_id"]
                .as_str()
                .expect("workspace id");
            let (cwd, path) = state
                .worktrees
                .remove(workspace_id)
                .expect("known worktree");
            let status = Command::new("git")
                .args(["worktree", "remove", &path])
                .current_dir(cwd)
                .status()
                .expect("git worktree remove");
            assert!(status.success(), "scripted Herdr could not remove worktree");
            state.workspaces.retain(|(id, _)| id != workspace_id);
            state
                .agents
                .retain(|(_, pane_id, _)| !pane_id.starts_with(&format!("{workspace_id}:")));
            json!({"workspace_id": workspace_id, "path": path, "forced": false})
        }
        "workspace.create" => {
            let label = request["params"]["label"].as_str().unwrap_or("factory");
            state.workspaces.push(("w1".into(), label.to_owned()));
            let pane = next_pane(&mut state);
            json!({
                "type": "workspace_created",
                "workspace": {"workspace_id": "w1", "label": label, "active_tab_id": pane.tab_id},
                "tab": {"tab_id": pane.tab_id, "workspace_id": "w1"},
                "root_pane": pane.info()
            })
        }
        "tab.create" => {
            let pane = next_pane(&mut state);
            json!({
                "type": "tab_created",
                "tab": {"tab_id": pane.tab_id, "workspace_id": "w1"},
                "root_pane": pane.info()
            })
        }
        "tab.close" => {
            if !state.close_keeps_agent {
                let tab_id = request["params"]["tab_id"]
                    .as_str()
                    .expect("tab close id")
                    .to_owned();
                let closed: Vec<String> = state
                    .panes
                    .iter()
                    .filter(|(_, tab)| *tab == tab_id)
                    .map(|(pane, _)| pane.clone())
                    .collect();
                state
                    .agents
                    .retain(|(_, pane_id, _)| !closed.contains(pane_id));
                state.panes.retain(|(_, tab)| *tab != tab_id);
            }
            json!({"type": "ok"})
        }
        "pane.get" => {
            let pane_id = request["params"]["pane_id"].as_str().unwrap_or_default();
            match tab_id_for_pane(&state, pane_id) {
                Some(tab_id) => json!({
                    "pane": {"pane_id": pane_id, "workspace_id": "w1", "tab_id": tab_id}
                }),
                // A pane Herdr no longer has. The runtime must treat a stored
                // pane id as a locator and open a tab instead of failing.
                None => json!({"pane": Value::Null}),
            }
        }
        "pane.split" => {
            let target = request["params"]["target_pane_id"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            match split_pane(&mut state, &target) {
                Some(pane) => json!({"pane": pane.info()}),
                None => json!({"pane": Value::Null}),
            }
        }
        "pane.close" => {
            if !state.close_keeps_agent {
                let pane_id = request["params"]["pane_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                state.agents.retain(|(_, pane, _)| *pane != pane_id);
                state.panes.retain(|(pane, _)| *pane != pane_id);
            }
            json!({"type": "ok"})
        }
        "agent.start" => {
            state.activation_required = false;
            let name = request["params"]["name"].as_str().unwrap_or("coding");
            let pane_id = request["params"]["pane_id"].as_str().unwrap_or("w1:p1");
            // Herdr reports a started agent through `agent.list` from then on.
            // Reconciliation reads that list, so the stand-in has to record it
            // or every session looks like it left its pane.
            state.agents.retain(|(existing, _, _)| existing != name);
            state
                .agents
                .push((name.to_owned(), pane_id.to_owned(), "idle".to_owned()));
            json!({
                "type": "agent_started",
                "agent": {
                    "pane_id": pane_id,
                    "name": name,
                    "agent": request["params"]["kind"],
                    "agent_status": "idle",
                    "interactive_ready": true
                }
            })
        }
        "agent.get" => {
            let ready = !state.start_not_ready;
            if ready {
                state.activation_required = false;
            }
            let target = request["params"]["target"].as_str().unwrap_or("w1:p1");
            json!({
                "type": "agent_info",
                "agent": {
                    "pane_id": target,
                    "name": "orchestrator",
                    "agent": "claude",
                    "agent_status": if ready { "idle" } else { "unknown" },
                    "interactive_ready": ready,
                    "launch_pending": !ready
                }
            })
        }
        "agent.prompt" => {
            let target = request["params"]["target"].as_str().unwrap_or("coding");
            let status = if state.prompt_idle_remaining > 0 {
                state.prompt_idle_remaining -= 1;
                "idle"
            } else {
                "working"
            };
            for (name, pane_id, current) in &mut state.agents {
                if name == target || pane_id == target {
                    *current = status.to_owned();
                }
            }
            json!({
                "type": "agent_prompted",
                "agent": {
                    "pane_id": "w1:p1",
                    "name": target,
                    "agent": "claude",
                    "agent_status": status
                }
            })
        }
        "agent.list" => json!({
            "agents": state.agents.iter().map(|(name, pane_id, status)| {
                json!({
                    "pane_id": pane_id,
                    "workspace_id": pane_id.split(':').next().unwrap_or("w1"),
                    "name": name,
                    "agent": "claude",
                    "agent_status": status
                })
            }).collect::<Vec<_>>()
        }),
        "agent.read" => json!({
            "type": "pane_read",
            "read": {
                "pane_id": "w1:p1",
                "workspace_id": "w1",
                "tab_id": "w1:t1",
                "source": "recent_unwrapped",
                "format": "text",
                "text": "",
                "revision": 1,
                "truncated": false
            }
        }),
        other => json!({"type": "ok", "echo": other}),
    }
}

fn record_pending_agent(state: &mut HerdrState, request: &Value) {
    let name = request["params"]["name"].as_str().unwrap_or("agent");
    let pane_id = request["params"]["pane_id"].as_str().unwrap_or("w1:p1");
    state.agents.retain(|(existing, _, _)| existing != name);
    state
        .agents
        .push((name.to_owned(), pane_id.to_owned(), "unknown".to_owned()));
}

struct CreatedPane {
    tab_id: String,
    pane_id: String,
}

impl CreatedPane {
    fn info(&self) -> Value {
        json!({
            "pane_id": self.pane_id,
            "workspace_id": "w1",
            "tab_id": self.tab_id
        })
    }
}

fn next_pane(state: &mut HerdrState) -> CreatedPane {
    let ordinal = state.next_ordinal;
    state.next_ordinal += 1;
    let pane = CreatedPane {
        tab_id: format!("w1:t{ordinal}"),
        pane_id: format!("w1:p{ordinal}"),
    };
    state
        .panes
        .push((pane.pane_id.clone(), pane.tab_id.clone()));
    pane
}

/// A pane added beside `target`, sharing the tab the target already sits in.
fn split_pane(state: &mut HerdrState, target: &str) -> Option<CreatedPane> {
    let tab_id = tab_id_for_pane(state, target)?;
    let ordinal = state.next_ordinal;
    state.next_ordinal += 1;
    let pane = CreatedPane {
        tab_id,
        pane_id: format!("w1:p{ordinal}"),
    };
    state
        .panes
        .push((pane.pane_id.clone(), pane.tab_id.clone()));
    Some(pane)
}

fn tab_id_for_pane(state: &HerdrState, pane_id: &str) -> Option<String> {
    state
        .panes
        .iter()
        .find(|(pane, _)| pane == pane_id)
        .map(|(_, tab)| tab.clone())
}

struct ModelCatalog {
    endpoint: String,
}

impl ModelCatalog {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else {
                    continue;
                };
                let mut buffer = Vec::new();
                let mut chunk = [0_u8; 512];
                let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
                while let Ok(read) = stream.read(&mut chunk) {
                    if read == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..read]);
                    if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                    if buffer.len() > 8192 {
                        break;
                    }
                }
                let body = r#"{"models":[{"name":"glm-5.2:cloud"}]}"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Self {
            endpoint: format!("http://{addr}"),
        }
    }
}

struct FactoryHarness {
    runtime: Runtime,
    herdr: ScriptedHerdr,
    run_id: String,
    orchestrator_pane: String,
    _repository: TempDir,
    _catalog: ModelCatalog,
}

impl FactoryHarness {
    fn start(start_failures: u32, prompt_failures: u32) -> Self {
        Self::start_with(start_failures, prompt_failures, false)
    }

    fn start_not_ready() -> Self {
        Self::start_with(0, 0, true)
    }

    fn start_rejecting_names() -> Self {
        Self::start_configured(0, 0, false, true, false, false, 0)
    }

    fn start_with_launch_pending() -> Self {
        Self::start_configured(0, 0, false, false, false, true, 0)
    }

    fn start_with_idle_prompt() -> Self {
        Self::start_configured(0, 0, false, false, false, false, 1)
    }

    fn start_with(start_failures: u32, prompt_failures: u32, start_not_ready: bool) -> Self {
        Self::start_configured(
            start_failures,
            prompt_failures,
            start_not_ready,
            false,
            false,
            false,
            0,
        )
    }

    fn start_configured(
        start_failures: u32,
        prompt_failures: u32,
        start_not_ready: bool,
        prompt_reject_names: bool,
        prompt_names_not_ready: bool,
        activation_required: bool,
        prompt_idle_remaining: u32,
    ) -> Self {
        let herdr = ScriptedHerdr::start_with(
            start_failures,
            prompt_failures,
            start_not_ready,
            prompt_reject_names,
            prompt_names_not_ready,
            activation_required,
        );
        herdr.set_prompt_idle_remaining(prompt_idle_remaining);
        let catalog = ModelCatalog::start();
        let repository = test_repository();
        let mut runtime =
            Runtime::connected_to_herdr(ProjectStore::open_in_memory().unwrap(), herdr.socket());

        let created = expect_success(
            runtime.handle_request(Request::new(
                "targetAgent.create",
                json!({
                    "name": "Loop Agent",
                    "objective": "Build a review agent",
                    "acceptanceCriteria": ["The artifact meets the acceptance criteria"],
                    "repositoryRoot": repository.1,
                    "draftName": "main",
                    "trusted": true
                }),
            )),
            "targetAgent.create",
        );
        let draft_id = created["draft"]["id"].as_str().unwrap().to_owned();

        let provider_id = expect_success(
            runtime.handle_request(Request::new(
                "llmProvider.create",
                json!({
                    "configuration": {
                        "name": "Loop Provider",
                        "type": "ollama",
                        "endpoint": catalog.endpoint,
                        "credentialRef": null,
                        "allowedModels": ["glm-5.2:cloud"]
                    }
                }),
            )),
            "llmProvider.create",
        )["providerId"]
            .as_str()
            .unwrap()
            .to_owned();

        let environment_id = expect_success(
            runtime.handle_request(Request::new(
                "environment.create",
                json!({
                    "configuration": {
                        "name": "Loop Environment",
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
            )),
            "environment.create",
        )["environmentId"]
            .as_str()
            .unwrap()
            .to_owned();

        let requested_run_id = uuid::Uuid::new_v4();
        let created = expect_success(
            runtime.handle_request(Request::new(
                "factoryRun.create",
                json!({
                    "runId": requested_run_id,
                    "agentDraftId": draft_id,
                    "environmentId": environment_id,
                    "objective": "Ship the requested Factory improvement"
                }),
            )),
            "factoryRun.create",
        );
        let run_id = created["run"]["id"].as_str().unwrap().to_owned();
        assert_eq!(run_id, requested_run_id.to_string());
        assert_eq!(
            created["run"]["objective"],
            "Ship the requested Factory improvement"
        );
        let orchestrator_pane = created["session"]["placement"]["paneId"]
            .as_str()
            .unwrap()
            .to_owned();

        Self {
            runtime,
            herdr,
            run_id,
            orchestrator_pane,
            _repository: repository.0,
            _catalog: catalog,
        }
    }

    fn snapshot(&mut self) -> Value {
        expect_success(
            self.runtime
                .handle_request(Request::new("snapshot.get", json!({}))),
            "snapshot.get",
        )
    }

    fn poll_until(&mut self, label: &str, predicate: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let _ = self.runtime.poll_events();
            let snapshot = self.snapshot();
            if predicate(&snapshot) {
                return snapshot;
            }
            if Instant::now() > deadline {
                panic!("{label} timed out; snapshot={}", snapshot);
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
}

fn expect_success(frames: Vec<Frame>, what: &str) -> Value {
    match frames.first() {
        Some(Frame::Response(Response {
            outcome: ResponseOutcome::Success { result },
            ..
        })) => result.clone(),
        Some(Frame::Response(Response {
            outcome: ResponseOutcome::Error { error },
            ..
        })) => panic!("{what} failed: {} ({:?})", error.message, error.code),
        other => panic!("{what} returned unexpected frames: {other:?}"),
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
    if !root.join("README.md").exists() {
        std::fs::write(root.join("README.md"), "test repository\n").unwrap();
    }
    run(&["add", "--all"]);
    run(&["commit", "-m", "initial"]);
}

fn test_repository() -> (TempDir, PathBuf) {
    let container = TempDir::new().unwrap();
    let root = container.path().join("repository");
    std::fs::create_dir(&root).unwrap();
    initialize_repository(&root);
    (container, root)
}

fn named_agent(value: &str) -> bool {
    let bytes = value.as_bytes();
    (1..=32).contains(&bytes.len())
        && bytes[0].is_ascii_lowercase()
        && bytes.iter().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

fn methods_named<'a>(requests: &'a [Value], method: &str) -> Vec<&'a Value> {
    requests
        .iter()
        .filter(|request| request["method"] == method)
        .collect()
}

fn managed_session<'a>(snapshot: &'a Value, run_id: &str, purpose: &str) -> &'a Value {
    snapshot["agentSessions"]
        .as_array()
        .expect("agent sessions")
        .iter()
        .filter(|session| session["factoryRunId"] == run_id && session["purpose"] == purpose)
        .max_by_key(|session| session["createdAtUnixMs"].as_u64().unwrap_or_default())
        .expect("managed session")
}

#[test]
fn start_run_retries_agent_start_then_falls_back_to_the_live_pane() {
    let mut harness = FactoryHarness::start(1, 1);
    let snapshot = harness.snapshot();
    let run = &snapshot["factoryRuns"][0];
    assert_eq!(run["state"], "orchestrating");
    let run_id = run["id"].as_str().unwrap();
    let session = managed_session(&snapshot, run_id, "orchestration");
    assert!(
        snapshot["agentSessions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|session| { session["factoryRunId"] != run_id || session["purpose"] != "coding" })
    );
    assert_eq!(session["purpose"], "orchestration");
    assert_eq!(session["lifecycle"], "working");

    let placement = &session["placement"];
    let agent_name = placement["agentName"].as_str().expect("agent name");
    assert!(
        named_agent(agent_name),
        "Herdr agent name `{agent_name}` must match {AGENT_NAME}"
    );
    assert!(
        uuid::Uuid::parse_str(agent_name).is_err(),
        "prompt target must not be a session/run UUID: {agent_name}"
    );

    let requests = harness.herdr.recorded();
    let starts = methods_named(&requests, "agent.start");
    assert!(
        starts.len() >= 2,
        "stand-in should reject the first start: {starts:?}"
    );
    for start in &starts {
        let name = start["params"]["name"].as_str().unwrap_or_default();
        assert!(named_agent(name), "agent.start name `{name}`");
        assert_eq!(name, agent_name);
        assert_eq!(
            start["params"]["args"],
            json!(["--model", "glm-5.2:cloud"]),
            "Claude must start with the Environment default model"
        );
    }

    let prompts = methods_named(&requests, "agent.prompt");
    assert_eq!(prompts.len(), 2, "name rejection should fall back once");
    assert_eq!(prompts[0]["params"]["target"], agent_name);
    assert_eq!(prompts[1]["params"]["target"], placement["paneId"]);
    assert!(uuid::Uuid::parse_str(agent_name).is_err());
}

#[test]
fn a_successful_idle_prompt_is_recorded_once() {
    let mut harness = FactoryHarness::start_with_idle_prompt();
    let snapshot = harness.snapshot();
    let run_id = snapshot["factoryRuns"][0]["id"].as_str().unwrap();
    let session = managed_session(&snapshot, run_id, "orchestration");
    let session_id = session["id"].as_str().unwrap();
    assert_eq!(session["lifecycle"], "idle");
    assert_eq!(session["briefDelivered"], true);
    assert!(
        !harness
            .runtime
            .pending_prompt_retry_at
            .contains_key(&uuid::Uuid::parse_str(session_id).unwrap())
    );
    assert_eq!(
        methods_named(&harness.herdr.recorded(), "agent.prompt").len(),
        1
    );

    for _ in 0..3 {
        harness
            .herdr
            .emit_status(&harness.orchestrator_pane, "idle");
        let _ = harness.runtime.poll_events();
    }
    assert_eq!(
        methods_named(&harness.herdr.recorded(), "agent.prompt").len(),
        1,
        "lifecycle invalidations must not duplicate an acknowledged prompt"
    );
}

#[test]
fn start_run_prompts_the_pane_when_the_name_is_missing() {
    let mut harness = FactoryHarness::start_rejecting_names();
    let snapshot = harness.snapshot();
    let run = &snapshot["factoryRuns"][0];
    assert_eq!(run["state"], "orchestrating");
    let session = managed_session(&snapshot, run["id"].as_str().unwrap(), "orchestration");
    assert_eq!(session["lifecycle"], "working");

    let pane_id = session["placement"]["paneId"].as_str().unwrap();
    let recorded = harness.herdr.recorded();
    let prompts = methods_named(&recorded, "agent.prompt");
    assert!(
        prompts
            .iter()
            .any(|prompt| prompt["params"]["target"] == pane_id),
        "brief must fall back to the live pane: {prompts:?}"
    );
}

#[test]
fn start_run_defers_the_brief_when_prompt_is_not_ready() {
    let mut harness = FactoryHarness::start(0, 100);
    let snapshot = harness.snapshot();
    let run = &snapshot["factoryRuns"][0];
    assert_eq!(run["state"], "orchestrating");
    let session = managed_session(&snapshot, run["id"].as_str().unwrap(), "orchestration");
    assert_eq!(session["briefDelivered"], false);
    assert!(
        methods_named(&harness.herdr.recorded(), "pane.close").is_empty(),
        "a transient prompt failure must not close the Orchestrator pane"
    );

    harness.herdr.set_prompt_failures(0);
    harness
        .herdr
        .emit_status(&harness.orchestrator_pane, "idle");
    harness.poll_until("deferred brief delivered", |snapshot| {
        snapshot["agentSessions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|session| {
                session["factoryRunId"] == snapshot["factoryRuns"][0]["id"]
                    && session["purpose"] == "orchestration"
                    && session["lifecycle"] == "working"
                    && session["briefDelivered"] == true
            })
    });
}

#[test]
fn start_run_keeps_the_orchestrator_when_the_named_agent_is_not_ready() {
    let mut harness = FactoryHarness::start_not_ready();
    let snapshot = harness.snapshot();
    let run = &snapshot["factoryRuns"][0];
    assert_eq!(run["state"], "orchestrating");
    let session = managed_session(&snapshot, run["id"].as_str().unwrap(), "orchestration");
    assert_eq!(session["purpose"], "orchestration");
    assert_eq!(session["lifecycle"], "unknown");
    assert!(session["placement"]["paneId"].is_string());
    assert!(session["initialPrompt"].as_str().is_some());

    let recorded = harness.herdr.recorded();
    let closes = methods_named(&recorded, "pane.close");
    assert!(
        closes.is_empty(),
        "agent_not_ready must keep the Orchestrator pane: {closes:?}"
    );
    assert!(
        methods_named(&recorded, "agent.prompt").is_empty(),
        "the brief waits until Herdr reports idle"
    );

    harness.herdr.set_start_not_ready(false);
    harness
        .herdr
        .emit_status(&harness.orchestrator_pane, "idle");
    let prompted = harness.poll_until("orchestrator prompted", |snapshot| {
        snapshot["agentSessions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|session| {
                session["factoryRunId"] == snapshot["factoryRuns"][0]["id"]
                    && session["purpose"] == "orchestration"
                    && session["lifecycle"] == "working"
            })
    });
    let session = managed_session(
        &prompted,
        prompted["factoryRuns"][0]["id"].as_str().unwrap(),
        "orchestration",
    );
    assert_eq!(session["lifecycle"], "working");
    assert!(
        !methods_named(&harness.herdr.recorded(), "agent.prompt").is_empty(),
        "idle should deliver the pending orchestrator brief"
    );
}

#[test]
fn start_run_resumes_an_existing_live_orchestrator() {
    let mut harness = FactoryHarness::start(0, 0);
    let first = harness.snapshot()["factoryRuns"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let draft_id = harness.snapshot()["factoryRuns"][0]["agentDraftId"]
        .as_str()
        .unwrap()
        .to_owned();
    let environment_id = harness.snapshot()["factoryRuns"][0]["environmentId"]
        .as_str()
        .unwrap()
        .to_owned();
    let again = expect_success(
        harness.runtime.handle_request(Request::new(
            "factoryRun.create",
            json!({
                "runId": uuid::Uuid::new_v4(),
                "agentDraftId": draft_id,
                "environmentId": environment_id,
                "objective": "Resume the existing objective"
            }),
        )),
        "factoryRun.create again",
    );
    assert_eq!(again["run"]["id"], first);
    assert_eq!(again["run"]["state"], "orchestrating");
    assert_eq!(again["session"]["purpose"], "orchestration");
}

#[test]
fn pending_launch_retries_agent_start_before_delivering_the_brief() {
    let mut harness = FactoryHarness::start_with_launch_pending();
    let before = harness.snapshot();
    let run_id = before["factoryRuns"][0]["id"].as_str().unwrap();
    let session_id = managed_session(&before, run_id, "orchestration")["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        before["agentSessions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|session| { session["factoryRunId"] != run_id || session["purpose"] != "coding" })
    );

    let prompted = harness.poll_until("pending launch activated", |snapshot| {
        snapshot["agentSessions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|session| session["id"] == session_id && session["lifecycle"] == "working")
    });
    assert!(
        prompted["agentSessions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|session| {
                session["factoryRunId"] != prompted["factoryRuns"][0]["id"]
                    || session["purpose"] != "coding"
            })
    );
    let requests = harness.herdr.recorded();
    assert_eq!(methods_named(&requests, "agent.start").len(), 1);
    assert_eq!(
        methods_named(&requests, "agent.get").len(),
        1,
        "launch_pending must be reconciled through Herdr"
    );
    assert_eq!(
        methods_named(&requests, "agent.prompt").len(),
        3,
        "the failed name/pane attempt is followed by one delivered prompt"
    );
}

/// Herdr outlives Agent Factory and restarts under it. Without reopening the
/// subscription the runtime goes permanently deaf: no lifecycle events arrive
/// and `is_connected` keeps refusing new sessions until the app is restarted.
#[test]
fn a_closed_event_stream_is_resubscribed() {
    let mut harness = FactoryHarness::start(0, 0);
    let subscriptions_before = methods_named(&harness.herdr.recorded(), "events.subscribe").len();
    assert_eq!(
        subscriptions_before, 2,
        "the runtime adds pane-scoped lifecycle invalidation after the Agent starts"
    );

    harness.herdr.close_event_stream();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut frames = Vec::new();
    loop {
        frames.extend(harness.runtime.poll_events());
        let resubscribed = methods_named(&harness.herdr.recorded(), "events.subscribe").len()
            > subscriptions_before;
        if resubscribed && harness.herdr.is_subscribed() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the runtime never resubscribed after the stream closed"
        );
        thread::sleep(Duration::from_millis(20));
    }

    assert!(
        harness.runtime.herdr.is_connected(),
        "a reopened subscription must report Herdr as reachable again"
    );
    assert!(
        frames.iter().any(|frame| matches!(
            frame,
            Frame::Event(event) if event.topic == "harness.changed"
        )),
        "recovering must republish Herdr status so the UI stops reporting it unavailable"
    );

    // The reopened stream carries lifecycle again.
    let pane = harness.orchestrator_pane.clone();
    harness.herdr.emit_status(&pane, "blocked");
    harness.poll_until("blocked after resubscribe", |snapshot| {
        snapshot["agentSessions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|session| session["lifecycle"] == "blocked")
    });
}

/// Events are the fast path, but a missed notification must not leave the UI
/// pinned to an obsolete authoritative lifecycle indefinitely.
#[test]
fn snapshot_poll_recovers_a_lifecycle_change_without_an_event() {
    let mut harness = FactoryHarness::start(0, 0);
    let snapshot = harness.snapshot();
    let session = snapshot["agentSessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|session| session["purpose"] == "orchestration")
        .expect("orchestrator session");
    let agent_name = session["placement"]["agentName"]
        .as_str()
        .unwrap()
        .to_owned();
    let pane_id = session["placement"]["paneId"].as_str().unwrap().to_owned();

    // Change only the authoritative snapshot. No event is sent.
    harness
        .herdr
        .set_agents(&[(&agent_name, &pane_id, "blocked")]);
    thread::sleep(Duration::from_millis(60));
    let _ = harness.runtime.poll_events();

    let refreshed = harness.snapshot();
    assert!(
        refreshed["agentSessions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|session| session["placement"]["paneId"] == pane_id
                && session["lifecycle"] == "blocked")
    );
}

/// Herdr may move a pane inside the binding's Workspace while the agent name
/// stays valid. The next authoritative snapshot must replace the old
/// placement before another command is authorized.
#[test]
fn a_moved_pane_keeps_receiving_lifecycle_events() {
    let mut harness = FactoryHarness::start(0, 0);
    let snapshot = harness.snapshot();
    let session = snapshot["agentSessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|session| session["purpose"] == "orchestration")
        .expect("orchestrator session")
        .clone();
    let agent_name = session["placement"]["agentName"].as_str().unwrap();
    let original_pane = session["placement"]["paneId"].as_str().unwrap().to_owned();
    let moved_pane = "w1:p7";
    assert_ne!(original_pane, moved_pane);

    harness
        .herdr
        .set_agents(&[(agent_name, moved_pane, "working")]);
    harness.herdr.emit_invalidation();
    harness.poll_until("placement follows the moved pane", |snapshot| {
        snapshot["agentSessions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|session| session["placement"]["paneId"] == moved_pane)
    });

    // The event for the new pane id now reaches the session; before the
    // placement was refreshed it matched nothing and was dropped.
    harness.herdr.emit_status(moved_pane, "blocked");
    harness.poll_until("lifecycle from the moved pane", |snapshot| {
        snapshot["agentSessions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|session| {
                session["placement"]["paneId"] == moved_pane && session["lifecycle"] == "blocked"
            })
    });
}

/// A failed snapshot says nothing about which agents are alive. Treating it as
/// an empty set would detach every session and null every placement. The
/// cached observation remains visible, but cannot authorize a live command.
#[test]
fn a_failed_agent_listing_leaves_sessions_attached() {
    let mut harness = FactoryHarness::start(0, 0);
    let before = harness.snapshot()["agentSessions"].clone();
    assert!(before[0]["placement"]["paneId"].is_string());

    harness.herdr.set_agent_list_fails(true);
    harness.herdr.emit_invalidation();
    for _ in 0..5 {
        let _ = harness.runtime.poll_events();
        thread::sleep(Duration::from_millis(10));
    }

    let after = harness.snapshot();
    assert_eq!(
        after["agentSessions"][0]["placement"],
        before[0]["placement"]
    );
    assert_eq!(
        after["agentSessions"][0]["lifecycle"],
        before[0]["lifecycle"]
    );
    assert_eq!(after["agentSessions"][0]["availability"], "last_observed");
    assert_eq!(after["herdr"]["freshness"], "last_observed");
}

#[test]
fn an_interrupted_session_never_retries_its_initial_prompt() {
    let mut harness = FactoryHarness::start_with_launch_pending();
    let snapshot = harness.snapshot();
    let run_id = snapshot["factoryRuns"][0]["id"].as_str().unwrap();
    let pending = managed_session(&snapshot, run_id, "orchestration").clone();
    assert_eq!(pending["briefDelivered"], false);
    assert!(methods_named(&harness.herdr.recorded(), "agent.prompt").is_empty());

    let agent_name = pending["placement"]["agentName"].as_str().unwrap();
    let pane_id = pending["placement"]["paneId"].as_str().unwrap();
    harness.herdr.set_agents(&[]);
    harness.herdr.emit_invalidation();
    let _ = harness.runtime.poll_events();
    let interrupted = harness.snapshot();
    let interrupted = managed_session(&interrupted, run_id, "orchestration");
    assert_eq!(interrupted["outcome"]["kind"], "interrupted");

    harness.herdr.set_agents(&[(agent_name, pane_id, "idle")]);
    harness.herdr.emit_invalidation();
    let _ = harness.runtime.poll_events();
    assert!(
        methods_named(&harness.herdr.recorded(), "agent.prompt").is_empty(),
        "a durable outcome permanently disables automatic prompt delivery"
    );
}

/// The Orchestrator drives its own loop. These cover the boundary it calls
/// through: the Environment is applied, the move is validated, and the Run is
/// recorded — with the loop itself left to the agent and Herdr.
mod agent_control_tests {
    use agent_control::{ControlCommand, ControlRequest, ControlResponse, FinishVerdict};

    use super::*;

    /// A harness whose Orchestrator has a real control token, taken from the
    /// pane environment Herdr was actually asked to create.
    struct DrivenRun {
        harness: FactoryHarness,
        token: String,
    }

    impl DrivenRun {
        fn start() -> Self {
            let mut harness = FactoryHarness::start(0, 0);
            harness
                .runtime
                .set_control_endpoint(PathBuf::from("/nonexistent/agent-control.sock"));
            // Restart the Run so its Orchestrator is created with the endpoint set.
            let run_id = harness.run_id.clone();
            let _ = harness
                .runtime
                .handle_request(Request::new("run.cancel", json!({ "runId": run_id })));
            let draft_id =
                harness.snapshot()["targetWorkspace"]["targetGroups"][0]["drafts"][0]["id"]
                    .as_str()
                    .unwrap()
                    .to_owned();
            let created = expect_success(
                harness.runtime.handle_request(Request::new(
                    "factoryRun.create",
                    json!({
                        "runId": uuid::Uuid::new_v4(),
                        "agentDraftId": draft_id,
                        "environmentId": "loop-environment",
                        "objective": "Continue after reconnecting control"
                    }),
                )),
                "factoryRun.create",
            );
            harness.run_id = created["run"]["id"].as_str().unwrap().to_owned();
            harness.orchestrator_pane = created["session"]["placement"]["paneId"]
                .as_str()
                .unwrap()
                .to_owned();

            let token = harness
                .herdr
                .recorded()
                .iter()
                .filter(|request| request["method"] == "tab.create")
                .filter_map(|request| {
                    request["params"]["env"][agent_control::TOKEN_ENV]
                        .as_str()
                        .map(str::to_owned)
                })
                .next_back()
                .expect("the Orchestrator's pane carries a control token");
            Self { harness, token }
        }

        fn call(&mut self, command: ControlCommand) -> ControlResponse {
            let (response, _) = self.harness.runtime.handle_control(ControlRequest {
                token: self.token.clone(),
                command,
            });
            response
        }

        /// The Run this Orchestrator drives. The fixture leaves an earlier
        /// cancelled Run behind, so position is not identity.
        fn driven_run(&mut self) -> Value {
            let run_id = self.harness.run_id.clone();
            self.harness.snapshot()["factoryRuns"]
                .as_array()
                .unwrap()
                .iter()
                .find(|run| run["id"] == run_id.as_str())
                .expect("the driven Run")
                .clone()
        }

        fn expect_ok(&mut self, command: ControlCommand) -> agent_control::RunView {
            match self.call(command) {
                ControlResponse::Ok(view) => view,
                ControlResponse::Error { code, message } => {
                    panic!("control command refused: {code}: {message}")
                }
            }
        }
    }

    #[test]
    fn the_orchestrator_is_briefed_with_its_command_vocabulary() {
        let run = DrivenRun::start();
        let snapshot = {
            let mut harness = run.harness;
            harness.snapshot()
        };
        let brief = snapshot["agentSessions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|session| session["purpose"] == "orchestration")
            .expect("orchestrator session")["initialPrompt"]
            .as_str()
            .unwrap()
            .to_owned();

        for expected in [
            "Use the Herdr skill",
            "own pane beside yours",
            "one tab per iteration",
            "Never spawn an agent",
            "agent-factory start coding",
            "agent-factory start evaluation",
            "agent-factory finish",
            "herdr agent prompt",
        ] {
            assert!(brief.contains(expected), "brief is missing `{expected}`");
        }
        assert!(
            brief.contains("agent-factory escalate"),
            "an unattended Run needs a way to reach a person"
        );
        // The Orchestrator acts now; it does not describe an action in a file
        // for Rust to carry out.
        for retired in [
            "decision file",
            "Write exactly one JSON object",
            "start_coding",
        ] {
            assert!(!brief.contains(retired), "brief still mentions `{retired}`");
        }
    }

    #[test]
    fn the_orchestrator_drives_coding_then_evaluation_then_finishes() {
        let mut run = DrivenRun::start();

        let coding = run.expect_ok(ControlCommand::StartCoding {
            brief: "Implement the objective".into(),
        });
        assert_eq!(coding.state, "coding");
        let agent = coding.agent.expect("a started Coding agent to prompt");
        assert!(agent.name.starts_with("coding-"), "{}", agent.name);
        let snapshot = run.harness.snapshot();
        assert!(
            snapshot["liveAgents"]
                .as_array()
                .unwrap()
                .iter()
                .any(|live| {
                    live["agentName"] == agent.name && live["managedSessionId"].is_string()
                })
        );

        let evaluating = run.expect_ok(ControlCommand::StartEvaluation { brief: None });
        assert_eq!(evaluating.state, "evaluating");
        assert!(evaluating.agent.is_some());

        let finished = run.expect_ok(ControlCommand::Finish {
            verdict: FinishVerdict::Pass,
            summary: "Acceptance criteria hold".into(),
        });
        assert_eq!(finished.state, "passed");
        assert_eq!(
            finished.evaluation.as_ref().map(|it| it.verdict.as_str()),
            Some("pass")
        );

        // The Run can no longer be driven, so its authority is gone.
        assert!(matches!(
            run.call(ControlCommand::Status),
            ControlResponse::Error { ref code, .. } if code == "unauthorized"
        ));
    }

    /// One Draft is one Workspace, one iteration is one tab, and a Run reads as
    /// columns inside it.
    #[test]
    fn an_iteration_is_one_tab_of_columns_and_the_next_opens_its_own() {
        let mut run = DrivenRun::start();
        let placement = |run: &mut DrivenRun, purpose: &str| {
            let snapshot = run.harness.snapshot();
            let session = managed_session(&snapshot, &run.harness.run_id, purpose);
            (
                session["placement"]["tabId"].as_str().unwrap().to_owned(),
                session["placement"]["paneId"].as_str().unwrap().to_owned(),
            )
        };

        let (orchestrator_tab, orchestrator_pane) = placement(&mut run, "orchestration");

        let _ = run.expect_ok(ControlCommand::StartCoding {
            brief: "Implement the objective".into(),
        });
        let (coding_tab, coding_pane) = placement(&mut run, "coding");
        assert_eq!(
            coding_tab, orchestrator_tab,
            "the first Coding agent is a column beside the Orchestrator"
        );
        assert_ne!(coding_pane, orchestrator_pane);

        // Evaluation judges the Coding agent it follows, so it joins that
        // iteration rather than opening one of its own.
        let _ = run.expect_ok(ControlCommand::StartEvaluation { brief: None });
        let (evaluation_tab, evaluation_pane) = placement(&mut run, "evaluation");
        assert_eq!(evaluation_tab, orchestrator_tab);
        assert_ne!(evaluation_pane, coding_pane);

        let _ = run.expect_ok(ControlCommand::StartCoding {
            brief: "Fix what the evaluator found".into(),
        });
        let (repair_tab, _) = placement(&mut run, "coding");
        assert_ne!(
            repair_tab, orchestrator_tab,
            "a second iteration opens its own tab"
        );

        // The Orchestrator stays where the Run began, so iteration 1 remains
        // readable after the next one starts.
        assert_eq!(placement(&mut run, "orchestration").0, orchestrator_tab);
    }

    /// A stored pane id is a locator. When the neighbour a session was going to
    /// stand beside is gone, it opens a tab of its own instead of failing.
    #[test]
    fn a_column_whose_neighbour_vanished_falls_back_to_its_own_tab() {
        let mut run = DrivenRun::start();
        let before = run.harness.snapshot();
        let orchestrator = managed_session(&before, &run.harness.run_id, "orchestration");
        let orchestrator_tab = orchestrator["placement"]["tabId"].as_str().unwrap();
        run.harness
            .herdr
            .forget_pane(orchestrator["placement"]["paneId"].as_str().unwrap());

        let coding = run.expect_ok(ControlCommand::StartCoding {
            brief: "Implement the objective".into(),
        });
        assert_eq!(coding.state, "coding");
        let snapshot = run.harness.snapshot();
        let session = managed_session(&snapshot, &run.harness.run_id, "coding");
        assert_ne!(
            session["placement"]["tabId"].as_str().unwrap(),
            orchestrator_tab
        );
    }

    #[test]
    fn cancel_run_closes_every_managed_pane_before_committing_cancelled() {
        let mut run = DrivenRun::start();
        let _ = run.expect_ok(ControlCommand::StartCoding {
            brief: "Implement the objective".into(),
        });
        run.harness
            .herdr
            .add_agent("other-runtime-agent", "w1:p99", "idle");

        let before = run.harness.snapshot();
        let managed = before["agentSessions"]
            .as_array()
            .expect("agent sessions")
            .iter()
            .filter(|session| session["factoryRunId"] == run.harness.run_id)
            .map(|session| {
                (
                    session["purpose"].as_str().unwrap().to_owned(),
                    session["placement"]["paneId"].as_str().unwrap().to_owned(),
                    session["placement"]["agentName"]
                        .as_str()
                        .unwrap()
                        .to_owned(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(managed.len(), 2);
        let orchestrator_pane = managed
            .iter()
            .find(|(purpose, _, _)| purpose == "orchestration")
            .map(|(_, pane, _)| pane.clone())
            .expect("orchestrator pane");
        let close_count = methods_named(&run.harness.herdr.recorded(), "pane.close").len();

        let cancelled = expect_success(
            run.harness.runtime.handle_request(Request::new(
                "run.cancel",
                json!({ "runId": run.harness.run_id }),
            )),
            "run.cancel",
        );
        assert_eq!(cancelled["run"]["state"], "cancelled");

        let recorded = run.harness.herdr.recorded();
        let closes = methods_named(&recorded, "pane.close");
        let cancellation_closes = &closes[close_count..];
        assert_eq!(cancellation_closes.len(), managed.len());
        assert_eq!(
            cancellation_closes.last().unwrap()["params"]["pane_id"],
            orchestrator_pane,
            "the Orchestrator must be stopped after its workers"
        );
        for (_, pane_id, _) in &managed {
            assert!(
                cancellation_closes
                    .iter()
                    .any(|request| request["params"]["pane_id"] == *pane_id),
                "managed pane {pane_id} was not closed"
            );
        }

        let snapshot = run.harness.snapshot();
        let cancelled_run = snapshot["factoryRuns"]
            .as_array()
            .unwrap()
            .iter()
            .find(|factory_run| factory_run["id"] == run.harness.run_id)
            .expect("cancelled Run");
        assert_eq!(cancelled_run["state"], "cancelled");
        let managed_names = managed
            .iter()
            .map(|(_, _, name)| name.as_str())
            .collect::<Vec<_>>();
        for session in snapshot["agentSessions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|session| session["factoryRunId"] == run.harness.run_id)
        {
            assert_eq!(session["availability"], "historical");
            assert_eq!(session["outcome"]["kind"], "stopped");
        }
        assert!(
            snapshot["liveAgents"]
                .as_array()
                .unwrap()
                .iter()
                .all(|agent| {
                    agent["agentName"]
                        .as_str()
                        .is_none_or(|name| !managed_names.contains(&name))
                })
        );
        assert!(
            snapshot["liveAgents"]
                .as_array()
                .unwrap()
                .iter()
                .any(|agent| {
                    agent["agentName"] == "other-runtime-agent"
                        && agent["managedSessionId"].is_null()
                })
        );
        assert!(matches!(
            run.call(ControlCommand::Status),
            ControlResponse::Error { ref code, .. } if code == "unauthorized"
        ));

        let cancelled_run_id = run.harness.run_id.clone();
        let draft_id = snapshot["targetWorkspace"]["targetGroups"][0]["drafts"][0]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let created = expect_success(
            run.harness.runtime.handle_request(Request::new(
                "factoryRun.create",
                json!({
                    "runId": uuid::Uuid::new_v4(),
                    "agentDraftId": draft_id,
                    "environmentId": "loop-environment",
                    "objective": "Start a follow-up Run"
                }),
            )),
            "factoryRun.create after cancellation",
        );
        assert_ne!(created["run"]["id"], cancelled_run_id);
        assert_eq!(created["run"]["state"], "orchestrating");
        assert_eq!(created["session"]["purpose"], "orchestration");
    }

    #[test]
    fn cancel_run_stays_active_until_herdr_confirms_termination() {
        let mut run = DrivenRun::start();
        let _ = run.expect_ok(ControlCommand::StartCoding {
            brief: "Implement the objective".into(),
        });
        run.harness.herdr.set_close_keeps_agent(true);

        let failed = run.harness.runtime.handle_request(Request::new(
            "run.cancel",
            json!({ "runId": run.harness.run_id }),
        ));
        match failed.first() {
            Some(Frame::Response(Response {
                outcome: ResponseOutcome::Error { error },
                ..
            })) => assert!(
                error.message.contains("cancellation is incomplete"),
                "unexpected cancellation error: {}",
                error.message
            ),
            other => panic!("unconfirmed cancellation returned {other:?}"),
        }
        assert_eq!(run.driven_run()["state"], "coding");
        assert!(matches!(
            run.call(ControlCommand::Status),
            ControlResponse::Ok(_)
        ));

        run.harness.herdr.set_close_keeps_agent(false);
        let cancelled = expect_success(
            run.harness.runtime.handle_request(Request::new(
                "run.cancel",
                json!({ "runId": run.harness.run_id }),
            )),
            "retry run.cancel",
        );
        assert_eq!(cancelled["run"]["state"], "cancelled");
    }

    #[test]
    fn cancel_run_closes_its_revalidated_tab_when_the_agent_name_changed() {
        let mut run = DrivenRun::start();
        let _ = run.expect_ok(ControlCommand::StartCoding {
            brief: "Implement the objective".into(),
        });
        let before = run.harness.snapshot();
        let sessions = before["agentSessions"]
            .as_array()
            .expect("agent sessions")
            .iter()
            .filter(|session| session["factoryRunId"] == run.harness.run_id)
            .map(|session| {
                (
                    session["purpose"].as_str().unwrap(),
                    session["placement"]["paneId"].as_str().unwrap(),
                    session["placement"]["agentName"].as_str().unwrap(),
                    session["placement"]["tabId"].as_str().unwrap(),
                )
            })
            .collect::<Vec<_>>();
        let orchestrator = sessions
            .iter()
            .find(|(purpose, _, _, _)| *purpose == "orchestration")
            .copied()
            .expect("orchestrator");
        let coding = sessions
            .iter()
            .find(|(purpose, _, _, _)| *purpose == "coding")
            .copied()
            .expect("coding");
        // The stand-in derives a tab ID from the pane ordinal. Put the renamed
        // process in a pane that belongs to the recorded Coding tab so the
        // snapshot models Herdr's explicit agent.tab_id faithfully.
        let renamed_pane = coding.3.replace(":t", ":p");
        run.harness.herdr.set_agents(&[
            (orchestrator.2, orchestrator.1, "idle"),
            ("renamed-coding-agent", &renamed_pane, "working"),
            ("other-runtime-agent", "w1:p99", "idle"),
        ]);

        let cancelled = expect_success(
            run.harness.runtime.handle_request(Request::new(
                "run.cancel",
                json!({ "runId": run.harness.run_id }),
            )),
            "run.cancel",
        );
        assert_eq!(cancelled["run"]["state"], "cancelled");
        let snapshot = run.harness.snapshot();
        assert!(
            snapshot["liveAgents"]
                .as_array()
                .unwrap()
                .iter()
                .any(|agent| {
                    agent["agentName"] == "other-runtime-agent"
                        && agent["managedSessionId"].is_null()
                })
        );
        assert!(
            snapshot["liveAgents"]
                .as_array()
                .unwrap()
                .iter()
                .all(|agent| { agent["agentName"] != "renamed-coding-agent" }),
            "renamed managed agent survived cancellation; snapshot={snapshot}"
        );
    }

    #[test]
    fn coding_pass_count_is_derived_from_authorized_sessions() {
        let mut run = DrivenRun::start();
        let first = run.expect_ok(ControlCommand::StartCoding {
            brief: "First attempt".into(),
        });
        assert_eq!(first.iteration, 1);

        let _ = run.expect_ok(ControlCommand::StartEvaluation { brief: None });
        let repair = run.expect_ok(ControlCommand::StartCoding {
            brief: "Fix what evaluation found".into(),
        });
        assert_eq!(repair.iteration, 2);
        assert_eq!(repair.state, "coding");
    }

    /// A Factory Run advances unattended. Escalation is the one thing that
    /// should pull a person back, and it must not cost the Orchestrator the Run.
    #[test]
    fn escalating_asks_a_person_without_ending_the_run() {
        let mut run = DrivenRun::start();
        let _ = run.expect_ok(ControlCommand::StartCoding {
            brief: "Implement the objective".into(),
        });

        let (response, frames) = run.harness.runtime.handle_control(ControlRequest {
            token: run.token.clone(),
            command: ControlCommand::Escalate {
                question: "Which database should the agent target?".into(),
            },
        });
        let view = match response {
            ControlResponse::Ok(view) => view,
            other => panic!("escalation refused: {other:?}"),
        };
        assert_eq!(view.state, "escalated");
        assert!(
            frames.iter().any(|frame| matches!(
                frame,
                Frame::Event(event) if event.topic == "notification.requested"
            )),
            "an unattended Run must reach the person it is waiting on"
        );

        assert_eq!(
            run.driven_run()["escalation"],
            "Which database should the agent target?"
        );

        // The Orchestrator keeps its authority, so answering it in its pane is
        // enough for the Run to carry on.
        let resumed = run.expect_ok(ControlCommand::StartCoding {
            brief: "Use Postgres, as answered".into(),
        });
        assert_eq!(resumed.state, "coding");
        assert!(
            run.driven_run()["escalation"].is_null(),
            "moving forward answers the outstanding question"
        );
    }

    #[test]
    fn an_illegal_move_is_refused_with_something_the_agent_can_act_on() {
        let mut run = DrivenRun::start();
        match run.call(ControlCommand::StartEvaluation { brief: None }) {
            ControlResponse::Error { code, message } => {
                assert_eq!(code, "invalid_request");
                assert!(
                    message.contains("start Coding first"),
                    "refusal must say how to correct it: {message}"
                );
            }
            other => panic!("evaluating nothing should be refused, got {other:?}"),
        }
    }

    #[test]
    fn a_token_that_authorizes_nothing_cannot_move_a_run() {
        let mut run = DrivenRun::start();
        let (response, frames) = run.harness.runtime.handle_control(ControlRequest {
            token: "not-a-token".into(),
            command: ControlCommand::Finish {
                verdict: FinishVerdict::Pass,
                summary: "should not happen".into(),
            },
        });
        assert!(matches!(
            response,
            ControlResponse::Error { ref code, .. } if code == "unauthorized"
        ));
        assert!(frames.is_empty(), "a refused command changes nothing");
    }

    #[test]
    fn only_the_orchestrator_is_handed_the_control_endpoint() {
        let mut run = DrivenRun::start();
        let _ = run.expect_ok(ControlCommand::StartCoding {
            brief: "Implement the objective".into(),
        });

        let carrying = run
            .harness
            .herdr
            .recorded()
            .iter()
            .filter(|request| request["method"] == "tab.create")
            .filter(|request| !request["params"]["env"][agent_control::TOKEN_ENV].is_null())
            .count();
        assert_eq!(
            carrying, 1,
            "a Coding agent must not be able to advance the Run it works for"
        );
    }
}
