use std::collections::BTreeMap;

use liyasa_core::conformance::block_on;
use liyasa_core::document::{BlockKind, Origin};
use liyasa_core::ids::Fingerprint;
use liyasa_core::net::{HostPattern, HostSet, HttpResponse};
use liyasa_core::span::{SourceId, Span};
use liyasa_core::vfs::Bytes;

use super::*;

/// A clock that only moves when a test says so, so a rate limit's arithmetic
/// is the same on every machine.
struct ManualClock(Instant);

impl Clock for ManualClock {
    fn now(&self) -> Instant {
        self.0
    }
}

fn manual() -> Arc<dyn Clock> {
    Arc::new(ManualClock(Instant::now()))
}

/// What a fixture host answers with: a status and the URL the body came
/// from, or a network failure.
type Answer<'a> = Result<(u16, &'a str), NetError>;

/// Answers by URL: `(status, final_url)`, or a network error.
struct Site {
    routes: BTreeMap<String, Result<(u16, String), NetError>>,
    log: Mutex<Vec<(String, String)>>,
}

impl Site {
    fn new(routes: &[(&str, Answer<'_>)]) -> Self {
        Self {
            routes: routes
                .iter()
                .map(|(url, answer)| {
                    (
                        (*url).to_owned(),
                        answer
                            .clone()
                            .map(|(status, final_url)| (status, final_url.to_owned())),
                    )
                })
                .collect(),
            log: Mutex::new(Vec::new()),
        }
    }

    fn requests(&self) -> Vec<(String, String)> {
        self.log.lock().expect("lock").clone()
    }
}

impl HttpClient for Site {
    fn fetch<'a>(
        &'a self,
        req: HttpRequest,
        _policy: &'a HttpPolicy,
    ) -> BoxFut<'a, Result<HttpResponse, NetError>> {
        let url = req.url.to_string();
        self.log
            .lock()
            .expect("lock")
            .push((req.method.to_string(), url.clone()));
        let answer = self
            .routes
            .get(&url)
            .cloned()
            .unwrap_or(Ok((404, url.clone())));
        Box::pin(std::future::ready(answer.map(|(status, final_url)| {
            HttpResponse {
                status,
                headers: Vec::new(),
                body: Bytes::from(Vec::new()),
                final_url: Url::parse(&final_url).unwrap_or(req.url),
            }
        })))
    }
}

/// Records what the rate limit asked the checker to wait for.
#[derive(Default)]
struct Waits(Mutex<Vec<Duration>>);

impl Pacer for Waits {
    fn wait<'a>(&'a self, delay: Duration) -> BoxFut<'a, ()> {
        self.0.lock().expect("lock").push(delay);
        Box::pin(std::future::ready(()))
    }
}

impl Waits {
    fn seen(&self) -> Vec<Duration> {
        self.0.lock().expect("lock").clone()
    }
}

fn limits() -> RateLimits {
    RateLimits::new(DEFAULT_PER_HOST_INTERVAL, manual())
}

// ---- VER-51's acceptance criterion ----

#[test]
fn ver_51_a_valid_a_redirecting_and_a_dead_link() {
    let site = Site::new(&[
        (
            "https://good.test/page",
            Ok((200, "https://good.test/page")),
        ),
        (
            "https://moved.test/old",
            Ok((200, "https://moved.test/new")),
        ),
        (
            "https://dead.test/gone",
            Ok((410, "https://dead.test/gone")),
        ),
    ]);
    let config = LinksConfig::default();
    let pacer = Waits::default();
    let checker = LinkChecker::new(&site, &config, limits(), &pacer);

    let results = block_on(checker.check_all(&[
        "https://good.test/page".to_owned(),
        "https://moved.test/old".to_owned(),
        "https://dead.test/gone".to_owned(),
    ]));

    assert_eq!(results[0].outcome, LinkOutcome::Ok { status: 200 });
    assert!(results[0].diagnostic().is_none());

    assert_eq!(
        results[1].outcome,
        LinkOutcome::Redirected {
            status: 200,
            final_url: "https://moved.test/new".to_owned(),
        },
        "a redirect passes"
    );
    assert!(results[1].diagnostic().is_none());

    assert!(results[2].outcome.is_broken());
    let diagnostic = results[2].diagnostic().expect("the dead link is reported");
    assert_eq!(diagnostic.code, code::W0404);
    assert!(diagnostic.message.contains("410"), "{}", diagnostic.message);
    assert!(
        diagnostic.message.contains("dead.test"),
        "{}",
        diagnostic.message
    );
}

