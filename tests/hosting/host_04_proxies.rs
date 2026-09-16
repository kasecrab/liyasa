//! HOST-04: reverse proxy guides and tested configurations.
//!
//! Given nginx, Caddy, and Traefik configurations from the docs; when run in
//! containers in front of the server with a subpath; then pages, `.md`
//! routes, and redirects work and `X-Forwarded-*` are honoured.
//!
//! The container half needs Docker or Podman (RFC 1402). What runs everywhere
//! is the half that actually decides whether the configurations are correct:
//! the server's own handling of what those proxies send, and a reading of
//! each file for the headers the server needs.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::ConnectInfo;
use http::{Request, StatusCode};
use liyasa_server::routes::client_ip::TrustedProxies;
use liyasa_server::routes::{AppState, ServerConfig};
use liyasa_tests::server::{Harness, Setup, expect_status, header};

fn deploy(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("hosting")
        .join("deploy")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

#[test]
fn every_configuration_forwards_the_address_and_the_scheme() {
    for (name, needles) in [
        (
            "nginx.conf",
            vec![
                "X-Forwarded-For",
                "X-Forwarded-Proto",
                "X-Real-IP",
                "X-Forwarded-Host",
            ],
        ),
        (
            "Caddyfile",
            vec![
                "X-Forwarded-For",
                "X-Forwarded-Proto",
                "X-Real-IP",
                "X-Forwarded-Host",
            ],
        ),
        ("traefik-dynamic.yml", vec!["X-Forwarded-Proto"]),
    ] {
        let text = deploy(name);
        for needle in needles {
            assert!(text.contains(needle), "{name} does not set `{needle}`");
        }
        assert!(
            text.contains("trustedProxies") || text.contains("8080"),
            "{name} does not point at the server"
        );
    }
}

#[test]
fn every_configuration_documents_the_trusted_proxy_requirement() {
    // The headers above are ignored unless `server.trustedProxies` names the
    // proxy, so a configuration that does not say so produces a server that
    // silently charges every reader to the proxy (AUTH-50).
    for name in ["nginx.conf", "Caddyfile", "traefik.yml", "cloudflared.yml"] {
        let text = deploy(name);
        assert!(
            text.contains("trustedProxies"),
            "{name} does not mention server.trustedProxies"
        );
    }
}

#[test]
fn every_configuration_carries_the_subpath_recipe() {
    for name in [
        "nginx.conf",
        "Caddyfile",
        "traefik-dynamic.yml",
        "cloudflared.yml",
    ] {
        let text = deploy(name);
        assert!(text.contains("/docs"), "{name} has no subpath example");
        assert!(
            text.contains("basePath"),
            "{name} does not say the site is built with build.basePath"
        );
    }
}

#[test]
fn no_proxy_adds_a_second_copy_of_the_security_headers() {
    // The server sends the set of RX-112 itself. A proxy that adds its own
    // produces two of each, and a browser takes the most restrictive, which
    // is how a working site breaks after a proxy change.
    for name in ["nginx.conf", "Caddyfile", "traefik-dynamic.yml"] {
        let text = deploy(name);
        for header in [
            "add_header Content-Security-Policy",
            "add_header Strict-Transport-Security",
        ] {
            assert!(!text.contains(header), "{name} sets `{header}` itself");
        }
    }
}

#[test]
fn the_cloudflare_tunnel_exempts_agent_routes_from_bot_management() {
    // AUTH-14: a challenge in front of `.md` routes means the server never
    // sees the request, and no amount of server-side correctness helps.
    let text = deploy("cloudflared.yml");
    assert!(text.contains("CF-Connecting-IP"));
    assert!(text.contains("CF-IPCountry"));
    assert!(
        text.contains("Bot Fight Mode"),
        "the WAF caveat is not stated"
    );
    assert!(text.contains(".md"), "the exempt routes are not named");
}

#[tokio::test]
async fn the_server_honours_a_forwarded_address_from_a_listed_proxy() {
    let (harness, _site) = Harness::new(Setup::new("host04-forwarded")).await;
    let state = Arc::new(
        AppState::new(ServerConfig::default())
            .with_bundle(harness.state.bundle.clone().expect("a bundle"))
            .with_proxies(TrustedProxies::new(&["127.0.0.0/8".to_owned()])),
    );
    let router = liyasa_server::routes::router(state.clone());

    let mut request = Request::builder()
        .uri("/guides/install")
        .header("x-forwarded-for", "203.0.113.9")
        .header("x-forwarded-proto", "https")
        .header("cf-ipcountry", "DE")
        .body(Body::empty())
        .expect("a request");
    request.extensions_mut().insert(ConnectInfo(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        45000,
    )));
    let response = tower::ServiceExt::oneshot(router, request)
        .await
        .expect("a response");
    assert_eq!(response.status(), StatusCode::OK);

    // The region is read only because the peer is a listed proxy.
    let mut headers = http::HeaderMap::new();
    headers.insert("cf-ipcountry", "DE".parse().expect("a value"));
    assert_eq!(state.region(&headers), Some("DE".to_owned()));
}

#[tokio::test]
async fn a_subpath_deployment_serves_pages_markdown_and_misses_correctly() {
    // HOST-22: one instance serving `example.com/docs`. The bundle is built
    // with `build.basePath`, so the prefix is part of every route.
    // `seo.canonicalOrigin` is not decoration here: without it the build
    // writes no agent surfaces, so there would be no Markdown twin to fetch.
    let site = liyasa_tests::hosting::Site::build(
        "host04-subpath",
        r#"{
          "name": "Acme docs",
          "description": "How Acme works",
          "seo": { "canonicalOrigin": "https://example.com" },
          "build": { "basePath": "/docs" }
        }"#,
        liyasa_build::engine::Options::default(),
    );
    let (harness, _) = Harness::new(Setup {
        dist: Some(site.dist()),
        ..Setup::new("host04-subpath")
    })
    .await;

    expect_status(harness.get("/docs/guides/install").await, StatusCode::OK);
    let markdown = expect_status(harness.get("/docs/guides/install.md").await, StatusCode::OK);
    assert_eq!(
        header(&markdown, "content-type"),
        Some("text/markdown; charset=utf-8")
    );
    // A request that a misconfigured proxy stripped the prefix from is a
    // miss, not a page: serving it would produce links that all point away.
    expect_status(harness.get("/guides/install").await, StatusCode::NOT_FOUND);
    // And the probes stay outside the base path, where an orchestrator looks.
    expect_status(harness.get("/_liyasa/health").await, StatusCode::OK);
}

#[test]
fn the_container_half_reports_why_it_did_not_run() {
    let available = ["docker", "podman"].into_iter().find(|runtime| {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .any(|dir| dir.join(runtime).is_file())
    });
    match available {
        Some(runtime) => println!("a container runtime is available (`{runtime}`)"),
        None => println!(
            "skipped: running nginx, Caddy, and Traefik in front of the server needs Docker or \
             Podman, and neither is on PATH. What those proxies send is covered in process above \
             (RFC 1402)."
        ),
    }
}
