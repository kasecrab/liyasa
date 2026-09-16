//! The connect-time rules, exercised against a loopback server.

use std::net::{IpAddr, Ipv4Addr};
use std::sync::atomic::Ordering;
use std::time::Duration;

use liyasa_core::net::{
    DenyReason, HostPattern, HostSet, HttpClient, HttpPolicy, HttpRequest, Method, NetError,
    Purpose, Url,
};
use liyasa_net::{Client, ClientOptions};

use crate::server;

fn client() -> Client {
    Client::new(ClientOptions::default()).expect("a client")
}

fn policy(allow_private: bool, purpose: Purpose) -> HttpPolicy {
    HttpPolicy {
        allow_hosts: HostSet(vec![HostPattern::Exact("127.0.0.1".to_owned())]),
        deny_hosts: HostSet::default(),
        allow_private,
        max_redirects: 3,
        max_bytes: 1024,
        timeout: Duration::from_secs(2),
        purpose,
    }
}

fn get(url: Url) -> HttpRequest {
    HttpRequest {
        method: Method::GET,
        url,
        headers: Vec::new(),
        body: None,
    }
}

#[tokio::test]
async fn a_loopback_address_is_refused_before_any_connection_is_made() {
    let server = server::start().await;
    let result = client()
        .fetch(get(server.url("/ok")), &policy(false, Purpose::LinkCheck))
        .await;
    assert_eq!(
        result.map(|r| r.status),
        Err(NetError::PolicyDenied {
            reason: DenyReason::AddressClass(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        })
    );
    assert_eq!(server.connections.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn allow_private_lets_an_operator_reach_an_intranet_host() {
    let server = server::start().await;
    let response = client()
        .fetch(get(server.url("/ok")), &policy(true, Purpose::LinkCheck))
        .await
        .expect("a fetch");
    assert_eq!(response.status, 200);
    assert_eq!(&response.body[..], b"hello");
    assert_eq!(response.final_url, server.url("/ok"));
}

#[tokio::test]
async fn http_is_refused_for_a_fact_source_unless_the_host_is_listed_insecure() {
    let server = server::start().await;
    let result = client()
        .fetch(get(server.url("/ok")), &policy(true, Purpose::FactSource))
        .await;
    assert!(matches!(
        result,
        Err(NetError::PolicyDenied {
            reason: DenyReason::Scheme
        })
    ));

    let options = ClientOptions {
        allow_insecure_hosts: HostSet(vec![HostPattern::Exact("127.0.0.1".to_owned())]),
        ..ClientOptions::default()
    };
    let response = Client::new(options)
        .expect("a client")
        .fetch(get(server.url("/ok")), &policy(true, Purpose::FactSource))
        .await
        .expect("listed insecure host");
    assert_eq!(response.status, 200);
}

#[tokio::test]
async fn a_host_outside_the_allow_list_is_refused_and_never_resolved() {
    let result = client()
        .fetch(
            get("https://example.invalid/".parse().expect("a url")),
            &policy(true, Purpose::SpecRef),
        )
        .await;
    assert_eq!(
        result.map(|r| r.status),
        Err(NetError::PolicyDenied {
            reason: DenyReason::HostNotAllowed("example.invalid".to_owned()),
        })
    );
}

#[tokio::test]
async fn credentials_in_the_url_are_refused() {
    let server = server::start().await;
    let mut url = server.url("/ok");
    url.set_username("user").expect("a username");
    let result = client()
        .fetch(get(url), &policy(true, Purpose::LinkCheck))
        .await;
    assert!(matches!(
        result,
        Err(NetError::PolicyDenied {
            reason: DenyReason::Credentials
        })
    ));
}

#[tokio::test]
async fn redirects_are_followed_up_to_the_cap_and_the_final_url_is_reported() {
    let server = server::start().await;
    let response = client()
        .fetch(
            get(server.url("/redirect/3")),
            &policy(true, Purpose::LinkCheck),
        )
        .await
        .expect("three hops fit in max_redirects 3");
    assert_eq!(response.status, 200);
    assert_eq!(response.final_url, server.url("/ok"));

    let result = client()
        .fetch(
            get(server.url("/redirect/4")),
            &policy(true, Purpose::LinkCheck),
        )
        .await;
    assert!(matches!(
        result,
        Err(NetError::PolicyDenied {
            reason: DenyReason::TooManyRedirects(3)
        })
    ));
}

#[tokio::test]
async fn a_redirect_that_leaves_the_allow_list_stops_at_that_hop() {
    let server = server::start().await;
    for path in ["/away", "/private"] {
        let result = client()
            .fetch(get(server.url(path)), &policy(true, Purpose::LinkCheck))
            .await;
        assert!(
            matches!(
                result,
                Err(NetError::PolicyDenied {
                    reason: DenyReason::RedirectHop(1)
                })
            ),
            "{path}: {result:?}"
        );
    }
}

#[test]
fn a_redirect_hop_that_lands_on_a_private_address_names_the_hop() {
    let policy = policy(false, Purpose::LinkCheck);
    let private = [IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))];
    assert_eq!(
        liyasa_net::policy::check_addresses(&private, &policy, 0),
        Err(DenyReason::AddressClass(private[0]))
    );
    assert_eq!(
        liyasa_net::policy::check_addresses(&private, &policy, 2),
        Err(DenyReason::RedirectHop(2))
    );
    let mixed = [IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34)), private[0]];
    assert!(
        liyasa_net::policy::check_addresses(&mixed, &policy, 0).is_err(),
        "one private address in the answer rejects the whole request"
    );
}

