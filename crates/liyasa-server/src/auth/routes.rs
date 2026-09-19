//! The auth endpoint table (AUTH-09, AUTH-22).
//!
//! ```text
//! GET  /_liyasa/auth/login          starts the configured flow
//! GET  /_liyasa/auth/callback       OIDC and JWT return; validates state and PKCE
//! POST /_liyasa/auth/password       password mode; Argon2id verify; rate limited
//! POST /_liyasa/auth/magic          platform-managed; single-use link, 15 minutes
//! GET  /_liyasa/auth/magic/{token}  consumes the link
//! POST /_liyasa/auth/logout         invalidates server-side
//! GET  /_liyasa/auth/session        introspection for the theme
//! ```
//!
//! On a public site nothing here is mounted at all (AUTH-01): [`router`]
//! returns an empty router, so `/_liyasa/auth/login` is a 404 exactly like any
//! other path the site does not have.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use http::{HeaderMap, Method, StatusCode, header};
use serde_json::json;

use crate::auth::config::Mode;
use crate::auth::cookie::{self, Cookie};
use crate::auth::csrf;
use crate::auth::jwt;
use crate::auth::magic::Consumed;
use crate::auth::oidc;
use crate::auth::password::Outcome;
use crate::auth::session::{Principal, Rejected};
use crate::auth::state::AuthState;
use crate::routes::problem::Problem;

/// The cookie the magic-link nonce is carried in.
pub const NONCE_COOKIE: &str = "liyasa_magic";
/// The cookie an OIDC flow's `state` is bound to, so a callback cannot be
/// replayed from another browser.
pub const STATE_COOKIE: &str = "liyasa_oidc";

pub fn router(state: Arc<AuthState>) -> Router {
    // AUTH-01: a public site has no auth code path, and that includes routes
    // that answer "you are not signed in".
    if state.config.mode.is_public() {
        return Router::new();
    }
    Router::new()
        .route("/_liyasa/auth/login", get(login))
        // AUTH-03 returns by POST and OIDC by GET, on one path because
        // AUTH-09's table names one endpoint (RFC 1505).
        .route("/_liyasa/auth/callback", get(callback).post(jwt_callback))
        .route("/_liyasa/auth/password", post(password))
        .route("/_liyasa/auth/magic", post(magic_request))
        .route("/_liyasa/auth/magic/{token}", get(magic_consume))
        .route("/_liyasa/auth/logout", post(logout))
        .route("/_liyasa/auth/session", get(session))
        .with_state(state)
}

/// Every state-changing endpoint runs this first (AUTH-09).
fn guard(
    state: &AuthState,
    method: &Method,
    headers: &HeaderMap,
    form: &[(String, String)],
) -> Option<Response> {
    if !csrf::is_state_changing(method) {
        return None;
    }
    let session_token = current(state, headers).map(|session| session.csrf);
    let supplied = csrf::supplied(headers, form);
    match csrf::check(headers, &state.origins, session_token.as_deref(), supplied) {
        Ok(()) => None,
        Err(refusal) => Some(
            Problem::new(StatusCode::FORBIDDEN, "Cross-site request refused")
                .detail(refusal.detail())
                .into_response(),
        ),
    }
}

fn current(state: &AuthState, headers: &HeaderMap) -> Option<crate::auth::session::Session> {
    let id = cookie::read(headers, &state.config.session.cookie_name)?;
    state.sessions.resolve(id).ok()
}

fn client_ip(state: &AuthState, headers: &HeaderMap, peer: Option<SocketAddr>) -> IpAddr {
    let peer = peer
        .map(|addr| addr.ip())
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    state.proxies.client_ip(peer, headers)
}

/// Signs a principal in: a fresh session, its cookie, and the CSRF token the
/// theme will echo.
fn sign_in(state: &AuthState, principal: Principal, redirect: Option<&str>) -> Response {
    let Ok(session) = state.sessions.begin(principal) else {
        return Problem::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "No session could be created",
        )
        .into_response();
    };
    let mut response = match redirect {
        Some(to) => (
            StatusCode::SEE_OTHER,
            [(header::LOCATION, oidc::safe_return_to(to))],
        )
            .into_response(),
        None => (StatusCode::OK, axum::Json(json!({ "csrf": session.csrf }))).into_response(),
    };
    Cookie::new(&state.config.session.cookie_name, &session.id)
        .max_age(state.sessions.policy().max_age)
        .append_to(response.headers_mut());
    // The one-off cookies have done their job.
    Cookie::cleared(NONCE_COOKIE).append_to(response.headers_mut());
    Cookie::cleared(STATE_COOKIE).append_to(response.headers_mut());
    response
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct LoginParams {
    #[serde(default, rename = "returnTo", alias = "return_to")]
    pub return_to: Option<String>,
}

