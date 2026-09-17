//! Custom domains: ownership, verification, certificates, aliases and
//! subpaths (HOST-20, HOST-22, HOST-23).
//!
//! The flow is deliberately three separable steps, because each one fails for
//! its own reason and an operator staring at a spinner cannot tell them apart:
//!
//! 1. **claim** — the host is reserved for a project and a TXT challenge is
//!    minted. A host another organization already owns is refused unless the
//!    claim is a contest, and a contest is settled by the same TXT proof plus
//!    a notice to the current owner (HOST-20).
//! 2. **verify** — the TXT record proves ownership and the CNAME (or, at an
//!    apex where CNAME is not allowed, the address records) proves the host
//!    actually points here.
//! 3. **certify** — a certificate is ordered for the verified host, and
//!    renewed before it ages out.
//!
//! Where a host is not the primary one it is an alias and redirects to the
//! primary with a 301 (HOST-23), and each host carries its own base path so one
//! instance serves `acme.com/docs` and `docs.acme.com` at once (HOST-22).

use std::collections::BTreeMap;
use std::sync::RwLock;
use std::time::Duration;

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::ids::{OrgId, ProjectId};
use liyasa_core::net::BoxFut;

use crate::auth::clock::Clock;
use crate::auth::dns::{self, Resolver};
use crate::auth::random;
use crate::routes::acme::Certificate;

/// The label the ownership challenge is published under.
pub const CHALLENGE_PREFIX: &str = "_liyasa-challenge";
/// The value prefix, so an unrelated TXT record on the same name cannot be
/// mistaken for a challenge.
pub const CHALLENGE_VALUE_PREFIX: &str = "liyasa-site-verification=";
/// How long a certificate is used before it is renewed, matching the ACME
/// module's own age check.
pub const RENEW_AFTER: Duration = Duration::from_secs(60 * 86_400);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Claimed; the TXT record is not there yet.
    Pending,
    /// TXT and the pointing record both check out.
    Verified,
    /// Verified and holding a certificate.
    Certified,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Domain {
    pub host: String,
    pub project: ProjectId,
    pub org: OrgId,
    /// HOST-22. `""` for a host serving the site at its root, else a path with
    /// a leading and no trailing slash.
    pub base_path: String,
    pub env: String,
    /// HOST-23: exactly one host per project and environment is primary; the
    /// rest redirect to it.
    pub primary: bool,
    pub state: State,
    pub challenge: String,
    pub certified_ms: Option<i64>,
}

impl Domain {
    /// The name the TXT challenge is published at.
    pub fn challenge_name(&self) -> String {
        format!("{CHALLENGE_PREFIX}.{}", self.host)
    }

    /// The whole record value, which is what the operator pastes into their
    /// DNS console.
    pub fn challenge_value(&self) -> String {
        format!("{CHALLENGE_VALUE_PREFIX}{}", self.challenge)
    }

    pub fn is_certified(&self) -> bool {
        self.state == State::Certified
    }
}

/// What a claim asks for.
#[derive(Debug, Clone)]
pub struct Request {
    pub host: String,
    pub project: ProjectId,
    pub org: OrgId,
    pub base_path: String,
    pub env: String,
    pub primary: bool,
    /// HOST-20: a claim on a host another organization owns is refused unless
    /// it says it is contesting, and even then it is settled by TXT proof.
    pub contest: bool,
}

impl Request {
    pub fn new(host: &str, project: ProjectId, org: OrgId) -> Self {
        Self {
            host: dns::normalize(host),
            project,
            org,
            base_path: String::new(),
            env: "production".to_owned(),
            primary: true,
            contest: false,
        }
    }

    pub fn base_path(mut self, path: &str) -> Self {
        self.base_path = path.to_owned();
        self
    }

    pub fn env(mut self, env: &str) -> Self {
        self.env = env.to_owned();
        self
    }

    pub fn alias(mut self) -> Self {
        self.primary = false;
        self
    }

    pub fn contest(mut self) -> Self {
        self.contest = true;
        self
    }
}

