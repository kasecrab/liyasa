//! Defect 65: the guard and the layer together.
//!
//! `routes::mount::guarded` was correct and unusable — it answers 401 when no
//! `Principal` is in the request extensions, and nothing put one there. So
//! every gated endpoint refused everyone while every ungated one served
//! everyone, from one cause.
//!
//! These tests compose the two halves the way `routes::application` must:
//! `auth::layer::with_session` outside, `mount::guarded` inside. They are the
//! reason to believe gating an endpoint now does something other than break
//! it, and each one names an input that would fail if the layer were wrong.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::routing::get;
use http::{Request, StatusCode};
use liyasa_server::auth::clock::Clock;
use liyasa_server::auth::config::{AuthConfig, Mode};
use liyasa_server::auth::layer::{StaticRoles, with_session};
use liyasa_server::auth::roles::{Permission, Role};
use liyasa_server::auth::session::Principal;
use liyasa_server::auth::state::AuthState;
use liyasa_server::routes::mount::guarded;
use tower::ServiceExt as _;

const COOKIE: &str = "liyasa_session";

/// An instance whose role source knows one administrator, which is what a
/// site with no organization behind it looks like.
fn state() -> Arc<AuthState> {
    let config = AuthConfig {
        mode: Mode::Password,
        ..AuthConfig::default()
    };
    let inner = AuthState::new(
        config,
        "production",
        vec!["https://docs.example.com".to_owned()],
        Clock::manual(),
    )
    .expect("entropy")
    .0
    .with_roles(Arc::new(StaticRoles::new().role("an-admin", Role::Admin)));
    Arc::new(inner)
}

/// One ungated route and one behind `DashboardRead`, composed the way the
/// seam composes a subtree.
fn application(state: Arc<AuthState>) -> Router {
    let public = Router::new().route("/public", get(|| async { "the docs" }));
    let dashboard = guarded(
        Router::new().route("/dashboard", get(|| async { "the panel" })),
        Permission::DashboardRead,
    );
    with_session(public.merge(dashboard), state)
}

async fn get_path(router: &Router, path: &str, headers: &[(&str, String)]) -> (StatusCode, String) {
    let mut builder = Request::builder().method("GET").uri(path);
    for (name, value) in headers {
        builder = builder.header(*name, value);
    }
    let response = router
        .clone()
        .oneshot(builder.body(Body::empty()).expect("a request"))
        .await
        .expect("the router answers");
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("a body");
    (status, String::from_utf8_lossy(&body).into_owned())
}

fn signed_in(state: &AuthState, subject: &str) -> Vec<(&'static str, String)> {
    let session = state
        .sessions
        .begin(Principal::new(subject))
        .expect("a session");
    vec![("cookie", format!("{COOKIE}={}", session.id))]
}

/// The sequencing requirement the packet names: mounting the layer must not
/// turn a disclosure gap into a site-wide 401.
#[tokio::test]
async fn an_anonymous_request_still_reaches_an_ungated_route() {
    let router = application(state());
    let (status, body) = get_path(&router, "/public", &[]).await;
    assert_eq!((status, body.as_str()), (StatusCode::OK, "the docs"));
}

#[tokio::test]
async fn an_anonymous_request_is_told_to_sign_in_rather_than_served() {
    let router = application(state());
    let (status, body) = get_path(&router, "/dashboard", &[]).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(body.contains("Not signed in"), "{body}");
}

/// The half that did not work before: a signed-in caller now gets past the
/// guard instead of the 401 everyone got.
#[tokio::test]
async fn an_administrator_reaches_the_guarded_route() {
    let state = state();
    let router = application(state.clone());
    let (status, body) = get_path(&router, "/dashboard", &signed_in(&state, "an-admin")).await;
    assert_eq!((status, body.as_str()), (StatusCode::OK, "the panel"));
}

