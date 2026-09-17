//! The server as an editor sees it: bytes in, bytes out.
//!
//! These are the acceptance tests for ED-61. Each one drives the real dispatch
//! through the real framing, so nothing here can pass because a helper was
//! convenient.

use std::io::Cursor;

use liyasa_lsp::jsonrpc::{self, Id, Incoming};
use liyasa_lsp::server::{Outgoing, Server};
use serde_json::{Value, json};

const URI: &str = "file:///w/docs/guides/install.md";

fn frame(body: &Value) -> Vec<u8> {
    let body = serde_json::to_string(body).expect("a message serializes");
    format!("Content-Length: {}\r\n\r\n{body}", body.len()).into_bytes()
}

fn request(id: i64, method: &str, params: Value) -> Incoming {
    let wire = frame(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
    jsonrpc::read(&mut Cursor::new(wire)).expect("a request reads")
}

fn notify(method: &str, params: Value) -> Incoming {
    let wire = frame(&json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    jsonrpc::read(&mut Cursor::new(wire)).expect("a notification reads")
}

/// The one response to a request, as JSON.
fn result(out: Vec<Outgoing>) -> Value {
    for message in out {
        if let Outgoing::Response(response) = message {
            assert!(response.error.is_none(), "unexpected error: {response:?}");
            return response.result.unwrap_or(Value::Null);
        }
    }
    panic!("no response");
}

fn error(out: Vec<Outgoing>) -> jsonrpc::Error {
    for message in out {
        if let Outgoing::Response(response) = message {
            return response.error.expect("an error was expected");
        }
    }
    panic!("no response");
}

fn published(out: Vec<Outgoing>) -> Value {
    for message in out {
        if let Outgoing::Notification(note) = message
            && note.method == "textDocument/publishDiagnostics"
        {
            return note.params;
        }
    }
    panic!("nothing was published");
}

/// An initialized server with one file open.
fn open(text: &str) -> Server {
    let mut server = Server::new();
    server.handle(&request(
        1,
        "initialize",
        json!({ "rootUri": "file:///w/docs" }),
    ));
    server.handle(&notify("initialized", json!({})));
    server.handle(&notify(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": URI, "languageId": "liyasa", "version": 1, "text": text } }),
    ));
    server
}

// ---- lifecycle ----

#[test]
fn initialize_advertises_what_the_packet_promises() {
    let mut server = Server::new();
    let result = result(server.handle(&request(1, "initialize", json!({ "rootUri": null }))));
    let capabilities = &result["capabilities"];
    assert_eq!(capabilities["textDocumentSync"], json!(1), "full sync");
    assert_eq!(capabilities["hoverProvider"], json!(true));
    assert_eq!(capabilities["definitionProvider"], json!(true));
    assert!(
        capabilities["completionProvider"]["triggerCharacters"]
            .as_array()
            .is_some_and(|characters| characters.contains(&json!(":"))),
        "a colon opens a directive: {capabilities}"
    );
    assert_eq!(result["serverInfo"]["name"], json!("liyasa-lsp"));
}

#[test]
fn a_request_before_initialize_is_refused_with_the_code_the_spec_reserves() {
    let mut server = Server::new();
    let error = error(server.handle(&request(1, "textDocument/hover", json!({}))));
    assert_eq!(error.code, jsonrpc::Error::SERVER_NOT_INITIALIZED);
}

#[test]
fn a_notification_before_initialize_is_ignored_not_answered() {
    let mut server = Server::new();
    assert!(
        server
            .handle(&notify("textDocument/didOpen", json!({})))
            .is_empty()
    );
}

#[test]
fn a_method_the_server_does_not_implement_is_method_not_found() {
    let mut server = open("# Title\n");
    let error = error(server.handle(&request(9, "textDocument/rename", json!({}))));
    assert_eq!(error.code, jsonrpc::Error::METHOD_NOT_FOUND);
    assert!(error.message.contains("textDocument/rename"), "{error:?}");
}

#[test]
fn an_unknown_notification_is_ignored() {
    let mut server = open("# Title\n");
    assert!(
        server
            .handle(&notify("$/setTrace", json!({ "value": "off" })))
            .is_empty()
    );
    assert!(
        server
            .handle(&notify("$/cancelRequest", json!({ "id": 1 })))
            .is_empty()
    );
}

#[test]
fn shutdown_then_exit_is_a_clean_session() {
    let mut server = open("# Title\n");
    assert_eq!(
        result(server.handle(&request(2, "shutdown", json!(null)))),
        Value::Null
    );
    server.handle(&notify("exit", json!(null)));
    assert!(server.is_exiting());
    assert_eq!(server.exit_code(), 0);
}

#[test]
fn exit_without_shutdown_is_a_failure() {
    let mut server = open("# Title\n");
    server.handle(&notify("exit", json!(null)));
    assert!(server.is_exiting());
    assert_eq!(server.exit_code(), 1, "the client abandoned the session");
}

#[test]
fn a_client_that_counts_in_utf8_is_answered_in_utf8() {
    let mut server = Server::new();
    let result = result(server.handle(&request(
        1,
        "initialize",
        json!({
            "rootUri": null,
            "capabilities": { "general": { "positionEncodings": ["utf-8", "utf-16"] } }
        }),
    )));
    assert_eq!(result["capabilities"]["positionEncoding"], json!("utf-8"));
}

#[test]
fn a_client_that_offers_nothing_gets_the_specs_default() {
    let mut server = Server::new();
    let result = result(server.handle(&request(1, "initialize", json!({ "rootUri": null }))));
    assert_eq!(result["capabilities"]["positionEncoding"], json!("utf-16"));
}

// ---- documents and diagnostics ----

#[test]
fn opening_a_file_publishes_its_diagnostics() {
    let mut server = Server::new();
    server.handle(&request(
        1,
        "initialize",
        json!({ "rootUri": "file:///w/docs" }),
    ));
    let out = server.handle(&notify(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": URI, "languageId": "liyasa", "version": 1,
                                  "text": "# Title\n\n:::note\nBody.\n" } }),
    ));
    let params = published(out);
    assert_eq!(params["uri"], json!(URI));
    assert_eq!(params["version"], json!(1));
    let codes: Vec<&str> = params["diagnostics"]
        .as_array()
        .expect("a list")
        .iter()
        .filter_map(|d| d["code"].as_str())
        .collect();
    assert!(codes.contains(&"E0310"), "{codes:?}");
}