/// What the current owner is told when their domain is taken from them
/// (HOST-20: "notifies the current owner").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerNotice {
    pub host: String,
    pub previous_org: OrgId,
    pub previous_project: ProjectId,
    pub claiming_org: OrgId,
    pub at_ms: i64,
}

/// Where a certificate comes from. The production implementation orders one
/// over ACME; a test supplies its own rather than reaching a directory.
pub trait Issuer: std::fmt::Debug + Send + Sync {
    fn issue<'a>(&'a self, hosts: &'a [String]) -> BoxFut<'a, Result<Certificate, String>>;
}

/// An issuer that refuses, which is what an offline instance has (HOST-08).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoIssuer;

impl Issuer for NoIssuer {
    fn issue<'a>(&'a self, _hosts: &'a [String]) -> BoxFut<'a, Result<Certificate, String>> {
        Box::pin(async {
            Err("an offline instance cannot obtain a certificate (HOST-08)".to_owned())
        })
    }
}

#[derive(Debug)]
pub struct Registry {
    domains: RwLock<BTreeMap<String, Domain>>,
    /// A claim on a host another project owns, held here until its TXT proof
    /// checks out. Keeping it out of `domains` is what makes a bare claim
    /// unable to take a live domain off its project.
    contests: RwLock<BTreeMap<String, Domain>>,
    notices: RwLock<Vec<OwnerNotice>>,
    certificates: RwLock<BTreeMap<String, Certificate>>,
    /// Where a CNAME must point, and the addresses an apex ALIAS must resolve
    /// to. Both come from the deployment's configuration.
    target: String,
    target_addresses: Vec<std::net::IpAddr>,
    clock: Clock,
}

impl Registry {
    pub fn new(target: &str, clock: Clock) -> Self {
        Self {
            domains: RwLock::new(BTreeMap::new()),
            contests: RwLock::new(BTreeMap::new()),
            notices: RwLock::new(Vec::new()),
            certificates: RwLock::new(BTreeMap::new()),
            target: dns::normalize(target),
            target_addresses: Vec::new(),
            clock,
        }
    }

    pub fn with_addresses(mut self, addresses: impl IntoIterator<Item = std::net::IpAddr>) -> Self {
        self.target_addresses = addresses.into_iter().collect();
        self
    }

    pub fn target(&self) -> &str {
        &self.target
    }

    pub fn clock(&self) -> &Clock {
        &self.clock
    }

    /// Step one. Reserves the host and mints the challenge.
    pub fn claim(&self, request: Request) -> Result<Domain, Diagnostic> {
        let host = dns::normalize(&request.host);
        if host.is_empty() {
            return Err(Diagnostic::new(code::E0801, "a domain needs a host name")
                .help("add the host you want the site served on, such as `docs.example.com`"));
        }
        let base_path = check_base_path(&request.base_path)?;

        let existing = self
            .domains
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&host)
            .cloned();
        if let Some(existing) = &existing {
            if existing.org != request.org && !request.contest {
                return Err(conflict(&host, existing));
            }
            if existing.org == request.org
                && existing.project != request.project
                && !request.contest
            {
                return Err(conflict(&host, existing));
            }
        }

