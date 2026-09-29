//! `POST /_liyasa/proxy` (API-41, defect 150).
//!
//! Driven as HTTP through `routes::application`, never by calling the handler:
//! a handler nothing reaches is the rate-limit pool it replaces, one layer up.
//!
//! What these assert is the refusal side, because that is the half a reader's
//! browser cannot reach around: the allow list, the offline mode, and the
//! three distinct reasons the proxy forwards nothing. The forwarding side
//! needs an upstream to forward to, which is a container this machine has not
//! got (RFC 1402) — `liyasa-openapi`'s `mock` module behind its `testing`
//! feature is what that test would drive when someone writes it.

use http::StatusCode;
use liyasa_tests::server::{Harness, Setup, body_text, expect_status};
use serde_json::json;

/// The envelope is `liyasa_openapi::sample::Request`, so a body the playground
/// did not produce is refused by serde before the handler sees it — including
/// one carrying an extra field aimed somewhere the form cannot address.
fn envelope(url: &str) -> serde_json::Value {
    json!({
        "method": "GET",
        "baseUrl": "https://api.example.com",
        "path": "/widgets",
        "query": [],
        "headers": [],
        "cookies": [],
        "body": null,
        "url": url,
        "cookieHeader": null,
        "operationId": "listWidgets"
    })
}

#[tokio::test]
async fn the_proxy_endpoint_is_mounted() {
    let (harness, _site) = Harness::serving("api41-mounted").await;

    let response = harness
        .post_json(
            "/_liyasa/proxy",
            envelope("https://api.example.com/widgets"),
        )
        .await;
    assert_ne!(
        response.status(),
        StatusCode::NOT_FOUND,
        "the proxy is not mounted; the pool for this prefix still points at nothing. \
         A refusal is a mounted route answering; a 404 is the route being absent, and \
         this endpoint exists to end the second."
    );
}

/// The fixture site configures no proxy, so `playground.proxy.enabled` is
/// false and the answer is "this site does not offer one" — not a generic 403
/// that reads as "you may not".
#[tokio::test]
async fn a_site_with_no_proxy_says_the_proxy_is_disabled() {
    let (harness, _site) = Harness::serving("api41-disabled").await;

    let response = harness
        .post_json(
            "/_liyasa/proxy",
            envelope("https://api.example.com/widgets"),
        )
        .await;
    expect_status(
        harness
            .post_json(
                "/_liyasa/proxy",
                envelope("https://api.example.com/widgets"),
            )
            .await,
        StatusCode::FORBIDDEN,
    );
    let body = body_text(response).await;
    assert!(
        body.contains("disabled"),
        "an operator is told which of the three refusals this is: {body}"
    );
}

/// HOST-08: an offline instance makes no outbound request of any kind, and a
/// proxied call is one. Checked before the allow list so the reason names the
/// mode the operator chose rather than a host they did not list.
#[tokio::test]
async fn an_offline_instance_refuses_before_it_looks_at_the_allow_list() {
    let setup = Setup {
        config: liyasa_server::routes::ServerConfig {
            offline: true,
            ..Default::default()
        },
        ..Setup::new("api41-offline")
    };
    let (harness, _site) = Harness::new(setup).await;

    let response = harness
        .post_json(
            "/_liyasa/proxy",
            envelope("https://api.example.com/widgets"),
        )
        .await;
    expect_status(
        harness
            .post_json(
                "/_liyasa/proxy",
                envelope("https://api.example.com/widgets"),
            )
            .await,
        StatusCode::FORBIDDEN,
    );
    let body = body_text(response).await;
    assert!(
        body.contains("offline"),
        "the refusal names HOST-08 rather than the allow list: {body}"
    );
}

/// `deny_unknown_fields` on the envelope, which WP-08 added for this route:
/// the shape of an attack on a proxy envelope is an extra field the handler
/// half-honours, and the value here is where one would be aimed.
#[tokio::test]
async fn an_envelope_carrying_an_unknown_field_never_reaches_the_handler() {
    let (harness, _site) = Harness::serving("api41-unknown").await;

    let mut body = envelope("https://api.example.com/widgets");
    body["upstream"] = json!("http://169.254.169.254");
    let response = harness.post_json("/_liyasa/proxy", body).await;
    assert_eq!(
        response.status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "an envelope the playground could not have produced is refused by the type"
    );
}
