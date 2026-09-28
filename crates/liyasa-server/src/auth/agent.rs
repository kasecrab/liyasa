//! AUTH-08: how an agent obtains a token (RFC 1508).
//!
//! `auth::tokens` has minted both kinds of token since 2026-09-17 and
//! `auth::layer` has accepted a presented one for just as long, so a token that
//! exists already works everywhere a session does, group rules included. What
//! was missing was any way to get one: the three minting functions were
//! reachable only from Rust.
//!
//! The two endpoints answer the same questions in opposite ways, and the
//! reasons are on each function.

use std::collections::{BTreeMap, BTreeSet};
use std::net::IpAddr;
use std::sync::RwLock;

use axum::response::{IntoResponse, Response};
use http::StatusCode;
use liyasa_core::ids::TokenId;
use serde_json::{Value, json};

use crate::auth::clock::Clock;
use crate::auth::session::Principal;
use crate::auth::state::AuthState;
use crate::auth::tokens::{self, Kind, Rejected};
use crate::routes::problem::Problem;

/// How many grant attempts one client id, or one address, gets per window.
const GRANT_ATTEMPTS: u32 = 10;
const GRANT_WINDOW_MS: i64 = 60_000;

/// The label a token gets when the caller names none.
const DEFAULT_LABEL: &str = "agent";

fn ok(body: Value) -> Response {
    (StatusCode::OK, axum::Json(body)).into_response()
}

fn field<'a>(form: &'a [(String, String)], name: &str) -> Option<&'a str> {
    form.iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
        .filter(|value| !value.is_empty())
}

/// Two windows for the client-credentials grant, per client id and per address.
///
/// Both, for the reason `auth::password` carries both: per id alone lets one
/// attacker spread across many ids, and per address alone lets a botnet spread
/// one id across many hosts. The grant compares a secret, so without a limit it
/// is a guessing oracle against every registered client, reachable by anyone.
#[derive(Debug, Default)]
pub struct Grants {
    by_client: RwLock<BTreeMap<String, (i64, u32)>>,
    by_address: RwLock<BTreeMap<String, (i64, u32)>>,
    clock: Clock,
}

impl Grants {
    pub fn new(clock: Clock) -> Self {
        Self {
            by_client: RwLock::new(BTreeMap::new()),
            by_address: RwLock::new(BTreeMap::new()),
            clock,
        }
    }

    /// Charges both windows and answers whether the attempt may proceed.
    ///
    /// Charged before the secret is compared, not after: a limit that counts
    /// only failures is no limit against an attacker who is occasionally right.
    pub fn charge(&self, client_id: &str, address: IpAddr) -> bool {
        let now = self.clock.now_ms();
        let bump = |map: &RwLock<BTreeMap<String, (i64, u32)>>, key: &str| {
            let mut map = map.write().unwrap_or_else(|e| e.into_inner());
            let slot = map.entry(key.to_owned()).or_insert((now, 0));
            if now - slot.0 >= GRANT_WINDOW_MS {
                *slot = (now, 0);
            }
            slot.1 += 1;
            slot.1 <= GRANT_ATTEMPTS
        };
        // Both are charged even when the first says no, so an attacker cannot
        // keep one window cool by tripping the other.
        let client_ok = bump(&self.by_client, client_id);
        let address_ok = bump(&self.by_address, &address.to_string());
        client_ok && address_ok
    }
}