/// Signing in is not the same as being allowed, and the two refusals are
/// different so that whoever reads one knows what to do about it.
#[tokio::test]
async fn a_signed_in_reader_is_refused_differently_from_an_anonymous_one() {
    let state = state();
    let router = application(state.clone());
    let (status, body) = get_path(&router, "/dashboard", &signed_in(&state, "a-reader")).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(body.contains("Not permitted"), "{body}");

    let (anonymous, _) = get_path(&router, "/dashboard", &[]).await;
    assert_ne!(
        anonymous, status,
        "`sign in` and `you may not` must not be the same answer"
    );
}

/// AUTH-08: an agent reaches the same routes under the same rules.
#[tokio::test]
async fn an_agents_token_reaches_the_guarded_route_under_the_same_rules() {
    let state = state();
    let router = application(state.clone());
    let issue = |subject: &str| {
        state
            .tokens
            .issue_personal(&Principal::new(subject), "ci", &Default::default(), None)
            .expect("a token")
    };

    let admin = issue("an-admin");
    let (status, body) = get_path(
        &router,
        "/dashboard",
        &[("authorization", format!("Bearer {}", admin.secret))],
    )
    .await;
    assert_eq!((status, body.as_str()), (StatusCode::OK, "the panel"));

    // And a token belonging to someone who may not, may not.
    let reader = issue("a-reader");
    let (status, _) = get_path(
        &router,
        "/dashboard",
        &[("authorization", format!("Bearer {}", reader.secret))],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

/// A revoked credential stops working without the layer having to refuse it:
/// the request simply arrives anonymous and the guard does the rest.
#[tokio::test]
async fn a_revoked_token_is_back_to_being_told_to_sign_in() {
    let state = state();
    let router = application(state.clone());
    let issued = state
        .tokens
        .issue_personal(&Principal::new("an-admin"), "ci", &Default::default(), None)
        .expect("a token");
    let header = [("authorization", format!("Bearer {}", issued.secret))];

    let (before, _) = get_path(&router, "/dashboard", &header).await;
    assert_eq!(before, StatusCode::OK);

    assert!(state.tokens.revoke(&issued.record.id));
    let (after, _) = get_path(&router, "/dashboard", &header).await;
    assert_eq!(after, StatusCode::UNAUTHORIZED);
    // The ungated route is unaffected either way.
    let (public, _) = get_path(&router, "/public", &header).await;
    assert_eq!(public, StatusCode::OK);
}

/// An expired session is not a refusal from the layer. It is anonymity, which
/// the guard then answers — and the ungated route still serves.
#[tokio::test]
async fn an_expired_session_does_not_take_the_public_site_down_with_it() {
    let state = state();
    let router = application(state.clone());
    let headers = signed_in(&state, "an-admin");

    let (before, _) = get_path(&router, "/dashboard", &headers).await;
    assert_eq!(before, StatusCode::OK);

    state
        .sessions
        .clock()
        .advance(state.sessions.policy().max_age);

    let (after, _) = get_path(&router, "/dashboard", &headers).await;
    assert_eq!(after, StatusCode::UNAUTHORIZED);
    let (public, body) = get_path(&router, "/public", &headers).await;
    assert_eq!((public, body.as_str()), (StatusCode::OK, "the docs"));
}

/// Without a role source nobody is elevated, so a guarded surface refuses
/// every signed-in reader. That is the safe direction, and it is why the
/// source is a seam rather than a default.
#[tokio::test]
async fn an_instance_with_no_role_source_gates_shut_rather_than_open() {
    let config = AuthConfig {
        mode: Mode::Password,
        ..AuthConfig::default()
    };
    let state = Arc::new(
        AuthState::new(
            config,
            "production",
            vec!["https://docs.example.com".to_owned()],
            Clock::manual(),
        )
        .expect("entropy")
        .0,
    );
    let router = application(state.clone());
    let (status, _) = get_path(&router, "/dashboard", &signed_in(&state, "an-admin")).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (public, _) = get_path(&router, "/public", &[]).await;
    assert_eq!(public, StatusCode::OK);
}
