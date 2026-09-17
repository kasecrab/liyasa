//! Agent access to private docs (AUTH-08).
//!
//! Personal access tokens and OAuth 2.1 client credentials, both reaching the
//! same `.md` routes and the same MCP server under the same group rules as a
//! browser session. "The same group rules" is the whole requirement: a token
//! that could read more than the reader who minted it would make every group
//! restriction on the site advisory.
//!
//! A token is shown once and stored as a hash, like a password. What is kept
//! is the prefix — enough to say which token was used in an audit line, not
//! enough to use.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::RwLock;
use std::time::Duration;

use liyasa_core::ids::{Fingerprint, TokenId};

use crate::auth::clock::{Clock, millis};
use crate::auth::random::{self, NoEntropy};
use crate::auth::session::Principal;

/// The visible prefix of a personal access token, so one found in a log or a
/// repository is recognizable as what it is.
pub const PAT_PREFIX: &str = "liy_pat_";
/// A client-credentials access token.
pub const AT_PREFIX: &str = "liy_at_";
/// How long an access token minted from client credentials lives.
pub const ACCESS_TOKEN_TTL: Duration = Duration::from_secs(3_600);
/// How much of a token is kept in the clear for identification.
const VISIBLE: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// AUTH-08: a reader's own token, carrying the reader's own groups.
    Personal,
    /// AUTH-08: OAuth 2.1 client credentials, carrying the client's groups.
    ClientCredentials,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub id: TokenId,
    pub kind: Kind,
    /// The subject this token acts as. For a personal token, the reader who
    /// minted it.
    pub subject: String,
    pub groups: BTreeSet<String>,
    pub label: String,
    /// What is shown in a list: `liy_pat_AbCd…`.
    pub hint: String,
    /// When it was minted, for the token list in the dashboard.
    pub created_ms: i64,
    hash: String,
    expires_ms: Option<i64>,
    revoked: bool,
}

impl Record {
    pub fn principal(&self) -> Principal {
        Principal {
            subject: self.subject.clone(),
            groups: self.groups.clone(),
            region: None,
            locale: None,
            data: BTreeMap::new(),
            role: crate::auth::roles::Role::Reader,
            via: match self.kind {
                Kind::Personal => "pat".to_owned(),
                Kind::ClientCredentials => "client_credentials".to_owned(),
            },
        }
    }
}

/// A token as it is handed over, once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issued {
    pub record: Record,
    /// The only time the secret exists outside a hash.
    pub secret: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rejected {
    Unknown,
    Expired,
    Revoked,
}

#[derive(Debug, Default)]
pub struct Tokens {
    by_hash: RwLock<BTreeMap<String, Record>>,
    /// Registered OAuth 2.1 clients: client id to (secret hash, groups).
    clients: RwLock<BTreeMap<String, (String, BTreeSet<String>)>>,
    clock: Clock,
}

impl Tokens {
    pub fn new(clock: Clock) -> Self {
        Self {
            by_hash: RwLock::new(BTreeMap::new()),
            clients: RwLock::new(BTreeMap::new()),
            clock,
        }
    }

    pub fn clock(&self) -> &Clock {
        &self.clock
    }

    /// Mints a personal access token for a reader.
    ///
    /// `groups` is intersected with the reader's own: AUTH-08 says "the same
    /// group rules", and a reader who could name a group they are not in would
    /// be escalating through the token endpoint.
    pub fn issue_personal(
        &self,
        reader: &Principal,
        label: &str,
        groups: &BTreeSet<String>,
        ttl: Option<Duration>,
    ) -> Result<Issued, NoEntropy> {
        let granted: BTreeSet<String> = match groups.is_empty() {
            true => reader.groups.clone(),
            false => groups.intersection(&reader.groups).cloned().collect(),
        };
        self.mint(
            Kind::Personal,
            PAT_PREFIX,
            &reader.subject,
            granted,
            label,
            ttl,
        )
    }

    /// Registers an OAuth 2.1 client. The secret is stored as a hash.
    pub fn register_client(&self, client_id: &str, client_secret: &str, groups: &BTreeSet<String>) {
        self.clients
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(client_id.to_owned(), (hash(client_secret), groups.clone()));
    }

