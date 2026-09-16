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
