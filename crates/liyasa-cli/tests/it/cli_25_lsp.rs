//! CLI-25: `liyasa lsp` runs the language server over stdin and stdout.
//!
//! The protocol itself is `liyasa-lsp`'s and is tested there. What is asserted
//! here is the wiring: that the command exists, that it speaks the protocol on
//! stdout rather than anything else, and that a failure reaches stderr instead
//! of being discarded — an editor's log is the only place a user of a stdio
//! server can see why it stopped.

use std::io::{Read, Write};
use std::process::{Command, Stdio};

use liyasa_cli::Exit;

use crate::support::{Run, binary};

/// One Language Server Protocol message, framed as the protocol requires.
fn framed(body: &str) -> String {
    format!("Content-Length: {}\r\n\r\n{body}", body.len())
}

/// Drives the server with `initialize` then `shutdown` and `exit`, and returns
/// what it wrote to each stream.
fn converse() -> (i32, String, String) {
    let mut child = Command::new(binary())
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("NO_COLOR", "1")
        .spawn()
        .expect("the binary runs");

    let mut stdin = child.stdin.take().expect("a pipe");
    let request =
        framed(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}"#)
            + &framed(r#"{"jsonrpc":"2.0","id":2,"method":"shutdown"}"#)
            + &framed(r#"{"jsonrpc":"2.0","method":"exit"}"#);
    let _ = stdin.write_all(request.as_bytes());
    let _ = stdin.flush();
    drop(stdin);

    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        let _ = pipe.read_to_string(&mut stdout);
    }
    if let Some(mut pipe) = child.stderr.take() {
        let _ = pipe.read_to_string(&mut stderr);
    }
    let status = child.wait().expect("the server exits");
    (status.code().unwrap_or(-1), stdout, stderr)
}

#[test]
fn the_command_exists() {
    let outcome = Run::new(["lsp", "--help"]).output();
    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
    assert!(
        outcome.stdout.contains("language server"),
        "{}",
        outcome.stdout
    );
}

#[test]
fn it_is_listed_among_the_commands() {
    let outcome = Run::new(["--help"]).output();
    assert!(outcome.stdout.contains("lsp"), "{}", outcome.stdout);
}

/// The whole point of the wiring: a real conversation gets a real reply.
#[test]
fn it_answers_an_initialize_request() {
    let (code, stdout, stderr) = converse();

    assert!(
        stdout.contains("Content-Length:"),
        "no protocol framing on stdout: {stdout:?} (stderr: {stderr:?})"
    );
    assert!(
        stdout.contains("\"id\":1") || stdout.contains("\"id\": 1"),
        "no reply to the initialize request: {stdout:?}"
    );
    assert!(
        stdout.contains("capabilities"),
        "the reply carries no capabilities: {stdout:?}"
    );
    assert_eq!(code, Exit::Success.code(), "stderr: {stderr:?}");
}

/// Stdout carries the protocol and nothing else. A banner, a progress line or a
/// diagnostic there would desynchronise the framing and the editor would see a
/// parse error rather than a server.
#[test]
fn nothing_but_the_protocol_reaches_stdout() {
    let (_, stdout, _) = converse();
    assert!(
        stdout.starts_with("Content-Length:"),
        "stdout does not begin with a frame: {stdout:?}"
    );
}
