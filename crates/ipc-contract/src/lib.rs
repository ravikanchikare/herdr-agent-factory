//! Versioned transport contract shared by the native host and Rust runtime.

use std::io::{self, Read, Write};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

pub const PROTOCOL_VERSION: u16 = 1;
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Frame {
    Hello(Hello),
    Ready(Ready),
    Request(Request),
    Response(Response),
    Event(Event),
    Shutdown(Shutdown),
}

impl Frame {
    pub fn version(&self) -> u16 {
        match self {
            Self::Hello(value) => value.version,
            Self::Ready(value) => value.version,
            Self::Request(value) => value.version,
            Self::Response(value) => value.version,
            Self::Event(value) => value.version,
            Self::Shutdown(value) => value.version,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Hello {
    pub version: u16,
    pub host_name: String,
    pub host_version: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Ready {
    pub version: u16,
    pub runtime_name: String,
    pub runtime_version: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Request {
    pub version: u16,
    pub id: Uuid,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

impl Request {
    pub fn new(method: impl Into<String>, params: Value) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            id: Uuid::new_v4(),
            method: method.into(),
            params,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Response {
    pub version: u16,
    pub id: Uuid,
    #[serde(flatten)]
    pub outcome: ResponseOutcome,
}

impl Response {
    pub fn success(id: Uuid, result: Value) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            id,
            outcome: ResponseOutcome::Success { result },
        }
    }

    pub fn error(id: Uuid, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            id,
            outcome: ResponseOutcome::Error {
                error: ErrorBody {
                    code,
                    message: message.into(),
                },
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ResponseOutcome {
    Success { result: Value },
    Error { error: ErrorBody },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidRequest,
    MethodNotFound,
    InvalidParams,
    Conflict,
    Internal,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Event {
    pub version: u16,
    pub sequence: u64,
    pub revision: u64,
    pub topic: String,
    pub payload: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Shutdown {
    pub version: u16,
    pub reason: String,
}

#[derive(Debug, Error)]
pub enum CodecError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("frame length {actual} exceeds limit {maximum}")]
    TooLarge { actual: usize, maximum: usize },
    #[error("invalid JSON frame: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("unsupported IPC protocol version {actual}; expected {expected}")]
    UnsupportedVersion { actual: u16, expected: u16 },
}

pub fn write_frame(mut writer: impl Write, frame: &Frame) -> Result<(), CodecError> {
    if frame.version() != PROTOCOL_VERSION {
        return Err(CodecError::UnsupportedVersion {
            actual: frame.version(),
            expected: PROTOCOL_VERSION,
        });
    }

    let payload = serde_json::to_vec(frame)?;
    if payload.len() > MAX_FRAME_BYTES {
        return Err(CodecError::TooLarge {
            actual: payload.len(),
            maximum: MAX_FRAME_BYTES,
        });
    }

    writer.write_all(&(payload.len() as u32).to_be_bytes())?;
    writer.write_all(&payload)?;
    writer.flush()?;
    Ok(())
}

pub fn read_frame(mut reader: impl Read) -> Result<Option<Frame>, CodecError> {
    let mut header = [0_u8; 4];
    match reader.read(&mut header[..1]) {
        Ok(0) => return Ok(None),
        Ok(1) => {}
        Ok(_) => unreachable!("read buffer contains one byte"),
        Err(error) => return Err(error.into()),
    }
    reader.read_exact(&mut header[1..])?;

    let length = u32::from_be_bytes(header) as usize;
    if length > MAX_FRAME_BYTES {
        return Err(CodecError::TooLarge {
            actual: length,
            maximum: MAX_FRAME_BYTES,
        });
    }

    let mut payload = vec![0; length];
    reader.read_exact(&mut payload)?;
    let frame: Frame = serde_json::from_slice(&payload)?;
    if frame.version() != PROTOCOL_VERSION {
        return Err(CodecError::UnsupportedVersion {
            actual: frame.version(),
            expected: PROTOCOL_VERSION,
        });
    }
    Ok(Some(frame))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_request() {
        let frame = Frame::Request(Request::new(
            "project.list",
            serde_json::json!({ "limit": 10 }),
        ));
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &frame).unwrap();

        assert_eq!(read_frame(bytes.as_slice()).unwrap(), Some(frame));
    }

    #[test]
    fn rejects_an_oversized_declared_frame_before_allocating() {
        let size = (MAX_FRAME_BYTES as u32 + 1).to_be_bytes();
        let error = read_frame(size.as_slice()).unwrap_err();

        assert!(matches!(error, CodecError::TooLarge { .. }));
    }

    #[test]
    fn rejects_an_unknown_protocol_version() {
        let frame = Frame::Hello(Hello {
            version: PROTOCOL_VERSION + 1,
            host_name: "test".into(),
            host_version: "0".into(),
        });
        let error = write_frame(Vec::new(), &frame).unwrap_err();

        assert!(matches!(error, CodecError::UnsupportedVersion { .. }));
    }

    #[test]
    fn clean_eof_has_no_frame() {
        assert_eq!(read_frame([].as_slice()).unwrap(), None);
    }

    #[test]
    fn truncated_payload_is_an_io_error() {
        let bytes = [0, 0, 0, 4, b'{'];
        let error = read_frame(bytes.as_slice()).unwrap_err();
        assert!(matches!(error, CodecError::Io(_)));
    }

    #[test]
    fn truncated_header_is_not_treated_as_clean_eof() {
        let error = read_frame([0, 0].as_slice()).unwrap_err();
        assert!(matches!(error, CodecError::Io(_)));
    }
}
