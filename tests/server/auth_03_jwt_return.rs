//! AUTH-03's return leg, decided by RFC 1505 and RFC 1506.
//!
//! The operator's application signs a JWT and posts the reader back to
//! `POST /_liyasa/auth/callback`. Until this landed, JWT mode could redirect a
//! reader to the login URL and had no handler that accepted them back, so the
//! mode could not complete a sign-in at all.
//!
//! Two of these tests are about what the endpoint *refuses*, and they matter
//! more than the one about what it accepts. A token in a query string reaches
//! every proxy access log; an unchecked origin turns the endpoint into login
//! CSRF, where an attacker's token signs a reader into the attacker's
//! identity and nothing looks broken afterwards.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use http::{Request, StatusCode, header};
use liyasa_server::auth::base64url;
use liyasa_server::auth::clock::Clock;
use liyasa_server::auth::config::{AuthConfig, JwtConfig, Mode};
use liyasa_server::auth::jwks::{Jwk, JwkSet};
use liyasa_server::auth::state::AuthState;
use tower::ServiceExt as _;

const ORIGIN: &str = "https://docs.acme.com";
const LOGIN: &str = "https://login.acme.com/sign-in?product=docs";
const LOGIN_ORIGIN: &str = "https://login.acme.com";
const SECRET: &[u8] = b"a shared secret long enough for HMAC-SHA256";

fn instance() -> (Router, Arc<AuthState>) {
    let clock = Clock::manual();
    let config = AuthConfig {
        mode: Mode::Jwt,
        jwt: JwtConfig {
            algs: vec!["HS256".to_owned()],
            iss: Some("https://idp.acme.com".to_owned()),
            aud: Some(ORIGIN.to_owned()),
            login_url: Some(LOGIN.to_owned()),
            ..JwtConfig::default()
        },
        ..AuthConfig::default()
    };
    let state = AuthState::new(config, "production", vec![ORIGIN.to_owned()], clock)
        .expect("entropy")
        .0;
    state.jwks.install(JwkSet {
        keys: vec![Jwk {
            kid: "k1".to_owned(),
            kty: "oct".to_owned(),
            alg: Some("HS256".to_owned()),
            k: Some(base64url::encode(SECRET)),
            ..Jwk::default()
        }],
    });
    let state = Arc::new(state);
    (liyasa_server::auth::routes::router(state.clone()), state)
}

/// Mints a token the way the operator's application would.
fn token(state: &AuthState, claims: serde_json::Value) -> String {
    let _ = state;
    let header = serde_json::json!({ "alg": "HS256", "kid": "k1", "typ": "JWT" });
    let header_b64 = base64url::encode(header.to_string().as_bytes());
    let claims_b64 = base64url::encode(claims.to_string().as_bytes());
    let signed = format!("{header_b64}.{claims_b64}");
    let tag = ring::hmac::sign(
        &ring::hmac::Key::new(ring::hmac::HMAC_SHA256, SECRET),
        signed.as_bytes(),
    );
    format!("{signed}.{}", base64url::encode(tag.as_ref()))
}

fn good_claims(now: i64) -> serde_json::Value {
    serde_json::json!({
        "iss": "https://idp.acme.com",
        "aud": ORIGIN,
        "sub": "reader-1",
        "exp": now + 3_600,
        "nbf": now - 10,
        "groups": ["partner"],
    })
}

async fn post(router: &Router, path: &str, origin: &str, body: String) -> http::Response<Body> {
    let request = Request::post(path)
        .header(header::ORIGIN, origin)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .expect("a request");
    router.clone().oneshot(request).await.expect("a response")
}

fn signed_in(response: &http::Response<Body>) -> bool {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .any(|value| {
            value
                .to_str()
                .unwrap_or_default()
                .contains("liyasa_session")
        })
}

