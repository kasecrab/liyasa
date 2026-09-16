//! AUTH-14: no bot-protection interference.
//!
//! Given a sustained 500-request multi-page scan against `liyasa serve` with
//! limits configured; when the limit is exceeded; then every over-limit
//! response is a `429` with `Retry-After`, no response holds a body open, no
//! response body contains a challenge page, and no `200` is returned for a
//! blocked request.

use std::sync::Arc;

use http::StatusCode;
use liyasa_core::server::{RateLimit, RateLimitPool, RateLimiter};
use liyasa_server::routes::limiter::Limiter;
use liyasa_tests::server::{Harness, Setup, body_bytes, header};

/// Markers that would mean an interstitial rather than an answer.
const CHALLENGE_MARKERS: &[&str] = &[
    "challenge",
    "captcha",
    "cf-browser-verification",
    "checking your browser",
    "verify you are human",
    "ddos protection",
    "just a moment",
    "ray id",
    "attention required",
];

fn limited() -> Arc<Limiter> {
    let limiter = Arc::new(Limiter::new());
    limiter.configure(
        RateLimitPool::Pages,
        RateLimit {
            per_minute: 60,
            burst: 20,
            daily: None,
        },
    );
    limiter.configure(
        RateLimitPool::AgentPages,
        RateLimit {
            per_minute: 300,
            burst: 100,
            daily: None,
        },
    );
    limiter
}

#[tokio::test]
async fn a_sustained_scan_is_answered_or_refused_and_never_challenged() {
    let (harness, _site) = Harness::new(Setup {
        limiter: Some(limited()),
        ..Setup::new("auth14-scan")
    })
    .await;

    let paths = [
        "/",
        "/guides/install",
        "/embed/widget",
        "/guides/install.md",
    ];
    let mut served = 0usize;
    let mut refused = 0usize;

    for n in 0..500 {
        let path = paths[n % paths.len()];
        let response = harness.get_from(path, 7).await;
        let status = response.status();
        let retry_after = header(&response, "retry-after").map(str::to_owned);
        let content_type = header(&response, "content-type").map(str::to_owned);
        // Reading the body to completion is the test for "never holds a
        // response body open after sending headers": a server that streamed a
        // challenge would not finish here.
        let body = body_bytes(response).await;
        let text = String::from_utf8_lossy(&body).to_ascii_lowercase();

        for marker in CHALLENGE_MARKERS {
            assert!(
                !text.contains(marker),
                "request {n} to {path} answered {status} with a `{marker}` body"
            );
        }

        match status {
            StatusCode::OK => {
                served += 1;
                assert!(
                    !text.is_empty(),
                    "request {n} to {path} answered 200 with an empty body"
                );
            }
            StatusCode::TOO_MANY_REQUESTS => {
                refused += 1;
                let retry_after = retry_after
                    .unwrap_or_else(|| panic!("request {n} to {path}: a 429 without Retry-After"));
                let seconds: u64 = retry_after.parse().unwrap_or_else(|_| {
                    panic!("Retry-After is whole seconds, got `{retry_after}`")
                });
                assert!(seconds >= 1, "Retry-After must not be zero");
                assert_eq!(
                    content_type.as_deref(),
                    Some("application/problem+json"),
                    "a refusal is problem details, not a page"
                );
                let problem: serde_json::Value =
                    serde_json::from_slice(&body).expect("a problem body");
                assert_eq!(problem["status"], 429);
                assert_eq!(problem["code"], "E0807");
                assert_eq!(problem["retryAfter"], seconds);
            }
            other => panic!("request {n} to {path}: unexpected {other}"),
        }
    }

    assert!(served > 0, "the scan was refused from the first request");
    assert!(
        refused > 0,
        "the scan never hit the limit, so the refusal path was not exercised"
    );
    assert_eq!(served + refused, 500);
}

#[tokio::test]
async fn markdown_traffic_has_its_own_higher_budget() {
    let (harness, _site) = Harness::new(Setup {
        limiter: Some(limited()),
        ..Setup::new("auth14-pools")
    })
    .await;

    // Exhaust the HTML budget for this address.
    let mut html_refusals = 0;
    for _ in 0..60 {
        if harness.get_from("/guides/install", 11).await.status() == StatusCode::TOO_MANY_REQUESTS {
            html_refusals += 1;
        }
    }
    assert!(html_refusals > 0, "the page pool was never exhausted");

    // The same client's Markdown fetches are charged elsewhere and still pass.
    let markdown = harness.get_from("/guides/install.md", 11).await;
    assert_eq!(
        markdown.status(),
        StatusCode::OK,
        "an agent's `.md` fetch must not be refused because a browser pool ran out"
    );
    assert_eq!(
        header(&markdown, "content-type"),
        Some("text/markdown; charset=utf-8")
    );
}

#[tokio::test]
async fn one_clients_burst_does_not_refuse_another_client() {
    let (harness, _site) = Harness::new(Setup {
        limiter: Some(limited()),
        ..Setup::new("auth14-subjects")
    })
    .await;

    let mut refused = 0;
    for _ in 0..80 {
        if harness.get_from("/", 21).await.status() == StatusCode::TOO_MANY_REQUESTS {
            refused += 1;
        }
    }
    assert!(refused > 0, "the noisy client was never limited");
    assert_eq!(
        harness.get_from("/", 22).await.status(),
        StatusCode::OK,
        "a second reader is not punished for the first"
    );
}

#[tokio::test]
async fn an_operations_probe_is_never_refused() {
    let (harness, _site) = Harness::new(Setup {
        limiter: Some(limited()),
        ..Setup::new("auth14-probes")
    })
    .await;
    // A monitoring system polls these; refusing one turns a healthy replica
    // unhealthy and takes it out of rotation.
    for _ in 0..300 {
        assert_eq!(
            harness.get_from("/_liyasa/health", 31).await.status(),
            StatusCode::OK
        );
    }
}

#[tokio::test]
async fn a_forwarded_header_cannot_spread_one_client_across_the_limiter() {
    let (harness, _site) = Harness::new(Setup {
        limiter: Some(limited()),
        ..Setup::new("auth14-spoof")
    })
    .await;

    // No trusted proxy is configured, so the header is ignored entirely and
    // every request is charged to the peer.
    let mut refused = 0;
    for n in 0..120u32 {
        let response = harness
            .get_with(
                "/",
                &[("x-forwarded-for", &format!("198.51.100.{}", n % 250))],
            )
            .await;
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            refused += 1;
        }
    }
    assert!(
        refused > 0,
        "a client rotating X-Forwarded-For escaped the limiter"
    );
}