#[test]
fn a_change_republishes_against_the_new_text() {
    let mut server = open("# Title\n\n:::note\nBody.\n");
    let out = server.handle(&notify(
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": URI, "version": 2 },
            "contentChanges": [{ "text": "# Title\n\n:::note\nBody.\n:::\n" }]
        }),
    ));
    let params = published(out);
    assert_eq!(params["version"], json!(2));
    assert_eq!(
        params["diagnostics"].as_array().map(Vec::len),
        Some(0),
        "closing the container fixed it: {params}"
    );
}

#[test]
fn closing_a_file_clears_its_diagnostics() {
    let mut server = open("# Title\n\n:::note\nBody.\n");
    let params = published(server.handle(&notify(
        "textDocument/didClose",
        json!({ "textDocument": { "uri": URI } }),
    )));
    assert_eq!(params["diagnostics"], json!([]));
    let error = error(server.handle(&request(
        3,
        "textDocument/hover",
        json!({ "textDocument": { "uri": URI }, "position": { "line": 0, "character": 2 } }),
    )));
    assert_eq!(error.code, jsonrpc::Error::REQUEST_FAILED);
}

// ---- the features ED-61 names ----

#[test]
fn completion_answers_at_a_position() {
    let mut server = open("# Title\n\n:::no\n");
    let result = result(server.handle(&request(
        4,
        "textDocument/completion",
        json!({ "textDocument": { "uri": URI }, "position": { "line": 2, "character": 5 } }),
    )));
    let labels: Vec<&str> = result["items"]
        .as_array()
        .expect("a list of items")
        .iter()
        .filter_map(|item| item["label"].as_str())
        .collect();
    assert!(labels.contains(&"note"), "{labels:?}");
    assert_eq!(result["isIncomplete"], json!(true));
}

#[test]
fn hover_answers_at_a_position() {
    let mut server = open("# Title\n\n:::note\nBody.\n:::\n");
    let result = result(server.handle(&request(
        5,
        "textDocument/hover",
        json!({ "textDocument": { "uri": URI }, "position": { "line": 2, "character": 5 } }),
    )));
    assert_eq!(result["contents"]["kind"], json!("markdown"));
    assert!(
        result["contents"]["value"]
            .as_str()
            .is_some_and(|v| v.contains("note")),
        "{result}"
    );
}

#[test]
fn hover_over_nothing_is_null_not_an_error() {
    let mut server = open("Just a sentence.\n");
    assert_eq!(
        result(server.handle(&request(
            6,
            "textDocument/hover",
            json!({ "textDocument": { "uri": URI }, "position": { "line": 0, "character": 7 } }),
        ))),
        Value::Null
    );
}

