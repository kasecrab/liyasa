//! MCP-04: the same MCP server over stdio, from a static build.
//!
//! The point of the requirement is that an OSS user with no server still gets
//! MCP, locally and in CI — so the assertions that matter are the ones about
//! *sameness*: the tool list is the same list, the answers come from the same
//! dispatcher, and the only differences are the two MCP-04 names.
//! `ask` degrades to keyword search with extractive snippets and says so, and
//! `report_issue` returns a clean tool error naming the server requirement.
//! Both stay in `tools/list` with an `unavailable` reason rather than being
//! omitted, because omitting them says the SITE cannot do these things when
//! it is this transport that cannot.
//!
//! No process is spawned. The transport takes its streams as arguments, so a
//! `Cursor` in and a `Vec` out drives exactly the code `liyasa mcp --dist`
//! drives — a test that shelled out to the binary would be measuring the
//! harness and would not run until WP-09 writes the subcommand.

use std::path::PathBuf;

use liyasa_build::engine::{self, Options};
use liyasa_build::git::NoGit;
use liyasa_config::vfs::OsVfs;
use liyasa_server::mcp::stdio;
use serde_json::{Value, json};

const SITE: &str = r#"{
  "name": "Acme docs",
  "seo": { "canonicalOrigin": "https://docs.acme.com" },
  "navigation": [
    "index",
    { "group": "Guides", "pages": ["guides/install"] },
    { "group": "Internal", "groups": ["staff"], "pages": ["internal/runbook"] }
  ]
}"#;

const PAGES: &[(&str, &str)] = &[
    (
        "index.md",
        "---\ntitle: Home\n---\n# Home\n\nWelcome to Acme.\n",
    ),
    (
        "guides/install.md",
        "---\ntitle: Install\n---\n# Install\n\nRun it with cargo.\n",
    ),
    (
        "internal/runbook.md",
        "---\ntitle: Failover runbook\n---\n# Failover runbook\n\n\
         The secret escalation number is in here.\n",
    ),
];

fn build_site(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("liyasa-mcp04-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a project directory");
    write(&root, "liyasa.json", SITE);
    for (path, body) in PAGES {
        write(&root, path, body);
    }
    let report = engine::build(
        &OsVfs::new(&root),
        &NoGit,
        &root,
        &Options {
            build_time: Some(1_789_473_600),
            ..Options::default()
        },
    );
    assert!(
        !report.failed(false),
        "the fixture failed to build: {:?}",
        report.diagnostics
    );
    root.join("dist")
}

fn write(root: &std::path::Path, path: &str, body: &str) {
    let full = root.join(path);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).expect("a directory");
    }
    std::fs::write(full, body).expect("a fixture file");
}

/// Feeds `messages` in as newline-delimited JSON and returns what came back,
/// one parsed value per output line.
async fn exchange(name: &str, messages: &[Value]) -> Vec<Value> {
    let dist = build_site(name);
    let config: Value = serde_json::from_str(SITE).expect("the fixture config is JSON");
    let reader = stdio::open(&dist, &config).expect("the build opens as a corpus");

    let mut input = String::new();
    for message in messages {
        input.push_str(&message.to_string());
        input.push('\n');
    }
    let mut output: Vec<u8> = Vec::new();
    stdio::run(
        reader,
        std::io::Cursor::new(input.into_bytes()),
        &mut output,
    )
    .await
    .expect("the transport ran to the end of its input");

    String::from_utf8(output)
        .expect("the transport writes UTF-8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("each line is one JSON message"))
        .collect()
}

fn call(id: i64, name: &str, arguments: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": { "name": name, "arguments": arguments }
    })
}

#[tokio::test]
async fn the_read_tools_answer_from_the_build_with_no_server_at_all() {
    let answers = exchange(
        "reads",
        &[
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
            call(2, "list_pages", json!({})),
            call(3, "search", json!({ "query": "cargo" })),
            call(4, "fetch", json!({ "route": "/guides/install" })),
        ],
    )
    .await;
    assert_eq!(answers.len(), 4, "{answers:#?}");

    assert_eq!(
        answers[0]["result"]["serverInfo"]["title"],
        json!("Acme docs")
    );

    let listed = answers[1]["result"]["content"][0]["text"]
        .as_str()
        .expect("text");
    assert!(listed.contains("/guides/install"), "{listed}");

    let searched = answers[2]["result"]["content"][0]["text"]
        .as_str()
        .expect("text");
    assert!(searched.contains("/guides/install"), "{searched}");

    let fetched = answers[3]["result"]["content"][0]["text"]
        .as_str()
        .expect("text");
    assert!(fetched.contains("Run it with cargo."), "{fetched}");
}

#[tokio::test]
async fn ask_and_report_issue_are_advertised_with_their_reason_rather_than_omitted() {
    let answers = exchange(
        "advertised",
        &[json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })],
    )
    .await;
    let tools = answers[0]["result"]["tools"].as_array().expect("tools");
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    assert_eq!(
        names,
        [
            "search",
            "fetch",
            "list_pages",
            "get_openapi_operation",
            "ask",
            "report_issue"
        ],
        "the tool list must be the same list the server offers"
    );

    for name in ["ask", "report_issue"] {
        let tool = tools
            .iter()
            .find(|tool| tool["name"] == name)
            .unwrap_or_else(|| panic!("`{name}`"));
        assert!(
            tool["_meta"]["liyasa/unavailable"].is_string(),
            "`{name}` carries no reason: {tool}"
        );
        assert!(
            tool["description"]
                .as_str()
                .is_some_and(|text| text.contains("Not fully available here")),
            "a model reads the description and not `_meta`: {tool}"
        );
    }
}

