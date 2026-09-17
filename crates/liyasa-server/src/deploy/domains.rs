//! Custom domains, with an optional base path (CLI-21).
//!
//! Two challenges are described and one is shipped. HOST-20 specifies a TXT
//! record; checking one needs a resolver, and the only resolver in the
//! workspace is private to `liyasa-net`
//! (`plan/rfcs/1606-domain-ownership-without-a-resolver.md`). So the shipped
//! path is an HTTP fetch of a token this server itself answers, the same shape
//! as HOST-02's ACME challenge sitting beside it — and it proves the stronger
//! fact, that the name resolves *here*, rather than only that someone with
//! zone access asked.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use liyasa_core::ids::{Fingerprint, ProjectId};
use liyasa_core::net::{HostSet, HttpClient, HttpPolicy, HttpRequest, Method, Purpose, Url};
use liyasa_store::SqliteStore;
use liyasa_store::repos::DomainRecord;
use serde::{Deserialize, Serialize};

/// Where the token is served, on the domain being verified.
pub const VERIFY_PREFIX: &str = "/.well-known/liyasa-domain-verify/";

/// The TXT record HOST-20 specifies, prefixed to the domain being verified.
pub const TXT_PREFIX: &str = "_liyasa-challenge";

/// How long a pending domain's token stays valid.
pub const TOKEN_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// The longest a DNS name may be, and the longest one of its labels.
pub const MAX_HOST: usize = 253;
pub const MAX_LABEL: usize = 63;

/// A response bigger than this is not a token file.
const MAX_BODY: u64 = 4 * 1024;
const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DomainError {
    #[error("`{0}` is not a host name")]
    NotAHost(String),
    #[error("`{0}` is reserved and cannot be a custom domain")]
    Reserved(String),
    #[error("`{0}` has not been added, so there is nothing to verify")]
    Unknown(String),
    #[error("the token for `{0}` has expired; add the domain again for a new one")]
    Expired(String),
    #[error("`{host}` did not answer: {detail}")]
    Unreachable { host: String, detail: String },
    #[error("`{host}` answered {status} rather than serving the token")]
    NotServed { host: String, status: u16 },
    #[error("`{host}` served a different token, so it points somewhere else")]
    Mismatch { host: String },
    #[error("`{0}` is verified; removing a verified domain is not implemented yet")]
    NotRemovable(String),
    #[error("{0}")]
    Store(String),
}

/// Names that must never be handed to a tenant.
const RESERVED: &[&str] = &["localhost", "local", "internal", "invalid", "example"];

/// Lowercases, drops a trailing dot, and refuses anything that is not a plain
/// ASCII host name.
///
/// No punycode conversion: no IDNA implementation is in the dependency table,
/// and silently accepting a Unicode name that later fails to resolve would be
/// worse than refusing it here with a message that says to enter the
/// `xn--` form.
pub fn normalize_host(host: &str) -> Result<String, DomainError> {
    let trimmed = host.trim().trim_end_matches('.').to_ascii_lowercase();
    let refuse = || DomainError::NotAHost(host.to_owned());
    if trimmed.is_empty() || trimmed.len() > MAX_HOST {
        return Err(refuse());
    }
    if trimmed.contains(['/', ':', '@', '?', '#', ' ']) {
        return Err(refuse());
    }
    if !trimmed.contains('.') {
        return Err(refuse());
    }
    for label in trimmed.split('.') {
        if label.is_empty() || label.len() > MAX_LABEL {
            return Err(refuse());
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err(refuse());
        }
        if !label
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(refuse());
        }
    }
    let last = trimmed.rsplit('.').next().unwrap_or_default();
    if RESERVED.contains(&last) || RESERVED.contains(&trimmed.as_str()) {
        return Err(DomainError::Reserved(trimmed));
    }
    Ok(trimmed)
}

/// `/docs`, or the empty string for the root. A base path is a URL prefix, so
/// it has a leading slash and no trailing one.
pub fn normalize_base_path(base_path: &str) -> String {
    let trimmed = base_path.trim().trim_matches('/');
    match trimmed.is_empty() {
        true => String::new(),
        false => format!("/{trimmed}"),
    }
}

