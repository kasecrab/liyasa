//! AUTH-05 and AUTH-09 through the endpoints, with a sender in the path.
//!
//! `auth::magic`'s own tests prove `Magic` correct in isolation: they call
//! `request` and `consume` directly and never build a message. That leaves one
//! joint untested, and it is the joint the whole feature rests on — the token
//! the handler mints has to be the token in the link the reader receives. A
//! sender that dropped it, truncated it or sent the previous one would pass
//! every existing test.
//!
//! So these drive the real router with a `Mail` that builds the real message
//! and keeps it, and sign in with the URL read back out of that message. The
//! single-use and expiry clauses are re-asserted here rather than taken from
//! the unit tests, because what they have to hold for is the link in the mail.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use http::{Request, StatusCode, header};
use liyasa_server::auth::clock::Clock;
use liyasa_server::auth::config::{AuthConfig, ManagedConfig, Mode};
use liyasa_server::auth::mail::{MailConfig, Security, SmtpConfig, SmtpMail};
use liyasa_server::auth::state::{AuthState, Mail, Unsent};
use tower::ServiceExt as _;

const ORIGIN: &str = "https://docs.acme.com";
const NONCE_COOKIE: &str = "liyasa_magic";

/// The messages `SmtpMail` would have handed the relay.
///
/// It delegates to a real [`SmtpMail`] rather than recording the token it was
/// given, so what the test reads is what the reader would read.
#[derive(Debug)]
struct Inbox {
    sender: SmtpMail,
    messages: Mutex<Vec<String>>,
}

impl Inbox {
    fn new() -> Arc<Self> {
        let config = MailConfig {
            from: "Docs <docs@acme.com>".to_owned(),
            reply_to: None,
            smtp: SmtpConfig {
                host: "smtp.acme.com".to_owned(),
                port: None,
                security: Security::Starttls,
                username: None,
                password: None,
            },
        };
        Arc::new(Self {
            sender: SmtpMail::new(&config, None, ORIGIN).expect("a sender"),
            messages: Mutex::new(Vec::new()),
        })
    }

    /// The link out of the most recent message, read the way a mail client
    /// reads one: quoted-printable soft breaks (`=` at end of line) are joined
    /// before looking, because a link this long does not fit in a 76-column
    /// line and the transport is allowed to fold it.
    fn latest_link(&self) -> Option<String> {
        let held = self.messages.lock().unwrap_or_else(|e| e.into_inner());
        let message = held.last()?.replace("=\r\n", "").replace("=\n", "");
        let start = message.find(ORIGIN)?;
        Some(
            message[start..]
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_owned(),
        )
    }

    fn count(&self) -> usize {
        self.messages
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }
}

impl Inbox {
    /// Takes the formatted bytes rather than the message, so this file does
    /// not need a `lettre` dependency of its own to name the type.
    fn keep(&self, formatted: Vec<u8>) {
        self.messages
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(String::from_utf8_lossy(&formatted).into_owned());
    }
}

impl Mail for Inbox {
    fn send_link(&self, address: &str, token: &str) {
        let message = self
            .sender
            .message(address, token)
            .expect("the address reached the sender intact");
        self.keep(message.formatted());
    }

    /// The notification path. Recorded the same way and reported as sent,
    /// because there is no relay here and the point is what was composed.
    fn send<'a>(
        &'a self,
        address: &'a str,
        subject: &'a str,
        body: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), Unsent>> + Send + 'a>> {
        Box::pin(async move {
            let message = self.sender.compose(address, subject, body)?;
            self.keep(message.formatted());
            Ok(())
        })
    }
}

/// A managed instance that lets anyone at `acme.com` sign in, with a clock the
/// test drives.
fn instance() -> (Router, Arc<Inbox>, Clock) {
    let clock = Clock::manual();
    let config = AuthConfig {
        mode: Mode::Managed,
        managed: ManagedConfig {
            allow_domains: vec!["acme.com".to_owned()],
            ..ManagedConfig::default()
        },
        ..AuthConfig::default()
    };
    let inbox = Inbox::new();
    let state = AuthState::new(config, "production", vec![ORIGIN.to_owned()], clock.clone())
        .expect("entropy")
        .0
        .with_mail(inbox.clone());
    (
        liyasa_server::auth::routes::router(Arc::new(state)),
        inbox,
        clock,
    )
}

async fn ask(router: &Router, address: &str) -> http::Response<Body> {
    let request = Request::post("/_liyasa/auth/magic")
        .header(header::ORIGIN, ORIGIN)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(format!("email={address}")))
        .expect("a request");
    router.clone().oneshot(request).await.expect("a response")
}

/// The nonce the browser was given, which `consume` matches against.
fn nonce_of(response: &http::Response<Body>) -> String {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find_map(|value| value.strip_prefix(&format!("{NONCE_COOKIE}=")))
        .map(|rest| rest.split(';').next().unwrap_or_default().to_owned())
        .expect("the request sets a nonce cookie")
}

