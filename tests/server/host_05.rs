//! HOST-05: health, readiness, metrics, structured logs, and traces.
//!
//! Given the server; when health, readiness, and metrics endpoints are
//! fetched; then they respond with the documented shapes and traces are
//! exported to a test collector.

use std::sync::Arc;

use axum::body::Body;
use http::{Request, StatusCode};
use liyasa_server::routes::serve::Runtime;
use liyasa_server::routes::telemetry::Tracer;
use liyasa_server::routes::{AppState, ServerConfig};
use liyasa_tests::server::{Harness, Setup, body_json, body_text, expect_status, header};

#[tokio::test]
async fn health_says_the_process_is_up() {
    let (harness, _site) = Harness::serving("host05-health").await;
    let response = expect_status(harness.get("/_liyasa/health").await, StatusCode::OK);
    assert_eq!(
        header(&response, "content-type"),
        Some("application/json"),
        "a probe parses JSON, not a page"
    );
    let body = body_json(response).await;
    assert_eq!(body["status"], "ok");
    assert!(body["version"].as_str().is_some_and(|v| !v.is_empty()));
    assert!(body["uptimeSeconds"].as_u64().is_some());
}

#[tokio::test]
async fn readiness_names_each_check_and_fails_when_one_does() {
    let (harness, _site) = Harness::serving("host05-ready").await;
    let body = body_json(expect_status(
        harness.get("/_liyasa/ready").await,
        StatusCode::OK,
    ))
    .await;
    assert_eq!(body["status"], "ready");
    let checks = body["checks"].as_array().expect("a check list");
    assert!(
        checks
            .iter()
            .any(|c| c["name"] == "bundle" && c["status"] == "pass"),
        "{body}"
    );
    assert!(
        checks
            .iter()
            .any(|c| c["name"] == "store" && c["status"] == "pass"),
        "{body}"
    );

    // A replica with no deployment is live but not ready: a restart would not
    // help it, so liveness stays green while readiness goes red.
    let (bare, _) = Harness::new(Setup {
        dist: Some(std::env::temp_dir().join("liyasa-absent-bundle")),
        with_store: false,
        ..Setup::new("host05-unready")
    })
    .await;
    let response = expect_status(
        bare.get("/_liyasa/ready").await,
        StatusCode::SERVICE_UNAVAILABLE,
    );
    let body = body_json(response).await;
    assert_eq!(body["status"], "unready");
    assert!(
        body["checks"]
            .as_array()
            .expect("a check list")
            .iter()
            .any(|c| c["name"] == "bundle" && c["status"] == "fail"),
        "{body}"
    );
    expect_status(bare.get("/_liyasa/health").await, StatusCode::OK);
}

#[tokio::test]
async fn a_draining_replica_reports_itself_unready() {
    let (harness, _site) = Harness::serving("host05-drain").await;
    expect_status(harness.get("/_liyasa/ready").await, StatusCode::OK);

    harness.state.begin_drain();
    let response = expect_status(
        harness.get("/_liyasa/ready").await,
        StatusCode::SERVICE_UNAVAILABLE,
    );
    let body = body_json(response).await;
    assert_eq!(body["status"], "unready");
    assert!(
        body["checks"]
            .as_array()
            .expect("a check list")
            .iter()
            .any(|c| c["name"] == "drain"),
        "{body}"
    );
    // Liveness is unaffected: the process is healthy, it is just leaving.
    expect_status(harness.get("/_liyasa/health").await, StatusCode::OK);
}

#[tokio::test]
async fn metrics_are_prometheus_text_and_count_what_was_served() {
    let (harness, _site) = Harness::serving("host05-metrics").await;
    harness.get("/").await;
    harness.get("/guides/install.md").await;

    let response = expect_status(harness.get("/_liyasa/metrics").await, StatusCode::OK);
    assert_eq!(
        header(&response, "content-type"),
        Some("text/plain; version=0.0.4; charset=utf-8")
    );
    let body = body_text(response).await;

    assert!(
        body.contains("# TYPE liyasa_http_requests_total counter"),
        "{body}"
    );
    assert!(
        body.contains("liyasa_http_requests_total{class=\"page\",status=\"200\"} 1"),
        "{body}"
    );
    assert!(
        body.contains("liyasa_http_requests_total{class=\"markdown\",status=\"200\"} 1"),
        "{body}"
    );
    assert!(
        body.contains("# TYPE liyasa_http_request_duration_seconds histogram"),
        "{body}"
    );
    assert!(
        body.contains("liyasa_http_request_duration_seconds_count{class=\"page\"}"),
        "{body}"
    );
    assert!(body.contains("liyasa_ingest_queue_depth "), "{body}");
    assert!(body.contains("liyasa_jobs_queue_depth "), "{body}");
    assert!(body.contains("liyasa_build_info{version="), "{body}");
}

