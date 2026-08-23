//! Newline-delimited JSON transport over the Herdr control socket.
//!
//! Herdr answers exactly one request per connection and then closes it, so a
//! call opens a socket, writes one frame, reads one frame, and drops it.
//! `events.subscribe` is the single exception: the connection stays open and
//! streams event frames until the client shuts it down.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

use crate::error::HerdrError;

/// Frame the request the same way the `herdr` CLI does.
#[derive(Serialize)]
struct RequestFrame<'a, P> {
    id: &'a str,
    method: &'a str,
    params: P,
}

pub(crate) struct Connection {
    stream: UnixStream,
    reader: BufReader<UnixStream>,
}

impl Connection {
    pub(crate) fn open(socket: &Path, timeout: Duration) -> Result<Self, HerdrError> {
        let stream = UnixStream::connect(socket).map_err(|source| HerdrError::Unreachable {
            socket: socket.to_path_buf(),
            source,
        })?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        let reader = BufReader::new(stream.try_clone()?);
        Ok(Self { stream, reader })
    }

    /// Send one request frame. Herdr ignores the id beyond echoing it back.
    pub(crate) fn send<P: Serialize>(&mut self, method: &str, params: P) -> Result<(), HerdrError> {
        let frame = serde_json::to_string(&RequestFrame {
            id: "agent-factory",
            method,
            params,
        })?;
        self.stream.write_all(frame.as_bytes())?;
        self.stream.write_all(b"\n")?;
        self.stream.flush()?;
        Ok(())
    }

    /// Read one frame, or `None` when the server closed the connection.
    pub(crate) fn read_frame(&mut self) -> Result<Option<Value>, HerdrError> {
        let mut line = String::new();
        loop {
            line.clear();
            if self.reader.read_line(&mut line)? == 0 {
                return Ok(None);
            }
            if line.trim().is_empty() {
                continue;
            }
            return Ok(Some(serde_json::from_str(&line)?));
        }
    }

    /// Read frames without a deadline. Used by the long-lived event stream.
    pub(crate) fn clear_read_timeout(&self) -> Result<(), HerdrError> {
        self.stream.set_read_timeout(None)?;
        Ok(())
    }

    pub(crate) fn shutdown_handle(&self) -> Result<UnixStream, HerdrError> {
        Ok(self.stream.try_clone()?)
    }
}

/// Split a response frame into its result payload or its server error.
pub(crate) fn into_result(frame: Value) -> Result<Value, HerdrError> {
    if let Some(error) = frame.get("error") {
        let code = error
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned();
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Herdr reported an error without a message")
            .to_owned();
        return Err(HerdrError::Server { code, message });
    }
    frame
        .get("result")
        .cloned()
        .ok_or_else(|| HerdrError::Protocol("response carried neither result nor error".into()))
}

/// Resolve the control socket for the default session or a named one.
///
/// Herdr keeps the default session socket beside its configuration and named
/// sessions under `sessions/<name>/`.
pub fn socket_path(config_root: &Path, session: Option<&str>) -> PathBuf {
    match session {
        Some(name) => config_root.join("sessions").join(name).join("herdr.sock"),
        None => config_root.join("herdr.sock"),
    }
}

/// The directory Herdr keeps its configuration and default socket in.
pub fn config_root() -> Result<PathBuf, HerdrError> {
    if let Some(root) = std::env::var_os("HERDR_CONFIG_DIR") {
        return Ok(PathBuf::from(root));
    }
    if let Some(base) = std::env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(base).join("herdr"));
    }
    let home = directories::BaseDirs::new().ok_or_else(|| {
        HerdrError::Protocol("no home directory is available to locate Herdr".into())
    })?;
    Ok(home.home_dir().join(".config").join("herdr"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_and_named_sessions_use_distinct_sockets() {
        let root = Path::new("/config/herdr");
        assert_eq!(
            socket_path(root, None),
            PathBuf::from("/config/herdr/herdr.sock")
        );
        assert_eq!(
            socket_path(root, Some("agent-factory")),
            PathBuf::from("/config/herdr/sessions/agent-factory/herdr.sock")
        );
    }

    #[test]
    fn server_errors_surface_their_code() {
        let frame = serde_json::json!({
            "id": "agent-factory",
            "error": {"code": "invalid_request", "message": "unknown pane"}
        });
        let error = into_result(frame).unwrap_err();
        assert!(matches!(
            error,
            HerdrError::Server { ref code, .. } if code == "invalid_request"
        ));
    }

    #[test]
    fn results_unwrap_to_their_payload() {
        let frame = serde_json::json!({"id": "agent-factory", "result": {"type": "ok"}});
        assert_eq!(into_result(frame).unwrap()["type"], "ok");
    }
}
