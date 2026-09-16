//! RX-110: the build's CSP in `_headers` and `vercel.json` is the policy the
//! requirement lists, its nonce is the build nonce, its style hash is the hash
//! of the critical block the page inlines, and an integration adds exactly
//! its declared sources.

use liyasa_build::engine::Options;
use liyasa_build::hosting::{self, digest, integrations};
use liyasa_tests::hosting::{CONFIG, CONFIG_BARE, Site, critical_block, header};

fn csp(site: &Site, path: &str) -> String {
    header(&site.output.rules.resolve(path), "Content-Security-Policy")
        .expect("a policy")
        .to_owned()
}

#[test]
fn the_policy_is_the_one_rx_110_lists() {
    let site = Site::build("rx110-policy", CONFIG, Options::default());
    let manifest = site.report.manifest.as_ref().expect("a manifest");
    let nonce = hosting::build_nonce(&manifest.build_id);
    let critical = critical_block(&site.read("index.html"));
    let policy = csp(&site, "/guides/install/");
    let expected = format!(
        "default-src 'self'; script-src 'self' 'nonce-{nonce}' plausible.io; \
         style-src 'self' {}; img-src 'self' data: images.acme.com; media-src 'self'; \
         frame-src 'self'; connect-src 'self' plausible.io; object-src 'none'; \
         base-uri 'self'; form-action 'self'; frame-ancestors 'none'",
        digest::csp_sha256(&critical)
    );
    assert_eq!(policy, expected);
    assert_eq!(
        header(&site.vercel_headers("/(.*)"), "Content-Security-Policy"),
        Some(policy.as_str())
    );
    assert!(
        site.headers_file()
            .contains(&format!("  Content-Security-Policy: {policy}\n"))
    );
}

#[test]
fn the_nonce_is_the_build_nonce_and_stable_across_a_rebuild() {
    let first = Site::build("rx110-nonce", CONFIG, Options::default());
    let manifest = first.report.manifest.as_ref().expect("a manifest");
    let nonce = hosting::build_nonce(&manifest.build_id);
    assert_eq!(first.output.policy.nonce, nonce);
    assert_eq!(nonce.len(), 24);
    // The same inputs and clock give the same build ID, so a `304` against a
    // cached page still matches the header a redeploy of the same build sends.
    let again = Site::build("rx110-nonce-again", CONFIG, Options::default());
    assert_eq!(again.output.policy.nonce, nonce);
    // TODO(rfc-1200): once the engine renders with `hosting::build_nonce`,
    // assert the page's `<script nonce="…">` carries the same value.
}

#[test]
fn the_style_hash_is_computed_over_the_inlined_block() {
    let site = Site::build("rx110-style", CONFIG, Options::default());
    let critical = critical_block(&site.read("index.html"));
    assert!(!critical.is_empty());
    let hash = digest::csp_sha256(&critical);
    assert!(csp(&site, "/").contains(&format!("style-src 'self' {hash};")));
    // Every page inlines the same block, so one hash covers the site.
    assert_eq!(
        critical_block(&site.read("guides/install/index.html")),
        critical
    );
}

#[test]
fn enabling_an_integration_adds_exactly_its_declared_sources() {
    let with = Site::build("rx110-with", CONFIG, Options::default());
    let without = Site::build("rx110-without", CONFIG_BARE, Options::default());
    let plausible = integrations::by_key("plausible").expect("a registry row");
    // The config is a build input, so the two builds have different nonces;
    // everything else in the policy must differ by the registry row alone.
    let mut expected = without.output.policy.clone();
    expected.add_integration(plausible);
    expected.nonce = with.output.policy.nonce.clone();
    assert_eq!(with.output.policy, expected);
    let stripped = csp(&with, "/")
        .replace(" plausible.io", "")
        .replace(&with.output.policy.nonce, &without.output.policy.nonce);
    assert_eq!(stripped, csp(&without, "/"));
}
