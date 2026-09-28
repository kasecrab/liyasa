//! JSON-RPC 2.0, the envelope every MCP message travels in.
//!
//! Pure data: no transport, no dispatch, no knowledge of MCP. The transport
//! decides how bytes arrive and [`super::protocol`] decides what a method
//! means; this module's only job is that a malformed message produces a
//! well-formed refusal rather than a panic or a dropped connection.
//!
//! **Batching is not supported, deliberately.** JSON-RPC 2.0 allows an array
//! of requests and the 2025-03-26 revision of MCP carried that through; the
//! current revision removed it. Accepting it anyway would mean a second
//! response shape that no current client sends and that every handler here
//! would have to be correct for.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const VERSION: &str = "2.0";

// The standard codes. A tool that fails is NOT one of these: it answers with
// a result carrying `isError`, so the model reads the failure and retries,
// rather than the transport failing under it.
pub const PARSE_ERROR: i32 = -32700;
pub const INVALID_REQUEST: i32 = -32600;
pub const METHOD_NOT_FOUND: i32 = -32601;
pub const INVALID_PARAMS: i32 = -32602;
pub const INTERNAL_ERROR: i32 = -32603;

/// A request id. The specification allows a string or a number and forbids
/// null; an absent id is a notification, which is [`Request::id`] being
/// `None` rather than an `Id` variant.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Id {
    Number(i64),
    String(String),
}