async fn open(router: &Router, link: &str, nonce: &str) -> http::Response<Body> {
    let path = link
        .strip_prefix(ORIGIN)
        .expect("the link is on this instance");
    let request = Request::get(path)
        .header(header::COOKIE, format!("{NONCE_COOKIE}={nonce}"))
        .body(Body::empty())
        .expect("a request");
    router.clone().oneshot(request).await.expect("a response")
}

#[tokio::test]
async fn the_link_in_the_message_is_the_link_that_signs_the_reader_in() {
    let (router, inbox, _clock) = instance();
    let asked = ask(&router, "reader@acme.com").await;
    assert_eq!(asked.status(), StatusCode::ACCEPTED);
    let nonce = nonce_of(&asked);

    let link = inbox.latest_link().expect("a message with a link");
    let opened = open(&router, &link, &nonce).await;
    assert!(
        opened.status().is_redirection() || opened.status() == StatusCode::OK,
        "opening the emailed link signs in, got {}",
        opened.status()
    );
    assert!(
        opened
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .any(|value| value
                .to_str()
                .unwrap_or_default()
                .contains("liyasa_session")),
        "a session cookie is set"
    );
}

/// The other half of AUTH-09: an address that cannot sign in gets the same
/// answer, and no message. The first clause is what a reader sees; the second
/// is what makes the first true.
#[tokio::test]
async fn an_address_that_cannot_sign_in_gets_the_same_answer_and_no_message() {
    let (router, inbox, _clock) = instance();
    let known = ask(&router, "reader@acme.com").await;
    let unknown = ask(&router, "stranger@elsewhere.com").await;

    assert_eq!(known.status(), unknown.status());
    let known_body = axum::body::to_bytes(known.into_body(), 64 * 1024)
        .await
        .expect("a body");
    let unknown_body = axum::body::to_bytes(unknown.into_body(), 64 * 1024)
        .await
        .expect("a body");
    assert_eq!(known_body, unknown_body, "the bodies must be identical");
    assert_eq!(
        inbox.count(),
        1,
        "only the address that can sign in is sent to"
    );
}

/// Rule 17, first clause: a link works once. Asserted here, on the emailed
/// link, because a sender that re-sent a token would make the unit test's
/// version of this pass while readers shared links that still worked.
#[tokio::test]
async fn the_emailed_link_works_once() {
    let (router, inbox, _clock) = instance();
    let asked = ask(&router, "reader@acme.com").await;
    let nonce = nonce_of(&asked);
    let link = inbox.latest_link().expect("a link");

    let first = open(&router, &link, &nonce).await;
    assert_ne!(first.status(), StatusCode::UNAUTHORIZED);
    let second = open(&router, &link, &nonce).await;
    assert_eq!(
        second.status(),
        StatusCode::UNAUTHORIZED,
        "the second use of an emailed link is refused"
    );
}

/// Rule 17, second clause: and it stops working. The default TTL is 15
/// minutes, so this asserts both sides of that edge rather than only that
/// something eventually expires.
#[tokio::test]
async fn the_emailed_link_expires_after_fifteen_minutes() {
    let (router, inbox, clock) = instance();
    let asked = ask(&router, "reader@acme.com").await;
    let nonce = nonce_of(&asked);
    let link = inbox.latest_link().expect("a link");

    clock.advance(Duration::from_secs(14 * 60 + 59));
    let inside = open(&router, &link, &nonce).await;
    assert_ne!(
        inside.status(),
        StatusCode::UNAUTHORIZED,
        "a link one second inside the window still works"
    );

    let (router, inbox, clock) = instance();
    let asked = ask(&router, "other@acme.com").await;
    let nonce = nonce_of(&asked);
    let link = inbox.latest_link().expect("a link");

    clock.advance(Duration::from_secs(15 * 60 + 1));
    let outside = open(&router, &link, &nonce).await;
    assert_eq!(
        outside.status(),
        StatusCode::UNAUTHORIZED,
        "a link one second outside the window does not"
    );
}

/// AUTH-09 again, from the browser's side: the link only works where it was
/// asked for. A sender is in the path here, so this is the clause that would
/// break if the nonce ever travelled in the message.
#[tokio::test]
async fn the_emailed_link_does_not_work_in_another_browser() {
    let (router, inbox, _clock) = instance();
    let asked = ask(&router, "reader@acme.com").await;
    let link = inbox.latest_link().expect("a link");
    let message = inbox
        .messages
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .last()
        .cloned()
        .expect("a message");
    let nonce = nonce_of(&asked);
    assert!(
        !message.contains(&nonce),
        "the nonce must not travel in the email; it is what proves the browser"
    );

    let elsewhere = open(&router, &link, "some-other-browsers-nonce").await;
    assert_eq!(elsewhere.status(), StatusCode::UNAUTHORIZED);
}
