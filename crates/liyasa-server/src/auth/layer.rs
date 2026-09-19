//! The session-extraction layer (defect 65, RFC 1403).
//!
//! `routes::mount::guarded` answers 401 when no [`Principal`] is in the
//! request extensions. Nothing put one there, so the project sat between two
//! failures at once: ungated endpoints open to anyone, and every gated one
//! permanently unreachable. They look like opposites and have one cause.
//!
//! This is the missing half. It reads whatever authenticated the request — a
//! session cookie, or a bearer token for an agent (AUTH-08) — and inserts the
//! reader it identifies.
//!
//! **It never refuses.** A request with no session, an expired session, or a
//! revoked token passes through carrying nothing, and an ungated route answers
//! it exactly as before. Refusing here would turn a disclosure gap into a
//! site-wide 401, which is the failure this layer exists to avoid rather than
//! to cause. Deciding who may is [`guarded`](crate::routes::mount::guarded)'s
//! job, one subtree at a time; this only says who is asking.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::Router;
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use http::HeaderMap;

use crate::auth::cookie;
use crate::auth::roles::{Grant, Role};
use crate::auth::session::Principal;
use crate::auth::state::AuthState;
use crate::auth::tokens;

/// Where a subject's role comes from.
///
/// Authentication says who is asking; this says what they may do, and the two
/// are different questions with different sources. A reader who signed in to
/// see a private page is a [`Role::Reader`] and carries no dashboard
/// permission at all — which is correct, and is why a site with no role source
/// gates its dashboard shut rather than open.
///
/// The real source is organization membership: `org::model::Member` carries a
/// [`Grant`] already. It is a trait rather than a direct call because `org`
/// depends on `auth` and not the other way round, so WP-28 can supply one
/// without a cycle and without this module knowing that organizations exist.
pub trait Roles: std::fmt::Debug + Send + Sync {
    /// The grant this subject holds, or `None` for somebody the source has
    /// never heard of — who stays whatever they authenticated as.
    fn grant_for(&self, subject: &str) -> Option<Grant>;
}

/// A role source written down in one place: an operator with no organization
/// behind them, and every test that needs a subject to hold a role.
#[derive(Debug, Default, Clone)]
pub struct StaticRoles(BTreeMap<String, Grant>);

impl StaticRoles {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, subject: impl Into<String>, grant: Grant) -> Self {
        self.0.insert(subject.into(), grant);
        self
    }

    pub fn role(self, subject: impl Into<String>, role: Role) -> Self {
        self.with(subject, Grant::role(role))
    }
}

impl Roles for StaticRoles {
    fn grant_for(&self, subject: &str) -> Option<Grant> {
        self.0.get(subject).cloned()
    }
}

