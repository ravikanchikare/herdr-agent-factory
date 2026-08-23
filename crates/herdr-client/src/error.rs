use std::path::PathBuf;

use thiserror::Error;

/// The lowest Herdr socket protocol Agent Factory speaks.
pub const REQUIRED_PROTOCOL: u32 = 19;

#[derive(Debug, Error)]
pub enum HerdrError {
    #[error("Herdr is not reachable at {socket}: {source}")]
    Unreachable {
        socket: PathBuf,
        source: std::io::Error,
    },
    #[error("Herdr speaks protocol {server} but Agent Factory requires {REQUIRED_PROTOCOL}")]
    IncompatibleProtocol { server: u32 },
    #[error("Herdr rejected the request ({code}): {message}")]
    Server { code: String, message: String },
    #[error("unexpected Herdr response: {0}")]
    Protocol(String),
    #[error("the Herdr event stream closed")]
    EventStreamClosed,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl HerdrError {
    /// Whether the failure means Herdr is simply not running.
    pub fn is_unreachable(&self) -> bool {
        matches!(self, Self::Unreachable { .. })
    }

    /// Transient start/prompt readiness: a new shell is not ready yet, or the
    /// named agent is still launching. Callers should retry until the name is
    /// active rather than treating this as a hard failure.
    pub fn is_transient(&self) -> bool {
        self.is_pane_busy() || self.is_agent_not_ready()
    }

    /// The pane's shell has not reached a prompt, so `agent start` cannot run.
    pub fn is_pane_busy(&self) -> bool {
        matches!(self, Self::Server { code, .. } if code == "agent_pane_busy")
    }

    /// The named agent exists but is not ready for prompts. The name remains
    /// valid for read and send-keys; Herdr reports idle when prompts can go.
    pub fn is_agent_not_ready(&self) -> bool {
        matches!(self, Self::Server { code, .. } if code == "agent_not_ready")
    }

    /// Herdr has no binding for this agent name. The pane may still host the
    /// occupant; callers should retry against the pane id.
    pub fn is_agent_not_found(&self) -> bool {
        matches!(self, Self::Server { code, .. } if code == "agent_not_found")
    }

    /// The name is missing or not prompt-ready. The pane id is the fallback.
    pub fn is_unbound_agent(&self) -> bool {
        self.is_agent_not_ready() || self.is_agent_not_found()
    }

    /// A message that is safe to show in the UI without leaking local paths.
    pub fn public_message(&self) -> String {
        match self {
            Self::Unreachable { .. } => {
                "Herdr is not running. Start it, then reconnect Agent Factory.".into()
            }
            Self::IncompatibleProtocol { server } => format!(
                "Herdr speaks protocol {server}; Agent Factory requires {REQUIRED_PROTOCOL}. Update Herdr."
            ),
            Self::Server { message, .. } => message.clone(),
            other => other.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unreachable_errors_do_not_leak_the_socket_path() {
        let error = HerdrError::Unreachable {
            socket: PathBuf::from("/Users/someone/.config/herdr/herdr.sock"),
            source: std::io::Error::other("no such file"),
        };
        assert!(error.is_unreachable());
        assert!(!error.public_message().contains("/Users/someone"));
    }

    #[test]
    fn server_errors_pass_their_message_through() {
        let error = HerdrError::Server {
            code: "invalid_request".into(),
            message: "unknown pane w9:p9".into(),
        };
        assert_eq!(error.public_message(), "unknown pane w9:p9");
        assert!(!error.is_transient());
    }

    #[test]
    fn pane_busy_and_named_agent_not_ready_are_transient() {
        let busy = HerdrError::Server {
            code: "agent_pane_busy".into(),
            message: "pane is not ready".into(),
        };
        assert!(busy.is_transient());
        assert!(busy.is_pane_busy());
        assert!(!busy.is_agent_not_ready());

        let not_ready = HerdrError::Server {
            code: "agent_not_ready".into(),
            message: "agent coding-abc is not an active named agent".into(),
        };
        assert!(not_ready.is_transient());
        assert!(not_ready.is_agent_not_ready());
        assert!(not_ready.is_unbound_agent());
        assert!(!not_ready.is_pane_busy());

        let missing = HerdrError::Server {
            code: "agent_not_found".into(),
            message: "agent target orch-abc is not an active named agent".into(),
        };
        assert!(missing.is_agent_not_found());
        assert!(missing.is_unbound_agent());
        assert!(!missing.is_transient());
    }
}
