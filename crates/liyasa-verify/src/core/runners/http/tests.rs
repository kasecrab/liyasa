use std::sync::Mutex;

use liyasa_core::conformance::block_on;
use liyasa_core::verify::{CheckOutcome, Expectation};
use liyasa_core::vfs::Bytes;
use serde_json::json;

use super::*;
use crate::core::config::StagingTarget;
use crate::core::runners::testing::{NoSandbox, Secrets, spec};

/// Answers every request with one canned response and records what it saw.
#[derive(Default)]
struct Canned {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    seen: Mutex<Vec<HttpRequest>>,
}

impl Canned {
    fn json(status: u16, body: Value) -> Arc<Self> {
        Arc::new(Self {
            status,
            headers: vec![("content-type".to_owned(), "application/json".to_owned())],
            body: serde_json::to_vec(&body).expect("json"),
            seen: Mutex::new(Vec::new()),
        })
    }

    fn response(&self, request: &HttpRequest) -> HttpResponse {
        HttpResponse {
            status: self.status,
            headers: self.headers.clone(),
            body: Bytes::from(self.body.clone()),
            final_url: request.url.clone(),
        }
    }
}

impl HttpClient for Canned {
    fn fetch<'a>(
        &'a self,
        req: HttpRequest,
        _policy: &'a HttpPolicy,
    ) -> BoxFut<'a, Result<HttpResponse, NetError>> {
        let response = self.response(&req);
        self.seen.lock().expect("lock").push(req);
        Box::pin(std::future::ready(Ok(response)))
    }
}

impl MockTarget for Canned {
    fn respond<'a>(
        &'a self,
        request: &'a HttpRequest,
    ) -> BoxFut<'a, Result<HttpResponse, Diagnostic>> {
        let response = self.response(request);
        self.seen.lock().expect("lock").push(request.clone());
        Box::pin(std::future::ready(Ok(response)))
    }
}

struct Offline;

impl HttpClient for Offline {
    fn fetch<'a>(
        &'a self,
        _req: HttpRequest,
        _policy: &'a HttpPolicy,
    ) -> BoxFut<'a, Result<HttpResponse, NetError>> {
        Box::pin(std::future::ready(Err(NetError::Timeout)))
    }
}

fn staging(base: &str) -> HttpConfig {
    HttpConfig {
        target: HttpTarget::Staging,
        staging: Some(StagingTarget {
            base_url: base.to_owned(),
            auth: None,
        }),
    }
}

fn run(runner: &HttpRunner, request: &str, expect: Vec<Expectation>) -> CheckOutcome {
    let check = spec(
        "/api#b#0",
        CheckInput::Http {
            request: request.to_owned(),
        },
        expect,
    );
    block_on(runner.run(&check, &NoSandbox, &Secrets::default())).outcome
}

// ---- request parsing ----

#[test]
fn a_request_line_with_headers_and_a_body_parses() {
    let request = parse_request(
        "POST /v1/orders HTTP/1.1\nContent-Type: application/json\nAccept: */*\n\n{\"id\": 1}",
        Some("https://staging.example.com"),
    )
    .expect("parsed");
    assert_eq!(request.method, Method::POST);
    assert_eq!(
        request.url.as_str(),
        "https://staging.example.com/v1/orders"
    );
    assert_eq!(
        request.headers,
        [
            ("Content-Type".to_owned(), "application/json".to_owned()),
            ("Accept".to_owned(), "*/*".to_owned()),
        ]
    );
    assert_eq!(request.body.as_deref(), Some(&b"{\"id\": 1}"[..]));
}

#[test]
fn an_absolute_url_needs_no_base() {
    let request = parse_request("GET https://api.example.com/health", None).expect("parsed");
    assert_eq!(request.url.as_str(), "https://api.example.com/health");
    assert!(request.body.is_none());
}

#[test]
fn a_relative_target_without_a_base_url_says_which_key_is_missing() {
    let error = parse_request("GET /health", None).expect_err("rejected");
    assert!(error.contains("baseUrl"), "{error}");
}

#[test]
fn a_method_with_a_character_http_forbids_is_rejected() {
    let error = parse_request("GE{T /health", Some("https://x.test")).expect_err("rejected");
    assert!(error.contains("GE{T"), "{error}");
}

#[test]
fn an_extension_method_is_accepted() {
    // WebDAV and cache-purge examples are real documentation; the parser is
    // not the place to decide which methods a reader is allowed to document.
    for method in ["PROPFIND", "PURGE", "PATCH"] {
        let request = parse_request(&format!("{method} /r"), Some("https://x.test"))
            .unwrap_or_else(|e| panic!("{method}: {e}"));
        assert_eq!(request.method.as_str(), method);
    }
}

