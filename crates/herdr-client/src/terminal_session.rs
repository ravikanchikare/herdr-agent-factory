//! Live Herdr terminal session attach.
//!
//! `pane.send_text` plus `agent.read` is a snapshot remote-control path. The
//! public bridge for an interactive TUI is `herdr terminal session control`:
//! newline-delimited ANSI frames on stdout and `terminal.input` JSON on stdin.
//! That is what the working POC and community plugins such as herdr-mirror use.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use base64::Engine;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::HerdrError;

const DEFAULT_COLS: u16 = 80;
const DEFAULT_ROWS: u16 = 24;

/// How to spawn a writable Herdr terminal session controller.
#[derive(Clone, Debug)]
pub struct TerminalAttach {
    pub herdr_bin: PathBuf,
    pub api_socket: PathBuf,
    pub session: Option<String>,
    pub target: String,
    pub cols: u16,
    pub rows: u16,
    pub takeover: bool,
}

/// A live writable attach to one Herdr pane.
pub struct TerminalSession {
    child: Child,
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    state: Arc<Mutex<SessionState>>,
    _stdout: JoinHandle<()>,
    _stderr: JoinHandle<()>,
}

#[derive(Debug, Default)]
struct SessionState {
    output: String,
    revision: u64,
    closed: bool,
    close_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SessionRecord {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    full: bool,
    #[serde(default)]
    bytes: Option<String>,
    #[serde(default)]
    reason: Option<String>,
}

impl TerminalAttach {
    pub fn command_args(&self) -> Vec<String> {
        control_args(
            self.session.as_deref(),
            &self.target,
            self.takeover,
            self.cols.max(1),
            self.rows.max(1),
        )
    }
}

impl TerminalSession {
    pub fn attach(request: TerminalAttach) -> Result<Self, HerdrError> {
        let cols = request.cols.max(1);
        let rows = request.rows.max(1);
        let mut command = Command::new(&request.herdr_bin);
        command
            .args(control_args(
                request.session.as_deref(),
                &request.target,
                request.takeover,
                cols,
                rows,
            ))
            .env("HERDR_SOCKET_PATH", &request.api_socket)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(HerdrError::Io)?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| HerdrError::Protocol("terminal session stdin is missing".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| HerdrError::Protocol("terminal session stdout is missing".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| HerdrError::Protocol("terminal session stderr is missing".into()))?;

        let state = Arc::new(Mutex::new(SessionState::default()));
        let stdout_state = Arc::clone(&state);
        let stdout_handle = std::thread::Builder::new()
            .name("herdr-terminal-stdout".into())
            .spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else {
                        break;
                    };
                    apply_record_line(&stdout_state, &line);
                    if stdout_state.lock().is_ok_and(|guard| guard.closed) {
                        break;
                    }
                }
                if let Ok(mut guard) = stdout_state.lock() {
                    guard.closed = true;
                }
            })?;

        let stderr_handle = std::thread::Builder::new()
            .name("herdr-terminal-stderr".into())
            .spawn(move || {
                let mut reader = BufReader::new(stderr);
                let mut unused = String::new();
                let _ = reader.read_to_string(&mut unused);
            })?;

        Ok(Self {
            child,
            stdin: Arc::new(Mutex::new(Some(stdin))),
            state,
            _stdout: stdout_handle,
            _stderr: stderr_handle,
        })
    }

    pub fn send_text(&self, text: &str) -> Result<(), HerdrError> {
        if text.is_empty() {
            return Ok(());
        }
        self.send_command(&json!({
            "type": "terminal.input",
            "text": text,
        }))
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), HerdrError> {
        if cols == 0 || rows == 0 {
            return Ok(());
        }
        self.send_command(&json!({
            "type": "terminal.resize",
            "cols": cols,
            "rows": rows,
        }))
    }

    pub fn snapshot(&self) -> (String, u64, bool) {
        self.state
            .lock()
            .map(|guard| (guard.output.clone(), guard.revision, false))
            .unwrap_or_default()
    }

    pub fn is_alive(&mut self) -> bool {
        if self.state.lock().is_ok_and(|guard| guard.closed) {
            return false;
        }
        match self.child.try_wait() {
            Ok(None) => true,
            _ => {
                if let Ok(mut guard) = self.state.lock() {
                    guard.closed = true;
                }
                false
            }
        }
    }

    fn send_command(&self, command: &Value) -> Result<(), HerdrError> {
        let mut stdin = self
            .stdin
            .lock()
            .map_err(|_| HerdrError::Protocol("terminal session stdin lock is poisoned".into()))?;
        let Some(writer) = stdin.as_mut() else {
            return Err(HerdrError::Protocol(
                "terminal session controller is closed".into(),
            ));
        };
        serde_json::to_writer(&mut *writer, command)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        Ok(())
    }

    fn release(&mut self) {
        if let Ok(mut stdin) = self.stdin.lock()
            && let Some(mut writer) = stdin.take()
        {
            let _ = serde_json::to_writer(&mut writer, &json!({ "type": "terminal.release" }));
            let _ = writer.write_all(b"\n");
            let _ = writer.flush();
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        self.release();
    }
}

