//! MCP-01, MCP-02, MCP-03 and MCP-05: the server an agent connects to.
//!
//! **Defect 162 is what this file is for.** `liyasa-build` writes
//! `MCP server: <origin>/mcp` into every generated `llms.txt` and advertises
//! the same address under `## MCP server` in `skill.md`. The string is
//! generated, so no author can remove it, and until today nothing answered
//! there: every site a user built published an absolute URL — to agents
//! rather than to people — for a service that did not exist. So the assertion
//! that matters most here is the dullest one, `the_address_llms_txt_publishes_
//! is_the_address_that_answers`: it reads the URL out of the built file and
//! sends a request to it.
//!
//! `/_liyasa/mcp` answers identically because `routes::pool_for` charges the
//! `Mcp` rate-limit bucket on that spelling (RFC 1900). Two addresses, one
//! handler, and a test that they cannot diverge.
//!
//! The router is driven directly rather than through `routes::application`,
//! because the `subtrees()` entry that composes it is WP-14's line to write
//! and is sequenced after this branch. `the_application_composes_the_subtree`
//! at the end is the one assertion that needs it, and it skips with the
//! reason rather than pinning the gap.

use std::path::PathBuf;

use axum::Router;
use axum::body::Body;
use http::{Request, StatusCode, header};
use liyasa_build::engine::{self, Options};
use liyasa_build::git::NoGit;
use liyasa_config::vfs::OsVfs;
use liyasa_server::auth::session::Principal;
use liyasa_server::mcp;
use liyasa_tests::server::{Harness, Setup};
use serde_json::{Value, json};
use tower::ServiceExt as _;

const SITE: &str = r#"{
  "name": "Acme docs",
  "description": "How Acme works",
  "seo": { "canonicalOrigin": "https://docs.acme.com" },
  "navigation": [
    "index",
    { "group": "Guides", "pages": ["guides/install"] },
    { "group": "Internal", "groups": ["staff"], "pages": ["internal/runbook"] }
  ],
  "openapi": [{ "id": "petstore", "source": "petstore.json" }]
}"#;

const PAGES: &[(&str, &str)] = &[
    (
        "index.md",
        "---\ntitle: Home\n---\n# Home\n\nWelcome to Acme.\n",
    ),
    (
        "guides/install.md",
        "---\ntitle: Install\n---\n# Install\n\nRun it.\n\n## From source\n\n\
         Clone the repository and build it with cargo.\n\n## From a package\n\n\
         Use your package manager.\n",
    ),
    // Behind `groups: [staff]` through its navigation group. MCP-03's
    // entitlement filter is asserted on this page and nothing else.
    (
        "internal/runbook.md",
        "---\ntitle: Failover runbook\n---\n# Failover runbook\n\n\
         The secret escalation number is in here.\n",
    ),
];

const SPEC: &str = r#"{
  "openapi": "3.1.0",
  "info": { "title": "Petstore", "version": "1.0.0" },
  "servers": [{ "url": "https://api.petstore.example" }],
  "paths": {
    "/pets/{id}": {
      "parameters": [{ "name": "id", "in": "path", "required": true, "description": "Which pet." }],
      "get": {
        "operationId": "getPet",
        "summary": "Read one pet",
        "responses": { "200": { "description": "ok" } }
      }
    }
  }
}"#;

