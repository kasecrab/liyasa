//! HOST-20: adding a custom domain.
//!
//! Given a custom domain and a test DNS server, adding it takes the TXT
//! challenge, the CNAME check and certificate issuance in turn, and a
//! conflicting claim by another organization is refused with the documented
//! flow rather than quietly reassigning the host.
//!
//! The "test DNS server" is `auth::dns::Zone`, the resolver seam the flow
//! reads through (RFC 1502). Certificate issuance goes through `auth::domains::Issuer`
//! for the same reason: an ACME directory is a container this machine does not
//! have, and the state machine around the order is what HOST-20 is about.

use std::net::{IpAddr, Ipv4Addr};

use liyasa_core::diagnostics::code;
use liyasa_core::ids::{OrgId, ProjectId};
use liyasa_core::net::BoxFut;
use liyasa_server::auth::clock::Clock;
use liyasa_server::auth::dns::{Offline, Zone};
use liyasa_server::auth::domains::{Issuer, Registry, Request, State};
use liyasa_server::routes::acme::Certificate;

const TARGET: &str = "sites.liyasa.dev";

fn project() -> ProjectId {
    ProjectId(liyasa_store::new_ulid())
}

fn org() -> OrgId {
    OrgId(liyasa_store::new_ulid())
}

/// Stands in for the ACME directory. It records what it was asked for, so the
/// test can assert the certificate was ordered for the host that verified and
/// not for something else.
#[derive(Debug, Default)]
struct RecordingIssuer {
    ordered: std::sync::Mutex<Vec<Vec<String>>>,
    fail: bool,
}

impl RecordingIssuer {
    fn failing() -> Self {
        Self {
            fail: true,
            ..Self::default()
        }
    }

    fn ordered(&self) -> Vec<Vec<String>> {
        self.ordered
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

impl Issuer for RecordingIssuer {
    fn issue<'a>(&'a self, hosts: &'a [String]) -> BoxFut<'a, Result<Certificate, String>> {
        self.ordered
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(hosts.to_vec());
        let fail = self.fail;
        Box::pin(async move {
            match fail {
                true => Err("the directory refused the order".to_owned()),
                false => Ok(Certificate {
                    chain_pem: "-----BEGIN CERTIFICATE-----\nYQ==\n-----END CERTIFICATE-----\n"
                        .to_owned(),
                    private_key_pem:
                        "-----BEGIN PRIVATE KEY-----\nYg==\n-----END PRIVATE KEY-----\n".to_owned(),
                }),
            }
        })
    }
}

fn registry() -> Registry {
    Registry::new(TARGET, Clock::manual())
}

/// The whole of HOST-20's happy path, in the order an operator walks it.
#[tokio::test]
async fn a_custom_domain_is_added_by_txt_challenge_then_cname_then_certificate() {
    let registry = registry();
    let zone = Zone::new();
    let issuer = RecordingIssuer::default();
    let host = "docs.example.com";

    let claimed = registry
        .claim(Request::new(host, project(), org()))
        .expect("the host is free");
    assert_eq!(claimed.state, State::Pending);
    assert_eq!(
        claimed.challenge_name(),
        "_liyasa-challenge.docs.example.com"
    );
    assert!(
        claimed
            .challenge_value()
            .starts_with("liyasa-site-verification=")
    );

    // Nothing published yet: verification fails and says what to publish.
    let error = registry
        .verify(host, &zone)
        .await
        .expect_err("no TXT record yet");
    assert_eq!(error.code, code::E0801);
    assert!(
        error.help.as_deref().is_some_and(|h| h.contains("TXT")),
        "{error:?}"
    );

    // The operator publishes the TXT record but has not pointed the host yet.
    zone.set_txt(&claimed.challenge_name(), [claimed.challenge_value()]);
    let error = registry
        .verify(host, &zone)
        .await
        .expect_err("the host does not point here");
    assert_eq!(error.code, code::E0801);
    assert!(error.message.contains("CNAME"), "{error:?}");

    // And now the CNAME.
    zone.set_cname(host, TARGET);
    let verified = registry.verify(host, &zone).await.expect("both records");
    assert_eq!(verified.state, State::Verified);

    let certified = registry
        .certify(host, &issuer)
        .await
        .expect("a certificate");
    assert_eq!(certified.state, State::Certified);
    assert_eq!(issuer.ordered(), [[host.to_owned()]]);
    assert!(registry.certificate(host).is_some());
}

#[tokio::test]
async fn an_apex_verifies_by_address_records_because_it_cannot_carry_a_cname() {
    let here = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10));
    let registry = Registry::new(TARGET, Clock::manual()).with_addresses([here]);
    let zone = Zone::new();
    let host = "example.com";

    let claimed = registry
        .claim(Request::new(host, project(), org()))
        .expect("the host is free");
    zone.set_txt(&claimed.challenge_name(), [claimed.challenge_value()]);