/// Who is asking, from the request's own credentials. `None` is anonymous,
/// which is an answer rather than a failure.
///
/// A session cookie is tried before a bearer token: a browser that carries
/// both is a reader using the dashboard, not the agent whose token happens to
/// be in their environment.
pub fn principal_for(state: &AuthState, headers: &HeaderMap) -> Option<Principal> {
    let from_session = cookie::read(headers, &state.config.session.cookie_name)
        .and_then(|id| state.sessions.resolve(id).ok())
        .map(|session| session.principal);

    let principal = from_session.or_else(|| {
        let presented = tokens::bearer(
            headers
                .get(http::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
        )?;
        state
            .tokens
            .resolve(presented)
            .ok()
            .map(|record| record.principal())
    })?;

    Some(apply_roles(state, principal))
}

/// The role is applied per request rather than baked in at sign-in, so a
/// membership change takes effect on the next request instead of on the next
/// sign-in. A subject the source does not know keeps what they arrived with.
fn apply_roles(state: &AuthState, mut principal: Principal) -> Principal {
    let Some(source) = state.roles.as_ref() else {
        return principal;
    };
    if let Some(grant) = source.grant_for(&principal.subject) {
        principal.role = grant.role;
        principal.grant = Some(grant);
    }
    principal
}

/// The middleware. Inserts a [`Principal`] when there is one and gets out of
/// the way when there is not.
pub async fn extract(
    State(state): State<Arc<AuthState>>,
    mut request: Request,
    next: Next,
) -> Response {
    // An outer layer or a test may have decided already; deciding twice would
    // let the weaker answer win depending on mount order.
    if request.extensions().get::<Principal>().is_none()
        && let Some(principal) = principal_for(&state, request.headers())
    {
        request.extensions_mut().insert(principal);
    }
    next.run(request).await
}

/// Wraps a router so every route under it sees the reader.
///
/// This belongs around the **whole** application rather than around one
/// subtree: `guarded` is applied per subtree by the seam, and a guard can only
/// succeed if the extraction ran outside it. Applying it to one subtree's
/// router leaves every other subtree answering 401 to everyone, which is the
/// state this fixes.
pub fn with_session(router: Router, state: Arc<AuthState>) -> Router {
    router.layer(axum::middleware::from_fn_with_state(state, extract))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::clock::Clock;
    use crate::auth::config::{AuthConfig, Mode};
    use crate::auth::roles::Permission;
    use crate::auth::session::Principal;
    use axum::body::Body;
    use axum::routing::get;
    use http::{Request as HttpRequest, StatusCode};
    use tower::ServiceExt as _;

    fn bare(mode: Mode) -> AuthState {
        let config = AuthConfig {
            mode,
            ..AuthConfig::default()
        };
        AuthState::new(
            config,
            "production",
            vec!["https://docs.example.com".to_owned()],
            Clock::manual(),
        )
        .expect("entropy")
        .0
    }

    fn state(mode: Mode) -> Arc<AuthState> {
        Arc::new(bare(mode))
    }

    fn with_admin(subject: &str) -> Arc<AuthState> {
        Arc::new(
            bare(Mode::Password)
                .with_roles(Arc::new(StaticRoles::new().role(subject, Role::Admin))),
        )
    }

    fn cookie_header(state: &AuthState, id: &str) -> String {
        format!("{}={id}", state.config.session.cookie_name)
    }

    /// A router that reports whether the layer gave it a reader, so a test
    /// asserts on what a handler actually sees.
    fn probe(state: Arc<AuthState>) -> Router {
        let router = Router::new().route(
            "/who",
            get(|request: Request| async move {
                match request.extensions().get::<Principal>() {
                    Some(principal) => format!(
                        "{}|{}|{}",
                        principal.subject,
                        principal.role.as_str(),
                        principal.via
                    ),
                    None => "anonymous".to_owned(),
                }
            }),
        );
        with_session(router, state)
    }

    async fn ask(router: &Router, headers: &[(&str, String)]) -> (StatusCode, String) {
        let mut builder = HttpRequest::builder().method("GET").uri("/who");
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

    #[tokio::test]
    async fn a_request_with_no_credentials_reaches_the_route_carrying_nothing() {
        // The sequencing requirement: mounting this must not break anything
        // that was working. An anonymous request is served, not refused.
        let router = probe(state(Mode::Password));
        assert_eq!(
            ask(&router, &[]).await,
            (StatusCode::OK, "anonymous".into())
        );
    }

    #[tokio::test]
    async fn a_session_cookie_becomes_the_reader_the_handler_sees() {
        let state = state(Mode::Password);
        let session = state
            .sessions
            .begin(Principal::new("reader-1").with_via("password"))
            .expect("a session");
        let router = probe(state.clone());

        let (status, body) = ask(&router, &[("cookie", cookie_header(&state, &session.id))]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "reader-1|reader|password");
    }

    #[tokio::test]
    async fn an_expired_session_is_anonymous_rather_than_refused() {
        let state = state(Mode::Password);
        let session = state
            .sessions
            .begin(Principal::new("reader-1"))
            .expect("a session");
        let router = probe(state.clone());
        state
            .sessions
            .clock()
            .advance(state.sessions.policy().max_age);

        let (status, body) = ask(&router, &[("cookie", cookie_header(&state, &session.id))]).await;
        assert_eq!(
            (status, body.as_str()),
            (StatusCode::OK, "anonymous"),
            "an expired session must not become a refusal in this layer"
        );
    }

    #[tokio::test]
    async fn a_cookie_naming_no_session_is_anonymous() {
        let state = state(Mode::Password);
        let router = probe(state.clone());
        let (status, body) = ask(&router, &[("cookie", cookie_header(&state, "invented"))]).await;
        assert_eq!((status, body.as_str()), (StatusCode::OK, "anonymous"));
    }

    #[tokio::test]
    async fn a_personal_access_token_identifies_an_agent() {
        let state = state(Mode::Password);
        let issued = state
            .tokens
            .issue_personal(
                &Principal::new("reader-1").with_groups(["partner"]),
                "a laptop",
                &Default::default(),
                None,
            )
            .expect("a token");
        let router = probe(state.clone());

        let (status, body) = ask(
            &router,
            &[("authorization", format!("Bearer {}", issued.secret))],
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "reader-1|reader|pat");
    }

    #[tokio::test]
    async fn a_revoked_token_is_anonymous_rather_than_refused() {
        let state = state(Mode::Password);
        let issued = state
            .tokens
            .issue_personal(
                &Principal::new("reader-1"),
                "a laptop",
                &Default::default(),
                None,
            )
            .expect("a token");
        assert!(state.tokens.revoke(&issued.record.id));
        let router = probe(state.clone());

        let (_, body) = ask(
            &router,
            &[("authorization", format!("Bearer {}", issued.secret))],
        )
        .await;
        assert_eq!(body, "anonymous");
    }

    #[tokio::test]
    async fn a_session_wins_over_a_token_the_same_request_happens_to_carry() {
        let state = state(Mode::Password);
        let session = state
            .sessions
            .begin(Principal::new("the-reader"))
            .expect("a session");
        let issued = state
            .tokens
            .issue_personal(
                &Principal::new("the-agent"),
                "ci",
                &Default::default(),
                None,
            )
            .expect("a token");
        let router = probe(state.clone());

        let (_, body) = ask(
            &router,
            &[
                ("cookie", cookie_header(&state, &session.id)),
                ("authorization", format!("Bearer {}", issued.secret)),
            ],
        )
        .await;
        assert!(body.starts_with("the-reader|"), "{body}");
    }

    #[tokio::test]
    async fn a_role_source_decides_what_a_reader_may_do() {
        let state = with_admin("an-admin");

        for (subject, expected) in [("an-admin", "admin"), ("a-reader", "reader")] {
            let session = state
                .sessions
                .begin(Principal::new(subject))
                .expect("a session");
            let router = probe(state.clone());
            let (_, body) = ask(&router, &[("cookie", cookie_header(&state, &session.id))]).await;
            assert_eq!(body, format!("{subject}|{expected}|"), "{subject}");
        }
    }

    #[tokio::test]
    async fn a_role_source_reaches_a_token_as_well_as_a_cookie() {
        let state = with_admin("an-admin");
        let issued = state
            .tokens
            .issue_personal(&Principal::new("an-admin"), "ci", &Default::default(), None)
            .expect("a token");
        let router = probe(state.clone());

        let (_, body) = ask(
            &router,
            &[("authorization", format!("Bearer {}", issued.secret))],
        )
        .await;
        assert_eq!(body, "an-admin|admin|pat");
    }

    #[test]
    fn a_grant_with_a_custom_role_reaches_the_principal_whole() {
        // `guarded` asks `principal.role`, which cannot carry a composed role.
        // The whole grant rides along so the finer answer exists the day the
        // guard asks for it.
        let custom = crate::auth::roles::CustomRole {
            name: "release-manager".to_owned(),
            extends: Some(Role::Editor),
            grant: [Permission::ProposalReview].into(),
            revoke: Default::default(),
        };
        let grant = Grant::role(Role::Reader).with_custom(custom);
        let source = StaticRoles::new().with("someone", grant.clone());

        let inner = bare(Mode::Password).with_roles(Arc::new(source));

        let principal = apply_roles(&inner, Principal::new("someone"));
        assert_eq!(principal.role, Role::Reader, "the built-in half");
        assert_eq!(principal.grant.as_ref(), Some(&grant));
        assert!(
            principal.allows(Permission::ProposalReview),
            "the composed half, which `role` alone cannot answer"
        );
        assert!(!principal.allows(Permission::OwnerAct));
    }

    #[test]
    fn a_principal_with_no_grant_answers_from_its_role_alone() {
        let reader = Principal::new("r");
        assert!(!reader.allows(Permission::DashboardRead));
        let admin = Principal::new("a").with_role(Role::Admin);
        assert!(admin.allows(Permission::DashboardRead));
        assert!(!admin.allows(Permission::OwnerAct));
    }

    #[tokio::test]
    async fn a_principal_an_outer_layer_already_decided_is_not_replaced() {
        let state = state(Mode::Password);
        let session = state
            .sessions
            .begin(Principal::new("from-the-cookie"))
            .expect("a session");
        let inner = probe(state.clone());
        // An outer layer that has already decided, as a proxy or a test does.
        let router = inner.layer(axum::middleware::from_fn(
            |mut request: Request, next: Next| async move {
                request
                    .extensions_mut()
                    .insert(Principal::new("from-outside"));
                next.run(request).await
            },
        ));

        let (_, body) = ask(&router, &[("cookie", cookie_header(&state, &session.id))]).await;
        assert!(body.starts_with("from-outside|"), "{body}");
    }
}
