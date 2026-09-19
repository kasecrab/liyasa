//! Platform-managed access: invitations, allowed domains and magic links
//! (AUTH-05, AUTH-09).
//!
//! Three properties of AUTH-09 shape the whole module and each one is easy to
//! lose:
//!
//! * **The response never says whether the address is known.** A sign-in form
//!   that answers differently for a known and an unknown address is an account
//!   enumeration endpoint, so [`Magic::request`] returns the same shape either
//!   way and the caller has nothing to branch on.
//! * **The link is bound to the browser that asked for it.** A nonce cookie is
//!   set on the requesting browser and the link is worthless without it, so
//!   forwarding the email — or an inbox scanner following the link — does not
//!   sign anyone in.
//! * **Opening it in the wrong browser is a named case, not an error.** Mail
//!   clients open links in a different profile all the time. The reader is
//!   told exactly that and offered a link to the browser they are in, which is
//!   the difference between a working product and a support ticket.
//!
//! Readers are stored as a hashed address and a display name (AUTH-05). The
//! address itself is never written down.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::RwLock;
use std::time::Duration;

use liyasa_core::ids::Fingerprint;

use crate::auth::clock::{Clock, millis};
use crate::auth::random::{self, NoEntropy, constant_time_eq};
use crate::auth::session::Principal;

/// AUTH-09: three per hour per address.
pub const REQUESTS_PER_HOUR: usize = 3;
const HOUR: Duration = Duration::from_secs(3_600);

/// A reader the platform knows. AUTH-05: a hashed address and a display name,
/// and nothing else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reader {
    pub address_hash: String,
    pub display_name: String,
    pub groups: BTreeSet<String>,
}

impl Reader {
    pub fn principal(&self) -> Principal {
        Principal {
            subject: self.address_hash.clone(),
            groups: self.groups.clone(),
            region: None,
            locale: None,
            data: BTreeMap::from([(
                "name".to_owned(),
                serde_json::Value::String(self.display_name.clone()),
            )]),
            grant: None,
            role: crate::auth::roles::Role::Reader,
            via: "managed".to_owned(),
        }
    }
}

#[derive(Debug, Clone)]
struct Link {
    address_hash: String,
    nonce_hash: String,
    created_ms: i64,
}

/// What `POST /_liyasa/auth/magic` produces. The nonce is set as a cookie on
/// the requesting browser whatever happened; `token` is `None` when there was
/// nobody to send to, and the handler must not let that show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requested {
    pub nonce: String,
    /// `Some` only when a link was actually minted. Never reaches the
    /// response body — it goes in the email.
    pub token: Option<String>,
    /// AUTH-09: rate limited per address, and the limit is not an oracle
    /// either. The response is the same; this is for the log.
    pub rate_limited: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Consumed {
    SignedIn(Box<Reader>),
    /// The link is good and this browser is not the one that asked for it.
    /// AUTH-09 wants this said in so many words.
    WrongBrowser,
    Expired,
    Unknown,
}

impl Consumed {
    /// The sentence AUTH-09 asks for, rather than a generic failure.
    pub fn message(&self) -> &'static str {
        match self {
            Consumed::SignedIn(_) => "signed in",
            Consumed::WrongBrowser => {
                "This link was opened in a different browser from the one that asked for it, \
                 which is usually your email app opening links somewhere else. Ask for a new \
                 link here and it will work in this browser."
            }
            Consumed::Expired => "This link has expired. Ask for a new one.",
            Consumed::Unknown => "This link has already been used. Ask for a new one.",
        }
    }

    /// Whether the page offers to resend to the browser the reader is in.
    pub fn offers_resend(&self) -> bool {
        matches!(
            self,
            Consumed::WrongBrowser | Consumed::Expired | Consumed::Unknown
        )
    }
}

#[derive(Debug)]
pub struct Magic {
    links: RwLock<BTreeMap<String, Link>>,
    /// Address hash to the instants it asked, newest last.
    requests: RwLock<BTreeMap<String, Vec<i64>>>,
    readers: RwLock<BTreeMap<String, Reader>>,
    /// `@acme.com` entries from `auth.managed.allowDomains`.
    allow_domains: Vec<String>,
    /// Salts the address hash, so a stolen database is not a rainbow table of
    /// every address on the internet.
    salt: [u8; 32],
    ttl: Duration,
    clock: Clock,
}