    let error = registry
        .verify(host, &zone)
        .await
        .expect_err("no addresses");
    assert!(error.message.contains("apex"), "{error:?}");

    // Pointed somewhere else entirely.
    zone.set_addresses(host, [IpAddr::V4(Ipv4Addr::new(198, 51, 100, 1))]);
    assert!(
        registry.verify(host, &zone).await.is_err(),
        "an apex pointing elsewhere is not verified"
    );

    zone.set_addresses(host, [here]);
    assert_eq!(
        registry.verify(host, &zone).await.expect("the apex").state,
        State::Verified
    );
}

#[tokio::test]
async fn a_txt_record_for_another_domain_does_not_verify_this_one() {
    let registry = registry();
    let zone = Zone::new();
    let mine = registry
        .claim(Request::new("docs.example.com", project(), org()))
        .expect("free");
    let theirs = registry
        .claim(Request::new("docs.other.example", project(), org()))
        .expect("free");

    // The operator pastes the other domain's value by mistake.
    zone.set_txt(&mine.challenge_name(), [theirs.challenge_value()]);
    zone.set_cname("docs.example.com", TARGET);
    let error = registry
        .verify("docs.example.com", &zone)
        .await
        .expect_err("a challenge is per host");
    assert_eq!(error.code, code::E0801);
}

#[tokio::test]
async fn a_claim_by_another_organization_is_refused_and_says_how_to_contest_it() {
    let registry = registry();
    let host = "docs.example.com";
    let owner_org = org();
    let owner_project = project();
    registry
        .claim(Request::new(host, owner_project, owner_org))
        .expect("the host is free");

    let error = registry
        .claim(Request::new(host, project(), org()))
        .expect_err("a domain belongs to one project");
    assert_eq!(error.code, code::E0814);
    let help = error.help.clone().unwrap_or_default();
    assert!(help.contains("contest"), "{help}");
    assert!(help.contains("_liyasa-challenge"), "{help}");
    assert!(help.contains("notified"), "{help}");

    // The refusal left the domain where it was.
    let still = registry.get(host).expect("the owner's record");
    assert_eq!(still.project, owner_project);
    assert_eq!(still.org, owner_org);
}

#[tokio::test]
async fn a_contest_takes_the_domain_only_when_its_proof_resolves_and_then_notifies() {
    let registry = registry();
    let zone = Zone::new();
    let host = "docs.example.com";
    let owner_org = org();
    let owner_project = project();
    registry
        .claim(Request::new(host, owner_project, owner_org))
        .expect("the host is free");

    let challenger_org = org();
    let contest = registry
        .claim(Request::new(host, project(), challenger_org).contest())
        .expect("a contest is accepted");

    // Claiming alone changes nothing: the live domain is still the owner's.
    assert_eq!(registry.get(host).expect("still theirs").org, owner_org);
    assert!(
        registry.notices().is_empty(),
        "nobody is told about a bare claim"
    );

    // An unproven contest does not resolve either.
    assert!(registry.verify(host, &zone).await.is_err());
    assert_eq!(registry.get(host).expect("still theirs").org, owner_org);

    zone.set_txt(&contest.challenge_name(), [contest.challenge_value()]);
    zone.set_cname(host, TARGET);
    let transferred = registry.verify(host, &zone).await.expect("the proof holds");
    assert_eq!(transferred.org, challenger_org);
    assert_eq!(registry.get(host).expect("now theirs").org, challenger_org);

    let notices = registry.notices();
    assert_eq!(notices.len(), 1, "the previous owner is notified once");
    assert_eq!(notices[0].host, host);
    assert_eq!(notices[0].previous_org, owner_org);
    assert_eq!(notices[0].previous_project, owner_project);
    assert_eq!(notices[0].claiming_org, challenger_org);
}

#[tokio::test]
async fn an_unverified_domain_gets_no_certificate() {
    let registry = registry();
    let issuer = RecordingIssuer::default();
    let host = "docs.example.com";
    registry
        .claim(Request::new(host, project(), org()))
        .expect("free");

    let error = registry
        .certify(host, &issuer)
        .await
        .expect_err("verification comes first");
    assert_eq!(error.code, code::E0802);
    assert!(issuer.ordered().is_empty(), "nothing was ordered");
}

#[tokio::test]
async fn a_refused_order_is_reported_as_a_certificate_failure() {
    let registry = registry();
    let zone = Zone::new();
    let host = "docs.example.com";
    let claimed = registry
        .claim(Request::new(host, project(), org()))
        .expect("free");
    zone.set_txt(&claimed.challenge_name(), [claimed.challenge_value()]);
    zone.set_cname(host, TARGET);
    registry.verify(host, &zone).await.expect("verified");

    let error = registry
        .certify(host, &RecordingIssuer::failing())
        .await
        .expect_err("the directory refused");
    assert_eq!(error.code, code::E0802);
    assert_eq!(
        registry.get(host).expect("the record").state,
        State::Verified,
        "a failed order leaves the domain verified, not certified"
    );
}