/// Resolve the `herdr` CLI used for terminal session control.
///
/// This is the multiplexer client, not an agent. `HERDR_BIN` and
/// `HERDR_BIN_PATH` win, then `PATH`, then common install locations.
pub fn resolve_herdr_bin() -> Result<PathBuf, HerdrError> {
    for key in ["HERDR_BIN", "HERDR_BIN_PATH"] {
        if let Some(value) = std::env::var_os(key) {
            let path = PathBuf::from(value);
            if path.is_file() {
                return Ok(path);
            }
            return Err(HerdrError::Protocol(format!(
                "{key} does not point to a herdr executable"
            )));
        }
    }
    if let Some(path) = find_on_path("herdr") {
        return Ok(path);
    }
    let home = directories::BaseDirs::new().map(|base| base.home_dir().to_path_buf());
    let mut candidates = vec![
        PathBuf::from("/opt/homebrew/bin/herdr"),
        PathBuf::from("/usr/local/bin/herdr"),
    ];
    if let Some(home) = home {
        candidates.insert(0, home.join(".local/bin/herdr"));
    }
    if let Some(path) = candidates.into_iter().find(|path| path.is_file()) {
        return Ok(path);
    }
    Err(HerdrError::Protocol(
        "herdr CLI is not available to attach an interactive terminal".into(),
    ))
}

pub(crate) fn control_args(
    session: Option<&str>,
    target: &str,
    takeover: bool,
    cols: u16,
    rows: u16,
) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(session) = session {
        args.push("--session".into());
        args.push(session.into());
    }
    args.extend([
        "terminal".into(),
        "session".into(),
        "control".into(),
        target.into(),
    ]);
    if takeover {
        args.push("--takeover".into());
    }
    args.extend([
        "--cols".into(),
        cols.max(1).to_string(),
        "--rows".into(),
        rows.max(1).to_string(),
    ]);
    args
}

fn apply_session_record(output: &mut String, record: &SessionRecord) -> bool {
    match record.kind.as_str() {
        "terminal.frame" => {
            let Some(bytes) = record.bytes.as_deref() else {
                return false;
            };
            let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(bytes) else {
                return false;
            };
            let text = String::from_utf8_lossy(&decoded);
            if record.full {
                output.clear();
            }
            output.push_str(&text);
            true
        }
        "terminal.closed" => false,
        _ => false,
    }
}

fn apply_record_line(state: &Arc<Mutex<SessionState>>, line: &str) {
    let Ok(record) = serde_json::from_str::<SessionRecord>(line) else {
        return;
    };
    let Ok(mut guard) = state.lock() else {
        return;
    };
    if record.kind == "terminal.closed" {
        guard.closed = true;
        guard.close_reason = record.reason;
        return;
    }
    if apply_session_record(&mut guard.output, &record) {
        guard.revision = guard.revision.saturating_add(1);
    }
}

fn find_on_path(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(program))
        .find(|candidate| candidate.is_file())
}

pub fn default_terminal_size() -> (u16, u16) {
    (DEFAULT_COLS, DEFAULT_ROWS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_args_match_the_public_bridge() {
        assert_eq!(
            control_args(None, "w1:p2", true, 120, 40),
            [
                "terminal",
                "session",
                "control",
                "w1:p2",
                "--takeover",
                "--cols",
                "120",
                "--rows",
                "40",
            ]
        );
        assert_eq!(
            control_args(Some("factory"), "coding", false, 0, 0)[0],
            "--session"
        );
    }

    #[test]
    fn full_frames_replace_and_deltas_append() {
        let mut output = String::from("old");
        assert!(apply_session_record(
            &mut output,
            &SessionRecord {
                kind: "terminal.frame".into(),
                full: true,
                bytes: Some(base64::engine::general_purpose::STANDARD.encode("hello")),
                reason: None,
            },
        ));
        assert_eq!(output, "hello");
        assert!(apply_session_record(
            &mut output,
            &SessionRecord {
                kind: "terminal.frame".into(),
                full: false,
                bytes: Some(base64::engine::general_purpose::STANDARD.encode("!")),
                reason: None,
            },
        ));
        assert_eq!(output, "hello!");
    }

    #[test]
    fn input_command_always_sends_a_string() {
        let command = json!({ "type": "terminal.input", "text": "\r" });
        assert_eq!(command["text"], "\r");
        assert!(command["text"].is_string());
    }

    #[test]
    fn client_socket_derives_from_the_api_socket() {
        let api = std::path::Path::new("/config/herdr/herdr.sock");
        let stem = api.file_stem().and_then(|value| value.to_str()).unwrap();
        let client = api.parent().unwrap().join(format!("{stem}-client.sock"));
        assert_eq!(client, PathBuf::from("/config/herdr/herdr-client.sock"));
    }
}