fn build_site(name: &str, extra: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("liyasa-mcp01-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a project directory");
    write(&root, "liyasa.json", SITE);
    write(&root, "petstore.json", SPEC);
    for (path, body) in PAGES.iter().chain(extra) {
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

/// A harness over the fixture, and the MCP subtree's own router.
async fn serve(name: &str, extra: &[(&str, &str)]) -> (Harness, Router) {
    let dist = build_site(name, extra);
    let (harness, _) = Harness::new(Setup {
        dist: Some(dist),
        site_config: Some(serde_json::from_str(SITE).expect("the fixture config is JSON")),
        ..Setup::new(name)
    })
    .await;
    let mount = mcp::mount(&harness.state);
    let router = mount.router.unwrap_or_else(|| {
        panic!(
            "the subtree mounted nothing: {}",
            mount.skipped.unwrap_or_default()
        )
    });
    (harness, router)
}

/// One JSON-RPC call, as the given reader.
async fn rpc_as(
    router: &Router,
    path: &str,
    method: &str,
    params: Value,
    reader: Option<Principal>,
) -> Value {
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    let mut request = Request::builder()
        .method("POST")
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .expect("a request");
    if let Some(reader) = reader {
        request.extensions_mut().insert(reader);
    }
    let response = router.clone().oneshot(request).await.expect("a response");
    assert_eq!(response.status(), StatusCode::OK, "{method} at {path}");
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .expect("a complete body");
    let answer: Value = serde_json::from_slice(&bytes).expect("a JSON-RPC response");
    assert_eq!(answer["jsonrpc"], json!("2.0"));
    assert_eq!(answer["id"], json!(1), "a response must carry its id");
    answer
}

async fn rpc(router: &Router, method: &str, params: Value) -> Value {
    rpc_as(router, "/mcp", method, params, None).await
}

/// The result of a `tools/call`, asserted not to be an error.
async fn tool(router: &Router, name: &str, arguments: Value) -> Value {
    let answer = rpc(
        router,
        "tools/call",
        json!({ "name": name, "arguments": arguments }),
    )
    .await;
    let result = &answer["result"];
    assert_eq!(
        result["isError"],
        json!(false),
        "`{name}` failed: {}",
        result["content"][0]["text"]
    );
    result.clone()
}

fn text_of(result: &Value) -> String {
    result["content"][0]["text"]
        .as_str()
        .expect("text content")
        .to_owned()
}

// ---- MCP-01: the tools ----

#[tokio::test]
async fn initialize_declares_the_protocol_the_tools_and_what_the_site_is() {
    let (_harness, router) = serve("initialize", &[]).await;
    let answer = rpc(
        &router,
        "initialize",
        json!({
            "protocolVersion": mcp::protocol::PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": { "name": "a test", "version": "0" }
        }),
    )
    .await;
    let result = &answer["result"];
    assert_eq!(
        result["protocolVersion"],
        json!(mcp::protocol::PROTOCOL_VERSION)
    );
    assert_eq!(result["serverInfo"]["title"], json!("Acme docs"));
    let instructions = result["instructions"].as_str().expect("instructions");
    assert!(instructions.contains("Acme docs"), "{instructions}");
    assert!(instructions.contains("How Acme works"), "{instructions}");
}

#[tokio::test]
async fn every_tool_mcp_01_names_is_offered() {
    let (_harness, router) = serve("tools", &[]).await;
    let answer = rpc(&router, "tools/list", json!({})).await;
    let names: Vec<&str> = answer["result"]["tools"]
        .as_array()
        .expect("a tools array")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert_eq!(
        names,
        [
            "search",
            "fetch",
            "list_pages",
            "get_openapi_operation",
            "ask",
            "report_issue"
        ]
    );
}

#[tokio::test]
async fn search_finds_a_page_and_names_the_section_the_match_is_in() {
    let (_harness, router) = serve("search", &[]).await;
    let result = tool(&router, "search", json!({ "query": "clone repository" })).await;
    let hits = result["structuredContent"]["results"]
        .as_array()
        .expect("results");
    let first = hits.first().expect("at least one hit");
    assert_eq!(first["route"], json!("/guides/install"));
    // An agent handed a route with no anchor has to re-read the whole page to
    // find what matched.
    assert_eq!(first["anchor"], json!("from-source"));
    assert!(
        text_of(&result).contains("/guides/install#from-source"),
        "{}",
        text_of(&result)
    );
}

#[tokio::test]
async fn fetch_returns_a_page_and_a_named_section_of_one() {
    let (_harness, router) = serve("fetch", &[]).await;

    let whole = tool(&router, "fetch", json!({ "route": "/guides/install" })).await;
    let markdown = text_of(&whole);
    assert!(markdown.contains("From source"), "{markdown}");
    assert!(markdown.contains("package manager"), "{markdown}");

    let section = tool(
        &router,
        "fetch",
        json!({ "route": "/guides/install", "section": "from-source" }),
    )
    .await;
    let markdown = text_of(&section);
    assert!(markdown.contains("Clone the repository"), "{markdown}");
    assert!(
        !markdown.contains("package manager"),
        "a section must not carry its sibling: {markdown}"
    );
    assert_eq!(section["structuredContent"]["anchor"], json!("from-source"));
}

#[tokio::test]
async fn fetch_takes_a_route_a_markdown_twin_or_the_page_s_own_url() {
    // MCP-01 says "by route or URL", and an agent that found a link in a
    // search result or a resource listing has the URL rather than the route.
    let (_harness, router) = serve("locators", &[]).await;
    for locator in [
        "/guides/install",
        "/guides/install.md",
        "https://docs.acme.com/guides/install",
        "https://docs.acme.com/guides/install.md",
    ] {
        let result = tool(&router, "fetch", json!({ "route": locator })).await;
        assert_eq!(
            result["structuredContent"]["route"],
            json!("/guides/install"),
            "{locator}"
        );
    }
}

#[tokio::test]
async fn list_pages_gives_the_routes_with_their_depth() {
    let (_harness, router) = serve("list", &[]).await;
    let result = tool(&router, "list_pages", json!({})).await;
    let pages = result["structuredContent"]["pages"]
        .as_array()
        .expect("pages");
    let routes: Vec<&str> = pages
        .iter()
        .filter_map(|page| page["route"].as_str())
        .collect();
    assert_eq!(routes, ["/", "/guides/install"]);
    assert_eq!(pages[0]["depth"], json!(0));
    assert_eq!(pages[1]["depth"], json!(2));
    assert_eq!(pages[1]["title"], json!("Install"));
}

#[tokio::test]
async fn get_openapi_operation_reads_the_published_specification() {
    let (_harness, router) = serve("openapi", &[]).await;
    for name in ["GET /pets/{id}", "getPet"] {
        let result = tool(
            &router,
            "get_openapi_operation",
            json!({ "operation": name }),
        )
        .await;
        let markdown = text_of(&result);
        assert!(markdown.contains("Read one pet"), "{name}: {markdown}");
        // The path-level parameter, which an agent without it cannot build a
        // request from.
        assert!(
            markdown.contains("`id` (path, required)"),
            "{name}: {markdown}"
        );
    }
}

#[tokio::test]
async fn a_tool_that_fails_answers_with_is_error_rather_than_a_transport_failure() {
    // A model can read a tool result and retry. It cannot read a JSON-RPC
    // error, so a refusal delivered that way ends the conversation.
    let (_harness, router) = serve("toolerror", &[]).await;
    let answer = rpc(
        &router,
        "tools/call",
        json!({ "name": "fetch", "arguments": { "route": "/nowhere" } }),
    )
    .await;
    assert!(answer["error"].is_null(), "{answer}");
    assert_eq!(answer["result"]["isError"], json!(true));
    let text = answer["result"]["content"][0]["text"]
        .as_str()
        .expect("text");
    assert!(
        text.contains("list_pages"),
        "a refusal says what to do next: {text}"
    );
}

// ---- MCP-01: resources and prompts ----

#[tokio::test]
async fn resources_are_llms_txt_and_every_page_addressed_by_their_own_urls() {
    let (_harness, router) = serve("resources", &[]).await;
    let answer = rpc(&router, "resources/list", json!({})).await;
    let uris: Vec<&str> = answer["result"]["resources"]
        .as_array()
        .expect("resources")
        .iter()
        .filter_map(|entry| entry["uri"].as_str())
        .collect();
    assert_eq!(
        uris,
        [
            "https://docs.acme.com/llms.txt",
            "https://docs.acme.com/index.md",
            "https://docs.acme.com/guides/install.md"
        ]
    );

    let read = rpc(
        &router,
        "resources/read",
        json!({ "uri": "https://docs.acme.com/guides/install.md" }),
    )
    .await;
    let text = read["result"]["contents"][0]["text"]
        .as_str()
        .expect("the page text");
    assert!(text.contains("From source"), "{text}");
}

#[tokio::test]
async fn prompts_cover_the_common_task_and_refuse_without_their_argument() {
    let (_harness, router) = serve("prompts", &[]).await;
    let listed = rpc(&router, "prompts/list", json!({})).await;
    let names: Vec<&str> = listed["result"]["prompts"]
        .as_array()
        .expect("prompts")
        .iter()
        .filter_map(|prompt| prompt["name"].as_str())
        .collect();
    assert!(names.contains(&"integrate"), "{names:?}");

    let got = rpc(
        &router,
        "prompts/get",
        json!({ "name": "integrate", "arguments": { "product": "Kubernetes" } }),
    )
    .await;
    let text = got["result"]["messages"][0]["content"]["text"]
        .as_str()
        .expect("the prompt text");
    assert!(text.contains("Kubernetes"), "{text}");
    assert!(text.contains("Acme docs"), "{text}");

    let missing = rpc(&router, "prompts/get", json!({ "name": "integrate" })).await;
    assert!(
        !missing["error"].is_null(),
        "a prompt whose required argument is absent must refuse: {missing}"
    );
}

// ---- MCP-02: discovery ----

#[tokio::test]
async fn both_well_known_paths_serve_the_same_card() {
    // The ecosystem has used both spellings. An agent that guessed the other
    // one would conclude this site has no server.
    let (_harness, router) = serve("discovery", &[]).await;
    let mut documents = Vec::new();
    for path in ["/.well-known/mcp", "/.well-known/mcp.json"] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(path)
                    .body(Body::empty())
                    .expect("a request"),
            )
            .await
            .expect("a response");
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("a complete body");
        documents.push(serde_json::from_slice::<Value>(&bytes).expect("JSON"));
    }
    assert_eq!(documents[0], documents[1], "the two paths must not diverge");

    let card = &documents[0];
    assert_eq!(
        card["discoveryVersion"],
        json!(mcp::discovery::DISCOVERY_VERSION)
    );
    assert_eq!(
        card["servers"][0]["url"],
        json!("https://docs.acme.com/mcp"),
        "the card must name the address `llms.txt` publishes"
    );
    assert_eq!(card["servers"][0]["transport"], json!("streamable-http"));
}

