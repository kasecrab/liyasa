//! Every subtree router the product mounts (RFC 1403).
//!
//! These assertions are deliberately about `routes::application`, the function
//! the binary calls, and not about a router composed here. WP-15's and WP-16's
//! handlers were each covered by passing tests for days while `liyasa serve`
//! routed neither of them, because both suites composed their own router. A
//! test that builds its own application cannot catch an application that is
//! never built.

use std::sync::Arc;

use http::StatusCode;
use liyasa_server::routes::mount::Mount;
use liyasa_server::routes::{self, AppState, ServerConfig};
use liyasa_tests::server::{Harness, Setup, body_json, expect_status};
use serde_json::json;

/// A configuration with auth turned on, so the auth subtree has something to
/// mount. `password` is the mode with the fewest moving parts, and its
/// section takes its defaults: `PasswordConfig` carries only `argon2` and
/// denies unknown fields, so inventing a key here is a skipped mount rather
/// than a test failure that names itself.
fn with_password_auth() -> serde_json::Value {
    json!({
        "name": "Acme docs",
        "seo": { "canonicalOrigin": "https://docs.acme.com" },
        "auth": { "mode": "password" }
    })
}

#[tokio::test]
async fn the_router_the_binary_builds_serves_the_deploy_routes() {
    // A store is configured, so the deploy subtree mounts.
    let (harness, _site) = Harness::serving("mount-deploy").await;

    for (method, path) in [
        ("GET", "/_liyasa/hooks"),
        ("GET", "/_liyasa/api/v1/builds"),
        ("GET", "/_liyasa/api/v1/deployments/production/history"),
    ] {
        let response = harness.request(method, path).await;
        assert_ne!(
            response.status(),
            StatusCode::NOT_FOUND,
            "{method} {path} is not routed by the application the binary builds"
        );
    }
}

#[tokio::test]
async fn the_router_the_binary_builds_serves_the_auth_routes() {
    let (harness, _site) = Harness::new(Setup {
        site_config: Some(with_password_auth()),
        ..Setup::new("mount-auth")
    })
    .await;

    // Assert the mount first: a 404 below could mean the subtree declined, and
    // "auth is not routed" would be the wrong thing to go and investigate.
    let auth = harness
        .mounted
        .iter()
        .find(|m| m.name == "auth")
        .expect("auth is a known subtree");
    assert!(auth.mounted, "auth declined to mount: {:?}", auth.skipped);

    for (method, path) in [
        ("GET", "/_liyasa/auth/session"),
        ("POST", "/_liyasa/auth/password"),
        ("POST", "/_liyasa/auth/logout"),
    ] {
        let response = harness.request(method, path).await;
        assert_ne!(
            response.status(),
            StatusCode::NOT_FOUND,
            "{method} {path} is not routed by the application the binary builds"
        );
    }
}

#[tokio::test]
async fn a_public_site_mounts_no_auth_routes_and_says_why() {
    // AUTH-01: a public site has no auth code path, and that includes routes
    // that answer "you are not signed in".
    let (harness, _site) = Harness::serving("mount-public").await;
    expect_status(
        harness.get("/_liyasa/auth/session").await,
        StatusCode::NOT_FOUND,
    );

    let auth = harness
        .mounted
        .iter()
        .find(|m| m.name == "auth")
        .expect("auth is a known subtree even when it mounts nothing");
    let reason = auth
        .skipped
        .as_deref()
        .expect("a subtree that mounts nothing says why");
    assert!(
        reason.contains("public"),
        "the reason must be one an operator can act on, got `{reason}`"
    );
}

#[tokio::test]
async fn a_server_with_no_store_mounts_no_deploy_routes_and_says_why() {
    let (harness, _site) = Harness::new(Setup {
        with_store: false,
        ..Setup::new("mount-nostore")
    })
    .await;
    expect_status(
        harness.get("/_liyasa/api/v1/builds").await,
        StatusCode::NOT_FOUND,
    );

    let deploy = harness
        .mounted
        .iter()
        .find(|m| m.name == "deploy")
        .expect("deploy is a known subtree even when it mounts nothing");
    assert!(
        deploy
            .skipped
            .as_deref()
            .is_some_and(|reason| reason.contains("store")),
        "{:?}",
        deploy.skipped
    );
}

