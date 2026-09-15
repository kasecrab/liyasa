//! External link checking (VER-51).
//!
//! HEAD first, then GET when the host does not answer HEAD usefully, following
//! redirects, with a per-host rate limit and an allow list for hosts that
//! refuse automated requests. A build-time failure is `W0404` carrying the
//! status; the scheduled sweep escalates one that has failed for longer than
//! `verify.links.grace` into a drift record, which is WP-20c's work — this
//! module reports how long a link has been failing and stops there.
//!
//! Waiting is the caller's: a rate limit needs a clock and a sleep, and
//! `liyasa-verify` has no runtime. [`RateLimits`] computes how long to wait
//! and [`Pacer`] does the waiting, so a test drives both without one.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::document::{Block, Inline, Node};
use liyasa_core::ids::{BlockId, Route};
use liyasa_core::net::{
    BoxFut, HttpClient, HttpPolicy, HttpRequest, Method, NetError, Purpose, Url,
};

use super::config::LinksConfig;

/// VER-51 asks for a per-host rate limit and names no number. Four requests a
/// second per host is slow enough not to look like a scrape and fast enough
/// that a thousand-link site finishes in minutes (RFC 1304).
pub const DEFAULT_PER_HOST_INTERVAL: Duration = Duration::from_millis(250);

/// Statuses that mean "this host does not answer HEAD", not "this link is
/// dead". Every one of them is retried with GET.
const RETRY_WITH_GET: &[u16] = &[400, 401, 403, 404, 405, 406, 409, 429, 500, 501, 502, 503];

pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// Waits the delay a rate limit asked for. The CLI hands in its runtime's
/// sleep; a test hands in one that records and returns.
pub trait Pacer: Send + Sync {
    fn wait<'a>(&'a self, delay: Duration) -> BoxFut<'a, ()>;
}

/// Runs every request as soon as the limiter allows it on paper. Correct for a
/// single-threaded check of a handful of hosts and for every test.
pub struct Immediate;

impl Pacer for Immediate {
    fn wait<'a>(&'a self, _delay: Duration) -> BoxFut<'a, ()> {
        Box::pin(std::future::ready(()))
    }
}

/// One slot per host, handed out `interval` apart.
pub struct RateLimits {
    interval: Duration,
    clock: Arc<dyn Clock>,
    next: Mutex<BTreeMap<String, Instant>>,
}

impl RateLimits {
    pub fn new(interval: Duration, clock: Arc<dyn Clock>) -> Self {
        Self {
            interval,
            clock,
            next: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn per_second(rate: u32, clock: Arc<dyn Clock>) -> Self {
        let interval = Duration::from_secs(1)
            .checked_div(rate.max(1))
            .unwrap_or_default();
        Self::new(interval, clock)
    }

    /// Claims the next slot for `host` and returns how long the caller must
    /// wait before using it.
    pub fn take(&self, host: &str) -> Duration {
        let now = self.clock.now();
        let mut next = self.next.lock().unwrap_or_else(|e| e.into_inner());
        let at = next.get(host).copied().unwrap_or(now).max(now);
        next.insert(host.to_owned(), at + self.interval);
        at.saturating_duration_since(now)
    }
}

/// What one request to one URL did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkOutcome {
    /// Reached, with the status it answered.
    Ok { status: u16 },
    /// Reached after one or more redirects; VER-51 says a redirect passes.
    Redirected { status: u16, final_url: String },
    /// `W0404` material: a status at or above 400, or the request failed.
    Broken { status: Option<u16>, reason: String },
    /// Not requested, and why.
    Skipped { reason: String },
}

impl LinkOutcome {
    pub fn is_broken(&self) -> bool {
        matches!(self, Self::Broken { .. })
    }