    /// The client-credentials grant. No user is present, so the token acts as
    /// the client and carries the groups the client was registered with.
    pub fn client_credentials(
        &self,
        client_id: &str,
        client_secret: &str,
    ) -> Result<Issued, Rejected> {
        let registered = self
            .clients
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(client_id)
            .cloned();
        let Some((secret_hash, groups)) = registered else {
            return Err(Rejected::Unknown);
        };
        if !random::constant_time_eq(hash(client_secret).as_bytes(), secret_hash.as_bytes()) {
            return Err(Rejected::Unknown);
        }
        self.mint(
            Kind::ClientCredentials,
            AT_PREFIX,
            client_id,
            groups,
            client_id,
            Some(ACCESS_TOKEN_TTL),
        )
        .map_err(|_| Rejected::Unknown)
    }

    /// Resolves a presented token. The lookup is by hash, so the table never
    /// holds anything usable.
    pub fn resolve(&self, presented: &str) -> Result<Record, Rejected> {
        let now = self.clock.now_ms();
        let record = self
            .by_hash
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&hash(presented))
            .cloned();
        let Some(record) = record else {
            return Err(Rejected::Unknown);
        };
        if record.revoked {
            return Err(Rejected::Revoked);
        }
        if record.expires_ms.is_some_and(|at| now >= at) {
            return Err(Rejected::Expired);
        }
        Ok(record)
    }

    pub fn revoke(&self, id: &TokenId) -> bool {
        let mut tokens = self.by_hash.write().unwrap_or_else(|e| e.into_inner());
        for record in tokens.values_mut() {
            if &record.id == id {
                record.revoked = true;
                return true;
            }
        }
        false
    }

    /// Every token of one subject, for the token list in the dashboard. No
    /// secrets: a `Record` holds a hash and a hint.
    pub fn list(&self, subject: &str) -> Vec<Record> {
        self.by_hash
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .filter(|record| record.subject == subject)
            .cloned()
            .collect()
    }

    fn mint(
        &self,
        kind: Kind,
        prefix: &str,
        subject: &str,
        groups: BTreeSet<String>,
        label: &str,
        ttl: Option<Duration>,
    ) -> Result<Issued, NoEntropy> {
        let now = self.clock.now_ms();
        let secret = format!("{prefix}{}", random::token()?);
        let record = Record {
            id: TokenId::new(random::token()?),
            kind,
            subject: subject.to_owned(),
            groups,
            label: label.to_owned(),
            hint: format!("{}…", &secret[..VISIBLE.min(secret.len())]),
            created_ms: now,
            hash: hash(&secret),
            expires_ms: ttl.map(|ttl| now + millis(ttl)),
            revoked: false,
        };
        self.by_hash
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(record.hash.clone(), record.clone());
        Ok(Issued { record, secret })
    }
}

fn hash(secret: &str) -> String {
    Fingerprint::of(secret.as_bytes()).to_hex()
}