#[tokio::test]
async fn the_discovery_version_a_site_pins_is_the_one_it_serves() {
    // MCP's discovery format is not settled. A card that did not label its
    // own shape would leave the next format change with nothing to be
    // conditional on (RFC 1901).
    let dist = build_site("pinned", &[]);
    let (harness, _) = Harness::new(Setup {
        dist: Some(dist),
        site_config: Some(json!({
            "name": "Acme docs",
            "seo": { "canonicalOrigin": "https://docs.acme.com" },
            "agents": { "mcp": { "discoveryVersion": "2099-01-01", "name": "Acme MCP" } }
        })),
        ..Setup::new("pinned")
    })
    .await;
    let router = mcp::mount(&harness.state).router.expect("a router");
    let response = router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/.well-known/mcp")
                .body(Body::empty())
                .expect("a request"),
        )
        .await
        .expect("a response");
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("a complete body");
    let card: Value = serde_json::from_slice(&bytes).expect("JSON");
    assert_eq!(card["discoveryVersion"], json!("2099-01-01"));
    assert_eq!(card["name"], json!("Acme MCP"));
}

// ---- defect 162: the advertised address is the one that answers ----

#[tokio::test]
async fn the_address_llms_txt_publishes_is_the_address_that_answers() {
    // The whole of defect 162 in one assertion. The URL is read out of the
    // built file rather than written here, so a change to `MCP_PATH` that
    // moved the advertisement without moving the endpoint fails here.
    let (harness, router) = serve("advertised", &[]).await;
    let llms = std::fs::read_to_string(harness.dist.join("llms.txt")).expect("llms.txt");
    let line = llms
        .lines()
        .find(|line| line.starts_with("MCP server: "))
        .expect("`llms.txt` advertises an MCP server; if it stopped, defect 162 changed shape");
    let advertised = line.trim_start_matches("MCP server: ").trim();
    assert_eq!(advertised, "https://docs.acme.com/mcp");

    let path = advertised
        .strip_prefix("https://docs.acme.com")
        .expect("the advertised URL is on this site");
    let answer = rpc_as(&router, path, "tools/list", json!({}), None).await;
    assert!(
        answer["result"]["tools"].is_array(),
        "the advertised address answered, but not as an MCP server: {answer}"
    );
}

