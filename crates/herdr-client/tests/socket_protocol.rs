//! Protocol-level tests against a stand-in Herdr server.
//!
//! The double answers on a real Unix socket with the exact framing Herdr uses:
//! one newline-delimited JSON response per connection, except for
//! `events.subscribe`, which acknowledges and then streams event frames.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::thread;
use std::time::{Duration, Instant};

use herdr_client::{AgentStatus, HerdrClient, PaneSpec, ReadSource, SplitDirection};
use serde_json::{Value, json};

struct FakeHerdr {
    socket: PathBuf,
    requests: std::sync::mpsc::Receiver<Value>,
    _directory: tempfile::TempDir,
}

impl FakeHerdr {
    fn start(protocol: u32) -> Self {
        Self::start_with_transient_failures(protocol, 0, 0)
    }

    fn start_with_transient_failures(
        protocol: u32,
        start_failures: usize,
        prompt_failures: usize,
    ) -> Self {
        let directory = tempfile::tempdir().expect("temp dir");
        let socket = directory.path().join("herdr.sock");
        let listener = UnixListener::bind(&socket).expect("bind");
        let (sender, requests) = channel();
        let start_failures = Arc::new(AtomicUsize::new(start_failures));
        let prompt_failures = Arc::new(AtomicUsize::new(prompt_failures));
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                let sender = sender.clone();
                let start_failures = Arc::clone(&start_failures);
                let prompt_failures = Arc::clone(&prompt_failures);
                thread::spawn(move || {
                    serve(stream, protocol, sender, start_failures, prompt_failures)
                });
            }
        });
        Self {
            socket,
            requests,
            _directory: directory,
        }
    }

    fn client(&self) -> HerdrClient {
        HerdrClient::new(self.socket.clone()).with_timeout(Duration::from_secs(5))
    }

    fn next_request(&self) -> Value {
        self.requests
            .recv_timeout(Duration::from_secs(5))
            .expect("a request")
    }
}

fn serve(
    stream: UnixStream,
    protocol: u32,
    sender: Sender<Value>,
    start_failures: Arc<AtomicUsize>,
    prompt_failures: Arc<AtomicUsize>,
) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
    let mut writer = stream;
    let mut line = String::new();
    if reader.read_line(&mut line).unwrap_or(0) == 0 {
        return;
    }
    let request: Value = serde_json::from_str(&line).expect("request json");
    let method = request["method"].as_str().unwrap_or_default().to_owned();
    sender.send(request.clone()).ok();

    let result = match method.as_str() {
        "ping" => json!({"type": "pong", "version": "0.8.0", "protocol": protocol}),
        "server.agent_manifests" => json!({
            "type": "agent_manifest_status",
            "manifests": [
                {"agent": "claude", "source": "remote:claude.toml", "source_kind": "remote", "active_version": "1.2.3"},
                {"agent": "codex", "source": "builtin", "source_kind": "builtin"}
            ]
        }),
        "workspace.create" => json!({
            "type": "workspace_created",
            "workspace": {"workspace_id": "w1", "label": "factory", "active_tab_id": "w1:t1"},
            "tab": {"tab_id": "w1:t1", "workspace_id": "w1"},
            "root_pane": {"pane_id": "w1:p1", "workspace_id": "w1", "tab_id": "w1:t1"}
        }),
        "pane.split" => json!({
            "type": "pane_info",
            "pane": {"pane_id": "w1:p2", "workspace_id": "w1", "tab_id": "w1:t1", "cwd": "/repo"}
        }),
        "agent.start"
            if start_failures
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
                .is_ok() =>
        {
            writeln!(
                writer,
                "{}",
                json!({"id": request["id"], "error": {"code": "agent_pane_busy", "message": "pane is not ready"}})
            )
            .ok();
            writer.flush().ok();
            return;
        }
        "agent.start" => json!({
            "type": "agent_started",
            "agent": {"pane_id": "w1:p2", "name": "coding", "agent": "claude", "agent_status": "idle", "interactive_ready": true}
        }),
        "agent.prompt"
            if prompt_failures
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
                .is_ok() =>
        {
            let target = request["params"]["target"].as_str().unwrap_or("coding");
            writeln!(
                writer,
                "{}",
                json!({"id": request["id"], "error": {"code": "agent_not_ready", "message": format!("agent {target} is not an active named agent")}})
            )
            .ok();
            writer.flush().ok();
            return;
        }
        "agent.prompt" => json!({
            "type": "agent_prompted",
            "agent": {"pane_id": "w1:p2", "name": "coding", "agent": "claude", "agent_status": "blocked"}
        }),
        "agent.read" => json!({
            "type": "pane_read",
            "read": {
                "pane_id": "w1:p2", "workspace_id": "w1", "tab_id": "w1:t1",
                "source": "recent_unwrapped", "format": "text",
                "text": "done\n", "revision": 7, "truncated": false
            }
        }),
        "agent.get" if request["params"]["target"] == "missing" => {
            writeln!(
                writer,
                "{}",
                json!({"id": request["id"], "error": {"code": "invalid_request", "message": "unknown agent `missing`"}})
            )
            .ok();
            writer.flush().ok();
            return;
        }
        "agent.get" => json!({
            "type": "agent_info",
            "agent": {"pane_id": "w1:p2", "name": "coding", "agent_status": "working"}
        }),
        "pane.close" => json!({"type": "ok"}),
        "agent.send_keys" => json!({"type": "ok"}),
        "events.subscribe" => {
            writeln!(
                writer,
                "{}",
                json!({"id": request["id"], "result": {"type": "subscription_started"}})
            )
            .ok();
            writer.flush().ok();
            for frame in [
                json!({"event": "pane.agent_status_changed", "data": {"pane_id": "w1:p2", "workspace_id": "w1", "agent_status": "working"}}),
                json!({"event": "layout_updated", "data": {"type": "layout_updated", "workspace_id": "w1"}}),
                json!({"event": "pane_exited", "data": {"type": "pane_exited", "pane_id": "w1:p2", "workspace_id": "w1"}}),
            ] {
                writeln!(writer, "{frame}").ok();
                writer.flush().ok();
            }
            // Hold the connection open the way a live subscription does.
            thread::sleep(Duration::from_secs(30));
            return;
        }
        "boom" => {
            writeln!(
                writer,
                "{}",
                json!({"id": request["id"], "error": {"code": "invalid_request", "message": "unknown pane w9:p9"}})
            )
            .ok();
            writer.flush().ok();
            return;
        }
        other => json!({"type": "ok", "echo": other}),
    };

    writeln!(writer, "{}", json!({"id": request["id"], "result": result})).ok();
    writer.flush().ok();
}