#[test]
fn ver_51_per_host_rate_limits_are_respected() {
    let site = Site::new(&[
        ("https://one.test/a", Ok((200, "https://one.test/a"))),
        ("https://one.test/b", Ok((200, "https://one.test/b"))),
        ("https://one.test/c", Ok((200, "https://one.test/c"))),
        ("https://two.test/a", Ok((200, "https://two.test/a"))),
    ]);
    let config = LinksConfig::default();
    let pacer = Waits::default();
    let checker = LinkChecker::new(&site, &config, limits(), &pacer);

    let results = block_on(checker.check_all(&[
        "https://one.test/a".to_owned(),
        "https://one.test/b".to_owned(),
        "https://one.test/c".to_owned(),
        "https://two.test/a".to_owned(),
    ]));

    // Three requests to one host are spaced; the fourth is a different host
    // and waits for nobody.
    let waited: Vec<Duration> = results.iter().map(|r| r.attempts[0].waited).collect();
    assert_eq!(
        waited,
        [
            Duration::ZERO,
            DEFAULT_PER_HOST_INTERVAL,
            DEFAULT_PER_HOST_INTERVAL * 2,
            Duration::ZERO,
        ]
    );
    assert_eq!(pacer.seen(), waited, "every delay was actually waited for");
}

// ---- HEAD, then GET ----

#[test]
fn a_host_that_answers_head_is_asked_once() {
    let site = Site::new(&[("https://x.test/a", Ok((200, "https://x.test/a")))]);
    let config = LinksConfig::default();
    let pacer = Waits::default();
    let checker = LinkChecker::new(&site, &config, limits(), &pacer);

    let status = block_on(checker.check("https://x.test/a"));
    assert_eq!(status.attempts.len(), 1);
    assert_eq!(status.attempts[0].method, "HEAD");
    assert_eq!(
        site.requests(),
        [("HEAD".to_owned(), "https://x.test/a".to_owned())]
    );
}

#[test]
fn a_host_that_refuses_head_is_retried_with_get() {
    // 405 on HEAD, 200 on GET: the link is fine, the host is fussy.
    let site = Site::new(&[("https://x.test/a", Ok((405, "https://x.test/a")))]);
    let config = LinksConfig::default();
    let pacer = Waits::default();
    let checker = LinkChecker::new(&site, &config, limits(), &pacer);

    let status = block_on(checker.check("https://x.test/a"));
    assert_eq!(
        status.attempts.iter().map(|a| a.method).collect::<Vec<_>>(),
        ["HEAD", "GET"]
    );
    // Both answers were 405, so the link really is broken.
    assert!(status.outcome.is_broken());
}