#[tokio::test]
async fn a_site_served_under_a_prefix_answers_under_that_prefix_too() {
    // `CanonicalOrigin::parse_with_base_path` puts `build.basePath` into every
    // address the agent surfaces publish (`engine/mod.rs:1551`), the MCP
    // endpoint among them — so a server listening only at `/mcp` would 404 the
    // address it had just published. Defect 162 again, one deployment shape
    // narrower, and Liyasa's own documentation site is served under a prefix.
    //
    // Both spellings answer. `tests/docs/mig_22.rs` has an ignored test
    // claiming the prefix does not reach the agent surfaces at all; one of the
    // two is stale and it is WP-31's to settle, and answering at both
    // addresses is right whichever way it goes.
    let root = std::env::temp_dir().join(format!("liyasa-mcp01-base-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a project directory");
    let config = SITE.replace(
        r#""name": "Acme docs","#,
        r#""name": "Acme docs", "build": { "basePath": "/docs" },"#,
    );
    write(&root, "liyasa.json", &config);
    write(&root, "petstore.json", SPEC);
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
    assert!(!report.failed(false), "{:?}", report.diagnostics);

    let (harness, _) = Harness::new(Setup {
        dist: Some(root.join("dist")),
        site_config: Some(serde_json::from_str(&config).expect("JSON")),
        ..Setup::new("baseprefix")
    })
    .await;
    let router = mcp::mount(&harness.state).router.expect("a router");

    for path in ["/docs/mcp", "/mcp", "/docs/_liyasa/mcp", "/_liyasa/mcp"] {
        let answer = rpc_as(&router, path, "tools/list", json!({}), None).await;
        assert!(
            answer["result"]["tools"].is_array(),
            "{path} did not answer as an MCP server: {answer}"
        );
    }

    // The card is served at the origin's root, because a well-known URI is
    // defined relative to the root (RFC 8615) — and it names the prefixed
    // endpoint, which is where the site's own index points.
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/.well-known/mcp")
                .body(Body::empty())
                .expect("a request"),
        )
        .await
        .expect("a response");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("a complete body");
    let card: Value = serde_json::from_slice(&bytes).expect("JSON");
    assert_eq!(
        card["servers"][0]["url"],
        json!("https://docs.acme.com/docs/mcp")
    );
    assert_eq!(
        card["documentation"],
        json!("https://docs.acme.com/docs/llms.txt")
    );
}