/// `GET /_liyasa/auth/login`.
async fn login(State(state): State<Arc<AuthState>>, Query(params): Query<LoginParams>) -> Response {
    let return_to = params.return_to.as_deref().unwrap_or("/");
    match state.config.mode {
        Mode::Public => StatusCode::NOT_FOUND.into_response(),
        Mode::Jwt => match &state.config.jwt.login_url {
            // AUTH-03: the reader goes to the operator's login URL and comes
            // back with a token.
            Some(url) => redirect_to(url),
            None => Problem::new(StatusCode::SERVICE_UNAVAILABLE, "No login URL configured")
                .detail("`auth.jwt.loginUrl` names where readers sign in")
                .into_response(),
        },
        Mode::Oidc => {
            let Some(flow) = state.oidc.as_ref() else {
                return Problem::new(StatusCode::SERVICE_UNAVAILABLE, "No provider configured")
                    .into_response();
            };
            // Discovery is lazy: `contribute` is not async, and a provider
            // that was down at startup would otherwise leave sign-in broken
            // until a restart. The first reader after it recovers pays for it.
            if let Err(error) = state.discover_if_needed().await {
                return Problem::new(StatusCode::SERVICE_UNAVAILABLE, "No provider endpoints")
                    .detail(error)
                    .into_response();
            }
            match flow.begin(return_to) {
                Ok(started) => {
                    let mut response = redirect_to(&started.authorize_url);
                    Cookie::new(STATE_COOKIE, &started.state)
                        .strict()
                        .max_age(oidc::FLOW_TTL)
                        .append_to(response.headers_mut());
                    response
                }
                Err(_) => {
                    Problem::new(StatusCode::SERVICE_UNAVAILABLE, "No provider endpoints")
                        .detail("OIDC discovery has not completed for this issuer")
                        .into_response()
                }
            }
        }
        // Password and managed mode are forms the theme renders; there is
        // nowhere to send the reader.
        Mode::Password | Mode::Managed => (
            StatusCode::OK,
            axum::Json(json!({ "mode": state.config.mode.as_str(), "returnTo": oidc::safe_return_to(return_to) })),
        )
            .into_response(),
    }
}

