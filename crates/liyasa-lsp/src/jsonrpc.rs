//! The base protocol: `Content-Length` framing and the four message shapes.
//!
//! TODO(rfc-3000): this is the seam to cut if the core lead adds an LSP crate
//! to PRD §6.2.1. Nothing above it knows how a message reached the wire.

use std::io::{BufRead, Write};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A request or notification identifier, which the specification allows to be
/// either a number or a string and requires be echoed back unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Id {
    Number(i64),
    String(String),
}

/// One message read off the wire. A request carries an `id` and expects a
/// response; a notification does not and must not be answered.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Incoming {
    #[serde(default)]
    pub id: Option<Id>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

impl Incoming {
    pub fn is_request(&self) -> bool {
        self.id.is_some()
    }

    /// The params deserialized into the method's own type. `params` is optional
    /// in the base protocol, and an absent one is offered to the type as an
    /// empty object rather than as `null`: a params type whose fields are all
    /// optional then succeeds, and one with a required field still reports the
    /// field it is missing rather than "expected a struct".
    pub fn parse<T: serde::de::DeserializeOwned>(&self) -> Result<T, Error> {
        let value = match self.params.clone() {
            Some(Value::Null) | None => Value::Object(serde_json::Map::new()),
            Some(value) => value,
        };
        serde_json::from_value(value).map_err(|e| Error::invalid_params(e.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Response {
    pub jsonrpc: &'static str,
    pub id: Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Error>,
}

impl Response {
    pub fn ok(id: Id, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: Id, error: Error) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(error),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Notification {
    pub jsonrpc: &'static str,
    pub method: String,
    pub params: Value,
}

impl Notification {
    pub fn new(method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Error {
    pub code: i64,
    pub message: String,
}

/// The codes the specification reserves. `SERVER_NOT_INITIALIZED` and
/// `REQUEST_FAILED` are LSP's own additions to the JSON-RPC set.
impl Error {
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const INTERNAL_ERROR: i64 = -32603;
    pub const SERVER_NOT_INITIALIZED: i64 = -32002;
    pub const REQUEST_FAILED: i64 = -32803;

    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn method_not_found(method: &str) -> Self {
        Self::new(Self::METHOD_NOT_FOUND, format!("unknown method `{method}`"))
    }

    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(Self::INVALID_PARAMS, message)
    }

    pub fn not_initialized() -> Self {
        Self::new(
            Self::SERVER_NOT_INITIALIZED,
            "the client has not sent `initialize` yet",
        )
    }

    pub fn request_failed(message: impl Into<String>) -> Self {
        Self::new(Self::REQUEST_FAILED, message)
    }
}

/// Why a message could not be read. `Eof` is the ordinary end of a session: a
/// client that closes the pipe without `exit` is not an error.
#[derive(Debug)]
pub enum ReadError {
    Eof,
    Io(std::io::Error),
    /// A frame arrived but its body is not a JSON-RPC message. The session
    /// continues: the specification wants a `PARSE_ERROR` response, not a
    /// dropped connection.
    Malformed(String),
}

impl From<std::io::Error> for ReadError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Reads one message. Headers are terminated by a blank line; only
/// `Content-Length` is honoured, and `Content-Type` is accepted and ignored as
/// the specification requires.
pub fn read(input: &mut impl BufRead) -> Result<Incoming, ReadError> {
    let mut length: Option<usize> = None;
    let mut line = String::new();
    loop {
        line.clear();
        if input.read_line(&mut line)? == 0 {
            return Err(ReadError::Eof);
        }
        let header = line.trim_end_matches(['\r', '\n']);
        if header.is_empty() {
            break;
        }
        let Some((name, value)) = header.split_once(':') else {
            return Err(ReadError::Malformed(format!(
                "header without a colon: {header}"
            )));
        };
        if name.trim().eq_ignore_ascii_case("content-length") {
            length = value.trim().parse().ok();
        }
    }

    let Some(length) = length else {
        return Err(ReadError::Malformed("no Content-Length header".to_owned()));
    };
    let mut body = vec![0u8; length];
    input.read_exact(&mut body)?;
    serde_json::from_slice(&body).map_err(|e| ReadError::Malformed(e.to_string()))
}

/// Writes one message with the framing the specification requires: the header
/// block, a blank line, then exactly `Content-Length` bytes.
pub fn write(output: &mut impl Write, message: &impl Serialize) -> std::io::Result<()> {
    let body = serde_json::to_vec(message)?;
    write!(output, "Content-Length: {}\r\n\r\n", body.len())?;
    output.write_all(&body)?;
    output.flush()
}