#[tokio::test]
async fn the_rate_limited_alias_answers_exactly_as_the_canonical_path_does() {
    // `routes::pool_for` charges the `Mcp` bucket on `/_liyasa/mcp`. Serving
    // only `/mcp` would leave agent traffic in the human page pool; serving
    // only the alias would leave the advertised URL a 404 (RFC 1900).
    let (_harness, router) = serve("alias", &[]).await;
    let canonical = rpc_as(&router, "/mcp", "tools/list", json!({}), None).await;
    let alias = rpc_as(&router, "/_liyasa/mcp", "tools/list", json!({}), None).await;
    assert_eq!(canonical, alias);
}

// ---- MCP-03: entitlement ----

#[tokio::test]
async fn a_restricted_page_is_absent_for_an_agent_with_no_session() {
    let (_harness, router) = serve("anonymous", &[]).await;

    let listed = tool(&router, "list_pages", json!({})).await;
    let text = text_of(&listed);
    assert!(
        !text.contains("/internal/runbook"),
        "a restricted page is listed to an anonymous agent: {text}"
    );

    // And not by searching for its text either, which is the failure a
    // listing filter alone would leave open.
    let searched = tool(&router, "search", json!({ "query": "escalation number" })).await;
    let searched = text_of(&searched);
    assert!(
        !searched.contains("/internal/runbook"),
        "a restricted page reached search: {searched}"
    );
    // The page's own sentence, not the query — the transcript echoes the
    // query back, so asserting on the query's words would pass for a server
    // that leaked the whole page.
    assert!(
        !searched.contains("secret escalation"),
        "a restricted page's text reached a snippet: {searched}"
    );
    assert!(
        searched.starts_with("No results"),
        "the only page holding that phrase is restricted, so there is nothing to return: \
         {searched}"
    );

    // Fetched directly, it is a miss rather than a refusal: on a public site
    // a restricted page must be indistinguishable from one that is not there,
    // and an agent is a much better enumerator of routes than a person.
    let answer = rpc(
        &router,
        "tools/call",
        json!({ "name": "fetch", "arguments": { "route": "/internal/runbook" } }),
    )
    .await;
    assert_eq!(answer["result"]["isError"], json!(true));
    let text = answer["result"]["content"][0]["text"]
        .as_str()
        .expect("text");
    assert!(text.contains("no page at"), "{text}");
    assert!(
        !text.contains("restricted") && !text.contains("session"),
        "the refusal confirms the page exists: {text}"
    );
}