#[test]
fn a_header_line_without_a_colon_is_rejected() {
    let error =
        parse_request("GET /health\nnot a header\n", Some("https://x.test")).expect_err("rejected");
    assert!(error.contains("not a header"), "{error}");
}

#[test]
fn leading_blank_lines_are_skipped() {
    let request = parse_request("\n\nGET /health", Some("https://x.test")).expect("parsed");
    assert_eq!(request.url.as_str(), "https://x.test/health");
}

// ---- assertions ----

#[test]
fn a_status_expectation_passes_and_fails() {
    let client = Canned::json(200, json!({ "ok": true }));
    let runner = HttpRunner::new(
        client,
        staging("https://x.test"),
        HttpRunner::default_policy(),
    );
    assert_eq!(
        run(&runner, "GET /health", vec![Expectation::Status(200)]),
        CheckOutcome::Pass
    );
    match run(&runner, "GET /health", vec![Expectation::Status(404)]) {
        CheckOutcome::Fail { excerpt } => assert!(excerpt.contains("status is 200"), "{excerpt}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_header_expectation_is_matched_without_regard_to_case() {
    let client = Canned::json(200, json!({}));
    let runner = HttpRunner::new(
        client,
        staging("https://x.test"),
        HttpRunner::default_policy(),
    );
    assert_eq!(
        run(
            &runner,
            "GET /health",
            vec![Expectation::Header {
                name: "Content-Type".to_owned(),
                value: "application/json".to_owned(),
            }]
        ),
        CheckOutcome::Pass
    );
}

#[test]
fn an_absent_header_says_so() {
    let client = Canned::json(200, json!({}));
    let runner = HttpRunner::new(
        client,
        staging("https://x.test"),
        HttpRunner::default_policy(),
    );
    match run(
        &runner,
        "GET /health",
        vec![Expectation::Header {
            name: "x-request-id".to_owned(),
            value: "1".to_owned(),
        }],
    ) {
        CheckOutcome::Fail { excerpt } => assert!(excerpt.contains("absent"), "{excerpt}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_json_path_expectation_reads_the_body() {
    let client = Canned::json(200, json!({ "data": { "id": "ord_1" } }));
    let runner = HttpRunner::new(
        client,
        staging("https://x.test"),
        HttpRunner::default_policy(),
    );
    assert_eq!(
        run(
            &runner,
            "GET /orders/1",
            vec![Expectation::JsonPath {
                path: "$.data.id".to_owned(),
                value: json!("ord_1"),
            }]
        ),
        CheckOutcome::Pass
    );
}

#[test]
fn every_failing_assertion_is_reported_not_only_the_first() {
    let client = Canned::json(500, json!({ "id": 2 }));
    let runner = HttpRunner::new(
        client,
        staging("https://x.test"),
        HttpRunner::default_policy(),
    );
    match run(
        &runner,
        "GET /orders/1",
        vec![
            Expectation::Status(200),
            Expectation::JsonPath {
                path: "$.id".to_owned(),
                value: json!(1),
            },
        ],
    ) {
        CheckOutcome::Fail { excerpt } => {
            assert!(excerpt.contains("status"), "{excerpt}");
            assert!(excerpt.contains("$.id"), "{excerpt}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_network_failure_is_a_failing_check_not_a_broken_one() {
    let runner = HttpRunner::new(
        Arc::new(Offline),
        staging("https://x.test"),
        HttpRunner::default_policy(),
    );
    assert!(matches!(
        run(&runner, "GET /health", vec![Expectation::Status(200)]),
        CheckOutcome::Fail { .. }
    ));
}

#[test]
fn the_response_body_never_reaches_the_result() {
    // VER-10: only pass or fail, a scrubbed excerpt, and a digest survive.
    let client = Canned::json(200, json!({ "secret_field": "swordfish-1234567890" }));
    let runner = HttpRunner::new(
        client,
        staging("https://x.test"),
        HttpRunner::default_policy(),
    );
    let check = spec(
        "/api#b#0",
        CheckInput::Http {
            request: "GET /orders/1".to_owned(),
        },
        vec![Expectation::Status(404)],
    );
    let result = block_on(runner.run(&check, &NoSandbox, &Secrets::default()));
    match result.outcome {
        CheckOutcome::Fail { excerpt } => assert!(!excerpt.contains("swordfish"), "{excerpt}"),
        other => panic!("{other:?}"),
    }
}

// ---- targets ----

#[test]
fn the_mock_target_is_used_when_config_says_mock() {
    let mock = Canned::json(201, json!({ "id": "ord_1" }));
    let runner = HttpRunner::new(
        Arc::new(Offline),
        HttpConfig {
            target: HttpTarget::Mock,
            staging: None,
        },
        HttpRunner::default_policy(),
    )
    .with_mock(mock.clone());
    assert_eq!(
        run(
            &runner,
            "POST https://api.example.com/orders",
            vec![Expectation::Status(201)]
        ),
        CheckOutcome::Pass
    );
    assert_eq!(mock.seen.lock().expect("lock").len(), 1);
}

#[test]
fn mock_with_no_mock_wired_up_skips_and_says_why() {
    let runner = HttpRunner::new(
        Arc::new(Offline),
        HttpConfig {
            target: HttpTarget::Mock,
            staging: None,
        },
        HttpRunner::default_policy(),
    );
    match run(&runner, "GET https://x.test/health", Vec::new()) {
        CheckOutcome::Skip { reason } => assert!(reason.contains("mock"), "{reason}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn staging_with_none_configured_skips() {
    let runner = HttpRunner::new(
        Arc::new(Offline),
        HttpConfig {
            target: HttpTarget::Staging,
            staging: None,
        },
        HttpRunner::default_policy(),
    );
    assert!(matches!(
        run(&runner, "GET https://x.test/health", Vec::new()),
        CheckOutcome::Skip { .. }
    ));
}

#[test]
fn both_means_both_and_one_failure_fails_the_check() {
    let mock = Canned::json(200, json!({}));
    let runner = HttpRunner::new(
        Arc::new(Offline),
        HttpConfig {
            target: HttpTarget::Both,
            staging: Some(StagingTarget {
                base_url: "https://staging.example.com".to_owned(),
                auth: None,
            }),
        },
        HttpRunner::default_policy(),
    )
    .with_mock(mock.clone());
    // The mock answers 200 and staging times out.
    assert!(matches!(
        run(&runner, "GET /health", vec![Expectation::Status(200)]),
        CheckOutcome::Fail { .. }
    ));
    assert_eq!(mock.seen.lock().expect("lock").len(), 1);
}

#[test]
fn a_block_in_another_language_is_skipped() {
    let runner = HttpRunner::new(
        Arc::new(Offline),
        staging("https://x.test"),
        HttpRunner::default_policy(),
    );
    let check = spec(
        "/api#b#0",
        CheckInput::Code {
            lang: "rust".to_owned(),
            source: String::new(),
            hidden_lines: Vec::new(),
        },
        Vec::new(),
    );
    assert!(matches!(
        block_on(runner.run(&check, &NoSandbox, &Secrets::default())).outcome,
        CheckOutcome::Skip { .. }
    ));
}

#[test]
fn a_block_that_is_not_a_request_is_an_authoring_error() {
    let runner = HttpRunner::new(
        Arc::new(Offline),
        staging("https://x.test"),
        HttpRunner::default_policy(),
    );
    match run(&runner, "", Vec::new()) {
        CheckOutcome::Error(diagnostic) => assert_eq!(diagnostic.code, code::E0601),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_default_policy_denies_private_addresses_and_caps_the_body() {
    let policy = HttpRunner::default_policy();
    assert!(!policy.allow_private);
    assert_eq!(policy.purpose, Purpose::FactSource);
    assert!(policy.max_bytes > 0);
}

// ---- the json-path subset ----

#[test]
fn the_json_path_subset_reads_what_docs_write() {
    let body = json!({
        "data": { "items": [{ "id": "a" }, { "id": "b" }], "a b": 1 }
    });
    assert_eq!(json_path(&body, "$.data.items[0].id"), Some(&json!("a")));
    assert_eq!(json_path(&body, "$.data.items[1].id"), Some(&json!("b")));
    assert_eq!(json_path(&body, r#"$.data["a b"]"#), Some(&json!(1)));
    assert_eq!(json_path(&body, "$"), Some(&body));
}

#[test]
fn a_path_that_is_not_in_the_document_is_none_not_null() {
    let body = json!({ "a": 1 });
    assert_eq!(json_path(&body, "$.b"), None);
    assert_eq!(json_path(&body, "$.a[0]"), None);
}

#[test]
fn an_unsupported_path_form_is_none_so_the_assertion_fails_loudly() {
    let body = json!({ "items": [1, 2, 3] });
    for path in ["$.items[*]", "$..id", "$.items[?(@ > 1)]"] {
        assert_eq!(json_path(&body, path), None, "{path}");
    }
}
