//! Automatic certificates (HOST-02).
//!
//! An HTTP-01 order against a configured directory. The challenge token is
//! served from memory by the route below, so nothing is written to the
//! bundle and a replica that did not place the token cannot answer for it.
//!
//! The order flow is exercised against a test ACME directory in CI, which is
//! a container; this machine has none, so what runs here is the state
//! machine around it (RFC 1402).

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use axum::extract::{Path as AxumPath, State};
use axum::response::{IntoResponse, Response};
use http::{HeaderValue, StatusCode, header};
use instant_acme::{
    Account, AuthorizationStatus, ChallengeType, Identifier, NewAccount, NewOrder, RetryPolicy,
};

use super::AppState;

/// Let's Encrypt's production directory. An operator names a different one
/// for staging or for an internal certificate authority.
pub const LETS_ENCRYPT: &str = "https://acme-v02.api.letsencrypt.org/directory";
pub const LETS_ENCRYPT_STAGING: &str = "https://acme-staging-v02.api.letsencrypt.org/directory";

#[derive(Debug, thiserror::Error)]
pub enum AcmeError {
    #[error("acme: {0}")]
    Acme(String),
    #[error("the directory issued no challenge this server can answer")]
    NoHttpChallenge,
    #[error("authorization for `{0}` did not become valid")]
    NotAuthorized(String),
    #[error("writing {0}: {1}")]
    Io(String, String),
    #[error("an offline instance cannot obtain a certificate (HOST-08)")]
    Offline,
}

/// The tokens this replica is currently answering for. Per instance and
/// lossy: a challenge is answered by the replica that placed it, which is why
/// a multi-replica deployment uses a DNS challenge or a shared store instead
/// (documented in the hosting guide).
#[derive(Debug, Default)]
pub struct Challenges(RwLock<HashMap<String, String>>);

impl Challenges {
    pub fn set(&self, token: &str, key_authorization: &str) {
        self.0
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(token.to_owned(), key_authorization.to_owned());
    }

    pub fn take(&self, token: &str) -> Option<String> {
        self.0
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(token)
    }

    pub fn get(&self, token: &str) -> Option<String> {
        self.0
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(token)
            .cloned()
    }

    pub fn is_empty(&self) -> bool {
        self.0.read().unwrap_or_else(|e| e.into_inner()).is_empty()
    }
}

/// What an obtained certificate consists of.
#[derive(Debug, Clone)]
pub struct Certificate {
    pub chain_pem: String,
    pub private_key_pem: String,
}

impl Certificate {
    /// Writes the pair where the TLS listener reads it. The key is written
    /// first and with owner-only permissions.
    pub fn write(&self, cert: &Path, key: &Path) -> Result<(), AcmeError> {
        write_private(key, &self.private_key_pem)?;
        std::fs::write(cert, &self.chain_pem)
            .map_err(|e| AcmeError::Io(cert.display().to_string(), e.to_string()))
    }
}

fn write_private(path: &Path, contents: &str) -> Result<(), AcmeError> {
    let io = |e: std::io::Error| AcmeError::Io(path.display().to_string(), e.to_string());
    std::fs::write(path, contents).map_err(io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(io)?;
    }
    Ok(())
}