#[tokio::test]
async fn the_same_page_is_served_to_an_agent_whose_reader_is_in_the_group() {
    // Without this the filter above would be satisfied by a server that
    // returns nothing to anybody.
    let (_harness, router) = serve("entitled", &[]).await;
    let staff = Principal::new("someone").with_groups(["staff"]);

    let answer = rpc_as(
        &router,
        "/mcp",
        "tools/call",
        json!({ "name": "fetch", "arguments": { "route": "/internal/runbook" } }),
        Some(staff.clone()),
    )
    .await;
    assert_eq!(
        answer["result"]["isError"],
        json!(false),
        "{}",
        answer["result"]["content"][0]["text"]
    );
    let text = answer["result"]["content"][0]["text"]
        .as_str()
        .expect("text");
    assert!(text.contains("escalation number"), "{text}");

    let listed = rpc_as(
        &router,
        "/mcp",
        "tools/call",
        json!({ "name": "list_pages", "arguments": {} }),
        Some(staff),
    )
    .await;
    assert!(
        listed["result"]["content"][0]["text"]
            .as_str()
            .is_some_and(|text| text.contains("/internal/runbook")),
        "{listed}"
    );
}

#[tokio::test]
async fn report_issue_is_unauthenticated_and_files_against_the_feedback_table() {
    // MCP-03: the one write tool, anonymous by design, size capped.
    let (harness, router) = serve("report", &[]).await;
    let result = tool(
        &router,
        "report_issue",
        json!({
            "summary": "The install page does not say which Rust version is needed",
            "detail": "Followed /guides/install and cargo refused to build.",
            "route": "/guides/install"
        }),
    )
    .await;
    let id = result["structuredContent"]["receipt"]
        .as_str()
        .and_then(|receipt| receipt.split('`').nth(1))
        .expect("the receipt quotes the record's id");
    assert!(id.starts_with("fb_"), "{id}");

    let store = harness.state.store.clone().expect("a store");
    let record = store
        .feedback()
        .get(id)
        .await
        .expect("the feedback table")
        .expect("the report that was just filed");
    assert_eq!(record.route, "/guides/install");
    assert_eq!(record.kind, liyasa_store::records::FeedbackKind::Agent);
    assert!(
        record
            .task
            .as_deref()
            .is_some_and(|task| task.contains("Rust version")),
        "{record:?}"
    );
}

#[tokio::test]
async fn a_report_about_a_page_the_caller_may_not_read_is_refused() {
    // Otherwise the route lands in a maintainer's inbox as evidence that an
    // anonymous caller enumerated it.
    let (_harness, router) = serve("reportleak", &[]).await;
    let answer = rpc(
        &router,
        "tools/call",
        json!({
            "name": "report_issue",
            "arguments": { "summary": "wrong", "route": "/internal/runbook" }
        }),
    )
    .await;
    assert_eq!(answer["result"]["isError"], json!(true));
}

// ---- MCP-05: analytics ----

#[tokio::test]
async fn every_call_is_an_analytics_event_with_caller_type_agent() {
    let (harness, router) = serve("analytics", &[]).await;
    let before = harness.state.ingest.drain(1024).len();
    assert_eq!(before, 0, "the fixture starts with an empty queue");

    let _ = tool(&router, "list_pages", json!({})).await;
    let events = harness.state.ingest.drain(1024);
    let event = events
        .iter()
        .find(|event| event.kind == "mcp_call")
        .unwrap_or_else(|| panic!("no `mcp_call` event: {events:?}"));
    assert_eq!(event.caller["kind"], json!("agent"));
    assert_eq!(event.props["method"], json!("tools/call"));
    assert_eq!(event.route, "/mcp");
    // The method, never the arguments: a question an agent asked is the
    // reader's text and §30.2.4 keeps it out of analytics.
    assert!(
        !event.props.to_string().contains("arguments"),
        "{:?}",
        event.props
    );
}

// ---- the transport ----

#[tokio::test]
async fn get_says_there_is_no_stream_and_delete_ends_cleanly() {
    let (_harness, router) = serve("transport", &[]).await;

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/mcp")
                .body(Body::empty())
                .expect("a request"),
        )
        .await
        .expect("a response");
    // Holding a stream open that never emits looks to a user like a hang.
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(
        response
            .headers()
            .get(header::ALLOW)
            .and_then(|v| v.to_str().ok()),
        Some("POST, DELETE")
    );

    let response = router
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/mcp")
                .body(Body::empty())
                .expect("a request"),
        )
        .await
        .expect("a response");
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn a_notification_is_accepted_with_no_body() {
    let (_harness, router) = serve("notify", &[]).await;
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mcp")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }).to_string(),
                ))
                .expect("a request"),
        )
        .await
        .expect("a response");
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let bytes = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .expect("a complete body");
    assert!(bytes.is_empty(), "a notification takes no response");
}