fn redirect_to(url: &str) -> Response {
    match http::HeaderValue::from_str(url) {
        Ok(value) => (StatusCode::SEE_OTHER, [(header::LOCATION, value)]).into_response(),
        Err(_) => Problem::new(StatusCode::INTERNAL_SERVER_ERROR, "Bad redirect").into_response(),
    }
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct CallbackParams {
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    /// Only ever read to refuse it. See [`callback`].
    #[serde(default)]
    pub token: Option<String>,
}

/// `GET /_liyasa/auth/callback`.
async fn callback(
    State(state): State<Arc<AuthState>>,
    Query(params): Query<CallbackParams>,
    headers: HeaderMap,
) -> Response {
    // RFC 1505: an operator who built against a guess is told, not silently
    // accommodated — accommodating it means the token is in their proxy's
    // access log either way.
    if params.token.is_some() {
        return Problem::new(
            StatusCode::METHOD_NOT_ALLOWED,
            "A token is returned by POST",
        )
        .detail(
            "a token in the query string reaches every proxy access log, the `Referer` of \
                 the landing page and browser history; POST it as `token` in a form body instead",
        )
        .into_response();
    }
    if let Some(error) = params.error {
        return Problem::new(StatusCode::BAD_REQUEST, "The provider refused the sign-in")
            .detail(error)
            .into_response();
    }
    let (Some(code), Some(returned_state)) = (params.code, params.state) else {
        return Problem::bad_request("`code` and `state` are both required").into_response();
    };
    // The `state` must also be the one this browser was given: a callback
    // replayed elsewhere carries the parameter but not the cookie.
    let held = cookie::read(&headers, STATE_COOKIE);
    if held != Some(returned_state.as_str()) {
        return Problem::new(StatusCode::BAD_REQUEST, "This sign-in did not start here")
            .detail("the `state` does not match the one this browser was issued")
            .into_response();
    }
    let (Some(flow), Some(exchange)) = (state.oidc.as_ref(), state.exchange.as_deref()) else {
        return Problem::new(StatusCode::SERVICE_UNAVAILABLE, "No provider configured")
            .into_response();
    };
    match flow.finish(&returned_state, &code, exchange).await {
        Ok((principal, return_to)) => sign_in(&state, principal, Some(&return_to)),
        Err(refused) => Problem::new(
            StatusCode::BAD_REQUEST,
            "The sign-in could not be completed",
        )
        .detail(format!("{refused:?}"))
        .into_response(),
    }
}

/// `POST /_liyasa/auth/callback` — AUTH-03's return leg (RFC 1505).
///
/// The operator's login application signs a JWT and posts the reader back
/// here. POST rather than a query parameter because the token is a bearer
/// credential and a query string reaches every proxy access log, the landing
/// page's `Referer` and browser history — one of our own four deployment
/// guides enables Traefik's default access log, which records the full URI.
async fn jwt_callback(
    State(state): State<Arc<AuthState>>,
    request: axum::extract::Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let headers = parts.headers.clone();
    let form = parse_body(&headers, &read_body(body).await);

    if state.config.mode != Mode::Jwt {
        return Problem::new(
            StatusCode::NOT_FOUND,
            "This site does not sign in with a JWT",
        )
        .into_response();
    }

    // TODO(rfc-1506): the accepted origins are `auth.jwt.loginUrl`'s plus this
    // site's own, and the slice is built HERE rather than pushed into
    // `state.origins`. That list is shared by `guard` across the password,
    // magic and logout endpoints; widening it would accept a cross-site
    // password submission and a cross-site logout from the operator's login
    // application, which is not what RFC 1505 decided and would be invisible
    // at the line that did it.
    let origins = return_origins(&state);
    if let Err(refused) = csrf::check(&headers, &origins, None, csrf::supplied(&headers, &form)) {
        return Problem::new(
            StatusCode::FORBIDDEN,
            "This sign-in did not come from your login",
        )
        .detail(refused.detail())
        .into_response();
    }

    let Some(token) = field(&form, "token") else {
        return Problem::bad_request("`token` is required").into_response();
    };
    let now = state.clock.now_secs();
    match jwt::verify(
        &token,
        &state.config.jwt,
        &state.jwks,
        &*state.jwks_source(),
        now,
    )
    .await
    {
        Ok(verified) => {
            let principal = verified.principal(&state.config.jwt);
            sign_in(
                &state,
                principal,
                Some(&oidc::safe_return_to(
                    field(&form, "returnTo").as_deref().unwrap_or("/"),
                )),
            )
        }
        // `Invalid::detail` is the operator's, not the reader's: it names which
        // check failed, which is a verification oracle for anyone probing with
        // crafted tokens. `reader_message` is the same sentence for every
        // variant, by design in `jwt`.
        Err(invalid) => {
            tracing::warn!(
                target: "liyasa_server",
                detail = %invalid.detail(),
                "a returned JWT was rejected"
            );
            Problem::new(StatusCode::UNAUTHORIZED, invalid.reader_message()).into_response()
        }
    }
}

/// The origins `POST /_liyasa/auth/callback` accepts, and only that endpoint
/// (RFC 1506).
fn return_origins(state: &AuthState) -> Vec<String> {
    let mut origins = state.origins.clone();
    if let Some(login) = state.config.jwt.login_url.as_deref()
        && let Some(origin) = origin_of(login)
    {
        origins.push(origin);
    }
    origins
}

/// Scheme, host and port of a URL, with the path discarded. An `Origin` header
/// has no path, so comparing against a configured URL means comparing against
/// its origin.
fn origin_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    if rest.is_empty() {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    match authority.is_empty() {
        true => None,
        false => Some(format!("{scheme}://{authority}")),
    }
}

/// `POST /_liyasa/auth/password`.
async fn password(
    State(state): State<Arc<AuthState>>,
    request: axum::extract::Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let headers = parts.headers.clone();
    let form = parse_body(&headers, &read_body(body).await);
    if let Some(refused) = guard(&state, &parts.method, &headers, &form) {
        return refused;
    }
    let Some(supplied) = field(&form, "password") else {
        return Problem::bad_request("`password` is required").into_response();
    };
    let peer = parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| *addr);
    let address = client_ip(&state, &headers, peer).to_string();
    match state.passwords.check(&state.env, &address, &supplied) {
        Outcome::Correct => {
            state.passwords.upgrade(&state.env, &supplied);
            // Everyone who knows the site password arrives as this one
            // subject, so it names a flow rather than a person and no role
            // source may resolve it (AUTH-02).
            let principal = Principal::new(format!("password:{}", state.env))
                .with_via("password")
                .shared();
            sign_in(&state, principal, field(&form, "returnTo").as_deref())
        }
        Outcome::Wrong => {
            Problem::new(StatusCode::UNAUTHORIZED, "That password is not correct").into_response()
        }
        Outcome::RateLimited => Problem::new(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many attempts; wait a minute and try again",
        )
        .into_response(),
    }
}