#[test]
fn probe_rejects_an_older_protocol() {
    let server = FakeHerdr::start(herdr_client::REQUIRED_PROTOCOL - 1);
    let error = server.client().probe().unwrap_err();
    assert!(matches!(
        error,
        herdr_client::HerdrError::IncompatibleProtocol { .. }
    ));
}

#[test]
fn probe_accepts_a_supported_protocol() {
    let server = FakeHerdr::start(herdr_client::REQUIRED_PROTOCOL);
    let info = server.client().probe().expect("probe");
    assert_eq!(info.version, "0.8.0");
}

#[test]
fn agent_kinds_come_from_herdr_manifests() {
    let server = FakeHerdr::start(herdr_client::REQUIRED_PROTOCOL);
    let manifests = server.client().agent_manifests().expect("manifests");
    let kinds = manifests
        .iter()
        .map(|manifest| manifest.agent.as_str())
        .collect::<Vec<_>>();
    assert_eq!(kinds, ["claude", "codex"]);
}

#[test]
fn a_pane_carries_the_environment_boundary_at_creation() {
    let server = FakeHerdr::start(herdr_client::REQUIRED_PROTOCOL);
    let spec = PaneSpec::new(
        Some("/repo".into()),
        [(
            "ANTHROPIC_BASE_URL".to_owned(),
            "http://127.0.0.1:9".to_owned(),
        )]
        .into_iter()
        .collect(),
    );
    let pane = server
        .client()
        .split_pane("w1:p1", SplitDirection::Right, &spec)
        .expect("split");
    assert_eq!(pane.pane_id, "w1:p2");

    let request = server.next_request();
    assert_eq!(request["method"], "pane.split");
    assert_eq!(request["params"]["cwd"], "/repo");
    assert_eq!(
        request["params"]["env"]["ANTHROPIC_BASE_URL"],
        "http://127.0.0.1:9"
    );
    assert_eq!(request["params"]["focus"], false);
}

