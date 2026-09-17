//! The base protocol: framing in, framing out, and the malformed frames a
//! client can send that must not end the session.

use std::io::Cursor;

use liyasa_lsp::jsonrpc::{self, Error, Id, Notification, ReadError, Response};

fn frame(body: &str) -> Vec<u8> {
    format!("Content-Length: {}\r\n\r\n{body}", body.len()).into_bytes()
}

#[test]
fn reads_a_request() {
    let wire = frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"rootUri":null}}"#);
    let message = jsonrpc::read(&mut Cursor::new(wire)).expect("a framed request reads");
    assert_eq!(message.method, "initialize");
    assert_eq!(message.id, Some(Id::Number(1)));
    assert!(message.is_request());
}

#[test]
fn reads_a_notification_as_one() {
    let wire = frame(r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#);
    let message = jsonrpc::read(&mut Cursor::new(wire)).expect("a notification reads");
    assert!(!message.is_request(), "no id means no response is expected");
}

#[test]
fn a_string_id_survives_the_round_trip() {
    let wire = frame(r#"{"jsonrpc":"2.0","id":"a7","method":"shutdown"}"#);
    let message = jsonrpc::read(&mut Cursor::new(wire)).expect("a string id reads");
    assert_eq!(message.id, Some(Id::String("a7".to_owned())));

    let mut out = Vec::new();
    let response = Response::ok(
        message.id.expect("the request has an id"),
        serde_json::Value::Null,
    );
    jsonrpc::write(&mut out, &response).expect("a response writes");
    let text = String::from_utf8(out).expect("the wire is utf-8");
    assert!(
        text.contains(r#""id":"a7""#),
        "the id is echoed unchanged: {text}"
    );
}

#[test]
fn headers_are_case_insensitive_and_content_type_is_ignored() {
    let body = r#"{"jsonrpc":"2.0","id":2,"method":"shutdown"}"#;
    let wire = format!(
        "content-length: {}\r\nContent-Type: application/vscode-jsonrpc; charset=utf-8\r\n\r\n{body}",
        body.len()
    );
    let message = jsonrpc::read(&mut Cursor::new(wire.into_bytes())).expect("the frame reads");
    assert_eq!(message.method, "shutdown");
}

#[test]
fn several_messages_read_in_order_from_one_stream() {
    let mut wire = frame(r#"{"jsonrpc":"2.0","id":1,"method":"one"}"#);
    wire.extend(frame(r#"{"jsonrpc":"2.0","id":2,"method":"two"}"#));
    let mut input = Cursor::new(wire);
    assert_eq!(jsonrpc::read(&mut input).expect("first").method, "one");
    assert_eq!(jsonrpc::read(&mut input).expect("second").method, "two");
    assert!(matches!(jsonrpc::read(&mut input), Err(ReadError::Eof)));
}

#[test]
fn a_frame_that_is_not_json_is_malformed_not_fatal() {
    let wire = frame("not json at all");
    match jsonrpc::read(&mut Cursor::new(wire)) {
        Err(ReadError::Malformed(_)) => {}
        other => panic!("expected a malformed frame, got {other:?}"),
    }
}

#[test]
fn a_header_block_without_a_length_is_malformed() {
    let wire = b"Content-Type: application/json\r\n\r\n{}".to_vec();
    match jsonrpc::read(&mut Cursor::new(wire)) {
        Err(ReadError::Malformed(message)) => assert!(message.contains("Content-Length")),
        other => panic!("expected a malformed frame, got {other:?}"),
    }
}

#[test]
fn a_closed_stream_is_eof_not_an_error() {
    assert!(matches!(
        jsonrpc::read(&mut Cursor::new(Vec::new())),
        Err(ReadError::Eof)
    ));
}

#[test]
fn written_frames_declare_their_byte_length_not_their_character_length() {
    let mut out = Vec::new();
    let note = Notification::new(
        "window/showMessage",
        serde_json::json!({ "message": "héllo" }),
    );
    jsonrpc::write(&mut out, &note).expect("a notification writes");

    let text = String::from_utf8(out).expect("the wire is utf-8");
    let (headers, body) = text
        .split_once("\r\n\r\n")
        .expect("a blank line ends the headers");
    let declared: usize = headers
        .strip_prefix("Content-Length: ")
        .and_then(|n| n.trim().parse().ok())
        .expect("the header declares a length");
    assert_eq!(declared, body.len(), "the length is in bytes: {body}");
}

#[test]
fn an_error_response_carries_no_result_field() {
    let mut out = Vec::new();
    let response = Response::err(
        Id::Number(9),
        Error::method_not_found("textDocument/rename"),
    );
    jsonrpc::write(&mut out, &response).expect("an error response writes");
    let text = String::from_utf8(out).expect("the wire is utf-8");
    assert!(!text.contains(r#""result""#), "result is omitted: {text}");
    assert!(text.contains(&Error::METHOD_NOT_FOUND.to_string()));
}

#[test]
fn absent_params_deserialize_as_an_all_optional_type() {
    let wire = frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#);
    let message = jsonrpc::read(&mut Cursor::new(wire)).expect("the frame reads");
    let params: liyasa_lsp::protocol::InitializeParams = message
        .parse()
        .expect("all-optional params tolerate a missing object");
    assert!(params.root_uri.is_none());
}