/// `POST /_liyasa/auth/token` — the OAuth 2.1 client-credentials grant.
///
/// **No CSRF guard, on purpose.** An OAuth client is not a browser: it sends no
/// `Origin` and no `Referer`, so `csrf::check` would answer `Unproven` and
/// refuse every legitimate client. There is nothing for CSRF to protect either
/// — the request carries a client secret, sets no cookie, and the token appears
/// only in the response body, so a cross-site page that forged it would receive
/// nothing it could read.
pub fn grant(state: &AuthState, address: IpAddr, form: &[(String, String)]) -> Response {
    // RFC 6749 names the error codes a client library reads.
    if field(form, "grant_type") != Some("client_credentials") {
        return Problem::new(StatusCode::BAD_REQUEST, "unsupported_grant_type")
            .detail(
                "this endpoint implements the client-credentials grant; \
                 send `grant_type=client_credentials`",
            )
            .into_response();
    }
    let (Some(client_id), Some(client_secret)) =
        (field(form, "client_id"), field(form, "client_secret"))
    else {
        return Problem::new(StatusCode::BAD_REQUEST, "invalid_request")
            .detail("`client_id` and `client_secret` are required")
            .into_response();
    };

    if !state.grants.charge(client_id, address) {
        return Problem::new(StatusCode::TOO_MANY_REQUESTS, "slow_down")
            .detail("too many grant attempts; wait a minute")
            .into_response();
    }

    match state.tokens.client_credentials(client_id, client_secret) {
        Ok(issued) => ok(json!({
            "access_token": issued.secret,
            "token_type": "Bearer",
            "expires_in": tokens::ACCESS_TOKEN_TTL.as_secs(),
        })),
        // One answer for "no such client" and "wrong secret": separating them
        // tells an attacker which client ids exist. `tokens` already collapses
        // them and compares in constant time, so keep it collapsed here.
        Err(Rejected::Unknown | Rejected::Expired | Rejected::Revoked) => {
            Problem::new(StatusCode::UNAUTHORIZED, "invalid_client")
                .detail("the client id or secret was not accepted")
                .into_response()
        }
    }
}

/// Whether a request may mint a personal access token.
///
/// **A token may never mint a token.** `auth::layer` inserts a perfectly good
/// `Principal` for a presented bearer token, and everywhere else that
/// uniformity is the point of AUTH-08. Here it is the whole vulnerability: a
/// leaked token that can mint a replacement renews itself faster than an
/// operator can revoke it, and revocation stops meaning anything. So this asks
/// which credential arrived rather than trusting the `Principal`.
pub fn may_mint(reader: &Principal) -> bool {
    !matches!(reader.via.as_str(), "pat" | "client_credentials")
}

/// `POST /_liyasa/auth/tokens` — a reader mints a token for their own agent.
///
/// The granted groups are the intersection of what was asked for and what the
/// reader has, which `tokens::issue_personal` does: a reader cannot mint a
/// credential stronger than the session minting it.
pub fn mint(state: &AuthState, reader: &Principal, form: &[(String, String)]) -> Response {
    if !may_mint(reader) {
        return Problem::new(StatusCode::FORBIDDEN, "A token cannot mint a token")
            .detail(
                "sign in and mint this from a session; a token that renews itself \
                 cannot be revoked",
            )
            .into_response();
    }
    let label = field(form, "label").unwrap_or(DEFAULT_LABEL);
    let groups: BTreeSet<String> = field(form, "groups")
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();

    match state.tokens.issue_personal(reader, label, &groups, None) {
        Ok(issued) => ok(json!({
            "token": issued.secret,
            "id": issued.record.id.as_str(),
            "label": issued.record.label,
            "groups": issued.record.groups,
            "hint": issued.record.hint,
        })),
        Err(_) => Problem::new(StatusCode::SERVICE_UNAVAILABLE, "No token could be minted")
            .detail("this instance has no entropy source")
            .into_response(),
    }
}

/// `GET /_liyasa/auth/tokens` — the reader's own tokens.
///
/// No secret in any entry. A token's value exists once, in the response that
/// minted it; a list endpoint returning live credentials would turn one read
/// into every credential the reader has.
pub fn list(state: &AuthState, reader: &Principal) -> Response {
    let rows: Vec<Value> = state
        .tokens
        .list(&reader.subject)
        .into_iter()
        .map(|record| {
            json!({
                "id": record.id.as_str(),
                "label": record.label,
                "kind": match record.kind {
                    Kind::Personal => "personal",
                    Kind::ClientCredentials => "client_credentials",
                },
                "hint": record.hint,
                "groups": record.groups,
                "created": record.created_ms,
            })
        })
        .collect();
    ok(json!({ "tokens": rows }))
}

