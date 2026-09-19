//! Every subtree router the product mounts (RFC 1403).
//!
//! These assertions are deliberately about `routes::application`, the function
//! the binary calls, and not about a router composed here. WP-15's and WP-16's
//! handlers were each covered by passing tests for days while `liyasa serve`
//! routed neither of them, because both suites composed their own router. A
//! test that builds its own application cannot catch an application that is
//! never built.

use std::sync::Arc;

use axum::body::Body;
use http::Request;
use http::StatusCode;
use liyasa_server::routes::mount::Mount;
use liyasa_server::routes::{self, AppState, ServerConfig};
use liyasa_tests::server::{Harness, Setup, body_json, body_text, expect_status, header};
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
async fn the_router_the_binary_builds_serves_the_org_routes() {
    // WP-28's subtree was complete and unreachable: `org::routes::router`
    // guards its own groups, but nothing registered it, so every org route
    // answered 404 rather than 401. A 404 and a guard are opposite facts with
    // one status code between them, so this asserts the two statuses rather
    // than `!= NOT_FOUND`.
    let (harness, _site) = Harness::serving("mount-org").await;

    let org = harness
        .mounted
        .iter()
        .find(|m| m.name == "org")
        .expect("org is a registered subtree");
    assert!(org.mounted, "org declined to mount: {:?}", org.skipped);

    // HOST-10 publishes the service levels for people deciding whether to
    // buy, so a subtree-level permission here would be a regression.
    expect_status(harness.get("/_liyasa/api/v1/org/slo").await, StatusCode::OK);

    // Guarded today means 401 for everyone, because nothing inserts a
    // `Principal`. That is defect 65 and not this subtree's bug; what this
    // pins is that the guard is the thing answering.
    for path in [
        "/_liyasa/api/v1/org",
        "/_liyasa/api/v1/org/members",
        "/_liyasa/api/v1/org/audit",
    ] {
        expect_status(harness.get(path).await, StatusCode::UNAUTHORIZED);
    }
}

#[tokio::test]
async fn the_auth_subtree_publishes_the_state_its_endpoints_use() {
    // RFC 1403, "One state, two consumers". The session layer is the second
    // consumer of `AuthState` and it must get THE object the endpoints were
    // built from. Two instances is the defect with no status code: sign-in
    // answers 200 and sets a cookie, and every later request resolves it
    // against the other `Sessions` table and arrives anonymous.
    //
    // Both directions are asserted, because one alone passes with two states
    // that happen to be configured alike.
    let (harness, _site) = Harness::new(Setup {
        site_config: Some(with_password_auth()),
        ..Setup::new("mount-auth-state")
    })
    .await;

    let published = harness
        .state
        .auth_state()
        .expect("the auth subtree mounted, so it published its state")
        .clone();

    // Direction one: a write through the published handle is visible to the
    // endpoints. With a second state the sign-in below answers 401, because
    // the endpoints' own `Passwords` never received this.
    published
        .passwords
        .rotate(&published.env, "correct horse battery staple")
        .expect("the password hashes");

    let request = Request::builder()
        .method("POST")
        .uri("/_liyasa/auth/password")
        .header("origin", "https://docs.acme.com")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from("password=correct+horse+battery+staple"))
        .expect("a well-formed request");
    let response = harness.send(request).await;
    assert_ne!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "the endpoint did not see the password set through the published state"
    );

    // Direction two: the session the endpoint minted resolves through the
    // published handle. With a second state this is the anonymous-forever
    // failure exactly.
    let cookie = header(&response, "set-cookie").expect("signing in sets a session cookie");
    let id = cookie
        .split(';')
        .next()
        .and_then(|pair| pair.split_once('='))
        .map(|(_, value)| value.to_owned())
        .expect("the cookie has a value");
    published
        .sessions
        .resolve(&id)
        .expect("the session the endpoint minted is in the published state's table");
}

#[tokio::test]
async fn a_subtree_route_is_counted_like_any_other_request() {
    // `observe` wraps the router, and the subtrees are merged after it, so a
    // request to a subtree route was invisible to metrics and to tracing: no
    // `liyasa_http_requests_total`, no duration sample, no `traceparent` on
    // the response. An operator watching the dashboard's own API would have
    // seen a server with no traffic on it. WP-15 spotted the ordering while
    // reading `application` for somewhere to put the session layer.
    let (harness, _site) = Harness::serving("mount-observe").await;

    let before = body_text(harness.get("/_liyasa/metrics").await).await;
    let response = harness.get("/_liyasa/api/v1/builds").await;
    assert_ne!(
        response.status(),
        StatusCode::NOT_FOUND,
        "the deploy subtree must be mounted for this to measure anything"
    );
    let after = body_text(harness.get("/_liyasa/metrics").await).await;

    let count = |text: &str| {
        text.lines()
            .filter(|line| line.starts_with("liyasa_http_requests_total"))
            .count()
    };
    assert!(
        count(&after) > count(&before) || after != before,
        "a request to a subtree route left no trace in the metrics"
    );
    assert!(
        after.contains("class=\"api\""),
        "the subtree request was not classified: {after}"
    );
}