#[tokio::test]
async fn readiness_reports_what_was_mounted() {
    // An operator whose login 404s needs one place that says the server chose
    // not to mount it, rather than a silent absence (RFC 1403).
    let (harness, _site) = Harness::serving("mount-ready").await;
    let body = body_json(expect_status(
        harness.get("/_liyasa/ready").await,
        StatusCode::OK,
    ))
    .await;
    let subtrees = body["subtrees"]
        .as_array()
        .expect("readiness lists the subtrees");
    let names: Vec<&str> = subtrees.iter().filter_map(|s| s["name"].as_str()).collect();
    assert!(names.contains(&"auth"), "{body}");
    assert!(names.contains(&"deploy"), "{body}");
    let auth = subtrees
        .iter()
        .find(|s| s["name"] == "auth")
        .expect("the auth row");
    assert_eq!(auth["mounted"], false);
    assert!(
        auth["skipped"]
            .as_str()
            .is_some_and(|r| r.contains("public"))
    );
}

#[tokio::test]
async fn every_registered_subtree_is_asked_exactly_once() {
    // The list is what a new package appends to; a duplicate entry would mount
    // one subtree's routes twice, which axum answers with a panic on a route
    // that is already registered.
    let names: Vec<&str> = routes::mount::subtrees().iter().map(|s| s.name).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(
        names.len(),
        sorted.len(),
        "a subtree is registered twice: {names:?}"
    );
    assert!(names.contains(&"auth"));
    assert!(names.contains(&"deploy"));

    let state = Arc::new(AppState::new(ServerConfig::default()));
    let application = routes::application(state);
    assert_eq!(
        application.mounted.len(),
        names.len(),
        "every registered subtree reports whether it mounted"
    );
    for record in &application.mounted {
        assert_eq!(
            record.skipped.is_some(),
            !record.mounted,
            "`{}` must say why it did not mount, and say nothing when it did",
            record.name
        );
    }
}

#[test]
fn a_mount_carries_a_reason_exactly_when_it_has_no_router() {
    let skipped = Mount::skipped("no store is configured");
    assert!(skipped.router.is_none());
    assert_eq!(skipped.skipped.as_deref(), Some("no store is configured"));

    let mounted = Mount::routes(axum::Router::new());
    assert!(mounted.router.is_some());
    assert!(mounted.skipped.is_none());
}

#[tokio::test]
async fn a_guarded_subtree_tells_an_anonymous_caller_to_sign_in() {
    // The guard is what lets a subtree outside this crate carry a permission
    // at all: `Permission` lives here, and `liyasa-server` depends on the
    // crates the subtrees live in, so they cannot name one (RFC 1403).
    use axum::routing::get;
    use liyasa_server::auth::roles::Permission;
    use liyasa_server::routes::mount::guarded;

    let router = guarded(
        axum::Router::new().route("/_liyasa/api/v1/insights", get(|| async { "secret" })),
        Permission::DashboardRead,
    );
    let response = tower::ServiceExt::oneshot(
        router,
        http::Request::builder()
            .uri("/_liyasa/api/v1/insights")
            .body(axum::body::Body::empty())
            .expect("a request"),
    )
    .await
    .expect("a response");

    // Not 403: nobody is signed in, and telling an anonymous caller they lack
    // a permission sends them looking for the wrong thing.
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let problem = body_json(response).await;
    assert_eq!(problem["status"], 401);
    assert!(
        problem["detail"]
            .as_str()
            .is_some_and(|d| d.contains("session")),
        "{problem}"
    );
}

#[test]
fn a_subtree_that_declares_no_permission_is_saying_something_deliberate() {
    // Both of today's subtrees are ungated on purpose: signing in cannot
    // require being signed in, and deploy authorizes per handler against the
    // request's actor. A future entry that leaves this `None` by accident is
    // the failure this test exists to make someone argue with.
    for subtree in routes::mount::subtrees() {
        match subtree.name {
            "auth" | "deploy" => assert!(
                subtree.permission.is_none(),
                "`{}` gained a permission; if that is intended, say why here",
                subtree.name
            ),
            other => panic!("`{other}` is registered and this test has not been told about it"),
        }
    }
}
