//! RX-112: every header beyond the CSP is in `_headers` and `vercel.json`
//! with the specified value, `frame-ancestors` is relaxed only on the
//! frame-mode route, and nothing in a static build is `no-store`.

use liyasa_build::engine::Options;
use liyasa_build::hosting::emulate::{Dist, Host};
use liyasa_build::hosting::headers::{CACHE_PRIVATE, HSTS, PERMISSIONS_POLICY};
use liyasa_tests::hosting::{CONFIG, FRAME_ROUTE, Site, header};

const EXPECTED: [(&str, &str); 6] = [
    ("Strict-Transport-Security", HSTS),
    ("X-Content-Type-Options", "nosniff"),
    ("Referrer-Policy", "strict-origin-when-cross-origin"),
    ("X-Frame-Options", "DENY"),
    ("Permissions-Policy", PERMISSIONS_POLICY),
    ("Cross-Origin-Opener-Policy", "same-origin"),
];

#[test]
fn every_response_carries_the_set_with_the_specified_values() {
    let site = Site::build("rx112-set", CONFIG, Options::default());
    for path in [
        "/",
        "/guides/install/",
        "/guides/install.md",
        "/assets/manual.pdf",
    ] {
        let headers = site.output.rules.resolve(path);
        for (name, value) in EXPECTED {
            assert_eq!(header(&headers, name), Some(value), "{name} on {path}");
        }
        assert!(
            header(&headers, "Cross-Origin-Embedder-Policy").is_none(),
            "{path}"
        );
    }
    let vercel = site.vercel_headers("/(.*)");
    for (name, value) in EXPECTED {
        assert_eq!(header(&vercel, name), Some(value), "{name} in vercel.json");
    }
    assert!(!site.headers_file().contains(CACHE_PRIVATE));
}

#[test]
fn frame_ancestors_is_relaxed_on_the_frame_route_only() {
    let site = Site::build("rx112-frame", CONFIG, Options::default());
    let dist = Dist::read(&site.dist()).expect("the upload");
    for host in [Host::CloudflarePages, Host::Netlify, Host::Vercel] {
        let widget = host.serve(&dist, &format!("{FRAME_ROUTE}/"));
        assert_eq!(widget.status, 200, "{}", host.name());
        let csp = widget.header("Content-Security-Policy").expect("a policy");
        assert!(
            csp.ends_with("frame-ancestors https://app.acme.com"),
            "{}: {csp}",
            host.name()
        );
        assert_eq!(widget.header("X-Frame-Options"), Some("SAMEORIGIN"));
        for path in ["/", "/guides/install/", "/embed/"] {
            let page = host.serve(&dist, path);
            let csp = page.header("Content-Security-Policy").expect("a policy");
            assert!(
                csp.ends_with("frame-ancestors 'none'"),
                "{}: {path}: {csp}",
                host.name()
            );
            assert_eq!(page.header("X-Frame-Options"), Some("DENY"));
        }
    }
}

#[test]
#[ignore = "waits on wp/00-contracts-harness: schema::without strips a key the schema does not declare"]
fn hsts_preload_is_opt_in_from_the_config() {
    // RFC 1201's key: a build reads `security.hstsPreload` into the policy, so
    // an operator who sets it gets the preload directive and one who does not
    // gets the plain HSTS header.
    //
    // `schema::check` reports an undeclared key as `E0103` at warning severity
    // and `schema::without` then removes it from the value, so until the
    // schema row is on `main` the engine cannot see what the operator wrote.
    // Swapping only `schemas/liyasa.schema.json` from
    // `wp/00-contracts-harness` into a tree makes this pass, which is how the
    // blocker was confirmed to be the merge rather than the wiring; the
    // engine's own reader is unit-tested either way.
    const PRELOAD: &str = r#"{
      "name": "Acme docs",
      "seo": { "canonicalOrigin": "https://docs.acme.com" },
      "security": { "hstsPreload": true }
    }"#;
    let site = Site::build("rx112-preload", PRELOAD, Options::default());
    let headers = site.headers_file();
    assert!(
        headers.contains("Strict-Transport-Security: max-age=63072000; includeSubDomains; preload"),
        "{headers}"
    );

    let without = Site::build("rx112-no-preload", CONFIG, Options::default());
    let plain = without.headers_file();
    assert!(plain.contains("Strict-Transport-Security: max-age=63072000; includeSubDomains\n"));
    assert!(!plain.contains("preload"), "{plain}");
}
