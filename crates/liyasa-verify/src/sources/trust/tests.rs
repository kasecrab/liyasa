use serde_json::{Value, json};

use super::*;

fn spec(declaration: Value) -> SourceSpec {
    let (spec, problems) = SourceSpec::parse("s", &declaration);
    assert!(problems.is_empty(), "{problems:#?}");
    spec
}

#[test]
fn trust_follows_ver_26s_table() {
    for (declaration, want) in [
        (
            json!({ "kind": "file", "path": "f.json" }),
            TrustLevel::Operator,
        ),
        (
            json!({ "kind": "manual", "owner": "ops", "expires": "2026-12-31", "values": { "a": 1 } }),
            TrustLevel::Operator,
        ),
        (
            json!({ "kind": "repo", "path": "f.json" }),
            TrustLevel::Member,
        ),
        (
            json!({ "kind": "url", "url": "https://x.test/" }),
            TrustLevel::External,
        ),
        (
            json!({ "kind": "command", "path": "./f.sh", "command": ["./f.sh"] }),
            TrustLevel::External,
        ),
        (
            json!({ "kind": "screenshot", "url": "https://x.test/" }),
            TrustLevel::External,
        ),
    ] {
        assert_eq!(trust_of(&spec(declaration.clone())), want, "{declaration}");
    }
}

#[test]
fn an_openapi_source_is_external_only_when_it_is_fetched() {
    assert_eq!(
        trust_of(&spec(
            json!({ "kind": "openapi", "url": "https://x.test/openapi.json" })
        )),
        TrustLevel::External
    );
    assert_eq!(
        trust_of(&spec(
            json!({ "kind": "openapi", "path": "api/openapi.json" })
        )),
        TrustLevel::Member
    );
}

#[test]
fn everything_below_operator_is_escaped_on_the_way_in() {
    assert!(!needs_escaping(TrustLevel::Operator));
    for trust in [
        TrustLevel::Member,
        TrustLevel::Anonymous,
        TrustLevel::External,
    ] {
        assert!(needs_escaping(trust), "{trust:?}");
    }
}

#[test]
fn https_is_the_transport_and_plain_http_is_e0806() {
    let policy = TransportPolicy::default();
    assert!(
        policy
            .check(&spec(
                json!({ "kind": "url", "url": "https://api.example.com/x" })
            ))
            .is_ok()
    );

    let refused = policy
        .check(&spec(
            json!({ "kind": "url", "url": "http://api.example.com/x" }),
        ))
        .expect_err("plain http off the allow list");
    assert_eq!(refused.code.as_str(), "E0806");
    assert!(
        refused.message.contains("allowInsecureHosts"),
        "{refused:?}"
    );
}

#[test]
fn localhost_is_the_one_insecure_host_by_default() {
    let policy = TransportPolicy::default();
    assert!(
        policy
            .check(&spec(
                json!({ "kind": "url", "url": "http://localhost:8080/x" })
            ))
            .is_ok()
    );
    // The default is `localhost`, not "anything that resolves to the machine".
    assert!(
        policy
            .check(&spec(
                json!({ "kind": "url", "url": "http://127.0.0.1:8080/x" })
            ))
            .is_err()
    );

    let opened = TransportPolicy::with_insecure_hosts(HostSet(vec![HostPattern::Exact(
        "127.0.0.1".to_owned(),
    )]));
    assert!(
        opened
            .check(&spec(
                json!({ "kind": "url", "url": "http://127.0.0.1:8080/x" })
            ))
            .is_ok()
    );
}

#[test]
fn a_scheme_that_is_not_http_is_refused() {
    let refused = TransportPolicy::default()
        .check(&spec(
            json!({ "kind": "url", "url": "ftp://files.example.com/x" }),
        ))
        .expect_err("not an HTTP scheme");
    assert_eq!(refused.code.as_str(), "E0806");
}

#[test]
fn a_pin_no_client_can_verify_refuses_the_fetch_rather_than_ignoring_it() {
    let refused = TransportPolicy::default()
        .check(&spec(json!({
            "kind": "url", "url": "https://api.example.com/x", "pin": "a".repeat(64)
        })))
        .expect_err("a pin this client cannot check");
    assert_eq!(refused.code.as_str(), "E0806");
    assert!(refused.message.contains("pin"), "{refused:?}");
}