#[tokio::test]
async fn the_org_subtree_mounts_from_the_state_application_published() {
    // The role source is an input to the auth state, and `auth` is mounted
    // before `org`, so the organization is built before the loop rather than
    // inside it. Reordering `subtrees()` would also have worked and would
    // have made the mount order load-bearing and silent — the class of defect
    // RFC 1403 already records twice.
    let (harness, _site) = Harness::serving("mount-org-state").await;

    let published = harness
        .state
        .org_state()
        .expect("`application` built and published the organization");

    let org = harness
        .mounted
        .iter()
        .find(|m| m.name == "org")
        .expect("org is a registered subtree");
    assert!(org.mounted, "org declined to mount: {:?}", org.skipped);

    // Why publishing matters rather than each consumer calling `org::state`:
    // it is a constructor, so a second call is a second organization. The
    // role source must be built from `published`, never from a fresh call.
    let second = liyasa_server::org::state(&harness.state);
    assert!(
        !Arc::ptr_eq(&second, &published),
        "`org::state` returned the same object twice, so this test no longer \
         proves anything — check what changed before deleting it"
    );

    // A behavioural round-trip — write through the published handle, read it
    // back through the API — is not possible yet: every org write route is
    // guarded and nothing inserts a `Principal`. When defect 65 closes, that
    // assertion belongs here and is stronger than this one.
}

#[test]
fn the_application_does_not_keep_its_own_state_alive() {
    // `OrgState` holds an `Arc<AppState>` (`org/state.rs:38`), so publishing a
    // strong handle to it on `AppState` is a cycle and neither ever drops —
    // no panic, no status code, just a server's worth of state leaked per
    // instance, and a test harness leaking a store handle per case.
    // `AppState.org_state` is a `Weak` for that reason, and this is the
    // assertion that fails if someone makes it strong.
    let state = Arc::new(AppState::new(ServerConfig::default()));
    let weak = Arc::downgrade(&state);
    let application = routes::application(state);
    assert!(
        weak.upgrade().is_some(),
        "the application is still holding the state it was built from"
    );
    drop(application);
    assert!(
        weak.upgrade().is_none(),
        "the application outlived itself: something published a strong \
         handle back to `AppState` and made a reference cycle"
    );
}

#[tokio::test]
async fn the_application_extracts_a_session_before_the_guards_run() {
    // The failure this rules out has no status code of its own: sign-in
    // answers 200 and sets a cookie, and every later request arrives
    // anonymous because the layer was never mounted, or was mounted over a
    // different `AuthState`. The tell is that a guarded route keeps saying
    // 401 to somebody who is signed in.
    //
    // WP-15's own tests compose a router to prove the layer works. This one
    // asserts it is wired into the router the BINARY builds, which is the
    // distinction that let auth and deploy sit unrouted for days.
    let (harness, _site) = Harness::new(Setup {
        site_config: Some(with_password_auth()),
        ..Setup::new("mount-session")
    })
    .await;

    let published = harness
        .state
        .auth_state()
        .expect("auth mounted, so it published its state")
        .clone();
    published
        .passwords
        .rotate(&published.env, "correct horse battery staple")
        .expect("the password hashes");

    // Anonymous: 401, because there is no session to extract.
    expect_status(
        harness.get("/_liyasa/api/v1/org/members").await,
        StatusCode::UNAUTHORIZED,
    );

    let request = Request::builder()
        .method("POST")
        .uri("/_liyasa/auth/password")
        .header("origin", "https://docs.acme.com")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from("password=correct+horse+battery+staple"))
        .expect("a well-formed request");
    let cookie = header(&harness.send(request).await, "set-cookie")
        .expect("signing in sets a session cookie")
        .split(';')
        .next()
        .expect("the cookie pair")
        .to_owned();

    // Signed in: 403, not 401. A `Reader` holds no `DashboardRead`, so the
    // guard still refuses — but it refuses the right way, which is only
    // possible if extraction ran outside it.
    let response = harness
        .get_with("/_liyasa/api/v1/org/members", &[("cookie", &cookie)])
        .await;
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "a signed-in caller is still arriving anonymous: the session layer is \
         not mounted, or not over the state the endpoints use"
    );
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
    assert!(names.contains(&"org"), "{body}");
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
    assert!(names.contains(&"org"));

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
    // Each of today's subtrees is ungated on purpose: signing in cannot
    // require being signed in, deploy authorizes per handler against the
    // request's actor, and org applies three different permissions to four
    // route groups in `org::routes::TABLE`, one of which is HOST-10's public
    // SLA. A future entry that leaves this `None` by accident is the failure
    // this test exists to make someone argue with.
    for subtree in routes::mount::subtrees() {
        match subtree.name {
            "auth" | "deploy" | "org" => assert!(
                subtree.permission.is_none(),
                "`{}` gained a permission; if that is intended, say why here",
                subtree.name
            ),
            other => panic!("`{other}` is registered and this test has not been told about it"),
        }
    }
}