#[test]
fn a_network_error_on_head_is_retried_with_get() {
    let site = Site::new(&[("https://x.test/a", Err(NetError::Timeout))]);
    let config = LinksConfig::default();
    let pacer = Waits::default();
    let checker = LinkChecker::new(&site, &config, limits(), &pacer);

    let status = block_on(checker.check("https://x.test/a"));
    assert_eq!(status.attempts.len(), 2);
    match status.outcome {
        LinkOutcome::Broken { status, reason } => {
            assert_eq!(status, None);
            assert!(reason.contains("timed out"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_retry_counts_against_the_hosts_rate_limit_too() {
    let site = Site::new(&[("https://x.test/a", Ok((403, "https://x.test/a")))]);
    let config = LinksConfig::default();
    let pacer = Waits::default();
    let checker = LinkChecker::new(&site, &config, limits(), &pacer);

    let status = block_on(checker.check("https://x.test/a"));
    assert_eq!(
        status.attempts.iter().map(|a| a.waited).collect::<Vec<_>>(),
        [Duration::ZERO, DEFAULT_PER_HOST_INTERVAL]
    );
}

// ---- allow and deny lists ----

#[test]
fn a_host_on_the_allow_list_is_trusted_rather_than_requested() {
    let site = Site::new(&[]);
    let config = LinksConfig {
        allow_hosts: HostSet(vec![HostPattern::Suffix("linkedin.com".to_owned())]),
        ..LinksConfig::default()
    };
    let pacer = Waits::default();
    let checker = LinkChecker::new(&site, &config, limits(), &pacer);

    let status = block_on(checker.check("https://www.linkedin.com/in/someone"));
    match &status.outcome {
        LinkOutcome::Skipped { reason } => assert!(reason.contains("allowHosts"), "{reason}"),
        other => panic!("{other:?}"),
    }
    assert!(status.attempts.is_empty());
    assert!(site.requests().is_empty(), "no request was made");
    assert!(status.diagnostic().is_none(), "a skip is not a W0404");
}

#[test]
fn a_host_on_the_deny_list_is_never_requested() {
    let site = Site::new(&[]);
    let config = LinksConfig {
        deny_hosts: HostSet(vec![HostPattern::Exact("internal.test".to_owned())]),
        ..LinksConfig::default()
    };
    let pacer = Waits::default();
    let checker = LinkChecker::new(&site, &config, limits(), &pacer);

    let status = block_on(checker.check("https://internal.test/secret"));
    match &status.outcome {
        LinkOutcome::Skipped { reason } => assert!(reason.contains("denyHosts"), "{reason}"),
        other => panic!("{other:?}"),
    }
    assert!(site.requests().is_empty());
}

#[test]
fn external_checking_turned_off_skips_every_link() {
    let site = Site::new(&[]);
    let config = LinksConfig {
        external: false,
        ..LinksConfig::default()
    };
    let pacer = Waits::default();
    let checker = LinkChecker::new(&site, &config, limits(), &pacer);

    assert!(matches!(
        block_on(checker.check("https://x.test/a")).outcome,
        LinkOutcome::Skipped { .. }
    ));
    assert!(site.requests().is_empty());
}

#[test]
fn a_url_that_is_not_a_url_is_broken_without_a_request() {
    let site = Site::new(&[]);
    let config = LinksConfig::default();
    let pacer = Waits::default();
    let checker = LinkChecker::new(&site, &config, limits(), &pacer);

    let status = block_on(checker.check("htp:/nonsense"));
    assert!(status.outcome.is_broken());
    assert!(site.requests().is_empty());
    assert_eq!(status.diagnostic().map(|d| d.code), Some(code::W0404));
}

// ---- the grace period ----

#[test]
fn a_failure_becomes_drift_once_it_is_older_than_the_grace() {
    let grace = Duration::from_secs(72 * 60 * 60);
    let start = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
    let failing = FailingSince(start);

    assert!(!failing.is_drift(grace, start + Duration::from_secs(60)));
    assert!(failing.is_drift(grace, start + grace));
    assert!(failing.is_drift(grace, start + grace + Duration::from_secs(1)));
}

#[test]
fn a_clock_that_has_gone_backwards_does_not_create_drift() {
    let grace = Duration::from_secs(10);
    let start = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    assert!(!FailingSince(start).is_drift(grace, start - Duration::from_secs(5)));
}

// ---- collecting the links off a page ----

fn block(kind: BlockKind, children: Vec<Node>) -> Block {
    Block {
        id: BlockId::implicit("b", &format!("{kind:?}"), "", 0),
        explicit_id: None,
        kind,
        origin: Origin::at(Span::new(SourceId(0), 0, 0)),
        children,
    }
}

trait Wrap {
    fn wrap(self) -> Node;
}

impl Wrap for Block {
    fn wrap(self) -> Node {
        Node::Block(self)
    }
}

fn link(href: &str) -> Node {
    Node::Inline(Inline::Link {
        href: href.to_owned(),
        title: None,
        children: vec![Inline::Text("text".to_owned())],
        resolved: None,
    })
}

#[test]
fn only_http_and_https_links_are_external() {
    let root = block(
        BlockKind::Document,
        vec![
            block(
                BlockKind::Paragraph,
                vec![
                    link("https://example.com/a"),
                    link("http://example.com/b"),
                    link("/internal"),
                    link("#anchor"),
                    link("mailto:someone@example.com"),
                    link("../relative.md"),
                ],
            )
            .wrap(),
        ],
    );
    let found = external_links(&Route::new("/page"), &root);
    assert_eq!(
        found
            .iter()
            .map(|(_, _, url)| url.as_str())
            .collect::<Vec<_>>(),
        ["https://example.com/a", "http://example.com/b"]
    );
}

#[test]
fn an_image_source_and_its_dark_variant_are_both_links() {
    let root = block(
        BlockKind::Document,
        vec![
            block(
                BlockKind::Paragraph,
                vec![Node::Inline(Inline::Image {
                    src: "https://cdn.test/light.png".to_owned(),
                    alt: "a".to_owned(),
                    title: None,
                    dark: Some("https://cdn.test/dark.png".to_owned()),
                })],
            )
            .wrap(),
        ],
    );
    let found = external_links(&Route::new("/page"), &root);
    assert_eq!(found.len(), 2);
}

#[test]
fn the_same_url_twice_is_checked_once() {
    let root = block(
        BlockKind::Document,
        vec![
            block(
                BlockKind::Paragraph,
                vec![link("https://example.com/a"), link("https://example.com/a")],
            )
            .wrap(),
        ],
    );
    assert_eq!(external_links(&Route::new("/page"), &root).len(), 1);
}

#[test]
fn a_link_reports_the_block_it_was_found_in() {
    let paragraph = block(BlockKind::Paragraph, vec![link("https://example.com/a")]);
    let id = paragraph.id;
    let root = block(BlockKind::Document, vec![paragraph.wrap()]);
    let found = external_links(&Route::new("/page"), &root);
    assert_eq!(found[0].0, Route::new("/page"));
    assert_eq!(found[0].1, id);
}

#[test]
fn a_rate_of_four_a_second_is_the_documented_interval() {
    let limits = RateLimits::per_second(4, manual());
    assert_eq!(limits.take("x.test"), Duration::ZERO);
    assert_eq!(limits.take("x.test"), DEFAULT_PER_HOST_INTERVAL);
}

#[test]
fn a_fingerprint_is_not_needed_here_but_the_imports_must_hold() {
    // Guards against an unused-import warning becoming a silent drift in what
    // this module depends on.
    let _ = Fingerprint::of(b"links");
}
