//! Newline-delimited JSON framing with a size limit.

use std::fmt;
use std::io::{self, BufRead};

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::protocol::MAX_LINE;

/// Errors of the network layer.
#[derive(Debug)]
pub enum NetError {
    Io(io::Error),
    LineTooLong,
    /// Not UTF-8, not JSON, or not a known message.
    Malformed(String),
    /// The other side closed the connection.
    Closed,
    /// The peer sent something that is not allowed in the current state.
    Protocol(String),
    PendingAdmission {
        request_id: String,
    },
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetError::Io(e) => write!(f, "network error: {e}"),
            NetError::LineTooLong => write!(f, "message longer than {MAX_LINE} bytes"),
            NetError::Malformed(m) => write!(f, "malformed message: {m}"),
            NetError::Closed => write!(f, "connection closed"),
            NetError::Protocol(m) => write!(f, "protocol error: {m}"),
            NetError::PendingAdmission { request_id } => write!(
                f,
                "admission outcome unknown; reconcile request {request_id}"
            ),
        }
    }
}

impl std::error::Error for NetError {}

impl From<io::Error> for NetError {
    fn from(e: io::Error) -> Self {
        NetError::Io(e)
    }
}

/// Serialises a message as one line including the trailing newline.
pub fn encode<T: Serialize>(msg: &T) -> Result<Vec<u8>, NetError> {
    let mut bytes = serde_json::to_vec(msg).map_err(|e| NetError::Malformed(e.to_string()))?;
    if bytes.len() > MAX_LINE {
        return Err(NetError::LineTooLong);
    }
    bytes.push(b'\n');
    Ok(bytes)
}

/// Parses one line (without the newline).
pub fn decode<T: DeserializeOwned>(line: &[u8]) -> Result<T, NetError> {
    if line.len() > MAX_LINE {
        return Err(NetError::LineTooLong);
    }
    let text = std::str::from_utf8(line).map_err(|_| NetError::Malformed("not UTF-8".into()))?;
    serde_json::from_str(text).map_err(|e| NetError::Malformed(e.to_string()))
}

pub fn decode_client(line: &[u8]) -> Result<crate::protocol::ClientMsg, NetError> {
    let value: serde_json::Value = decode(line)?;
    if value.get("type").and_then(serde_json::Value::as_str) == Some("Hello")
        || value.get("request_id").is_some()
        || value.get("room_id").is_some()
    {
        let request = serde_json::from_value(value)
            .map_err(|_| NetError::Malformed("invalid room protocol message".into()))?;
        Ok(crate::protocol::ClientMsg::RoomV2(Box::new(request)))
    } else {
        serde_json::from_value(value)
            .map_err(|_| NetError::Malformed("invalid client message".into()))
    }
}

/// Reads lines without ever buffering more than [`MAX_LINE`] bytes of one line.
pub struct LineReader<R> {
    inner: R,
}

impl<R: BufRead> LineReader<R> {
    pub fn new(inner: R) -> Self {
        Self { inner }
    }

    /// Next line without its newline; `Ok(None)` at end of stream.
    pub fn read_line(&mut self) -> Result<Option<Vec<u8>>, NetError> {
        let mut line = Vec::new();
        loop {
            let buf = self.inner.fill_buf()?;
            if buf.is_empty() {
                return if line.is_empty() {
                    Ok(None)
                } else {
                    Err(NetError::Closed)
                };
            }
            match buf.iter().position(|b| *b == b'\n') {
                Some(i) => {
                    line.extend_from_slice(&buf[..i]);
                    self.inner.consume(i + 1);
                    if line.len() > MAX_LINE {
                        return Err(NetError::LineTooLong);
                    }
                    return Ok(Some(line));
                }
                None => {
                    let n = buf.len();
                    line.extend_from_slice(buf);
                    self.inner.consume(n);
                    if line.len() > MAX_LINE {
                        return Err(NetError::LineTooLong);
                    }
                }
            }
        }
    }
}