        let domain = Domain {
            host: host.clone(),
            project: request.project,
            org: request.org,
            base_path,
            env: request.env,
            primary: request.primary,
            state: State::Pending,
            challenge: random::token().map_err(|_| {
                Diagnostic::new(code::E0801, "no randomness for a domain challenge")
            })?,
            certified_ms: None,
        };
        // A contest does NOT displace the owner here: the current record stays
        // until the challenge is proven, so a claim alone cannot take a
        // domain off a project.
        match existing {
            Some(existing)
                if existing.org != request.org || existing.project != request.project =>
            {
                self.pending_contests().insert(host.clone(), domain.clone());
            }
            _ => {
                self.write().insert(host, domain.clone());
            }
        }
        Ok(domain)
    }

    /// Step two. Proves ownership with the TXT record, then proves the host
    /// points here with a CNAME or, at an apex, with address records.
    pub async fn verify(&self, host: &str, resolver: &dyn Resolver) -> Result<Domain, Diagnostic> {
        let host = dns::normalize(host);
        let contested = self.pending_contests().get(&host).cloned();
        let domain = contested
            .clone()
            .or_else(|| self.read().get(&host).cloned())
            .ok_or_else(|| {
                Diagnostic::new(
                    code::E0801,
                    format!("`{host}` has not been claimed by any project"),
                )
                .help("add the domain before verifying it")
            })?;

        let name = domain.challenge_name();
        let expected = domain.challenge_value();
        let found = resolver.txt(&name).await;
        if !found.iter().any(|value| value.trim() == expected) {
            return Err(
                Diagnostic::new(code::E0801, format!("no ownership record for `{host}`")).help(
                    format!("add a TXT record at `{name}` with the value `{expected}`"),
                ),
            );
        }

        if dns::is_apex(&host) {
            let addresses = resolver.addresses(&host).await;
            if addresses.is_empty() || !self.addresses_match(&addresses) {
                return Err(Diagnostic::new(
                    code::E0801,
                    format!("`{host}` is an apex and does not resolve here"),
                )
                .help(format!(
                    "point `{host}` at this deployment with an ALIAS or ANAME record to `{}`, \
                     or with the address records your provider documents",
                    self.target
                )));
            }
        } else {
            let target = resolver.cname(&host).await.map(|t| dns::normalize(&t));
            if target.as_deref() != Some(self.target.as_str()) {
                let seen = target.unwrap_or_else(|| "nothing".to_owned());
                return Err(Diagnostic::new(
                    code::E0801,
                    format!("`{host}` is a CNAME to {seen}, not to `{}`", self.target),
                )
                .help(format!(
                    "point `{host}` at `{}` with a CNAME record",
                    self.target
                )));
            }
        }

        // The proof is good. A contest takes effect here and nowhere earlier,
        // and the owner it displaces is told.
        let mut verified = domain;
        verified.state = State::Verified;
        if contested.is_some() {
            if let Some(previous) = self.read().get(&host).cloned() {
                self.notices
                    .write()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(OwnerNotice {
                        host: host.clone(),
                        previous_org: previous.org,
                        previous_project: previous.project,
                        claiming_org: verified.org,
                        at_ms: self.clock.now_ms(),
                    });
            }
            self.pending_contests().remove(&host);
        }
        self.write().insert(host, verified.clone());
        Ok(verified)
    }

    /// Step three. Orders a certificate for a verified host.
    pub async fn certify(&self, host: &str, issuer: &dyn Issuer) -> Result<Domain, Diagnostic> {
        let host = dns::normalize(host);
        let mut domain = self.read().get(&host).cloned().ok_or_else(|| {
            Diagnostic::new(code::E0802, format!("`{host}` has not been claimed"))
        })?;
        if domain.state == State::Pending {
            return Err(Diagnostic::new(
                code::E0802,
                format!("`{host}` is not verified, so no certificate can be ordered for it"),
            )
            .help("publish the TXT record and verify the domain first"));
        }
        let hosts = [host.clone()];
        let certificate = issuer.issue(&hosts).await.map_err(|error| {
            Diagnostic::new(
                code::E0802,
                format!("no certificate could be issued for `{host}`: {error}"),
            )
            .help("check that this deployment answers `/.well-known/acme-challenge/` on port 80")
        })?;
        self.certificates
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(host.clone(), certificate);
        domain.state = State::Certified;
        domain.certified_ms = Some(self.clock.now_ms());
        self.write().insert(host, domain.clone());
        Ok(domain)
    }

    /// Hosts whose certificate is old enough to renew.
    pub fn renewals_due(&self) -> Vec<String> {
        let now = self.clock.now_ms();
        let after = crate::auth::clock::millis(RENEW_AFTER);
        self.read()
            .values()
            .filter(|domain| match domain.certified_ms {
                Some(at) => now.saturating_sub(at) >= after,
                None => false,
            })
            .map(|domain| domain.host.clone())
            .collect()
    }

    pub fn certificate(&self, host: &str) -> Option<Certificate> {
        self.certificates
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&dns::normalize(host))
            .cloned()
    }

    pub fn get(&self, host: &str) -> Option<Domain> {
        self.read().get(&dns::normalize(host)).cloned()
    }

    pub fn list(&self) -> Vec<Domain> {
        self.read().values().cloned().collect()
    }

    pub fn notices(&self) -> Vec<OwnerNotice> {
        self.notices
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// HOST-22: the base path a request on this host is served under, and the
    /// rest of the path after it. `None` when the host serves nothing here or
    /// the path is outside its base.
    pub fn route(&self, host: &str, path: &str) -> Option<(Domain, String)> {
        let domain = self.get(host)?;
        if domain.base_path.is_empty() {
            return Some((domain, path.to_owned()));
        }
        let rest = path.strip_prefix(domain.base_path.as_str())?;
        match rest.is_empty() {
            true => Some((domain, "/".to_owned())),
            false if rest.starts_with('/') => {
                let rest = rest.to_owned();
                Some((domain, rest))
            }
            false => None,
        }
    }

    /// HOST-23: where an alias sends a reader, preserving the path under the
    /// primary host's own base path.
    pub fn alias_redirect(&self, host: &str, path: &str) -> Option<String> {
        let domain = self.get(host)?;
        if domain.primary {
            return None;
        }
        let primary = self
            .read()
            .values()
            .find(|other| {
                other.primary && other.project == domain.project && other.env == domain.env
            })
            .cloned()?;
        let (_, rest) = self.route(host, path)?;
        Some(format!(
            "https://{}{}{}",
            primary.host, primary.base_path, rest
        ))
    }

    fn addresses_match(&self, found: &[std::net::IpAddr]) -> bool {
        match self.target_addresses.is_empty() {
            // Nothing to compare against: an operator who configured no
            // addresses gets ownership proof and reachability, which is what
            // the apex case can check without them.
            true => true,
            false => found
                .iter()
                .any(|address| self.target_addresses.contains(address)),
        }
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, BTreeMap<String, Domain>> {
        self.domains.read().unwrap_or_else(|e| e.into_inner())
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, BTreeMap<String, Domain>> {
        self.domains.write().unwrap_or_else(|e| e.into_inner())
    }

    fn pending_contests(&self) -> std::sync::RwLockWriteGuard<'_, BTreeMap<String, Domain>> {
        self.contests.write().unwrap_or_else(|e| e.into_inner())
    }
}

fn conflict(host: &str, existing: &Domain) -> Diagnostic {
    Diagnostic::new(
        code::E0814,
        format!("`{host}` already belongs to another project"),
    )
    .help(format!(
        "a domain belongs to one project at a time. To move it, claim it as a contest and \
         publish the TXT record at `{CHALLENGE_PREFIX}.{host}`; the current owner \
         (project {}) is notified when the proof checks out.",
        existing.project
    ))
}

/// HOST-22's base path: empty, or a leading slash, no trailing slash, and no
/// path traversal.
pub fn check_base_path(path: &str) -> Result<String, Diagnostic> {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return Ok(String::new());
    }
    let bad = |detail: &str| {
        Diagnostic::new(
            code::E0815,
            format!("`{trimmed}` is not a base path: {detail}"),
        )
        .help("a base path looks like `/docs`: a leading slash, no trailing slash")
    };
    if !trimmed.starts_with('/') {
        return Err(bad("it does not start with `/`"));
    }
    if trimmed.ends_with('/') {
        return Err(bad("it ends with `/`"));
    }
    if trimmed.contains("//") {
        return Err(bad("it has an empty segment"));
    }
    if trimmed
        .split('/')
        .any(|segment| segment == "." || segment == "..")
    {
        return Err(bad("it has a relative segment"));
    }
    if trimmed.contains(['?', '#', ' ']) {
        return Err(bad("it has a character that is not part of a path"));
    }
    Ok(trimmed.to_owned())
}