/// The token in an `Authorization: Bearer` header, if there is one.
pub fn bearer(header: Option<&str>) -> Option<&str> {
    let value = header?.trim();
    let (scheme, token) = value.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then(|| token.trim())
        .filter(|token| !token.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reader(groups: &[&str]) -> Principal {
        Principal::new("reader-1").with_groups(groups.iter().copied())
    }

    fn tokens() -> Tokens {
        Tokens::new(Clock::manual())
    }

    #[test]
    fn a_personal_token_carries_the_readers_own_groups() {
        let tokens = tokens();
        let issued = tokens
            .issue_personal(
                &reader(&["partner", "staff"]),
                "my laptop",
                &BTreeSet::new(),
                None,
            )
            .expect("a token");
        assert!(issued.secret.starts_with(PAT_PREFIX));

        let resolved = tokens.resolve(&issued.secret).expect("the token resolves");
        assert_eq!(
            resolved
                .groups
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["partner", "staff"]
        );
        assert_eq!(resolved.principal().subject, "reader-1");
        assert_eq!(resolved.principal().via, "pat");
    }

    #[test]
    fn a_token_cannot_be_given_a_group_its_reader_is_not_in() {
        let tokens = tokens();
        let asked: BTreeSet<String> = ["partner".to_owned(), "admin".to_owned()].into();
        let issued = tokens
            .issue_personal(&reader(&["partner"]), "l", &asked, None)
            .expect("a token");
        assert_eq!(
            issued.record.groups,
            ["partner".to_owned()].into(),
            "AUTH-08's `same group rules` has to hold at minting too"
        );
    }

    #[test]
    fn a_narrower_token_is_narrower() {
        let tokens = tokens();
        let asked: BTreeSet<String> = ["partner".to_owned()].into();
        let issued = tokens
            .issue_personal(&reader(&["partner", "staff"]), "read-only", &asked, None)
            .expect("a token");
        assert_eq!(issued.record.groups, asked);
    }

    #[test]
    fn a_token_is_stored_as_a_hash_with_a_hint_and_nothing_usable() {
        let tokens = tokens();
        let issued = tokens
            .issue_personal(&reader(&[]), "l", &BTreeSet::new(), None)
            .expect("a token");
        let listed = tokens.list("reader-1");
        assert_eq!(listed.len(), 1);
        assert!(listed[0].hint.starts_with(PAT_PREFIX));
        assert!(listed[0].hint.len() < issued.secret.len());
        assert!(
            !listed[0].hash.contains(&issued.secret),
            "the table must not hold the secret"
        );
        assert_ne!(listed[0].hash, issued.secret);
    }

    #[test]
    fn a_revoked_token_stops_working() {
        let tokens = tokens();
        let issued = tokens
            .issue_personal(&reader(&[]), "l", &BTreeSet::new(), None)
            .expect("a token");
        assert!(tokens.resolve(&issued.secret).is_ok());
        assert!(tokens.revoke(&issued.record.id));
        assert_eq!(tokens.resolve(&issued.secret), Err(Rejected::Revoked));
    }

    #[test]
    fn a_token_with_a_lifetime_expires() {
        let tokens = tokens();
        let issued = tokens
            .issue_personal(
                &reader(&[]),
                "ci",
                &BTreeSet::new(),
                Some(Duration::from_secs(60)),
            )
            .expect("a token");
        assert!(tokens.resolve(&issued.secret).is_ok());
        tokens.clock().advance(Duration::from_secs(60));
        assert_eq!(tokens.resolve(&issued.secret), Err(Rejected::Expired));
    }

    #[test]
    fn a_token_with_no_lifetime_does_not_expire() {
        let tokens = tokens();
        let issued = tokens
            .issue_personal(&reader(&[]), "l", &BTreeSet::new(), None)
            .expect("a token");
        tokens.clock().advance(Duration::from_secs(365 * 86_400));
        assert!(tokens.resolve(&issued.secret).is_ok());
    }

    #[test]
    fn a_token_nobody_issued_resolves_to_nothing() {
        let tokens = tokens();
        for junk in ["", "liy_pat_made-up", "not a token"] {
            assert_eq!(tokens.resolve(junk), Err(Rejected::Unknown), "{junk}");
        }
    }

    #[test]
    fn client_credentials_mint_a_token_with_the_clients_groups() {
        let tokens = tokens();
        let groups: BTreeSet<String> = ["partner".to_owned()].into();
        tokens.register_client("agent-1", "s3cret", &groups);

        let issued = tokens
            .client_credentials("agent-1", "s3cret")
            .expect("a grant");
        assert!(issued.secret.starts_with(AT_PREFIX));
        assert_eq!(issued.record.kind, Kind::ClientCredentials);
        assert_eq!(issued.record.groups, groups);
        assert_eq!(issued.record.principal().via, "client_credentials");

        tokens.clock().advance(ACCESS_TOKEN_TTL);
        assert_eq!(tokens.resolve(&issued.secret), Err(Rejected::Expired));
    }

    #[test]
    fn a_wrong_client_secret_is_the_same_refusal_as_an_unknown_client() {
        let tokens = tokens();
        tokens.register_client("agent-1", "s3cret", &BTreeSet::new());
        assert_eq!(
            tokens.client_credentials("agent-1", "wrong"),
            Err(Rejected::Unknown)
        );
        assert_eq!(
            tokens.client_credentials("agent-2", "s3cret"),
            Err(Rejected::Unknown),
            "a refusal must not say which half was wrong"
        );
    }

    #[test]
    fn a_bearer_header_is_read_and_anything_else_is_not() {
        assert_eq!(bearer(Some("Bearer liy_pat_x")), Some("liy_pat_x"));
        assert_eq!(bearer(Some("bearer liy_pat_x")), Some("liy_pat_x"));
        assert_eq!(bearer(Some("  Bearer   liy_pat_x  ")), Some("liy_pat_x"));
        for wrong in ["Basic abc", "liy_pat_x", "Bearer", "Bearer ", ""] {
            assert_eq!(bearer(Some(wrong)), None, "{wrong}");
        }
        assert_eq!(bearer(None), None);
    }

    #[test]
    fn two_tokens_are_never_the_same_token() {
        let tokens = tokens();
        let mut seen = BTreeSet::new();
        for _ in 0..64 {
            let issued = tokens
                .issue_personal(&reader(&[]), "l", &BTreeSet::new(), None)
                .expect("a token");
            assert!(seen.insert(issued.secret.clone()));
            assert!(seen.insert(issued.record.id.to_string()));
        }
    }
}