impl Magic {
    pub fn new(allow_domains: Vec<String>, ttl: Duration, clock: Clock) -> Result<Self, NoEntropy> {
        Ok(Self {
            links: RwLock::new(BTreeMap::new()),
            requests: RwLock::new(BTreeMap::new()),
            readers: RwLock::new(BTreeMap::new()),
            allow_domains: allow_domains
                .into_iter()
                .map(|domain| normalize_domain(&domain))
                .collect(),
            salt: random::bytes::<32>()?,
            ttl,
            clock,
        })
    }

    pub fn clock(&self) -> &Clock {
        &self.clock
    }

    /// The stored identity of an address. The address is not recoverable from
    /// it, and two sites hashing the same address get different values.
    pub fn address_hash(&self, address: &str) -> String {
        Fingerprint::of_parts([&self.salt[..], normalize_address(address).as_bytes()]).to_hex()
    }

    /// AUTH-05: invite a reader by address. Only the hash is kept.
    pub fn invite(&self, address: &str, display_name: &str, groups: &[&str]) -> Reader {
        let reader = Reader {
            address_hash: self.address_hash(address),
            display_name: display_name.to_owned(),
            groups: groups.iter().map(|g| (*g).to_owned()).collect(),
        };
        self.readers
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(reader.address_hash.clone(), reader.clone());
        reader
    }

    pub fn revoke(&self, address: &str) -> bool {
        self.readers
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.address_hash(address))
            .is_some()
    }

    /// Whether this address may sign in at all: invited, or in an allowed
    /// domain. Never observable from outside — it decides whether an email is
    /// sent, not what the response says.
    pub fn is_allowed(&self, address: &str) -> bool {
        if self
            .readers
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(&self.address_hash(address))
        {
            return true;
        }
        let normalized = normalize_address(address);
        let Some((_, domain)) = normalized.split_once('@') else {
            return false;
        };
        self.allow_domains.iter().any(|allowed| allowed == domain)
    }

    /// `POST /_liyasa/auth/magic`.
    pub fn request(&self, address: &str) -> Result<Requested, NoEntropy> {
        let nonce = random::token()?;
        let hash = self.address_hash(address);
        let now = self.clock.now_ms();
        let rate_limited = !self.charge(&hash, now);

        // Both refusals produce the same outward result. They are separate
        // fields so the handler can log them and the tests can tell them
        // apart; neither may reach the response body.
        if rate_limited || !self.is_allowed(address) {
            return Ok(Requested {
                nonce,
                token: None,
                rate_limited,
            });
        }

        let token = random::token()?;
        self.links
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(
                token.clone(),
                Link {
                    address_hash: hash,
                    nonce_hash: hash_nonce(&nonce),
                    created_ms: now,
                },
            );
        Ok(Requested {
            nonce,
            token: Some(token),
            rate_limited: false,
        })
    }

    /// `GET /_liyasa/auth/magic/<token>`, with the nonce cookie this browser
    /// is carrying.
    pub fn consume(&self, token: &str, nonce: Option<&str>) -> Consumed {
        let now = self.clock.now_ms();
        let link = self
            .links
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(token)
            .cloned();
        let Some(link) = link else {
            return Consumed::Unknown;
        };
        if now.saturating_sub(link.created_ms) >= millis(self.ttl) {
            self.links
                .write()
                .unwrap_or_else(|e| e.into_inner())
                .remove(token);
            return Consumed::Expired;
        }
        let presented = nonce.map(hash_nonce).unwrap_or_default();
        if !constant_time_eq(presented.as_bytes(), link.nonce_hash.as_bytes()) {
            // Deliberately not consumed: the browser that asked for it has not
            // used it yet, and burning the link here would turn a mail client
            // opening the wrong profile into a lockout.
            return Consumed::WrongBrowser;
        }
        let reader = self
            .readers
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&link.address_hash)
            .cloned();
        // Single use, whatever happens next.
        self.links
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(token);
        match reader {
            Some(reader) => Consumed::SignedIn(Box::new(reader)),
            // Invited through an allowed domain rather than by name: the
            // reader exists from this moment, with the address hash as their
            // whole identity.
            None => Consumed::SignedIn(Box::new(Reader {
                address_hash: link.address_hash,
                display_name: String::new(),
                groups: BTreeSet::new(),
            })),
        }
    }

    pub fn live_links(&self) -> usize {
        self.links.read().unwrap_or_else(|e| e.into_inner()).len()
    }

    fn charge(&self, address_hash: &str, now: i64) -> bool {
        let window = millis(HOUR);
        let mut requests = self.requests.write().unwrap_or_else(|e| e.into_inner());
        let times = requests.entry(address_hash.to_owned()).or_default();
        times.retain(|at| now.saturating_sub(*at) < window);
        times.push(now);
        times.len() <= REQUESTS_PER_HOUR
    }
}