#[tokio::test]
async fn see_other_switches_to_get_and_temporary_redirect_keeps_the_method() {
    let server = server::start().await;
    let post = |path: &str| HttpRequest {
        method: Method::POST,
        url: server.url(path),
        headers: vec![("content-type".to_owned(), "text/plain".to_owned())],
        body: Some(bytes::Bytes::from_static(b"payload")),
    };
    let response = client()
        .fetch(post("/see-other"), &policy(true, Purpose::Webhook))
        .await
        .expect("a fetch");
    assert_eq!(&response.body[..], b"GET");
    let response = client()
        .fetch(post("/keep-method"), &policy(true, Purpose::Webhook))
        .await
        .expect("a fetch");
    assert_eq!(&response.body[..], b"POST");
}

#[tokio::test]
async fn a_body_past_max_bytes_is_too_large_with_or_without_a_length_header() {
    let server = server::start().await;
    for path in ["/big", "/big-no-length"] {
        let result = client()
            .fetch(get(server.url(path)), &policy(true, Purpose::LinkCheck))
            .await;
        assert!(
            matches!(result, Err(NetError::TooLarge)),
            "{path}: {result:?}"
        );
    }
}

#[tokio::test]
async fn the_total_timeout_covers_a_slow_body() {
    let server = server::start().await;
    let started = std::time::Instant::now();
    let result = client()
        .fetch(get(server.url("/slow")), &policy(true, Purpose::LinkCheck))
        .await;
    assert!(matches!(result, Err(NetError::Timeout)), "{result:?}");
    assert!(started.elapsed() < Duration::from_secs(4));
}

#[tokio::test]
async fn a_name_that_resolves_to_loopback_is_refused_without_allow_private() {
    let server = server::start().await;
    let url: Url = format!("http://localhost:{}/ok", server.port)
        .parse()
        .expect("a url");
    let policy = HttpPolicy {
        allow_hosts: HostSet(vec![HostPattern::Exact("localhost".to_owned())]),
        ..policy(false, Purpose::LinkCheck)
    };
    let result = client().fetch(get(url), &policy).await;
    match result {
        Err(NetError::PolicyDenied {
            reason: DenyReason::AddressClass(addr),
        }) => assert!(addr.is_loopback()),
        // A machine whose resolver refuses `localhost` entirely still never
        // connected, which is the property under test.
        Err(NetError::Dns(_)) => {}
        other => panic!("{other:?}"),
    }
    assert_eq!(server.connections.load(Ordering::SeqCst), 0);
}

/// The conformance kit needs a public TLS endpoint; `LIYASA_NET_TESTS=1`
/// opts in on a machine with network access.
#[test]
fn the_conformance_kit_passes_against_a_public_host() {
    if std::env::var_os("LIYASA_NET_TESTS").is_none() {
        return;
    }
    let runtime = tokio::runtime::Runtime::new().expect("a runtime");
    let _guard = runtime.enter();
    let client = client();
    let fixture = liyasa_core::conformance::http_client::Fixture {
        allowed: "https://example.com/".parse().expect("a url"),
        forbidden_host: "https://example.org/".parse().expect("a url"),
        insecure: "http://example.com/".parse().expect("a url"),
        redirects_to_private: None,
        oversized: None,
    };
    liyasa_core::conformance::http_client::check(&client, &fixture);
}
