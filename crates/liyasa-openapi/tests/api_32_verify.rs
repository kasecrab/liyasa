//! API-32: the generated samples are executed against a mock server derived
//! from the spec, so a template regression is caught before release.
//!
//! A runtime that is not installed is reported and skipped rather than failing
//! the suite: `go` and `ruby` are not on every machine, and a developer
//! without them should still be able to run `cargo test`. CI has them, and the
//! curl generator — which needs nothing but `curl` — always runs.

use std::io::Write;
use std::process::{Command, Stdio};

use liyasa_openapi::codegen::Registry;
use liyasa_openapi::mock::{Mock, Received};
use liyasa_openapi::sample::{Options, Request};

#[path = "support.rs"]
mod support;

const SPEC: &str = r##"
openapi: 3.1.0
info: { title: Widgets, version: "1" }
security:
  - bearer: []
components:
  securitySchemes:
    bearer: { type: http, scheme: bearer }
paths:
  /widgets/{id}:
    get:
      operationId: getWidget
      parameters:
        - { name: id, in: path, required: true, schema: { type: string }, example: "w-42" }
        - { name: fields, in: query, required: true, schema: { type: string }, example: "name,size" }
        - { name: X-Trace, in: header, required: true, schema: { type: string }, example: "abc 123" }
      responses:
        "200":
          description: The widget
          content:
            application/json:
              schema: { type: object, properties: { id: { type: string } } }
              example: { id: "w-42" }
  /widgets:
    post:
      operationId: createWidget
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: object
              properties:
                name: { type: string }
                size: { type: integer }
            example: { name: "a widget", size: 7 }
      responses:
        "201":
          description: Made
          content:
            application/json:
              example: { id: "w-43" }
"##;