#[tokio::test]
async fn a_trace_reaches_a_collector_in_the_otlp_shape() {
    // A collector that keeps the one body it is sent.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a port");
    let port = listener.local_addr().expect("an address").port();
    let received = Arc::new(tokio::sync::Mutex::new(Vec::<u8>::new()));
    let sink = received.clone();
    tokio::spawn(async move {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let Ok((mut stream, _)) = listener.accept().await else {
            return;
        };
        let mut buffer = vec![0u8; 256 * 1024];
        let mut read = 0;
        // Read until the body is complete: the collector knows the length
        // from the header the client sent.
        let mut expected = None::<usize>;
        loop {
            let n = match stream.read(&mut buffer[read..]).await {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            read += n;
            let text = String::from_utf8_lossy(&buffer[..read]).into_owned();
            if let Some(head_end) = text.find("\r\n\r\n") {
                if expected.is_none() {
                    expected = text
                        .to_ascii_lowercase()
                        .split("content-length:")
                        .nth(1)
                        .and_then(|rest| rest.split("\r\n").next())
                        .and_then(|value| value.trim().parse().ok());
                }
                let body_start = head_end + 4;
                if expected.is_some_and(|length| read - body_start >= length) {
                    *sink.lock().await = buffer[body_start..read].to_vec();
                    break;
                }
            }
        }
        let _ = stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await;
        let _ = stream.shutdown().await;
    });

    let tracer = Arc::new(Tracer::new("liyasa", true));
    let (harness, _site) = Harness::new(Setup::new("host05-traces")).await;
    let state = Arc::new(
        AppState::new(ServerConfig::default())
            .with_bundle(harness.state.bundle.clone().expect("a bundle"))
            .with_tracer(tracer.clone()),
    );
    let router = liyasa_server::routes::router(state.clone());

    // A request that continues a trace started at the edge.
    let request = Request::builder()
        .uri("/guides/install")
        .header(
            "traceparent",
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01",
        )
        .body(Body::empty())
        .expect("a request");
    let response = tower::ServiceExt::oneshot(router, request)
        .await
        .expect("a response");
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        header(&response, "traceparent").is_some(),
        "the response carries the span it was served under"
    );

    let runtime = Runtime::new(state);
    let client = liyasa_net::Client::new(liyasa_net::ClientOptions::default()).expect("a client");
    let endpoint: liyasa_core::net::Url = format!("http://127.0.0.1:{port}/v1/traces")
        .parse()
        .expect("a url");
    runtime.export_once(&client, &endpoint).await;

    let body = received.lock().await.clone();
    assert!(!body.is_empty(), "the collector received nothing");
    let payload: serde_json::Value = serde_json::from_slice(&body).expect("OTLP JSON");
    let span = &payload["resourceSpans"][0]["scopeSpans"][0]["spans"][0];
    assert_eq!(
        span["traceId"], "0af7651916cd43dd8448eb211c80319c",
        "the trace from the edge continues rather than starting again"
    );
    assert_eq!(span["parentSpanId"], "b7ad6b7169203331");
    assert_eq!(span["kind"], 2, "a server span");
    assert_eq!(span["status"]["code"], 1);
    assert_eq!(
        payload["resourceSpans"][0]["resource"]["attributes"][0]["value"]["stringValue"],
        "liyasa"
    );
}

#[tokio::test]
async fn the_server_answers_over_a_real_socket() {
    // Every other test here drives the router in process. This one goes
    // through the listener, the connection loop, and the outbound client, so
    // a fault in the part no in-process test touches cannot hide behind them.
    let (harness, _site) = Harness::serving("host05-socket").await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a port");
    let port = listener.local_addr().expect("an address").port();

    let runtime = Runtime::new(harness.state.clone());
    let stopper = runtime.stopper();
    let router = liyasa_server::routes::router(harness.state.clone());
    let serving = tokio::spawn(runtime.serve(listener, router));

    let client = liyasa_net::Client::new(liyasa_net::ClientOptions::default()).expect("a client");
    let policy = liyasa_core::net::HttpPolicy {
        allow_hosts: liyasa_core::net::HostSet::default(),
        deny_hosts: liyasa_core::net::HostSet::default(),
        // Loopback is private space, which is exactly what an operator
        // reaching their own instance has to allow.
        allow_private: true,
        max_redirects: 0,
        max_bytes: 1024 * 1024,
        timeout: std::time::Duration::from_secs(5),
        purpose: liyasa_core::net::Purpose::LinkCheck,
    };
    let get = |path: &str| liyasa_core::net::HttpRequest {
        method: liyasa_core::net::Method::GET,
        url: format!("http://127.0.0.1:{port}{path}")
            .parse()
            .expect("a url"),
        headers: Vec::new(),
        body: None,
    };

    use liyasa_core::net::HttpClient as _;
    let health = client
        .fetch(get("/_liyasa/health"), &policy)
        .await
        .expect("the listener answers");
    assert_eq!(health.status, 200);

    let page = client
        .fetch(get("/guides/install"), &policy)
        .await
        .expect("a page over the wire");
    assert_eq!(page.status, 200);
    assert!(
        page.headers
            .iter()
            .any(|(n, v)| n.eq_ignore_ascii_case("content-type") && v.starts_with("text/html")),
        "{:?}",
        page.headers
    );
    assert!(!page.body.is_empty());

    // NFR-31: the signal drains rather than cutting the connection.
    stopper.stop();
    let stopped = tokio::time::timeout(std::time::Duration::from_secs(10), serving).await;
    assert!(stopped.is_ok(), "the server did not finish draining");
    assert!(harness.state.draining(), "the drain flag was never set");
}