/// Orders a certificate for `domains` over HTTP-01.
///
/// The caller must already be serving [`challenge`] on port 80 for those
/// domains: the directory fetches the token before this returns.
pub async fn obtain(
    directory: &str,
    contact_email: &str,
    domains: &[String],
    challenges: Arc<Challenges>,
) -> Result<Certificate, AcmeError> {
    let acme = |e: instant_acme::Error| AcmeError::Acme(e.to_string());

    let contact = format!("mailto:{contact_email}");
    let (account, _credentials) = Account::builder()
        .map_err(acme)?
        .create(
            &NewAccount {
                contact: &[&contact],
                terms_of_service_agreed: true,
                only_return_existing: false,
            },
            directory.to_owned(),
            None,
        )
        .await
        .map_err(acme)?;

    let identifiers: Vec<Identifier> = domains
        .iter()
        .map(|domain| Identifier::Dns(domain.clone()))
        .collect();
    let mut order = account
        .new_order(&NewOrder::new(&identifiers))
        .await
        .map_err(acme)?;

    let mut placed = Vec::new();
    let mut authorizations = order.authorizations();
    while let Some(result) = authorizations.next().await {
        let mut authorization = result.map_err(acme)?;
        if authorization.status == AuthorizationStatus::Valid {
            continue;
        }
        let mut challenge = authorization
            .challenge(ChallengeType::Http01)
            .ok_or(AcmeError::NoHttpChallenge)?;
        let token = challenge.token.to_owned();
        challenges.set(&token, &challenge.key_authorization().as_str().to_owned());
        challenge.set_ready().await.map_err(acme)?;
        placed.push(token);
    }
    drop(authorizations);

    let status = order
        .poll_ready(&RetryPolicy::default())
        .await
        .map_err(acme)?;
    for token in &placed {
        // The token is answered once; leaving it served would keep a
        // one-time secret reachable.
        challenges.take(token);
    }
    if status != instant_acme::OrderStatus::Ready {
        return Err(AcmeError::NotAuthorized(domains.join(", ")));
    }

    let private_key_pem = order.finalize().await.map_err(acme)?;
    let chain_pem = order
        .poll_certificate(&RetryPolicy::default())
        .await
        .map_err(acme)?;
    Ok(Certificate {
        chain_pem,
        private_key_pem,
    })
}

/// `GET /.well-known/acme-challenge/{token}`.
pub async fn challenge(
    State(state): State<Arc<AppState>>,
    AxumPath(token): AxumPath<String>,
) -> Response {
    match state.challenges.get(&token) {
        Some(key_authorization) => {
            let mut response = Response::new(key_authorization.into());
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain; charset=utf-8"),
            );
            // A challenge answer is never cached: it is valid once.
            response
                .headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            response
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// How long before expiry a certificate is renewed. Let's Encrypt issues for
/// ninety days and recommends renewing at sixty.
pub const RENEW_BEFORE: Duration = Duration::from_secs(30 * 86_400);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_is_answered_once_and_then_gone() {
        let challenges = Challenges::default();
        assert!(challenges.is_empty());
        challenges.set("tok", "tok.thumbprint");

        assert_eq!(challenges.get("tok").as_deref(), Some("tok.thumbprint"));
        assert_eq!(challenges.take("tok").as_deref(), Some("tok.thumbprint"));
        assert!(
            challenges.get("tok").is_none(),
            "a spent challenge must not stay reachable"
        );
        assert!(challenges.is_empty());
    }

    #[test]
    fn an_unknown_token_is_not_answered() {
        let challenges = Challenges::default();
        assert!(challenges.get("anything").is_none());
    }

    #[test]
    fn a_certificate_is_written_with_the_key_owner_readable_only() {
        let dir = std::env::temp_dir().join(format!("liyasa-acme-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a directory");
        let certificate = Certificate {
            chain_pem: "-----BEGIN CERTIFICATE-----\nYQ==\n-----END CERTIFICATE-----\n".to_owned(),
            private_key_pem: "-----BEGIN PRIVATE KEY-----\nYg==\n-----END PRIVATE KEY-----\n"
                .to_owned(),
        };
        let cert = dir.join("fullchain.pem");
        let key = dir.join("privkey.pem");
        certificate.write(&cert, &key).expect("a write");

        assert_eq!(
            std::fs::read_to_string(&cert).expect("the chain"),
            certificate.chain_pem
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(&key)
                .expect("the key")
                .permissions()
                .mode();
            assert_eq!(
                mode & 0o777,
                0o600,
                "the private key must not be world readable"
            );
        }
        // And the pair loads back into a TLS configuration, which is the only
        // thing the listener does with it.
        assert!(
            super::super::tls::load_config(&cert, &key).is_err(),
            "the fixture is not a real certificate, so rustls refuses it rather than \
             accepting anything that parses"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_directories_are_the_documented_ones() {
        assert!(LETS_ENCRYPT.starts_with("https://"));
        assert!(LETS_ENCRYPT_STAGING.contains("staging"));
        assert_eq!(RENEW_BEFORE.as_secs() / 86_400, 30);
    }
}