impl std::fmt::Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Id::Number(n) => write!(f, "{n}"),
            Id::String(s) => write!(f, "{s}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    /// Absent for a notification, which takes no response at all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Id>,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl Request {
    pub fn is_notification(&self) -> bool {
        self.id.is_none()
    }

    /// `params`, or an empty object. Every MCP method with optional parameters
    /// reads them from a map, and a caller omitting the key entirely is not a
    /// caller making a mistake.
    pub fn params(&self) -> Value {
        match &self.params {
            Some(Value::Null) | None => Value::Object(serde_json::Map::new()),
            Some(value) => value.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorObject {
    pub code: i32,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub jsonrpc: String,
    /// Null only when the request could not be parsed far enough to have one,
    /// which is the single case the specification permits it in.
    pub id: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorObject>,
}

impl Response {
    pub fn result(id: Option<Id>, result: Value) -> Self {
        Self {
            jsonrpc: VERSION.to_owned(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Option<Id>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: VERSION.to_owned(),
            id,
            result: None,
            error: Some(ErrorObject {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }

    pub fn with_data(mut self, data: Value) -> Self {
        if let Some(error) = self.error.as_mut() {
            error.data = Some(data);
        }
        self
    }
}

/// What a transport hands to the dispatcher: a request, or the response to
/// send instead of calling one.
pub enum Incoming {
    Call(Request),
    /// A notification. Nothing is sent back, whatever the handler decides.
    Notify(Request),
    Refuse(Box<Response>),
}

/// Parses one message.
///
/// The three refusals are distinguished because a client can act on the
/// difference: bad JSON means a framing bug, a missing `method` means a
/// serialization bug, and the wrong `jsonrpc` means the peer is speaking
/// another protocol.
pub fn parse(body: &str) -> Incoming {
    let value: Value = match serde_json::from_str(body) {
        Ok(value) => value,
        Err(error) => {
            return Incoming::Refuse(Box::new(
                Response::error(None, PARSE_ERROR, "the request body is not JSON")
                    .with_data(Value::String(error.to_string())),
            ));
        }
    };
    if value.is_array() {
        return Incoming::Refuse(Box::new(Response::error(
            None,
            INVALID_REQUEST,
            "this server does not accept batched requests; send one message per POST",
        )));
    }
    // Read the id before validating anything else, so a refusal can be
    // correlated with the request that caused it. A client that cannot match
    // an error to a call has to time the whole connection out instead.
    let id = value
        .get("id")
        .and_then(|id| serde_json::from_value::<Id>(id.clone()).ok());
    let request: Request = match serde_json::from_value(value) {
        Ok(request) => request,
        Err(error) => {
            return Incoming::Refuse(Box::new(
                Response::error(id, INVALID_REQUEST, "not a JSON-RPC request")
                    .with_data(Value::String(error.to_string())),
            ));
        }
    };
    if request.jsonrpc != VERSION {
        return Incoming::Refuse(Box::new(Response::error(
            request.id,
            INVALID_REQUEST,
            format!(
                "`jsonrpc` must be \"{VERSION}\"; this message says \"{}\"",
                request.jsonrpc
            ),
        )));
    }
    match request.is_notification() {
        true => Incoming::Notify(request),
        false => Incoming::Call(request),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(body: &str) -> Request {
        match parse(body) {
            Incoming::Call(request) => request,
            Incoming::Notify(_) => panic!("a notification, not a call: {body}"),
            Incoming::Refuse(response) => panic!("refused: {:?}", response.error),
        }
    }

    fn refusal(body: &str) -> ErrorObject {
        match parse(body) {
            Incoming::Refuse(response) => response.error.expect("a refusal carries an error"),
            _ => panic!("accepted, and should not have been: {body}"),
        }
    }

    #[test]
    fn a_request_with_an_id_is_a_call_and_one_without_is_a_notification() {
        let request = call(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
        assert_eq!(request.id, Some(Id::Number(1)));
        assert_eq!(request.method, "tools/list");

        let body = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
        assert!(matches!(parse(body), Incoming::Notify(_)));
    }

    #[test]
    fn a_string_id_survives_as_a_string() {
        // A client that sends "1" and is answered 1 cannot match the two, and
        // an `Id` that parsed through a number would do exactly that.
        let request = call(r#"{"jsonrpc":"2.0","id":"1","method":"ping"}"#);
        assert_eq!(request.id, Some(Id::String("1".to_owned())));
        let response = Response::result(request.id, Value::Null);
        let text = serde_json::to_string(&response).expect("serializable");
        assert!(text.contains(r#""id":"1""#), "{text}");
    }

    #[test]
    fn bad_json_is_a_parse_error_and_a_bad_envelope_is_an_invalid_request() {
        assert_eq!(refusal("{not json").code, PARSE_ERROR);
        assert_eq!(refusal(r#"{"jsonrpc":"2.0","id":1}"#).code, INVALID_REQUEST);
        assert_eq!(
            refusal(r#"{"jsonrpc":"1.0","id":1,"method":"ping"}"#).code,
            INVALID_REQUEST
        );
    }

    #[test]
    fn a_refusal_keeps_the_id_whenever_the_message_carried_one() {
        // Without this a client has nothing to correlate the error with and
        // must time the request out instead of failing it.
        match parse(r#"{"jsonrpc":"2.0","id":7,"method":42}"#) {
            Incoming::Refuse(response) => assert_eq!(response.id, Some(Id::Number(7))),
            _ => panic!("a numeric method is not a valid request"),
        }
    }

    #[test]
    fn a_batch_is_refused_by_name_rather_than_half_handled() {
        let error = refusal(r#"[{"jsonrpc":"2.0","id":1,"method":"ping"}]"#);
        assert_eq!(error.code, INVALID_REQUEST);
        assert!(error.message.contains("batched"), "{}", error.message);
    }

    #[test]
    fn absent_and_null_params_both_read_as_an_empty_object() {
        for body in [
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":null}"#,
        ] {
            assert_eq!(call(body).params(), serde_json::json!({}), "{body}");
        }
    }

    #[test]
    fn a_result_and_an_error_are_never_both_present() {
        let ok = Response::result(Some(Id::Number(1)), serde_json::json!({}));
        assert!(ok.error.is_none());
        let text = serde_json::to_string(&ok).expect("serializable");
        assert!(!text.contains("error"), "{text}");

        let bad = Response::error(Some(Id::Number(1)), METHOD_NOT_FOUND, "no such method");
        assert!(bad.result.is_none());
        let text = serde_json::to_string(&bad).expect("serializable");
        assert!(!text.contains("result"), "{text}");
    }
}