#[tokio::test]
async fn an_offline_instance_verifies_nothing_rather_than_reaching_out() {
    let registry = registry();
    let host = "docs.example.com";
    let claimed = registry
        .claim(Request::new(host, project(), org()))
        .expect("free");
    assert!(!claimed.challenge.is_empty(), "a claim needs no network");

    let error = registry
        .verify(host, &Offline)
        .await
        .expect_err("HOST-08 leaves an offline instance no records to read");
    assert_eq!(error.code, code::E0801);
}

#[tokio::test]
async fn a_certificate_is_renewed_once_it_is_old_enough() {
    let registry = registry();
    let zone = Zone::new();
    let issuer = RecordingIssuer::default();
    let host = "docs.example.com";
    let claimed = registry
        .claim(Request::new(host, project(), org()))
        .expect("free");
    zone.set_txt(&claimed.challenge_name(), [claimed.challenge_value()]);
    zone.set_cname(host, TARGET);
    registry.verify(host, &zone).await.expect("verified");
    registry
        .certify(host, &issuer)
        .await
        .expect("a certificate");

    assert!(
        registry.renewals_due().is_empty(),
        "a fresh certificate is left alone"
    );
    registry
        .clock()
        .advance(std::time::Duration::from_secs(61 * 86_400));
    assert_eq!(registry.renewals_due(), [host.to_owned()]);
}

/// HOST-22: one instance, two hosts, different base paths.
#[tokio::test]
async fn a_base_path_is_per_domain_so_one_instance_serves_both_shapes() {
    let registry = registry();
    let project = project();
    let org = org();
    registry
        .claim(Request::new("docs.acme.com", project, org))
        .expect("free");
    registry
        .claim(
            Request::new("acme.com", project, org)
                .base_path("/docs")
                .alias(),
        )
        .expect("free");

    let (domain, path) = registry
        .route("docs.acme.com", "/guides/install")
        .expect("a route");
    assert_eq!(domain.base_path, "");
    assert_eq!(path, "/guides/install");

    let (domain, path) = registry
        .route("acme.com", "/docs/guides/install")
        .expect("a route");
    assert_eq!(domain.base_path, "/docs");
    assert_eq!(path, "/guides/install");

    assert_eq!(
        registry.route("acme.com", "/docs").map(|(_, p)| p),
        Some("/".to_owned()),
        "the base path itself is the site root"
    );
    assert!(
        registry.route("acme.com", "/blog").is_none(),
        "a path outside the base path is not this site's"
    );
    assert!(
        registry.route("acme.com", "/docsearch").is_none(),
        "a path that merely starts with the same characters is not inside it"
    );
}

#[test]
fn a_base_path_that_is_not_a_subpath_is_refused() {
    use liyasa_server::auth::domains::check_base_path;
    assert_eq!(check_base_path(""), Ok(String::new()));
    assert_eq!(check_base_path("/"), Ok(String::new()));
    assert_eq!(check_base_path("/docs"), Ok("/docs".to_owned()));
    assert_eq!(check_base_path("/docs/v2"), Ok("/docs/v2".to_owned()));
    for bad in [
        "docs",
        "/docs/",
        "/docs//v2",
        "/docs/../etc",
        "/docs?a=1",
        "/a b",
    ] {
        let error = check_base_path(bad).expect_err(bad);
        assert_eq!(error.code, code::E0815, "{bad}");
    }
}

/// HOST-23: aliases redirect to the primary, path and base path preserved.
#[tokio::test]
async fn an_alias_redirects_to_the_primary_host() {
    let registry = registry();
    let project = project();
    let org = org();
    registry
        .claim(Request::new("docs.acme.com", project, org))
        .expect("free");
    registry
        .claim(Request::new("acme.io", project, org).alias())
        .expect("free");
    registry
        .claim(
            Request::new("old.acme.com", project, org)
                .base_path("/help")
                .alias(),
        )
        .expect("free");

    assert_eq!(
        registry.alias_redirect("acme.io", "/guides/install"),
        Some("https://docs.acme.com/guides/install".to_owned())
    );
    assert_eq!(
        registry.alias_redirect("old.acme.com", "/help/guides/install"),
        Some("https://docs.acme.com/guides/install".to_owned()),
        "the alias's own base path is stripped before the primary's is applied"
    );
    assert_eq!(
        registry.alias_redirect("docs.acme.com", "/guides/install"),
        None,
        "the primary does not redirect to itself"
    );
}

#[tokio::test]
async fn a_second_project_may_not_take_a_domain_inside_the_same_organization_either() {
    let registry = registry();
    let host = "docs.example.com";
    let shared_org = org();
    registry
        .claim(Request::new(host, project(), shared_org))
        .expect("free");
    let error = registry
        .claim(Request::new(host, project(), shared_org))
        .expect_err("one project per domain, whoever owns them");
    assert_eq!(error.code, code::E0814);
}