#[tokio::test]
async fn ask_degrades_to_search_with_snippets_and_says_so_in_its_own_result() {
    // In the result, not only in the tool list: the caller that reads a
    // result is often not the one that read the list.
    let answers = exchange(
        "degraded",
        &[call(1, "ask", json!({ "question": "how do I install it" }))],
    )
    .await;
    let result = &answers[0]["result"];
    assert_eq!(result["isError"], json!(false));
    assert_eq!(result["structuredContent"]["degraded"], json!(true));

    let text = result["content"][0]["text"].as_str().expect("text");
    assert!(text.contains("no model is configured"), "{text}");
    // And it still answers with something useful rather than only apologising.
    assert!(text.contains("/guides/install"), "{text}");
}

#[tokio::test]
async fn report_issue_refuses_cleanly_and_names_what_would_serve_it() {
    let answers = exchange(
        "report",
        &[call(1, "report_issue", json!({ "summary": "a typo" }))],
    )
    .await;
    let result = &answers[0]["result"];
    // A tool error, not a transport error: the model reads this and moves on.
    assert!(answers[0]["error"].is_null(), "{:#?}", answers[0]);
    assert_eq!(result["isError"], json!(true));
    let text = result["content"][0]["text"].as_str().expect("text");
    assert!(text.contains("liyasa serve"), "{text}");
}

#[tokio::test]
async fn a_local_server_sees_what_an_anonymous_reader_sees_and_no_more() {
    // RFC 1902. Whoever runs this holds `dist/` and could `cat` the file, but
    // the caller is a model — often somebody else's, often relaying a third
    // party's text — and AUTH-10's rule is that every surface filters alike.
    let answers = exchange(
        "entitlement",
        &[
            call(1, "list_pages", json!({})),
            call(2, "search", json!({ "query": "escalation" })),
            call(3, "fetch", json!({ "route": "/internal/runbook" })),
        ],
    )
    .await;

    let listed = answers[0]["result"]["content"][0]["text"]
        .as_str()
        .expect("text");
    assert!(!listed.contains("/internal/runbook"), "{listed}");

    let searched = answers[1]["result"]["content"][0]["text"]
        .as_str()
        .expect("text");
    assert!(!searched.contains("secret escalation"), "{searched}");

    assert_eq!(answers[2]["result"]["isError"], json!(true));
}

#[tokio::test]
async fn a_notification_produces_no_line_and_a_blank_line_is_not_an_error() {
    // A response to a notification frames as a response to a DIFFERENT
    // request, and shells and wrappers insert blank lines.
    let dist = build_site("framing");
    let config: Value = serde_json::from_str(SITE).expect("JSON");
    let reader = stdio::open(&dist, &config).expect("a corpus");

    let input = format!(
        "{}\n\n   \n{}\n",
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        json!({ "jsonrpc": "2.0", "id": 7, "method": "ping" })
    );
    let mut output: Vec<u8> = Vec::new();
    stdio::run(
        reader,
        std::io::Cursor::new(input.into_bytes()),
        &mut output,
    )
    .await
    .expect("the transport ran");

    let text = String::from_utf8(output).expect("UTF-8");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 1, "{text}");
    let answer: Value = serde_json::from_str(lines[0]).expect("JSON");
    assert_eq!(answer["id"], json!(7));
}

#[tokio::test]
async fn one_message_is_one_line_however_long_the_answer_is() {
    // The framing is newline-delimited, so an embedded newline in a response
    // would frame as two messages and the second would not parse.
    let answers = exchange(
        "framing-long",
        &[call(1, "fetch", json!({ "route": "/guides/install" }))],
    )
    .await;
    assert_eq!(answers.len(), 1);
    let markdown = answers[0]["result"]["structuredContent"]["markdown"]
        .as_str()
        .expect("the page markdown");
    assert!(
        markdown.contains('\n'),
        "the page has to contain a newline for this test to prove anything"
    );
}

#[tokio::test]
async fn a_malformed_message_is_refused_and_the_stream_keeps_going() {
    // A parse error that ended the session would make one bad line from a
    // wrapper look like the server crashing.
    let dist = build_site("malformed");
    let config: Value = serde_json::from_str(SITE).expect("JSON");
    let reader = stdio::open(&dist, &config).expect("a corpus");

    let input = format!(
        "{{not json\n{}\n",
        json!({ "jsonrpc": "2.0", "id": 2, "method": "ping" })
    );
    let mut output: Vec<u8> = Vec::new();
    stdio::run(
        reader,
        std::io::Cursor::new(input.into_bytes()),
        &mut output,
    )
    .await
    .expect("the transport ran");

    let text = String::from_utf8(output).expect("UTF-8");
    let lines: Vec<Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).expect("JSON"))
        .collect();
    assert_eq!(lines.len(), 2, "{text}");
    assert!(!lines[0]["error"].is_null(), "{}", lines[0]);
    assert_eq!(lines[1]["id"], json!(2));
}