#[tokio::test]
async fn a_token_posted_from_the_login_application_signs_the_reader_in() {
    let (router, state) = instance();
    let token = token(&state, good_claims(state.clock.now_secs()));
    let response = post(
        &router,
        "/_liyasa/auth/callback",
        LOGIN_ORIGIN,
        format!("token={token}"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER, "{:?}", response);
    assert!(signed_in(&response), "a session cookie is set");
}

/// RFC 1506: the origin of `auth.jwt.loginUrl` is accepted because the
/// operator named it, and the path on that URL is not part of an `Origin`.
#[tokio::test]
async fn the_login_url_contributes_its_origin_and_not_its_path() {
    let (router, state) = instance();
    let token = token(&state, good_claims(state.clock.now_secs()));
    // The same origin with a different path is still that origin.
    let response = post(
        &router,
        "/_liyasa/auth/callback",
        "https://login.acme.com",
        format!("token={token}"),
    )
    .await;
    assert!(signed_in(&response), "{:?}", response.status());
}

/// Login CSRF: an attacker's valid token, posted from an attacker's page,
/// would sign the reader into the attacker's identity.
#[tokio::test]
async fn a_token_posted_from_anywhere_else_is_refused() {
    let (router, state) = instance();
    let token = token(&state, good_claims(state.clock.now_secs()));
    let response = post(
        &router,
        "/_liyasa/auth/callback",
        "https://attacker.example",
        format!("token={token}"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(!signed_in(&response));
}

/// **The regression test RFC 1506 exists for.** `AuthState.origins` is one
/// list shared by `guard` across the password, magic and logout endpoints. If
/// the login origin were pushed into it rather than built per-endpoint, this
/// would start passing a cross-site password submission — and nothing at the
/// line that widened it would say so.
#[tokio::test]
async fn the_login_origin_does_not_become_an_accepted_origin_for_every_endpoint() {
    let (router, _state) = instance();
    for (path, body) in [
        ("/_liyasa/auth/password", "password=hunter2".to_owned()),
        ("/_liyasa/auth/magic", "email=reader@acme.com".to_owned()),
        ("/_liyasa/auth/logout", String::new()),
    ] {
        let response = post(&router, path, LOGIN_ORIGIN, body).await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{path} must not accept the login application's origin"
        );
    }
}

/// RFC 1505: refused, and the refusal says why. Accommodating the guess would
/// mean the credential is in the operator's proxy log either way.
#[tokio::test]
async fn a_token_in_the_query_string_is_refused_with_a_reason() {
    let (router, state) = instance();
    let token = token(&state, good_claims(state.clock.now_secs()));
    let request = Request::get(format!("/_liyasa/auth/callback?token={token}"))
        .body(Body::empty())
        .expect("a request");
    let response = router.oneshot(request).await.expect("a response");
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert!(!signed_in(&response));
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .expect("a body");
    let text = String::from_utf8_lossy(&body);
    assert!(text.contains("POST"), "{text}");
    assert!(
        !text.contains(&token),
        "the refusal must not repeat the credential: {text}"
    );
}

/// The reader is told the same sentence whichever check failed. `jwt::Invalid`
/// carries a precise reason for the operator's log; putting it in the response
/// would answer "which of my guesses was closest" for anyone probing.
#[tokio::test]
async fn every_rejected_token_gets_the_same_answer() {
    let (router, state) = instance();
    let now = state.clock.now_secs();
    let mut bodies = Vec::new();
    for claims in [
        serde_json::json!({ "iss": "https://elsewhere.example", "aud": ORIGIN,
                            "sub": "r", "exp": now + 60, "nbf": now - 10 }),
        serde_json::json!({ "iss": "https://idp.acme.com", "aud": "https://other.example",
                            "sub": "r", "exp": now + 60, "nbf": now - 10 }),
        serde_json::json!({ "iss": "https://idp.acme.com", "aud": ORIGIN,
                            "sub": "r", "exp": now - 60, "nbf": now - 120 }),
        serde_json::json!({ "iss": "https://idp.acme.com", "aud": ORIGIN,
                            "exp": now + 60, "nbf": now - 10 }),
    ] {
        let token = token(&state, claims);
        let response = post(
            &router,
            "/_liyasa/auth/callback",
            LOGIN_ORIGIN,
            format!("token={token}"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(!signed_in(&response));
        let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("a body");
        bodies.push(String::from_utf8_lossy(&body).into_owned());
    }
    let first = bodies.first().expect("four bodies").clone();
    for body in &bodies {
        assert_eq!(
            body, &first,
            "a wrong issuer and an expired token read alike"
        );
    }
}

/// The groups in the token are the groups the session carries — AUTH-03 says
/// the payload is where they come from, and AUTH-10 reads them from there.
#[tokio::test]
async fn the_claims_reach_the_session() {
    let (router, state) = instance();
    let token = token(&state, good_claims(state.clock.now_secs()));
    let response = post(
        &router,
        "/_liyasa/auth/callback",
        LOGIN_ORIGIN,
        format!("token={token}"),
    )
    .await;
    let cookie = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find_map(|value| value.strip_prefix("liyasa_session="))
        .map(|rest| rest.split(';').next().unwrap_or_default().to_owned())
        .expect("a session cookie");
    let session = state
        .sessions
        .resolve(&cookie)
        .expect("the session resolves");
    assert_eq!(session.principal.subject, "reader-1");
    assert!(
        session.principal.groups.contains("partner") && session.principal.groups.len() == 1,
        "{:?}",
        session.principal.groups
    );
    assert_eq!(session.principal.via, "jwt");
}

/// A site that does not sign in with a JWT has no return leg, and says 404
/// rather than refusing on origin — the endpoint is not there for it.
#[tokio::test]
async fn a_password_site_has_no_jwt_return_leg() {
    let config = AuthConfig {
        mode: Mode::Password,
        ..AuthConfig::default()
    };
    let state = AuthState::new(
        config,
        "production",
        vec![ORIGIN.to_owned()],
        Clock::manual(),
    )
    .expect("entropy")
    .0;
    let router = liyasa_server::auth::routes::router(Arc::new(state));
    let response = post(
        &router,
        "/_liyasa/auth/callback",
        ORIGIN,
        "token=x".to_owned(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