#[test]
fn a_position_past_the_end_of_the_buffer_is_answered_not_fatal() {
    let mut server = open("# Title\n");
    assert_eq!(
        result(server.handle(&request(
            7,
            "textDocument/hover",
            json!({ "textDocument": { "uri": URI },
                    "position": { "line": 900, "character": 900 } }),
        ))),
        Value::Null
    );
}

#[test]
fn preview_returns_the_html_and_the_diagnostics_together() {
    let mut server = open("# Title\n\n:::note\nMind this.\n:::\n");
    let result = result(server.handle(&request(
        8,
        "liyasa/preview",
        json!({ "textDocument": { "uri": URI } }),
    )));
    assert_eq!(result["uri"], json!(URI));
    assert_eq!(result["version"], json!(1));
    let html = result["html"].as_str().expect("the preview is html");
    assert!(html.contains("<h1"), "{html}");
    assert!(html.contains("Mind this."), "{html}");
    assert_eq!(result["diagnostics"], json!([]));
}

#[test]
fn preview_of_a_file_that_is_not_open_is_an_error_not_an_empty_page() {
    let mut server = open("# Title\n");
    let error = error(server.handle(&request(
        9,
        "liyasa/preview",
        json!({ "textDocument": { "uri": "file:///w/docs/other.md" } }),
    )));
    assert_eq!(error.code, jsonrpc::Error::REQUEST_FAILED);
}

// ---- the loop ----

#[test]
fn a_whole_session_runs_over_a_pipe() {
    let mut wire = frame(&json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "rootUri": null }
    }));
    wire.extend(frame(
        &json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }),
    ));
    wire.extend(frame(&json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": { "uri": URI, "languageId": "liyasa", "version": 1,
                                      "text": "# Title\n\n:::note\nBody.\n" } }
    })));
    wire.extend(frame(
        &json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown" }),
    ));
    wire.extend(frame(&json!({ "jsonrpc": "2.0", "method": "exit" })));

    let mut input = Cursor::new(wire);
    let mut output: Vec<u8> = Vec::new();
    let code = liyasa_lsp::serve(&mut input, &mut output).expect("the session runs");
    assert_eq!(code, 0);

    let text = String::from_utf8(output).expect("the wire is utf-8");
    assert!(text.contains("\"serverInfo\""), "initialize was answered");
    assert!(
        text.contains("publishDiagnostics"),
        "the open file was diagnosed"
    );
    assert!(text.contains("E0310"), "with the code it earns");
    assert_eq!(
        text.matches("Content-Length: ").count(),
        3,
        "one framed message per answer: {text}"
    );
}

#[test]
fn a_malformed_frame_is_logged_and_the_session_carries_on() {
    let body = "{ not json";
    let mut wire = format!("Content-Length: {}\r\n\r\n{body}", body.len()).into_bytes();
    wire.extend(frame(&json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "rootUri": null }
    })));
    wire.extend(frame(
        &json!({ "jsonrpc": "2.0", "id": 2, "method": "shutdown" }),
    ));
    wire.extend(frame(&json!({ "jsonrpc": "2.0", "method": "exit" })));

    let mut output: Vec<u8> = Vec::new();
    let code = liyasa_lsp::serve(&mut Cursor::new(wire), &mut output).expect("the session runs");
    assert_eq!(code, 0, "one bad frame does not end the session");
    let text = String::from_utf8(output).expect("the wire is utf-8");
    assert!(text.contains("window/logMessage"), "{text}");
    assert!(
        text.contains("\"serverInfo\""),
        "and the next message was answered"
    );
}

#[test]
fn a_closed_pipe_without_exit_is_a_failure() {
    let wire = frame(&json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "rootUri": null }
    }));
    let mut output: Vec<u8> = Vec::new();
    assert_eq!(
        liyasa_lsp::serve(&mut Cursor::new(wire), &mut output).expect("the session ends"),
        1
    );
}

#[test]
fn the_id_of_every_answer_is_the_id_of_its_request() {
    let mut server = open("# Title\n");
    let out = server.handle(&request(
        77,
        "textDocument/hover",
        json!({
            "textDocument": { "uri": URI }, "position": { "line": 0, "character": 3 }
        }),
    ));
    match out.into_iter().next() {
        Some(Outgoing::Response(response)) => assert_eq!(response.id, Id::Number(77)),
        other => panic!("expected a response, got {other:?}"),
    }
}