#[test]
fn start_and_prompt_retry_transient_readiness_errors() {
    let server = FakeHerdr::start_with_transient_failures(herdr_client::REQUIRED_PROTOCOL, 1, 1);
    let client = server.client();

    let start = client.start_agent("coding", "claude", "w1:p2", &[], Duration::from_secs(5));
    assert!(
        start
            .as_ref()
            .err()
            .is_some_and(|error| error.is_transient()),
        "first start should be agent_pane_busy: {start:?}"
    );
    let prompt = client.prompt_agent("coding", "ship it", None);
    assert!(
        prompt
            .as_ref()
            .err()
            .is_some_and(|error| error.is_transient()),
        "first prompt should be agent_not_ready: {prompt:?}"
    );

    herdr_client::retry_transient(Duration::from_secs(2), || {
        client.start_agent("coding", "claude", "w1:p2", &[], Duration::from_secs(5))
    })
    .expect("start should succeed once the pane is ready");
    let prompted = herdr_client::retry_transient(Duration::from_secs(2), || {
        client.prompt_agent("coding", "ship it", None)
    })
    .expect("prompt should succeed once the name is active");
    assert_eq!(prompted.agent_status, AgentStatus::Blocked);

    let methods: Vec<_> = std::iter::from_fn(|| server.requests.try_recv().ok())
        .map(|request| request["method"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(
        methods
            .iter()
            .filter(|method| *method == "agent.start")
            .count(),
        2
    );
    assert_eq!(
        methods
            .iter()
            .filter(|method| *method == "agent.prompt")
            .count(),
        2
    );
}

#[test]
fn starting_an_agent_names_it_and_reports_readiness() {
    let server = FakeHerdr::start(herdr_client::REQUIRED_PROTOCOL);
    let agent = server
        .client()
        .start_agent("coding", "claude", "w1:p2", &[], Duration::from_secs(30))
        .expect("start");
    assert_eq!(agent.name.as_deref(), Some("coding"));
    assert_eq!(agent.agent_status, AgentStatus::Idle);
    assert!(agent.interactive_ready);

    let request = server.next_request();
    assert_eq!(request["params"]["kind"], "claude");
    assert_eq!(request["params"]["pane_id"], "w1:p2");
    assert_eq!(request["params"]["timeout_ms"], 30_000);
}

#[test]
fn a_waited_prompt_returns_the_settled_state() {
    let server = FakeHerdr::start(herdr_client::REQUIRED_PROTOCOL);
    let agent = server
        .client()
        .prompt_agent("coding", "ship it", Some(Duration::from_secs(120)))
        .expect("prompt");
    assert_eq!(agent.agent_status, AgentStatus::Blocked);

    let request = server.next_request();
    assert_eq!(request["params"]["text"], "ship it");
    assert_eq!(request["params"]["wait"]["timeout_ms"], 120_000);
}

#[test]
fn transcripts_read_unwrapped_text() {
    let server = FakeHerdr::start(herdr_client::REQUIRED_PROTOCOL);
    let read = server
        .client()
        .read_agent("coding", ReadSource::RecentUnwrapped, Some(200))
        .expect("read");
    assert_eq!(read.text, "done\n");

    let request = server.next_request();
    assert_eq!(request["params"]["source"], "recent_unwrapped");
    assert_eq!(request["params"]["lines"], 200);
    assert_eq!(request["params"]["strip_ansi"], true);
}

#[test]
fn pane_text_sends_raw_bytes() {
    let server = FakeHerdr::start(herdr_client::REQUIRED_PROTOCOL);
    server.client().send_pane_text("w1:p2", "\r").expect("send");
    let request = server.next_request();
    assert_eq!(request["method"], "pane.send_text");
    assert_eq!(request["params"]["pane_id"], "w1:p2");
    assert_eq!(request["params"]["text"], "\r");
}

#[test]
fn pane_input_omits_absent_text() {
    let server = FakeHerdr::start(herdr_client::REQUIRED_PROTOCOL);
    server
        .client()
        .send_pane_input("w1:p2", None, &["enter"])
        .expect("send");
    let request = server.next_request();
    assert_eq!(request["method"], "pane.send_input");
    assert_eq!(request["params"]["pane_id"], "w1:p2");
    assert_eq!(request["params"]["keys"], json!(["enter"]));
    assert!(request["params"].get("text").is_none());
}

#[test]
fn agent_screen_reads_visible_ansi() {
    let server = FakeHerdr::start(herdr_client::REQUIRED_PROTOCOL);
    let _ = server.client().read_agent_screen("coding").expect("screen");
    let request = server.next_request();
    assert_eq!(request["method"], "agent.read");
    assert_eq!(request["params"]["source"], "visible");
    assert_eq!(request["params"]["format"], "ansi");
    assert_eq!(request["params"]["strip_ansi"], false);
    assert!(request["params"].get("lines").is_none());
}

#[test]
fn server_errors_carry_their_message() {
    let server = FakeHerdr::start(herdr_client::REQUIRED_PROTOCOL);
    let error = server.client().agent("missing").unwrap_err();
    assert!(!error.is_unreachable());
    assert_eq!(error.public_message(), "unknown agent `missing`");
}

#[test]
fn a_stopped_herdr_is_reported_as_unreachable() {
    let client = HerdrClient::new(PathBuf::from("/nonexistent/herdr.sock"))
        .with_timeout(Duration::from_millis(200));
    assert!(client.probe().unwrap_err().is_unreachable());
}

#[test]
fn subscriptions_scope_agent_status_and_treat_all_events_as_invalidations() {
    let server = FakeHerdr::start(herdr_client::REQUIRED_PROTOCOL);
    let events = server
        .client()
        .subscribe(["w1:p2".to_owned()])
        .expect("subscribe");
    let request = server.next_request();
    let subscriptions = request["params"]["subscriptions"].as_array().unwrap();
    let status_subscriptions = subscriptions
        .iter()
        .filter(|subscription| subscription["type"].as_str() == Some("pane.agent_status_changed"))
        .collect::<Vec<_>>();
    assert_eq!(status_subscriptions.len(), 1);
    assert_eq!(status_subscriptions[0]["pane_id"], "w1:p2");

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut received = 0;
    while Instant::now() < deadline && received < 3 {
        received += events.drain();
        thread::sleep(Duration::from_millis(20));
    }

    assert!(events.is_connected());
    assert_eq!(received, 3);
}
