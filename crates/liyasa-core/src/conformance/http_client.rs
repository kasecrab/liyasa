//! What every `HttpClient` must do (PRD §30.2.3, §34.9).
//!
//! The rules here are the SSRF ones: policy is enforced at connect time, so a
//! client that only inspects the request URL fails this kit.

use std::time::Duration;

use super::{block_on, require};
use crate::net::{
    DenyReason, HostPattern, HostSet, HttpClient, HttpPolicy, HttpRequest, Method, NetError,
    Purpose, Url,
};

/// URLs the implementation is pointed at. Each must be reachable from the test
/// environment, or the kit cannot tell a policy denial from a network failure.
pub struct Fixture {
    /// An allowed URL that returns 200.
    pub allowed: Url,
    /// A URL on a host the policy will not list.
    pub forbidden_host: Url,
    /// An `http://` URL on the allowed host.
    pub insecure: Url,
    /// An allowed URL that redirects to a loopback or RFC 1918 address.
    pub redirects_to_private: Option<Url>,
    /// An allowed URL whose body is larger than `max_bytes` below.
    pub oversized: Option<Url>,
}

fn policy(fixture: &Fixture) -> HttpPolicy {
    let host = fixture.allowed.host_str().unwrap_or_default().to_owned();
    HttpPolicy {
        allow_hosts: HostSet(vec![HostPattern::Exact(host)]),
        deny_hosts: HostSet::default(),
        allow_private: false,
        max_redirects: 3,
        max_bytes: 1024,
        timeout: Duration::from_secs(10),
        purpose: Purpose::SpecRef,
    }
}

fn get(url: &Url) -> HttpRequest {
    HttpRequest {
        method: Method::GET,
        url: url.clone(),
        headers: Vec::new(),
        body: None,
    }
}

pub fn check(client: &dyn HttpClient, fixture: &Fixture) {
    let policy = policy(fixture);

    let response = block_on(client.fetch(get(&fixture.allowed), &policy))
        .expect("an allowed https URL fetches");
    require!(
        response.status == 200,
        "the fixture URL returned {}",
        response.status
    );
    require!(
        response.final_url.host_str() == fixture.allowed.host_str(),
        "final_url must be the URL the body actually came from"
    );

    match block_on(client.fetch(get(&fixture.forbidden_host), &policy)) {
        Err(NetError::PolicyDenied {
            reason: DenyReason::HostNotAllowed(_),
        }) => {}
        other => panic!(
            "contract violated: a host outside the allow list gave {other:?}, \
             not PolicyDenied(HostNotAllowed)"
        ),
    }

    match block_on(client.fetch(get(&fixture.insecure), &policy)) {
        Err(NetError::PolicyDenied {
            reason: DenyReason::Scheme,
        }) => {}
        other => {
            panic!("contract violated: an http:// URL gave {other:?}, not PolicyDenied(Scheme)")
        }
    }

    let mut with_credentials = fixture.allowed.clone();
    if with_credentials.set_username("user").is_ok() {
        match block_on(client.fetch(get(&with_credentials), &policy)) {
            Err(NetError::PolicyDenied {
                reason: DenyReason::Credentials,
            }) => {}
            other => panic!(
                "contract violated: a URL carrying credentials gave {other:?}, \
                 not PolicyDenied(Credentials)"
            ),
        }
    }

    if let Some(url) = &fixture.redirects_to_private {
        match block_on(client.fetch(get(url), &policy)) {
            Err(NetError::PolicyDenied {
                reason: DenyReason::AddressClass(_) | DenyReason::RedirectHop(_),
            }) => {}
            other => panic!(
                "contract violated: a redirect to a private address gave {other:?}; \
                 every hop is re-validated at connect time"
            ),
        }
    }

    if let Some(url) = &fixture.oversized {
        match block_on(client.fetch(get(url), &policy)) {
            Err(NetError::TooLarge) => {}
            other => {
                panic!("contract violated: a body past max_bytes gave {other:?}, not TooLarge")
            }
        }
    }

    let no_redirects = HttpPolicy {
        max_redirects: 0,
        ..policy
    };
    let _ = block_on(client.fetch(get(&fixture.allowed), &no_redirects));
}