/// `POST /_liyasa/auth/magic`.
async fn magic_request(
    State(state): State<Arc<AuthState>>,
    request: axum::extract::Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let headers = parts.headers.clone();
    let form = parse_body(&headers, &read_body(body).await);
    if let Some(refused) = guard(&state, &parts.method, &headers, &form) {
        return refused;
    }
    let Some(address) = field(&form, "email") else {
        return Problem::bad_request("`email` is required").into_response();
    };
    let Ok(requested) = state.magic.request(&address) else {
        return Problem::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "No link could be created",
        )
        .into_response();
    };
    if let (Some(token), Some(mail)) = (&requested.token, state.mail.as_deref()) {
        mail.send_link(&address, token);
    }
    // AUTH-09: identical whether or not the address is known, and identical
    // whether or not it was rate limited.
    let mut response = (
        StatusCode::ACCEPTED,
        axum::Json(json!({
            "sent": true,
            "message": "If that address can sign in here, a link is on its way. It is good for 15 minutes and only in this browser."
        })),
    )
        .into_response();
    Cookie::new(NONCE_COOKIE, &requested.nonce)
        .strict()
        .max_age(state.config.managed.ttl())
        .append_to(response.headers_mut());
    response
}

/// `GET /_liyasa/auth/magic/{token}`.
async fn magic_consume(
    State(state): State<Arc<AuthState>>,
    Path(token): Path<String>,
    headers: HeaderMap,
) -> Response {
    let nonce = cookie::read(&headers, NONCE_COOKIE);
    match state.magic.consume(&token, nonce) {
        Consumed::SignedIn(reader) => sign_in(&state, reader.principal(), Some("/")),
        other => (
            StatusCode::UNAUTHORIZED,
            axum::Json(json!({
                "message": other.message(),
                "canResend": other.offers_resend(),
                "resendAt": "/_liyasa/auth/magic",
            })),
        )
            .into_response(),
    }
}

/// `POST /_liyasa/auth/logout` (AUTH-22).
async fn logout(State(state): State<Arc<AuthState>>, request: axum::extract::Request) -> Response {
    let (parts, body) = request.into_parts();
    let headers = parts.headers.clone();
    let form = parse_body(&headers, &read_body(body).await);
    if let Some(refused) = guard(&state, &parts.method, &headers, &form) {
        return refused;
    }
    if let Some(id) = cookie::read(&headers, &state.config.session.cookie_name) {
        state.sessions.invalidate(id);
    }
    let mut response = match &state.config.logout.redirect {
        Some(to) => redirect_to(&oidc::safe_return_to(to)),
        None => StatusCode::NO_CONTENT.into_response(),
    };
    Cookie::cleared(&state.config.session.cookie_name).append_to(response.headers_mut());
    response
}

/// `GET /_liyasa/auth/session`: what the theme needs for the "logged in as"
/// indicator (AUTH-22) and for hiding what the reader cannot reach.
async fn session(State(state): State<Arc<AuthState>>, headers: HeaderMap) -> Response {
    let id = cookie::read(&headers, &state.config.session.cookie_name);
    let resolved = id.map(|id| state.sessions.resolve(id));
    let body = match resolved {
        Some(Ok(session)) => json!({
            "authenticated": true,
            "subject": session.principal.subject,
            "name": session.principal.data.get("name"),
            "groups": session.principal.groups,
            "role": session.principal.role,
            "via": session.principal.via,
            "region": session.principal.region,
            "locale": session.principal.locale,
            "csrf": session.csrf,
            "mode": state.config.mode.as_str(),
        }),
        Some(Err(Rejected::Expired)) | Some(Err(Rejected::Idle)) => json!({
            "authenticated": false,
            "expired": true,
            "mode": state.config.mode.as_str(),
        }),
        _ => json!({ "authenticated": false, "mode": state.config.mode.as_str() }),
    };
    let mut response = (StatusCode::OK, axum::Json(body)).into_response();
    // Never shared: this is one reader's answer.
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        http::HeaderValue::from_static("private, no-store"),
    );
    response
}

