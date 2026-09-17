//! External links in the built site, and the sleep the rate limiter needs.
//!
//! `liyasa-verify` finds a page's external links in its syntax tree, which the
//! verification orchestrator will hand it. Until that exists, the built HTML is
//! what a reader would click: it is the same links after expansion, and reading
//! it needs nothing the CLI does not already have.

use std::collections::BTreeMap;
use std::time::Duration;

use liyasa_build::hosting::emulate::Dist;
use liyasa_core::net::BoxFut;
use liyasa_verify::core::links::Pacer;

/// Waits the delay the rate limiter asked for, on the CLI's runtime.
pub struct Sleep;

impl Pacer for Sleep {
    fn wait<'a>(&'a self, delay: Duration) -> BoxFut<'a, ()> {
        Box::pin(tokio::time::sleep(delay))
    }
}

/// Every external URL the built site links to, with the first page that does.
///
/// Only `<a href>`: an image or a script that does not load is an asset
/// problem, which the build already decides, and a link check that reported
/// both would be answering two questions under one name.
///
/// The theme's own "Built with Liyasa" link (THM-40) is left out. It is on
/// every page of every site, the author did not write it and cannot fix it,
/// and reporting it would put one unactionable warning in every run.
pub fn external(output: &std::path::Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Ok(dist) = Dist::read(output) else {
        return out;
    };
    for path in dist.paths() {
        if !path.ends_with(".html") {
            continue;
        }
        let Some(html) = dist.text(path) else {
            continue;
        };
        for url in anchors(&html) {
            if is_external(&url) && !is_the_badge(&url) {
                out.entry(url).or_insert_with(|| path.to_owned());
            }
        }
    }
    out
}

fn is_the_badge(url: &str) -> bool {
    url.trim_end_matches('/') == liyasa_core::site::SITE_URL
}

fn is_external(url: &str) -> bool {
    matches!(
        url.split_once("://"),
        Some((scheme, _)) if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
    )
}

/// The `href` of every `<a>` in `html`, in document order.
pub fn anchors(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find("<a") {
        let after = &rest[at + 2..];
        // `<a ` and `<a\n` are anchors; `<abbr` and `<article` are not.
        let opens = after
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_whitespace() || c == '>' || c == '/');
        let end = after.find('>').unwrap_or(after.len());
        if opens && let Some(href) = attribute(&after[..end], "href") {
            out.push(href);
        }
        rest = &after[end..];
        if rest.is_empty() {
            break;
        }
        rest = &rest['>'.len_utf8()..];
    }
    out
}

/// The value of `name` inside one tag's attribute text, quoted or bare.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let mut rest = tag;
    loop {
        let at = rest.find(name)?;
        let before_is_boundary = at == 0
            || rest[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_whitespace());
        let after = rest[at + name.len()..].trim_start();
        if before_is_boundary && let Some(value) = after.strip_prefix('=') {
            let value = value.trim_start();
            let text = match value.chars().next() {
                Some(quote @ ('"' | '\'')) => {
                    let inner = &value[1..];
                    &inner[..inner.find(quote).unwrap_or(inner.len())]
                }
                _ => {
                    let end = value
                        .find(|c: char| c.is_ascii_whitespace())
                        .unwrap_or(value.len());
                    &value[..end]
                }
            };
            if text.is_empty() {
                return None;
            }
            return Some(unescape(text));
        }
        rest = &rest[at + name.len()..];
    }
}