#[tokio::test]
async fn a_protocol_version_this_server_does_not_speak_is_refused() {
    let (_harness, router) = serve("version", &[]).await;
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mcp")
                .header(header::CONTENT_TYPE, "application/json")
                .header("mcp-protocol-version", "1999-01-01")
                .body(Body::from(
                    json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" }).to_string(),
                ))
                .expect("a request"),
        )
        .await
        .expect("a response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// ---- mounting ----

#[tokio::test]
async fn a_site_that_turned_the_server_off_mounts_nothing_and_says_so() {
    let dist = build_site("disabled", &[]);
    let (harness, _) = Harness::new(Setup {
        dist: Some(dist),
        site_config: Some(json!({
            "name": "Acme docs",
            "agents": { "mcp": { "enabled": false } }
        })),
        ..Setup::new("disabled")
    })
    .await;
    let mount = mcp::mount(&harness.state);
    assert!(mount.router.is_none());
    assert!(
        mount.skipped.unwrap_or_default().contains("agents.mcp"),
        "a subtree that mounts nothing has to say which switch turned it off"
    );
}

#[tokio::test]
async fn a_page_at_the_endpoint_raises_w0818_and_the_endpoint_still_answers() {
    // `/mcp` is in the site's own namespace and an explicit route beats the
    // page fallback, so a site with a page there loses it silently. Declining
    // to serve the endpoint on such a site was the other option and is worse:
    // it makes the endpoint's existence depend on the site's content, so the
    // URL in `llms.txt` would be right on most sites and wrong on some
    // (RFC 1900).
    let (harness, _) = {
        let dist = build_site("shadow", &[("mcp.md", "---\ntitle: MCP\n---\n# MCP\n")]);
        Harness::new(Setup {
            dist: Some(dist),
            site_config: Some(serde_json::from_str(SITE).expect("JSON")),
            ..Setup::new("shadow")
        })
        .await
    };
    let mount = mcp::mount(&harness.state);
    let codes: Vec<&str> = mount
        .diagnostics
        .as_slice()
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect();
    assert!(codes.contains(&"W0818"), "{codes:?}");

    let router = mount.router.expect("the endpoint still mounts");
    let answer = rpc(&router, "ping", json!({})).await;
    assert!(answer["error"].is_null(), "{answer}");
}

#[tokio::test]
async fn an_instance_with_no_site_mounts_nothing_rather_than_failing_per_request() {
    let (harness, _) = Harness::new(Setup {
        dist: Some(std::env::temp_dir().join("liyasa-mcp01-no-such-bundle")),
        ..Setup::new("nobundle")
    })
    .await;
    let mount = mcp::mount(&harness.state);
    assert!(mount.router.is_none());
    assert!(mount.skipped.is_some());
}

/// The one assertion that needs WP-14's line in `routes/mount.rs`.
///
/// It is written as the behaviour wanted and skips with a verified reason
/// while the entry is absent, rather than pinning the gap: a test that
/// asserted "mcp is NOT in `subtrees()`" would go red on the commit that
/// fixes it, which is the wrong way round and deadlocks the package that has
/// to write it.
#[tokio::test]
async fn the_application_composes_the_subtree() {
    let registered = liyasa_server::routes::mount::subtrees()
        .iter()
        .any(|subtree| subtree.name == "mcp");
    if !registered {
        eprintln!(
            "skipped: `routes::mount::subtrees()` has no `mcp` entry yet. That file is WP-14's \
             and the entry is sequenced after this branch, so an unresolved `crate::mcp::mount` \
             never reaches anyone rebasing through it (defect 194). Everything this test would \
             assert is covered against the subtree's own router above; what is missing is only \
             the composition."
        );
        return;
    }

    let (harness, _) = serve("composed", &[]).await;
    let record = harness
        .mounted
        .iter()
        .find(|record| record.name == "mcp")
        .expect("`subtrees()` names `mcp`, so `application` must record it");
    assert!(
        record.mounted,
        "the subtree was registered and did not mount: {:?}",
        record.skipped
    );

    let response = harness
        .send(
            Request::builder()
                .method("POST")
                .uri("/mcp")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }).to_string(),
                ))
                .expect("a request"),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
}