fn hash_nonce(nonce: &str) -> String {
    Fingerprint::of(nonce.as_bytes()).to_hex()
}

/// An address as it is compared: trimmed and lower cased. No further
/// normalization — stripping dots or `+tags` is provider-specific and guessing
/// wrong would merge two people's accounts.
pub fn normalize_address(address: &str) -> String {
    address.trim().to_ascii_lowercase()
}

fn normalize_domain(domain: &str) -> String {
    domain.trim().trim_start_matches('@').to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn magic() -> Magic {
        Magic::new(
            vec!["@acme.com".to_owned()],
            Duration::from_secs(900),
            Clock::manual(),
        )
        .expect("entropy")
    }

    #[test]
    fn an_invited_reader_signs_in_through_the_link_they_asked_for() {
        let magic = magic();
        magic.invite("Reader@Example.com", "A Reader", &["partner"]);
        let requested = magic.request("reader@example.com").expect("a request");
        let token = requested.token.clone().expect("a link was minted");

        let Consumed::SignedIn(reader) = magic.consume(&token, Some(&requested.nonce)) else {
            panic!("the reader should be signed in");
        };
        assert_eq!(reader.display_name, "A Reader");
        assert!(reader.groups.contains("partner"));
        assert_eq!(
            reader.principal().subject,
            magic.address_hash("reader@example.com")
        );
    }

    #[test]
    fn a_link_is_single_use() {
        let magic = magic();
        magic.invite("r@example.com", "R", &[]);
        let requested = magic.request("r@example.com").expect("a request");
        let token = requested.token.clone().expect("a link");
        assert!(matches!(
            magic.consume(&token, Some(&requested.nonce)),
            Consumed::SignedIn(_)
        ));
        assert_eq!(
            magic.consume(&token, Some(&requested.nonce)),
            Consumed::Unknown
        );
        assert_eq!(magic.live_links(), 0);
    }

    #[test]
    fn a_forwarded_link_does_not_sign_the_other_browser_in() {
        let magic = magic();
        magic.invite("r@example.com", "R", &[]);
        let requested = magic.request("r@example.com").expect("a request");
        let token = requested.token.clone().expect("a link");

        assert_eq!(magic.consume(&token, None), Consumed::WrongBrowser);
        assert_eq!(
            magic.consume(&token, Some("somebody-elses-nonce")),
            Consumed::WrongBrowser
        );
        // And the reader who asked for it can still use it.
        assert!(matches!(
            magic.consume(&token, Some(&requested.nonce)),
            Consumed::SignedIn(_)
        ));
    }

    #[test]
    fn the_wrong_browser_page_says_what_happened_and_offers_a_new_link() {
        let wrong = Consumed::WrongBrowser;
        let message = wrong.message();
        assert!(message.contains("different browser"), "{message}");
        assert!(message.contains("email app"), "{message}");
        assert!(message.contains("new link"), "{message}");
        assert!(wrong.offers_resend());
        assert!(
            !Consumed::SignedIn(Box::new(Reader {
                address_hash: String::new(),
                display_name: String::new(),
                groups: BTreeSet::new(),
            }))
            .offers_resend()
        );
    }

    #[test]
    fn a_link_expires_after_the_configured_time() {
        let magic = magic();
        magic.invite("r@example.com", "R", &[]);
        let requested = magic.request("r@example.com").expect("a request");
        let token = requested.token.clone().expect("a link");

        magic.clock().advance(Duration::from_secs(899));
        assert!(matches!(
            magic.consume(&token, Some(&requested.nonce)),
            Consumed::SignedIn(_)
        ));

        let requested = magic.request("r@example.com").expect("a request");
        let token = requested.token.clone().expect("a link");
        magic.clock().advance(Duration::from_secs(900));
        assert_eq!(
            magic.consume(&token, Some(&requested.nonce)),
            Consumed::Expired
        );
        assert_eq!(magic.live_links(), 0, "an expired link is dropped");
    }

    #[test]
    fn the_response_is_identical_whether_or_not_the_address_is_known() {
        let magic = magic();
        magic.invite("known@example.com", "K", &[]);
        let known = magic.request("known@example.com").expect("a request");
        let unknown = magic.request("nobody@example.com").expect("a request");

        // The one thing a caller can see is the nonce, and both have one of
        // the same shape. Whether a link exists is not in the result the
        // handler writes.
        assert_eq!(known.nonce.len(), unknown.nonce.len());
        assert!(!known.rate_limited && !unknown.rate_limited);
        assert!(known.token.is_some());
        assert!(unknown.token.is_none(), "no link is sent to nobody");
    }

    #[test]
    fn an_allowed_domain_signs_in_without_an_invitation() {
        let magic = magic();
        assert!(magic.is_allowed("anyone@acme.com"));
        assert!(magic.is_allowed("Anyone@ACME.com"));
        assert!(!magic.is_allowed("anyone@notacme.com"));
        assert!(
            !magic.is_allowed("anyone@sub.acme.com"),
            "a subdomain is not the domain"
        );

        let requested = magic.request("anyone@acme.com").expect("a request");
        let token = requested.token.clone().expect("a link");
        let Consumed::SignedIn(reader) = magic.consume(&token, Some(&requested.nonce)) else {
            panic!("an allowed domain signs in");
        };
        assert_eq!(reader.address_hash, magic.address_hash("anyone@acme.com"));
    }

    #[test]
    fn three_requests_an_hour_and_then_no_more() {
        let magic = magic();
        magic.invite("r@example.com", "R", &[]);
        for _ in 0..REQUESTS_PER_HOUR {
            let requested = magic.request("r@example.com").expect("a request");
            assert!(!requested.rate_limited);
            assert!(requested.token.is_some());
        }
        let refused = magic.request("r@example.com").expect("a request");
        assert!(refused.rate_limited);
        assert!(refused.token.is_none(), "no link past the limit");
        assert!(
            !refused.nonce.is_empty(),
            "the response still looks the same to whoever asked"
        );

        magic.clock().advance(HOUR);
        assert!(
            magic
                .request("r@example.com")
                .expect("a request")
                .token
                .is_some()
        );
    }

    #[test]
    fn the_limit_is_per_address_rather_than_for_everyone() {
        let magic = magic();
        magic.invite("a@example.com", "A", &[]);
        magic.invite("b@example.com", "B", &[]);
        for _ in 0..REQUESTS_PER_HOUR + 2 {
            magic.request("a@example.com").expect("a request");
        }
        assert!(
            magic
                .request("b@example.com")
                .expect("a request")
                .token
                .is_some()
        );
    }

    #[test]
    fn an_address_is_stored_only_as_a_salted_hash() {
        let magic = magic();
        let reader = magic.invite("secret@example.com", "S", &[]);
        assert!(!reader.address_hash.contains("secret"));
        assert!(!reader.address_hash.contains('@'));
        assert_eq!(reader.address_hash.len(), 64);

        // A second site hashes the same address differently.
        let elsewhere =
            Magic::new(Vec::new(), Duration::from_secs(900), Clock::manual()).expect("entropy");
        assert_ne!(
            magic.address_hash("secret@example.com"),
            elsewhere.address_hash("secret@example.com")
        );
    }

    #[test]
    fn an_address_is_compared_case_insensitively_and_nothing_further() {
        let magic = magic();
        assert_eq!(
            magic.address_hash(" Reader@Example.com "),
            magic.address_hash("reader@example.com")
        );
        assert_ne!(
            magic.address_hash("reader+docs@example.com"),
            magic.address_hash("reader@example.com"),
            "stripping a tag would merge two accounts at some providers"
        );
    }

    #[test]
    fn revoking_a_reader_stops_them_signing_in() {
        let magic = magic();
        magic.invite("r@example.com", "R", &[]);
        assert!(magic.revoke("r@example.com"));
        assert!(!magic.is_allowed("r@example.com"));
        assert!(
            magic
                .request("r@example.com")
                .expect("a request")
                .token
                .is_none()
        );
    }

    #[test]
    fn an_unknown_token_is_not_an_oracle_either() {
        let magic = magic();
        assert_eq!(magic.consume("made-up", Some("n")), Consumed::Unknown);
        assert_eq!(magic.consume("", None), Consumed::Unknown);
    }
}