/// The five entities an attribute value can carry. A URL with anything else in
/// it is not one this site generated.
fn unescape(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

/// What one external link run did.
pub struct Outcome {
    pub diagnostics: liyasa_core::Diagnostics,
    pub checked: usize,
    pub skipped: usize,
    pub broken: usize,
}

/// CLI-07's three knobs: how many requests are in flight, how long one may
/// take, and the hosts to accept without asking.
pub struct Options {
    pub concurrency: usize,
    pub timeout: Duration,
    pub allow: Vec<String>,
}

/// Checks every external link in the built site (VER-51, CLI-07).
pub fn check(
    network: &crate::net::Network,
    config: &serde_json::Value,
    output: &std::path::Path,
    options: &Options,
) -> Outcome {
    use liyasa_core::net::{HostPattern, HttpPolicy, Purpose};
    use liyasa_verify::core::config::VerifyConfig;
    use liyasa_verify::core::links::{
        DEFAULT_PER_HOST_INTERVAL, LinkChecker, RateLimits, SystemClock,
    };

    let found = external(output);
    let (verify, _) =
        VerifyConfig::from_value(config.get("verify").unwrap_or(&serde_json::Value::Null));
    let mut links = verify.links;
    for allowed in &options.allow {
        links
            .allow_hosts
            .0
            .push(HostPattern::Exact(host_of(allowed)));
    }

    let policy = HttpPolicy {
        allow_hosts: liyasa_core::net::HostSet::default(),
        deny_hosts: links.deny_hosts.clone(),
        allow_private: false,
        max_redirects: 5,
        // A link check reads the status line, never the document.
        max_bytes: 64 * 1024,
        timeout: options.timeout,
        purpose: Purpose::LinkCheck,
    };
    let limits = RateLimits::new(DEFAULT_PER_HOST_INTERVAL, std::sync::Arc::new(SystemClock));
    let pacer = Sleep;
    let checker = LinkChecker::new(network.client(), &links, limits, &pacer).with_policy(policy);

    let urls: Vec<&String> = found.keys().collect();
    let statuses = network.block_on(bounded(
        urls.iter().map(|url| checker.check(url.as_str())).collect(),
        options.concurrency,
    ));

    let mut out = Outcome {
        diagnostics: liyasa_core::Diagnostics::new(),
        checked: 0,
        skipped: 0,
        broken: 0,
    };
    for status in statuses {
        match &status.outcome {
            liyasa_verify::core::links::LinkOutcome::Skipped { .. } => out.skipped += 1,
            _ => out.checked += 1,
        }
        if let Some(mut diagnostic) = status.diagnostic() {
            out.broken += 1;
            if let Some(page) = found.get(&status.url) {
                diagnostic.message = format!("{} (linked from `{page}`)", diagnostic.message);
            }
            out.diagnostics.push(diagnostic);
        }
    }
    out
}

/// An allow-list entry is a URL or a bare host; both name a host.
fn host_of(value: &str) -> String {
    liyasa_core::net::Url::parse(value)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_else(|| value.trim().to_owned())
}

/// Runs `tasks` with at most `limit` in flight, giving the results back in the
/// order the tasks were handed over.
///
/// `LinkChecker::check_all` runs one request at a time, and `--concurrency`
/// promises otherwise. Every future here borrows the one checker, so they
/// cannot be spawned onto the runtime; they are polled together instead.
pub async fn bounded<F: Future>(tasks: Vec<F>, limit: usize) -> Vec<F::Output> {
    use std::collections::VecDeque;
    use std::pin::Pin;
    use std::task::Poll;

    let limit = limit.max(1);
    let mut queue: VecDeque<(usize, F)> = tasks.into_iter().enumerate().collect();
    let mut results: Vec<Option<F::Output>> = (0..queue.len()).map(|_| None).collect();
    let mut flight: Vec<(usize, Pin<Box<F>>)> = Vec::new();

    let done = std::future::poll_fn(move |cx| {
        loop {
            while flight.len() < limit
                && let Some((index, task)) = queue.pop_front()
            {
                flight.push((index, Box::pin(task)));
            }
            if flight.is_empty() {
                return Poll::Ready(std::mem::take(&mut results));
            }
            // One waker serves the whole set, so a wake from any future
            // re-polls the rest. That costs a poll and keeps the set honest.
            let mut progressed = false;
            let mut at = 0;
            while at < flight.len() {
                match flight[at].1.as_mut().poll(cx) {
                    Poll::Ready(value) => {
                        let (index, _) = flight.remove(at);
                        results[index] = Some(value);
                        progressed = true;
                    }
                    Poll::Pending => at += 1,
                }
            }
            if !progressed {
                return Poll::Pending;
            }
        }
    })
    .await;

    done.into_iter().flatten().collect()
}