/// A bounded read: an auth form is a few hundred bytes and a body larger than
/// this is not one.
async fn read_body(body: axum::body::Body) -> Vec<u8> {
    axum::body::to_bytes(body, 64 * 1024)
        .await
        .map(|bytes| bytes.to_vec())
        .unwrap_or_default()
}

/// Reads a body as JSON or as `application/x-www-form-urlencoded`, so the
/// theme's no-JavaScript form and its fetch both work.
pub fn parse_body(headers: &HeaderMap, body: &[u8]) -> Vec<(String, String)> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if content_type.starts_with("application/json") {
        let Ok(serde_json::Value::Object(fields)) = serde_json::from_slice(body) else {
            return Vec::new();
        };
        return fields
            .into_iter()
            .map(|(key, value)| {
                let value = match value {
                    serde_json::Value::String(text) => text,
                    other => other.to_string(),
                };
                (key, value)
            })
            .collect();
    }
    String::from_utf8_lossy(body)
        .split('&')
        .filter(|pair| !pair.is_empty())
        .filter_map(|pair| pair.split_once('='))
        .map(|(key, value)| (unescape(key), unescape(value)))
        .collect()
}

fn field(form: &[(String, String)], name: &str) -> Option<String> {
    form.iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.clone())
        .filter(|value| !value.is_empty())
}

fn unescape(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        out.push(byte);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_form_body_is_read_and_percent_decoded() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            "application/x-www-form-urlencoded"
                .parse()
                .expect("a value"),
        );
        let form = parse_body(&headers, b"password=hunter+2&returnTo=%2Fguides%2Finstall");
        assert_eq!(field(&form, "password").as_deref(), Some("hunter 2"));
        assert_eq!(field(&form, "returnTo").as_deref(), Some("/guides/install"));
        assert_eq!(field(&form, "absent"), None);
    }

    #[test]
    fn a_json_body_is_read_too() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            "application/json".parse().expect("a value"),
        );
        let form = parse_body(&headers, br#"{"password":"hunter 2","returnTo":"/x"}"#);
        assert_eq!(field(&form, "password").as_deref(), Some("hunter 2"));
        assert_eq!(field(&form, "returnTo").as_deref(), Some("/x"));
    }

    #[test]
    fn an_empty_field_is_the_same_as_an_absent_one() {
        let form = parse_body(&HeaderMap::new(), b"password=");
        assert_eq!(field(&form, "password"), None);
    }

    #[test]
    fn a_body_that_is_not_what_it_says_it_is_reads_as_nothing() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            "application/json".parse().expect("a value"),
        );
        assert!(parse_body(&headers, b"not json").is_empty());
        assert!(parse_body(&headers, b"[1,2,3]").is_empty());
    }

    /// RFC 1506 derives the accepted return origin from `auth.jwt.loginUrl`.
    /// A value that is not a URL must contribute **nothing** rather than a
    /// garbage entry: the degenerate case is "only this site's origins", which
    /// refuses the operator's login application and is the safe direction.
    #[test]
    fn a_login_url_that_is_not_a_url_contributes_no_origin() {
        assert_eq!(
            origin_of("https://login.acme.com"),
            Some("https://login.acme.com".to_owned())
        );
        assert_eq!(
            origin_of("https://login.acme.com/sign-in?product=docs"),
            Some("https://login.acme.com".to_owned())
        );
        assert_eq!(
            origin_of("https://login.acme.com:8443/x"),
            Some("https://login.acme.com:8443".to_owned())
        );
        for not_a_url in [
            "",
            "login.acme.com",
            "https://",
            "https:///sign-in",
            "//login",
        ] {
            assert_eq!(origin_of(not_a_url), None, "{not_a_url}");
        }
    }

    #[test]
    fn a_truncated_escape_is_left_alone_rather_than_dropped() {
        assert_eq!(unescape("a%2"), "a%2");
        assert_eq!(unescape("a%zz"), "a%zz");
        assert_eq!(unescape("a%2Fb"), "a/b");
    }
}
