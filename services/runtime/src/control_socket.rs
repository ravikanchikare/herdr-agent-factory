//! The listener an Orchestrator calls to drive its own Factory Run.
//!
//! Herdr owns the agents; the Orchestrator drives them with the `herdr` CLI from
//! inside its own pane. It comes here only for the moves that need an
//! Environment applied or a durable transition recorded, because neither can be
//! carried by a pane the agent splits for itself.
//!
//! Accepting happens on its own thread, but nothing is decided there: each call
//! is handed to the single dispatch loop that owns all domain state, and the
//! answer is sent back to the waiting connection. That keeps one writer for the
//! store without making the socket block the runtime.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, SyncSender, channel, sync_channel};

use agent_control::{ControlRequest, ControlResponse};

/// One call waiting on an answer from the dispatch loop.
pub(crate) struct ControlCall {
    pub(crate) request: ControlRequest,
    respond: SyncSender<ControlResponse>,
}

impl ControlCall {
    pub(crate) fn answer(self, response: ControlResponse) {
        // A caller that hung up mid-call is not the runtime's problem; the
        // transition it asked for has already been recorded either way.
        let _ = self.respond.send(response);
    }
}

/// A bound control socket. Dropping it stops accepting and removes the file.
pub(crate) struct ControlListener {
    path: PathBuf,
    calls: Receiver<ControlCall>,
}

impl ControlListener {
    /// Bind the socket, replacing a stale one left by a previous process.
    ///
    /// The socket is owner-only: any local process that can open it can act on a
    /// run, so the token is the authorization and the file mode is the fence
    /// around who may present one.
    pub(crate) fn bind(path: &Path) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // A socket file outlives the process that made it, so a crashed runtime
        // would otherwise make every later start fail with "address in use".
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let listener = UnixListener::bind(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;

        let (sender, calls) = channel();
        std::thread::Builder::new()
            .name("agent-control".into())
            .spawn(move || accept_loop(listener, sender))?;

        Ok(Self {
            path: path.to_path_buf(),
            calls,
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Take every call that arrived since the last tick.
    pub(crate) fn drain(&self) -> Vec<ControlCall> {
        let mut calls = Vec::new();
        while let Ok(call) = self.calls.try_recv() {
            calls.push(call);
        }
        calls
    }
}

impl Drop for ControlListener {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn accept_loop(listener: UnixListener, calls: Sender<ControlCall>) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let calls = calls.clone();
        // One connection per thread so a caller that stops reading cannot hold
        // up the next Orchestrator.
        if std::thread::Builder::new()
            .name("agent-control-call".into())
            .spawn(move || serve(stream, &calls))
            .is_err()
        {
            continue;
        }
    }
}

fn serve(stream: UnixStream, calls: &Sender<ControlCall>) {
    let mut reader = BufReader::new(match stream.try_clone() {
        Ok(stream) => stream,
        Err(_) => return,
    });
    let mut writer = stream;
    let mut line = String::new();
    if reader.read_line(&mut line).unwrap_or(0) == 0 {
        return;
    }
    let response = match serde_json::from_str::<ControlRequest>(&line) {
        Ok(request) => {
            let (respond, answer) = sync_channel(1);
            if calls.send(ControlCall { request, respond }).is_err() {
                ControlResponse::Error {
                    code: "runtime_unavailable".into(),
                    message: "Agent Factory is shutting down.".into(),
                }
            } else {
                answer.recv().unwrap_or(ControlResponse::Error {
                    code: "runtime_unavailable".into(),
                    message: "Agent Factory stopped before answering.".into(),
                })
            }
        }
        Err(error) => ControlResponse::Error {
            code: "invalid_request".into(),
            message: format!("that is not a control request: {error}"),
        },
    };
    if let Ok(mut encoded) = serde_json::to_string(&response) {
        encoded.push('\n');
        let _ = writer.write_all(encoded.as_bytes());
        let _ = writer.flush();
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use agent_control::{ControlCommand, ControlRequest, RunView};

    use super::*;

    fn view(message: &str) -> RunView {
        RunView {
            state: "orchestrating".into(),
            iteration: 0,
            objective: "Build it".into(),
            acceptance_criteria: Vec::new(),
            changed_file_count: 0,
            agent: None,
            evaluation: None,
            message: message.into(),
        }
    }

    #[test]
    fn a_call_reaches_the_dispatch_loop_and_its_answer_returns() {
        let directory = tempfile::TempDir::new().unwrap();
        let path = directory.path().join("agent-control.sock");
        let listener = ControlListener::bind(&path).unwrap();

        let caller = std::thread::spawn({
            let path = path.clone();
            move || {
                agent_control::call(
                    &path,
                    &ControlRequest {
                        token: "token".into(),
                        command: ControlCommand::Status,
                    },
                    Duration::from_secs(5),
                )
                .unwrap()
            }
        });

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            for call in listener.drain() {
                assert_eq!(call.request.token, "token");
                call.answer(ControlResponse::Ok(view("seen")));
            }
            if caller.is_finished() || std::time::Instant::now() > deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        match caller.join().unwrap() {
            ControlResponse::Ok(view) => assert_eq!(view.message, "seen"),
            other => panic!("unexpected answer: {other:?}"),
        }
    }

    #[test]
    fn the_socket_is_owner_only_and_replaces_a_stale_file() {
        let directory = tempfile::TempDir::new().unwrap();
        let path = directory.path().join("agent-control.sock");
        std::fs::write(&path, b"stale").unwrap();

        let listener = ControlListener::bind(&path).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);

        drop(listener);
        assert!(!path.exists(), "the socket is removed with the listener");
    }

    #[test]
    fn a_malformed_line_is_refused_without_reaching_the_loop() {
        let directory = tempfile::TempDir::new().unwrap();
        let path = directory.path().join("agent-control.sock");
        let listener = ControlListener::bind(&path).unwrap();

        let mut stream = UnixStream::connect(&path).unwrap();
        stream.write_all(b"{ not json\n").unwrap();
        stream.flush().unwrap();
        let mut answer = String::new();
        BufReader::new(&stream).read_line(&mut answer).unwrap();

        let response: ControlResponse = serde_json::from_str(&answer).unwrap();
        assert!(matches!(
            response,
            ControlResponse::Error { ref code, .. } if code == "invalid_request"
        ));
        assert!(listener.drain().is_empty());
    }
}