/// `DELETE /_liyasa/auth/tokens/{id}` — revoke one of the reader's own.
///
/// The ownership check is the point. `Tokens::revoke` takes an id and does not
/// know who is asking, so without it any reader with a session could revoke any
/// other reader's token by guessing an id — a denial of service needing no
/// privilege at all.
///
/// A token that is not the reader's and a token that does not exist get the same
/// answer, because distinguishing them confirms an id belongs to somebody.
pub fn revoke(state: &AuthState, reader: &Principal, id: &str) -> Response {
    let owned = state
        .tokens
        .list(&reader.subject)
        .into_iter()
        .any(|record| record.id.as_str() == id);
    let gone = || {
        Problem::new(StatusCode::NOT_FOUND, "No such token")
            .detail("this session has no token with that id")
            .into_response()
    };
    if !owned {
        return gone();
    }
    match state.tokens.revoke(&TokenId::new(id.to_owned())) {
        true => ok(json!({ "revoked": id })),
        false => gone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::config::{AuthConfig, Mode};
    use std::net::Ipv4Addr;

    fn state() -> AuthState {
        let config = AuthConfig {
            mode: Mode::Password,
            ..AuthConfig::default()
        };
        AuthState::new(
            config,
            "production",
            vec!["https://docs.acme.com".to_owned()],
            Clock::manual(),
        )
        .expect("entropy")
        .0
    }

    fn form(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    fn here() -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(203, 0, 113, 7))
    }

    fn reader(via: &str) -> Principal {
        let mut principal = Principal::new("reader-1");
        principal.via = via.to_owned();
        principal.groups = ["partner".to_owned()].into_iter().collect();
        principal
    }

    async fn body_of(response: Response) -> Value {
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("a body");
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    }

    #[tokio::test]
    async fn a_registered_client_exchanges_its_secret_for_a_bearer_token() {
        let state = state();
        state
            .tokens
            .register_client("agent-1", "s3cret", &["partner".to_owned()].into());
        let response = grant(
            &state,
            here(),
            &form(&[
                ("grant_type", "client_credentials"),
                ("client_id", "agent-1"),
                ("client_secret", "s3cret"),
            ]),
        );
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_of(response).await;
        assert_eq!(body["token_type"], "Bearer");
        assert_eq!(body["expires_in"], tokens::ACCESS_TOKEN_TTL.as_secs());
        let token = body["access_token"].as_str().expect("a token");
        // The point of the endpoint: what comes out is accepted by the same
        // resolver the layer reads through.
        let record = state.tokens.resolve(token).expect("the token resolves");
        assert_eq!(record.principal().via, "client_credentials");
    }

    /// The granted groups are not in the response. A stolen secret should not
    /// also tell its holder what the credential is worth before spending it.
    #[tokio::test]
    async fn the_grant_does_not_disclose_the_clients_groups() {
        let state = state();
        state
            .tokens
            .register_client("agent-1", "s3cret", &["partner".to_owned()].into());
        let body = body_of(grant(
            &state,
            here(),
            &form(&[
                ("grant_type", "client_credentials"),
                ("client_id", "agent-1"),
                ("client_secret", "s3cret"),
            ]),
        ))
        .await;
        assert!(body.get("groups").is_none(), "{body}");
        assert!(!body.to_string().contains("partner"), "{body}");
    }

    /// An unknown client and a wrong secret read alike, so the endpoint does
    /// not answer "which client ids exist".
    #[tokio::test]
    async fn an_unknown_client_and_a_wrong_secret_read_alike() {
        let state = state();
        state
            .tokens
            .register_client("agent-1", "s3cret", &BTreeSet::new());
        let wrong_secret = grant(
            &state,
            here(),
            &form(&[
                ("grant_type", "client_credentials"),
                ("client_id", "agent-1"),
                ("client_secret", "wrong"),
            ]),
        );
        let no_client = grant(
            &state,
            here(),
            &form(&[
                ("grant_type", "client_credentials"),
                ("client_id", "nobody"),
                ("client_secret", "wrong"),
            ]),
        );
        assert_eq!(wrong_secret.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(no_client.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(body_of(wrong_secret).await, body_of(no_client).await);
    }

    /// Without this the endpoint is an online guessing oracle against every
    /// registered client, reachable by anyone.
    #[tokio::test]
    async fn the_grant_stops_answering_after_too_many_attempts() {
        let state = state();
        state
            .tokens
            .register_client("agent-1", "s3cret", &BTreeSet::new());
        let attempt = |secret: &str| {
            grant(
                &state,
                here(),
                &form(&[
                    ("grant_type", "client_credentials"),
                    ("client_id", "agent-1"),
                    ("client_secret", secret),
                ]),
            )
            .status()
        };
        for _ in 0..GRANT_ATTEMPTS {
            assert_eq!(attempt("wrong"), StatusCode::UNAUTHORIZED);
        }
        // Charged before the secret is compared, so the correct one is refused
        // too once the window is spent. A limit that only counted failures
        // would let an attacker through on the guess that happened to be right.
        assert_eq!(attempt("s3cret"), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn the_window_reopens_once_it_has_passed() {
        let state = state();
        state
            .tokens
            .register_client("agent-1", "s3cret", &BTreeSet::new());
        for _ in 0..=GRANT_ATTEMPTS {
            grant(
                &state,
                here(),
                &form(&[
                    ("grant_type", "client_credentials"),
                    ("client_id", "agent-1"),
                    ("client_secret", "wrong"),
                ]),
            );
        }
        state
            .clock
            .advance(std::time::Duration::from_millis(GRANT_WINDOW_MS as u64 + 1));
        let response = grant(
            &state,
            here(),
            &form(&[
                ("grant_type", "client_credentials"),
                ("client_id", "agent-1"),
                ("client_secret", "s3cret"),
            ]),
        );
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn another_grant_type_is_refused_by_name() {
        let state = state();
        let response = grant(
            &state,
            here(),
            &form(&[("grant_type", "authorization_code"), ("code", "x")]),
        );
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = body_of(response).await;
        assert!(
            body.to_string().contains("unsupported_grant_type"),
            "a client library reads this code: {body}"
        );
    }

    /// **The rule RFC 1508 exists for.** A leaked token that can mint a
    /// replacement renews itself faster than an operator can revoke it.
    #[tokio::test]
    async fn a_token_may_not_mint_a_token() {
        let state = state();
        for via in ["pat", "client_credentials"] {
            let response = mint(&state, &reader(via), &form(&[("label", "second")]));
            assert_eq!(
                response.status(),
                StatusCode::FORBIDDEN,
                "a `{via}` credential minted a token"
            );
        }
        // And a session may, so the test above is about the credential rather
        // than about minting being broken.
        let response = mint(&state, &reader("password"), &form(&[("label", "first")]));
        assert_eq!(response.status(), StatusCode::OK);
    }

    /// A reader cannot mint a credential stronger than the session minting it.
    #[tokio::test]
    async fn a_token_cannot_carry_a_group_the_reader_does_not_have() {
        let state = state();
        let body = body_of(mint(
            &state,
            &reader("password"),
            &form(&[("groups", "partner,admin")]),
        ))
        .await;
        assert_eq!(body["groups"], json!(["partner"]), "{body}");
    }

    /// The secret exists once, in the response that minted it.
    #[tokio::test]
    async fn the_list_carries_no_token_value() {
        let state = state();
        let who = reader("password");
        let minted = body_of(mint(&state, &who, &form(&[("label", "laptop")]))).await;
        let secret = minted["token"].as_str().expect("a token").to_owned();
        let listed = body_of(list(&state, &who)).await;
        assert_eq!(listed["tokens"][0]["label"], "laptop");
        assert!(
            !listed.to_string().contains(&secret),
            "a live credential in a list endpoint"
        );
    }

    /// Without the ownership check, any reader with a session could revoke any
    /// other reader's token by guessing an id.
    #[tokio::test]
    async fn a_reader_cannot_revoke_someone_elses_token() {
        let state = state();
        let mine = reader("password");
        let minted = body_of(mint(&state, &mine, &form(&[]))).await;
        let id = minted["id"].as_str().expect("an id").to_owned();
        let secret = minted["token"].as_str().expect("a token").to_owned();

        let mut stranger = Principal::new("reader-2");
        stranger.via = "password".to_owned();
        assert_eq!(
            revoke(&state, &stranger, &id).status(),
            StatusCode::NOT_FOUND
        );
        // Still usable, which is what the refusal has to mean.
        assert!(state.tokens.resolve(&secret).is_ok());

        assert_eq!(revoke(&state, &mine, &id).status(), StatusCode::OK);
        assert!(state.tokens.resolve(&secret).is_err());
    }

    /// A stranger's id and an id that never existed read alike, so the endpoint
    /// does not confirm that an id belongs to somebody.
    #[tokio::test]
    async fn a_missing_token_and_another_readers_read_alike() {
        let state = state();
        let mine = reader("password");
        let id = body_of(mint(&state, &mine, &form(&[]))).await["id"]
            .as_str()
            .expect("an id")
            .to_owned();
        let mut stranger = Principal::new("reader-2");
        stranger.via = "password".to_owned();
        let theirs = revoke(&state, &stranger, &id);
        let nowhere = revoke(&state, &stranger, "liy_nothing");
        assert_eq!(theirs.status(), nowhere.status());
        assert_eq!(body_of(theirs).await, body_of(nowhere).await);
    }
}