fn have(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// The request one operation's sample describes, aimed at `base_url`.
fn request(spec: &liyasa_openapi::Spec, operation_id: &str, base_url: &str) -> Request {
    let operation = spec
        .by_operation_id(operation_id)
        .unwrap_or_else(|| panic!("the spec has `{operation_id}`"));
    Request::build(
        spec,
        &operation,
        &Options {
            base_url: Some(base_url.to_owned()),
            ..Options::default()
        },
    )
}

fn sample(language: &str, request: &Request) -> String {
    Registry::new()
        .render(language, request)
        .unwrap_or_else(|error| panic!("the {language} template renders: {error}"))
        .source
}

fn run(program: &str, args: &[&str], stdin: Option<&str>) -> String {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("{program} starts: {error}"));
    if let Some(text) = stdin {
        child
            .stdin
            .as_mut()
            .expect("stdin is piped")
            .write_all(text.as_bytes())
            .expect("the script is written");
    }
    drop(child.stdin.take());
    let output = child.wait_with_output().expect("it finishes");
    assert!(
        output.status.success(),
        "{program} failed: {}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// What every generator must have sent for `getWidget`, whatever its syntax.
fn assert_get(got: &Received, language: &str) {
    assert!(
        got.matched,
        "{language} sent {} {}, which no operation in the spec describes",
        got.method, got.path
    );
    assert_eq!(got.method, "GET", "{language}");
    assert_eq!(
        got.path, "/widgets/w-42",
        "{language} did not substitute the path parameter"
    );
    assert_eq!(
        got.query_value("fields"),
        Some("name,size"),
        "{language} sent the query as {:?}",
        got.query
    );
    assert_eq!(
        got.header("x-trace"),
        Some("abc 123"),
        "{language} dropped or mangled a header"
    );
    assert_eq!(
        got.header("authorization"),
        Some("Bearer $ACCESS_TOKEN"),
        "{language} dropped the credential placeholder"
    );
}

fn assert_post(got: &Received, language: &str) {
    assert!(got.matched, "{language} sent {} {}", got.method, got.path);
    assert_eq!(got.method, "POST", "{language}");
    assert_eq!(got.path, "/widgets", "{language}");
    assert!(
        got.header("content-type")
            .is_some_and(|value| value.starts_with("application/json")),
        "{language} sent content-type {:?}",
        got.header("content-type")
    );
    let body: serde_json::Value = serde_json::from_str(&got.body_text())
        .unwrap_or_else(|error| panic!("{language} sent a body that is not JSON: {error}"));
    assert_eq!(
        body,
        serde_json::json!({ "name": "a widget", "size": 7 }),
        "{language} sent {}",
        got.body_text()
    );
}

#[test]
fn the_curl_sample_sends_what_the_operation_describes() {
    let spec = support::spec(SPEC);
    let server = Mock::new(&spec).serve().expect("the mock server binds");

    let source = sample("curl", &request(&spec, "getWidget", &server.base_url()));
    run(
        "sh",
        &["-c", &format!("{source} --silent --show-error")],
        None,
    );
    assert_get(&server.last().expect("the server saw a request"), "curl");

    let source = sample("curl", &request(&spec, "createWidget", &server.base_url()));
    run(
        "sh",
        &["-c", &format!("{source} --silent --show-error")],
        None,
    );
    assert_post(&server.last().expect("the server saw a request"), "curl");
}

#[test]
fn the_javascript_sample_sends_what_the_operation_describes() {
    if !have("node") {
        eprintln!("skipping the javascript generator: node is not installed");
        return;
    }
    let spec = support::spec(SPEC);
    let server = Mock::new(&spec).serve().expect("the mock server binds");

    for (operation_id, check) in [
        ("getWidget", assert_get as fn(&Received, &str)),
        ("createWidget", assert_post),
    ] {
        let source = sample(
            "javascript",
            &request(&spec, operation_id, &server.base_url()),
        );
        run("node", &["--input-type=module", "-e", &source], None);
        check(
            &server.last().expect("the server saw a request"),
            "javascript",
        );
    }
}

#[test]
fn the_python_sample_sends_what_the_operation_describes() {
    if !have("python3")
        || !Command::new("python3")
            .args(["-c", "import requests"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    {
        eprintln!("skipping the python generator: python3 with requests is not installed");
        return;
    }
    let spec = support::spec(SPEC);
    let server = Mock::new(&spec).serve().expect("the mock server binds");

    for (operation_id, check) in [
        ("getWidget", assert_get as fn(&Received, &str)),
        ("createWidget", assert_post),
    ] {
        let source = sample("python", &request(&spec, operation_id, &server.base_url()));
        run("python3", &["-"], Some(&source));
        check(&server.last().expect("the server saw a request"), "python");
    }
}

#[test]
fn the_go_sample_sends_what_the_operation_describes() {
    if !have("go") {
        eprintln!("skipping the go generator: go is not installed");
        return;
    }
    let spec = support::spec(SPEC);
    let server = Mock::new(&spec).serve().expect("the mock server binds");
    let dir = std::env::temp_dir().join(format!("liyasa-api-32-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("the scratch directory is writable");

    for (operation_id, check) in [
        ("getWidget", assert_get as fn(&Received, &str)),
        ("createWidget", assert_post),
    ] {
        let source = sample("go", &request(&spec, operation_id, &server.base_url()));
        let file = dir.join(format!("{operation_id}.go"));
        std::fs::write(&file, source).expect("the program is written");
        run("go", &["run", &file.to_string_lossy()], None);
        check(&server.last().expect("the server saw a request"), "go");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_request_the_spec_does_not_describe_is_a_miss_rather_than_a_pass() {
    let spec = support::spec(SPEC);
    let server = Mock::new(&spec).serve().expect("the mock server binds");
    let base = server.base_url();

    run(
        "curl",
        &[
            "--silent",
            "--show-error",
            &format!("{base}/widgets/w-42/history"),
        ],
        None,
    );
    let got = server.last().expect("the server saw it");
    assert!(
        !got.matched,
        "a path with no operation behind it must not look like a pass"
    );
}

#[test]
fn the_mock_answers_with_the_response_the_spec_wrote() {
    let spec = support::spec(SPEC);
    let server = Mock::new(&spec).serve().expect("the mock server binds");
    let base = server.base_url();

    let body = run(
        "curl",
        &[
            "--silent",
            "--show-error",
            &format!("{base}/widgets/w-42?fields=name"),
        ],
        None,
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body).expect("it is JSON"),
        serde_json::json!({ "id": "w-42" }),
        "the mock serves the spec's own example"
    );
}