    pub fn passed(&self) -> bool {
        matches!(self, Self::Ok { .. } | Self::Redirected { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkStatus {
    pub url: String,
    pub outcome: LinkOutcome,
    /// Every request the checker made for this link, in order.
    pub attempts: Vec<Attempt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attempt {
    pub method: &'static str,
    pub host: String,
    /// What the rate limit made this request wait for.
    pub waited: Duration,
}

impl LinkStatus {
    /// The build-time diagnostic VER-51 asks for: `W0404` carrying the status.
    pub fn diagnostic(&self) -> Option<Diagnostic> {
        match &self.outcome {
            LinkOutcome::Broken { status, reason } => Some(
                Diagnostic::new(
                    code::W0404,
                    match status {
                        Some(status) => {
                            format!("`{}` answered {status}", self.url)
                        }
                        None => format!("`{}` could not be reached: {reason}", self.url),
                    },
                )
                .help("add the host to `verify.links.allowHosts` if it blocks automated checks"),
            ),
            _ => None,
        }
    }
}

/// How long a link has been failing, which is what `verify.links.grace` is
/// measured against. The store keeps this between runs (WP-20c).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FailingSince(pub SystemTime);

impl FailingSince {
    /// VER-51: a failure older than the grace period becomes a drift record.
    pub fn is_drift(self, grace: Duration, now: SystemTime) -> bool {
        now.duration_since(self.0).unwrap_or_default() >= grace
    }
}

pub struct LinkChecker<'a> {
    client: &'a dyn HttpClient,
    config: &'a LinksConfig,
    policy: HttpPolicy,
    limits: RateLimits,
    pacer: &'a dyn Pacer,
}

impl<'a> LinkChecker<'a> {
    pub fn new(
        client: &'a dyn HttpClient,
        config: &'a LinksConfig,
        limits: RateLimits,
        pacer: &'a dyn Pacer,
    ) -> Self {
        let policy = HttpPolicy {
            allow_hosts: liyasa_core::net::HostSet::default(),
            deny_hosts: config.deny_hosts.clone(),
            allow_private: false,
            max_redirects: 5,
            // A link check reads the status line, never the document.
            max_bytes: 64 * 1024,
            timeout: Duration::from_secs(10),
            purpose: Purpose::LinkCheck,
        };
        Self {
            client,
            config,
            policy,
            limits,
            pacer,
        }
    }

    #[must_use]
    pub fn with_policy(mut self, policy: HttpPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub async fn check_all(&self, urls: &[String]) -> Vec<LinkStatus> {
        let mut out = Vec::with_capacity(urls.len());
        for url in urls {
            out.push(self.check(url).await);
        }
        out
    }

    pub async fn check(&self, url: &str) -> LinkStatus {
        let parsed = match Url::parse(url) {
            Ok(parsed) => parsed,
            Err(error) => {
                return LinkStatus {
                    url: url.to_owned(),
                    outcome: LinkOutcome::Broken {
                        status: None,
                        reason: format!("not a URL: {error}"),
                    },
                    attempts: Vec::new(),
                };
            }
        };
        // `Url::parse` accepts `htp:/x` and `mailto:` alike, so the scheme and
        // host are checked here rather than trusting whatever produced the
        // list: a link check must never be the thing that opens a socket to a
        // scheme nobody asked for.
        let host = match (parsed.scheme(), parsed.host_str()) {
            ("http" | "https", Some(host)) => host.to_owned(),
            _ => {
                return LinkStatus {
                    url: url.to_owned(),
                    outcome: LinkOutcome::Broken {
                        status: None,
                        reason: "not an http or https URL with a host".to_owned(),
                    },
                    attempts: Vec::new(),
                };
            }
        };

        if let Some(reason) = self.skip_reason(&host) {
            return LinkStatus {
                url: url.to_owned(),
                outcome: LinkOutcome::Skipped { reason },
                attempts: Vec::new(),
            };
        }

        let mut attempts = Vec::new();
        // HEAD first: it is the cheap request, and most hosts answer it.
        let head = self
            .request(Method::HEAD, &parsed, &host, &mut attempts)
            .await;
        let outcome = match head {
            Ok(outcome) if !retry_with_get(&outcome) => outcome,
            // A host that will not answer HEAD gets one GET, which is what a
            // reader's browser would have sent anyway.
            _ => {
                match self
                    .request(Method::GET, &parsed, &host, &mut attempts)
                    .await
                {
                    Ok(outcome) => outcome,
                    Err(error) => LinkOutcome::Broken {
                        status: None,
                        reason: error.to_string(),
                    },
                }
            }
        };
        LinkStatus {
            url: url.to_owned(),
            outcome,
            attempts,
        }
    }

    fn skip_reason(&self, host: &str) -> Option<String> {
        if !self.config.external {
            return Some("`verify.links.external` is off".to_owned());
        }
        if self.config.deny_hosts.matches(host) {
            return Some(format!("`{host}` is in `verify.links.denyHosts`"));
        }
        if self.config.allow_hosts.matches(host) {
            // VER-51's allow list is for hosts that block bots: the link is
            // trusted rather than requested (RFC 1304).
            return Some(format!(
                "`{host}` is in `verify.links.allowHosts`, which blocks automated checks"
            ));
        }
        None
    }

    async fn request(
        &self,
        method: Method,
        url: &Url,
        host: &str,
        attempts: &mut Vec<Attempt>,
    ) -> Result<LinkOutcome, NetError> {
        let waited = self.limits.take(host);
        self.pacer.wait(waited).await;
        attempts.push(Attempt {
            method: method_name(&method),
            host: host.to_owned(),
            waited,
        });
        let response = self
            .client
            .fetch(
                HttpRequest {
                    method,
                    url: url.clone(),
                    headers: Vec::new(),
                    body: None,
                },
                &self.policy,
            )
            .await?;
        Ok(classify(url, response.status, &response.final_url))
    }
}

fn classify(requested: &Url, status: u16, final_url: &Url) -> LinkOutcome {
    if status >= 400 {
        return LinkOutcome::Broken {
            status: Some(status),
            reason: format!("status {status}"),
        };
    }
    if final_url != requested {
        LinkOutcome::Redirected {
            status,
            final_url: final_url.to_string(),
        }
    } else {
        LinkOutcome::Ok { status }
    }
}

fn retry_with_get(outcome: &LinkOutcome) -> bool {
    match outcome {
        LinkOutcome::Broken {
            status: Some(status),
            ..
        } => RETRY_WITH_GET.contains(status),
        LinkOutcome::Broken { status: None, .. } => true,
        _ => false,
    }
}

fn method_name(method: &Method) -> &'static str {
    match *method {
        Method::HEAD => "HEAD",
        Method::GET => "GET",
        _ => "OTHER",
    }
}

/// Every external URL a page links to or embeds, in document order and
/// deduplicated, with the block each first appeared in.
pub fn external_links(page: &Route, root: &Block) -> Vec<(Route, BlockId, String)> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    walk_block(root, root.id, &mut |block, url| {
        if is_external(url) && seen.insert(url.to_owned()) {
            out.push((page.clone(), block, url.to_owned()));
        }
    });
    out
}

fn is_external(url: &str) -> bool {
    matches!(
        url.split_once("://"),
        Some((scheme, _)) if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
    )
}

fn walk_block(block: &Block, owner: BlockId, visit: &mut impl FnMut(BlockId, &str)) {
    for child in &block.children {
        match child {
            Node::Block(inner) => walk_block(inner, inner.id, visit),
            Node::Inline(inline) => walk_inline(inline, owner, visit),
        }
    }
}

fn walk_inline(inline: &Inline, owner: BlockId, visit: &mut impl FnMut(BlockId, &str)) {
    match inline {
        Inline::Link { href, children, .. } => {
            visit(owner, href);
            for child in children {
                walk_inline(child, owner, visit);
            }
        }
        Inline::Image { src, dark, .. } => {
            visit(owner, src);
            if let Some(dark) = dark {
                visit(owner, dark);
            }
        }
        Inline::Emph(children) | Inline::Strong(children) | Inline::Strike(children) => {
            for child in children {
                walk_inline(child, owner, visit);
            }
        }
        Inline::InlineComponent {
            children, props, ..
        } => {
            for value in props.0.values() {
                if let liyasa_core::document::PropValue::Str(text) = value {
                    visit(owner, text);
                }
            }
            for child in children {
                walk_inline(child, owner, visit);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests;