/// The token for a host. Derived rather than random so two replicas that were
/// asked independently agree, and keyed by a per-installation secret so it
/// cannot be computed by whoever wants to claim the domain.
pub fn token_for(secret: &[u8], host: &str, project: &ProjectId) -> String {
    let digest = Fingerprint::of_parts([secret, host.as_bytes(), project.to_string().as_bytes()]);
    digest.to_hex()[..32].to_owned()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pending {
    pub host: String,
    pub project: ProjectId,
    pub env: String,
    pub base_path: String,
    pub token: String,
    pub created_at: i64,
}

impl Pending {
    /// Where the operator must make this token appear.
    pub fn verify_url(&self) -> String {
        format!("http://{}{VERIFY_PREFIX}{}", self.host, self.token)
    }

    /// The name and value of the TXT record HOST-20 specifies.
    ///
    /// Returned even though nothing checks it yet, because it is what an
    /// operator is told to create when a resolver becomes available, and
    /// because the two challenges must agree on the token (RFC 1606).
    pub fn txt_record(&self) -> (String, String) {
        (
            format!("{TXT_PREFIX}.{}", self.host),
            format!("liyasa-domain-verify={}", self.token),
        )
    }

    pub fn expired(&self, now_ms: i64) -> bool {
        now_ms.saturating_sub(self.created_at) >= TOKEN_TTL.as_millis() as i64
    }

    /// What `liyasa domain add` prints: this server answers the token itself
    /// once the name resolves here, so the only step an operator has is DNS.
    pub fn instructions(&self) -> String {
        let (name, value) = self.txt_record();
        format!(
            "Point `{}` at this server with a CNAME or an A record, then verify.\n\
             This server answers {} once the name resolves here; nothing has to be uploaded.\n\
             \n\
             If you would rather prove ownership before cutting the name over, create\n\
             the TXT record `{name}` with the value `{value}`. Checking it needs a\n\
             resolver this build does not have yet (RFC 1606), so it is recorded here\n\
             rather than accepted.",
            self.host,
            self.verify_url()
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct DnsError(pub String);

/// A TXT lookup, for the challenge HOST-20 specifies.
///
/// TODO(rfc-1606): nothing implements this. `liyasa-net` holds the only
/// resolver in the workspace and exposes no lookup API; when it does, this is
/// a few lines and [`Domains::verify`] prefers it.
pub trait DnsResolver: Send + Sync + std::fmt::Debug {
    fn txt<'a>(
        &'a self,
        name: &'a str,
    ) -> liyasa_core::net::BoxFut<'a, Result<Vec<String>, DnsError>>;
}

/// Domains added and not yet verified.
///
/// In memory, like the ACME challenges beside it: a token is worthless once
/// used and cheap to reissue, so persisting it would buy nothing but a
/// migration. A replica that restarts mid-verification is re-run with `add`.
#[derive(Debug, Default)]
pub struct PendingDomains(RwLock<HashMap<String, Pending>>);

impl PendingDomains {
    pub fn put(&self, pending: Pending) {
        if let Ok(mut map) = self.0.write() {
            map.insert(pending.host.clone(), pending);
        }
    }

    pub fn get(&self, host: &str) -> Option<Pending> {
        self.0.read().ok()?.get(host).cloned()
    }

    pub fn take(&self, host: &str) -> Option<Pending> {
        self.0.write().ok()?.remove(host)
    }

    /// The pending domain a token belongs to, so this server can answer its
    /// own challenge.
    pub fn by_token(&self, token: &str) -> Option<Pending> {
        self.0
            .read()
            .ok()?
            .values()
            .find(|pending| pending.token == token)
            .cloned()
    }

    pub fn len(&self) -> usize {
        self.0.read().map(|map| map.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// `liyasa domain add|verify|remove` (CLI-21).
#[derive(Clone)]
pub struct Domains {
    store: Arc<SqliteStore>,
    http: Arc<dyn HttpClient>,
    pending: Arc<PendingDomains>,
    secret: Vec<u8>,
    allow_private: bool,
    resolver: Option<Arc<dyn DnsResolver>>,
}

impl std::fmt::Debug for Domains {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Domains")
            .field("pending", &self.pending.len())
            .finish_non_exhaustive()
    }
}

impl Domains {
    pub fn new(store: Arc<SqliteStore>, http: Arc<dyn HttpClient>, secret: Vec<u8>) -> Self {
        Self {
            store,
            http,
            pending: Arc::new(PendingDomains::default()),
            secret,
            allow_private: false,
            resolver: None,
        }
    }

    /// Prefers the TXT challenge of HOST-20 when a resolver is available
    /// (RFC 1606).
    pub fn with_resolver(mut self, resolver: Arc<dyn DnsResolver>) -> Self {
        self.resolver = Some(resolver);
        self
    }

    /// An installation whose custom domains resolve inside a private network
    /// — a company intranet — has to say so, because the default refuses a
    /// private address for every other purpose (§30.2.3).
    pub fn with_private_addresses(mut self, allow: bool) -> Self {
        self.allow_private = allow;
        self
    }

    pub fn pending(&self) -> &Arc<PendingDomains> {
        &self.pending
    }

    /// Registers a domain and returns what the operator has to do.
    pub fn add(
        &self,
        host: &str,
        project: &ProjectId,
        env: &str,
        base_path: &str,
        now_ms: i64,
    ) -> Result<Pending, DomainError> {
        let host = normalize_host(host)?;
        let pending = Pending {
            token: token_for(&self.secret, &host, project),
            host,
            project: *project,
            env: env.to_owned(),
            base_path: normalize_base_path(base_path),
            created_at: now_ms,
        };
        self.pending.put(pending.clone());
        Ok(pending)
    }

    /// Fetches the token from the domain and records it when it matches.
    pub async fn verify(&self, host: &str, now_ms: i64) -> Result<DomainRecord, DomainError> {
        let host = normalize_host(host)?;
        let pending = self
            .pending
            .get(&host)
            .ok_or_else(|| DomainError::Unknown(host.clone()))?;
        if pending.expired(now_ms) {
            self.pending.take(&host);
            return Err(DomainError::Expired(host));
        }

        if let Some(resolver) = &self.resolver {
            let (name, expected) = pending.txt_record();
            let records = resolver
                .txt(&name)
                .await
                .map_err(|error| DomainError::Unreachable {
                    host: host.clone(),
                    detail: error.to_string(),
                })?;
            if !records.iter().any(|record| record.trim() == expected) {
                return Err(DomainError::Mismatch { host });
            }
            return self.record(&pending).await;
        }

        let url: Url = pending
            .verify_url()
            .parse()
            .map_err(|_| DomainError::NotAHost(host.clone()))?;
        let policy = HttpPolicy {
            allow_hosts: HostSet::default(),
            deny_hosts: HostSet::default(),
            allow_private: self.allow_private,
            // A verification that follows a redirect proves whoever controls
            // the redirect target owns the name, which is the wrong party.
            max_redirects: 0,
            max_bytes: MAX_BODY,
            timeout: TIMEOUT,
            // Not a purpose of its own: `LinkCheck` is "fetch this URL and see
            // what it serves", which is exactly this, and `Purpose` is a
            // frozen contract in liyasa-core.
            purpose: Purpose::LinkCheck,
        };
        let response = self
            .http
            .fetch(
                HttpRequest {
                    method: Method::GET,
                    url,
                    headers: Vec::new(),
                    body: None,
                },
                &policy,
            )
            .await
            .map_err(|error| DomainError::Unreachable {
                host: host.clone(),
                detail: error.to_string(),
            })?;
        if !(200..300).contains(&response.status) {
            return Err(DomainError::NotServed {
                host,
                status: response.status,
            });
        }
        let served = String::from_utf8_lossy(&response.body);
        if served.trim() != pending.token {
            return Err(DomainError::Mismatch { host });
        }

        self.record(&pending).await
    }

    async fn record(&self, pending: &Pending) -> Result<DomainRecord, DomainError> {
        let record = DomainRecord {
            host: pending.host.clone(),
            project: pending.project,
            base_path: pending.base_path.clone(),
            env: pending.env.clone(),
        };
        self.store
            .domains()
            .put(&record)
            .await
            .map_err(|error| DomainError::Store(error.to_string()))?;
        self.pending.take(&pending.host);
        Ok(record)
    }

    /// Removes a domain that has not been verified yet.
    ///
    /// A verified one cannot be removed: `liyasa-store`'s `Domains` offers
    /// `put`, `get`, `list`, `rename` and `redirect_for` and no delete, and
    /// `domain.project_id` is a foreign key, so there is no row this package
    /// could write that means "gone". That is a missing method in a crate this
    /// package does not own rather than a decision, so it refuses by name
    /// instead of pretending, and the gap is recorded where the package parks.
    pub async fn remove(&self, host: &str) -> Result<(), DomainError> {
        let host = normalize_host(host)?;
        if self.pending.take(&host).is_some() {
            return Ok(());
        }
        match self
            .store
            .domains()
            .get(&host)
            .await
            .map_err(|error| DomainError::Store(error.to_string()))?
        {
            Some(_) => Err(DomainError::NotRemovable(host)),
            None => Err(DomainError::Unknown(host)),
        }
    }

    pub async fn list(&self) -> Result<Vec<DomainRecord>, DomainError> {
        self.store
            .domains()
            .list()
            .await
            .map_err(|error| DomainError::Store(error.to_string()))
    }
}

/// The HTTP surface of `liyasa domain add|verify|remove` (CLI-21).
///
/// Its own `Router` with its own state rather than a field on `DeployState`:
/// an installation that serves no custom domains does not construct a
/// [`Domains`] at all, and an optional field would push that decision into
/// every handler.
pub mod routes {
    use std::sync::Arc;

    use axum::Router;
    use axum::extract::{Path, State};
    use axum::response::{IntoResponse, Response};
    use axum::routing::{get, post};
    use http::StatusCode;
    use liyasa_core::diagnostics::code::E0801;
    use liyasa_core::ids::ProjectId;
    use serde::Deserialize;
    use serde_json::json;

    use super::{DomainError, Domains};
    use crate::routes::api::{Json, JsonStatus};
    use crate::routes::problem::Problem;

    #[derive(Debug, Clone, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct AddInput {
        pub host: String,
        pub project: String,
        #[serde(default)]
        pub env: Option<String>,
        #[serde(default)]
        pub base_path: Option<String>,
    }

    fn refuse(error: &DomainError) -> Response {
        match error {
            // The four that mean "the domain did not prove itself" are the
            // ones E0801 is registered for.
            DomainError::Unreachable { .. }
            | DomainError::NotServed { .. }
            | DomainError::Mismatch { .. }
            | DomainError::Expired(_) => Problem::code(StatusCode::CONFLICT, E0801)
                .detail(error.to_string())
                .into_response(),
            DomainError::NotAHost(_) | DomainError::Reserved(_) => {
                Problem::bad_request(error.to_string()).into_response()
            }
            DomainError::Unknown(_) => Problem::not_found("domain").into_response(),
            DomainError::NotRemovable(_) => {
                Problem::new(StatusCode::NOT_IMPLEMENTED, "Not removable")
                    .detail(error.to_string())
                    .into_response()
            }
            DomainError::Store(detail) => {
                tracing::error!(target: "liyasa_server", %detail, "a domain could not be stored");
                Problem::new(StatusCode::INTERNAL_SERVER_ERROR, "Storage failed").into_response()
            }
        }
    }

    pub async fn add(
        State(domains): State<Arc<Domains>>,
        axum::Json(input): axum::Json<AddInput>,
    ) -> Response {
        let Some(project) = ProjectId::parse(&input.project) else {
            return Problem::bad_request("`project` is a ULID").into_response();
        };
        match domains.add(
            &input.host,
            &project,
            input.env.as_deref().unwrap_or("production"),
            input.base_path.as_deref().unwrap_or_default(),
            liyasa_store::now_ms(),
        ) {
            Ok(pending) => {
                let (name, value) = pending.txt_record();
                JsonStatus(
                    StatusCode::ACCEPTED,
                    json!({
                        "host": pending.host,
                        "project": pending.project.to_string(),
                        "env": pending.env,
                        "basePath": pending.base_path,
                        "verifyUrl": pending.verify_url(),
                        "txtRecord": { "name": name, "value": value },
                        "instructions": pending.instructions(),
                    }),
                )
                .into_response()
            }
            Err(error) => refuse(&error),
        }
    }

    pub async fn verify(State(domains): State<Arc<Domains>>, Path(host): Path<String>) -> Response {
        match domains.verify(&host, liyasa_store::now_ms()).await {
            Ok(record) => Json(json!({
                "host": record.host,
                "project": record.project.to_string(),
                "env": record.env,
                "basePath": record.base_path,
                "verified": true,
            }))
            .into_response(),
            Err(error) => refuse(&error),
        }
    }

    pub async fn remove(State(domains): State<Arc<Domains>>, Path(host): Path<String>) -> Response {
        match domains.remove(&host).await {
            Ok(()) => StatusCode::NO_CONTENT.into_response(),
            Err(error) => refuse(&error),
        }
    }

    pub async fn list(State(domains): State<Arc<Domains>>) -> Response {
        match domains.list().await {
            Ok(records) => Json(json!({
                "items": records
                    .iter()
                    .map(|record| json!({
                        "host": record.host,
                        "project": record.project.to_string(),
                        "env": record.env,
                        "basePath": record.base_path,
                    }))
                    .collect::<Vec<_>>(),
            }))
            .into_response(),
            Err(error) => refuse(&error),
        }
    }

    /// The challenge this server answers for a domain pointed at it. Plain
    /// text, because the thing fetching it is Liyasa comparing bytes.
    pub async fn challenge(
        State(domains): State<Arc<Domains>>,
        Path(token): Path<String>,
    ) -> Response {
        match domains.pending().by_token(&token) {
            Some(pending) => (
                StatusCode::OK,
                [("content-type", "text/plain; charset=utf-8")],
                pending.token,
            )
                .into_response(),
            None => StatusCode::NOT_FOUND.into_response(),
        }
    }

    pub fn router(domains: Arc<Domains>) -> Router {
        Router::new()
            .route("/_liyasa/api/v1/domains", get(list).post(add))
            .route(
                "/_liyasa/api/v1/domains/{host}",
                axum::routing::delete(remove),
            )
            .route("/_liyasa/api/v1/domains/{host}/verify", post(verify))
            .route("/.well-known/liyasa-domain-verify/{token}", get(challenge))
            .with_state(domains)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn a_domain_that_did_not_prove_itself_is_the_registered_code() {
            for error in [
                DomainError::Mismatch {
                    host: "docs.example.com".to_owned(),
                },
                DomainError::NotServed {
                    host: "docs.example.com".to_owned(),
                    status: 404,
                },
                DomainError::Unreachable {
                    host: "docs.example.com".to_owned(),
                    detail: "timed out".to_owned(),
                },
                DomainError::Expired("docs.example.com".to_owned()),
            ] {
                assert_eq!(refuse(&error).status(), StatusCode::CONFLICT, "{error}");
            }
            assert_eq!(E0801.as_str(), "E0801");
        }

        #[test]
        fn removing_a_verified_domain_says_it_is_not_implemented_rather_than_succeeding() {
            assert_eq!(
                refuse(&DomainError::NotRemovable("docs.example.com".to_owned())).status(),
                StatusCode::NOT_IMPLEMENTED
            );
        }

        #[test]
        fn a_malformed_host_is_the_callers_mistake_rather_than_the_domains() {
            assert_eq!(
                refuse(&DomainError::NotAHost("nope".to_owned())).status(),
                StatusCode::BAD_REQUEST
            );
            assert_eq!(
                refuse(&DomainError::Unknown("docs.example.com".to_owned())).status(),
                StatusCode::NOT_FOUND
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> ProjectId {
        ProjectId(ulid::Ulid::from_bytes([5; 16]))
    }

    #[test]
    fn a_host_is_lowercased_and_stripped_of_its_root_dot() {
        assert_eq!(
            normalize_host("  Docs.Example.COM.  ").expect("a host"),
            "docs.example.com"
        );
    }

    #[test]
    fn anything_that_is_not_a_plain_host_name_is_refused() {
        for bad in [
            "",
            "example",
            "https://docs.example.com",
            "docs.example.com/docs",
            "docs.example.com:443",
            "docs..example.com",
            "-docs.example.com",
            "docs-.example.com",
            "dócs.example.com",
            "user@example.com",
        ] {
            assert!(
                matches!(normalize_host(bad), Err(DomainError::NotAHost(_))),
                "`{bad}` was accepted"
            );
        }
    }

    #[test]
    fn a_reserved_name_is_refused_by_name_rather_than_as_malformed() {
        for reserved in ["docs.localhost", "site.internal", "a.invalid"] {
            assert!(
                matches!(normalize_host(reserved), Err(DomainError::Reserved(_))),
                "`{reserved}` was accepted"
            );
        }
    }

    #[test]
    fn a_label_longer_than_sixty_three_characters_is_refused() {
        let long = format!("{}.example.com", "a".repeat(MAX_LABEL + 1));
        assert!(normalize_host(&long).is_err());
        let fine = format!("{}.example.com", "a".repeat(MAX_LABEL));
        assert!(normalize_host(&fine).is_ok());
    }

    #[test]
    fn a_base_path_gets_one_leading_slash_and_no_trailing_one() {
        assert_eq!(normalize_base_path("/docs/"), "/docs");
        assert_eq!(normalize_base_path("docs"), "/docs");
        assert_eq!(normalize_base_path("/a/b/"), "/a/b");
        assert_eq!(normalize_base_path("/"), "");
        assert_eq!(normalize_base_path(""), "");
    }

    #[test]
    fn a_token_is_stable_for_a_host_and_different_for_another() {
        let secret = b"an-installation-secret";
        let one = token_for(secret, "docs.example.com", &project());
        assert_eq!(one, token_for(secret, "docs.example.com", &project()));
        assert_ne!(one, token_for(secret, "other.example.com", &project()));
        assert_ne!(
            one,
            token_for(b"a-different-secret", "docs.example.com", &project())
        );
        assert_eq!(one.len(), 32);
    }

    #[test]
    fn a_token_cannot_be_computed_without_the_installation_secret() {
        // The same host and project, guessed by an outsider with no secret.
        let guessed = token_for(b"", "docs.example.com", &project());
        let real = token_for(b"an-installation-secret", "docs.example.com", &project());
        assert_ne!(guessed, real);
    }

    #[test]
    fn the_instructions_name_the_url_this_server_will_answer() {
        let pending = Pending {
            host: "docs.example.com".to_owned(),
            project: project(),
            env: "production".to_owned(),
            base_path: "/docs".to_owned(),
            token: "abc123".to_owned(),
            created_at: 0,
        };
        assert_eq!(
            pending.verify_url(),
            "http://docs.example.com/.well-known/liyasa-domain-verify/abc123"
        );
        let instructions = pending.instructions();
        assert!(instructions.contains("CNAME"), "{instructions}");
        assert!(
            instructions.contains("nothing has to be uploaded"),
            "{instructions}"
        );
    }

    #[test]
    fn the_txt_record_names_the_challenge_subdomain_and_the_same_token() {
        let pending = Pending {
            host: "docs.example.com".to_owned(),
            project: project(),
            env: "production".to_owned(),
            base_path: String::new(),
            token: "abc123".to_owned(),
            created_at: 0,
        };
        let (name, value) = pending.txt_record();
        assert_eq!(name, "_liyasa-challenge.docs.example.com");
        assert_eq!(value, "liyasa-domain-verify=abc123");
        assert!(
            pending.verify_url().contains("abc123"),
            "both challenges carry the same token"
        );
        assert!(
            pending.instructions().contains(&name),
            "{}",
            pending.instructions()
        );
    }

    #[test]
    fn a_token_expires_after_a_week() {
        let pending = Pending {
            host: "docs.example.com".to_owned(),
            project: project(),
            env: "production".to_owned(),
            base_path: String::new(),
            token: "t".to_owned(),
            created_at: 0,
        };
        let week = 7 * 24 * 60 * 60 * 1000;
        assert!(!pending.expired(week - 1));
        assert!(pending.expired(week));
    }

    #[test]
    fn a_pending_domain_is_found_by_host_and_by_token() {
        let pending = PendingDomains::default();
        assert!(pending.is_empty());
        pending.put(Pending {
            host: "docs.example.com".to_owned(),
            project: project(),
            env: "production".to_owned(),
            base_path: String::new(),
            token: "tok".to_owned(),
            created_at: 0,
        });
        assert_eq!(pending.len(), 1);
        assert!(pending.get("docs.example.com").is_some());
        assert_eq!(
            pending.by_token("tok").map(|p| p.host),
            Some("docs.example.com".to_owned())
        );
        assert!(pending.by_token("other").is_none());
        assert!(pending.take("docs.example.com").is_some());
        assert!(pending.is_empty());
    }
}
