//! The contract between a Factory Run's Orchestrator and the Rust runtime.
//!
//! Herdr is the runtime agents live on, and an Orchestrator running inside a
//! Herdr pane can already split panes, start agents, and prompt them. The one
//! thing it cannot do for itself is apply an Environment: the resolved provider
//! gateway, model, secrets, skills, and permissions never travel with a pane the
//! agent splits on its own, and asking it to carry them would pull credentials
//! into a context window.
//!
//! So the Orchestrator drives its own loop with the `herdr` CLI and calls this
//! narrow verb set for the steps where an Environment boundary or a durable
//! transition is at stake. The runtime applies the boundary, validates the move,
//! records it, and answers with the agent name to prompt next.
//!
//! The transport mirrors Herdr's own: newline-delimited JSON over a Unix domain
//! socket, one request per connection.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Names the control socket for the Orchestrator's process.
pub const ENDPOINT_ENV: &str = "AGENT_FACTORY_ENDPOINT";
/// Authorizes one Factory Run. Every command is scoped to the run it belongs to,
/// so the Orchestrator never names a run and cannot address another one.
pub const TOKEN_ENV: &str = "AGENT_FACTORY_CONTROL_TOKEN";
/// File name of the socket inside the application data directory.
pub const SOCKET_FILE_NAME: &str = "agent-control.sock";

pub fn socket_path(data_directory: &Path) -> PathBuf {
    data_directory.join(SOCKET_FILE_NAME)
}

/// What the Orchestrator is asking the runtime to do.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "camelCase")]
pub enum ControlCommand {
    /// Where the run stands. The Orchestrator calls this to orient itself after
    /// a restart, or before deciding what to do next.
    Status,
    /// Start Coding, or start the next iteration when Coding has already run.
    StartCoding { brief: String },
    /// Start Evaluation against the current workspace.
    StartEvaluation {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        brief: Option<String>,
    },
    /// Stop and ask a person, without ending the run.
    ///
    /// Use this only when the decision is genuinely not the Orchestrator's to
    /// make. The run stays yours: the answer arrives as a prompt in your own
    /// pane, and your next command clears the question.
    Escalate { question: String },
    /// End the run. This is the only way a run reaches a terminal state.
    Finish {
        verdict: FinishVerdict,
        summary: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FinishVerdict {
    /// The objective is met and the acceptance criteria hold.
    Pass,
    /// The Orchestrator cannot conclude on its own; a person should look.
    NeedsReview,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlRequest {
    pub token: String,
    #[serde(flatten)]
    pub command: ControlCommand,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum ControlResponse {
    Ok(RunView),
    /// A refusal the Orchestrator can read and correct, such as an illegal move
    /// for the run's current state.
    Error {
        code: String,
        message: String,
    },
}

/// What the Orchestrator gets back: enough to prompt the agent it just started,
/// and enough to decide what to do next without reading any terminal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunView {
    pub state: String,
    pub iteration: u32,
    pub objective: String,
    pub acceptance_criteria: Vec<String>,
    pub changed_file_count: u32,
    /// The Herdr agent this command started, ready to be prompted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentHandle>,
    /// The evaluator's verdict, once one exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evaluation: Option<EvaluationView>,
    /// A sentence for the Orchestrator, not for a log.
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentHandle {
    /// The unique Herdr agent name. Prompt it with `herdr agent prompt <name>`.
    pub name: String,
    pub pane_id: String,
    pub harness_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationView {
    pub verdict: String,
    pub summary: String,
}

/// Send one command and read one answer. One request per connection, as Herdr
/// does, so a hung caller cannot wedge the runtime's listener.
pub fn call(
    socket: &Path,
    request: &ControlRequest,
    timeout: Duration,
) -> Result<ControlResponse, ControlError> {
    let stream = UnixStream::connect(socket).map_err(|source| ControlError::Unreachable {
        socket: socket.to_path_buf(),
        source,
    })?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    let mut writer = stream.try_clone()?;
    let mut line = serde_json::to_string(request)?;
    line.push('\n');
    writer.write_all(line.as_bytes())?;
    writer.flush()?;

    let mut reader = BufReader::new(stream);
    let mut answer = String::new();
    if reader.read_line(&mut answer)? == 0 {
        return Err(ControlError::Protocol(
            "the runtime closed the connection without answering".into(),
        ));
    }
    Ok(serde_json::from_str(&answer)?)
}

#[derive(Debug, Error)]
pub enum ControlError {
    #[error("Agent Factory is not listening on {socket}: {source}")]
    Unreachable {
        socket: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("the control connection failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("the runtime sent something unreadable: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("{0}")]
    Protocol(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_carries_its_token_beside_the_command() {
        let request = ControlRequest {
            token: "abc".into(),
            command: ControlCommand::StartCoding {
                brief: "Implement it".into(),
            },
        };
        let encoded = serde_json::to_value(&request).unwrap();
        assert_eq!(encoded["token"], "abc");
        assert_eq!(encoded["command"], "startCoding");
        assert_eq!(encoded["brief"], "Implement it");
        assert_eq!(
            serde_json::from_value::<ControlRequest>(encoded).unwrap(),
            request
        );
    }

    #[test]
    fn evaluation_brief_is_optional_and_omitted_when_absent() {
        let encoded = serde_json::to_value(ControlRequest {
            token: "t".into(),
            command: ControlCommand::StartEvaluation { brief: None },
        })
        .unwrap();
        assert!(encoded.get("brief").is_none());
    }

    #[test]
    fn a_refusal_stays_readable_by_the_agent_that_caused_it() {
        let response = ControlResponse::Error {
            code: "illegal_transition".into(),
            message: "Coding cannot start from `evaluating`".into(),
        };
        let encoded = serde_json::to_string(&response).unwrap();
        assert!(encoded.contains("\"status\":\"error\""));
        assert_eq!(
            serde_json::from_str::<ControlResponse>(&encoded).unwrap(),
            response
        );
    }
}
