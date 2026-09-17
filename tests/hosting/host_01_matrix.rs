//! HOST-01: the fixture, uploaded to an emulator of each listed host, yields
//! the matrix the docs show, and that matrix is the checked-in one.
//!
//! `LIYASA_UPDATE_MATRIX=1` rewrites `MATRIX.md` from the current model; the
//! diff is then reviewed like any other change to the emulators.

use liyasa_build::engine::Options;
use liyasa_build::hosting::emulate::{Dist, Host};
use liyasa_build::hosting::matrix::{self, Outcome};
use liyasa_tests::hosting::{CONFIG, Site};

const MATRIX_PATH: &str = "crates/liyasa-build/src/hosting/MATRIX.md";

#[test]
fn the_generated_matrix_is_the_checked_in_one() {
    let site = Site::build("host01-matrix", CONFIG, Options::default());
    let dist = Dist::read(&site.dist()).expect("the upload");
    let generated = matrix::generate(&dist).markdown();
    if std::env::var_os("LIYASA_UPDATE_MATRIX").is_some() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(MATRIX_PATH);
        std::fs::write(&path, &generated).expect("MATRIX.md is writable");
    }
    assert_eq!(
        generated,
        matrix::EXPECTED,
        "the matrix moved; review the emulators, then run with LIYASA_UPDATE_MATRIX=1"
    );
}

/// Every URL the built page asks the browser for, in document order.
fn referenced_assets(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (attribute, marker) in [("href=\"", "rel=\"stylesheet\""), ("src=\"", "<script")] {
        let mut rest = html;
        while let Some(at) = rest.find(marker) {
            rest = &rest[at + marker.len()..];
            let element_end = rest.find('>').unwrap_or(rest.len());
            let element = &rest[..element_end];
            if let Some(start) = element.find(attribute) {
                let value = &element[start + attribute.len()..];
                if let Some(end) = value.find('"') {
                    let url = &value[..end];
                    if url.starts_with('/') {
                        out.push(url.to_owned());
                    }
                }
            }
        }
    }
    out
}

/// The defect WP-16 found: a `dist/` on GitHub Pages renders with no
/// stylesheet and no client behaviour, because Jekyll publishes nothing whose
/// name begins with `_` or `.` and the theme lives in `_liyasa/`.
#[test]
fn every_host_serves_the_assets_the_page_asks_for() {
    let site = Site::build("host01-assets", CONFIG, Options::default());
    let dist = Dist::read(&site.dist()).expect("the upload");
    let assets = referenced_assets(&site.read("index.html"));
    assert!(
        assets.iter().any(|url| url.ends_with(".css")),
        "the page links a stylesheet: {assets:?}"
    );
    assert!(
        assets.iter().any(|url| url.ends_with(".js")),
        "the page loads a script: {assets:?}"
    );
    for host in Host::ALL {
        for url in &assets {
            let response = host.serve(&dist, url);
            assert_eq!(
                response.status,
                200,
                "{} does not serve {url}, which the page cannot render without",
                host.name()
            );
        }
        // The agent surfaces live under a dot-directory and go the same way.
        for path in dist
            .paths()
            .filter(|path| path.starts_with(".well-known/"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
        {
            assert_eq!(
                host.serve(&dist, &path).status,
                200,
                "{} does not serve {path}",
                host.name()
            );
        }
        assert!(
            host.dropped(&dist).is_empty(),
            "{} drops {:?}",
            host.name(),
            host.dropped(&dist)
        );
    }
}

#[test]
fn the_hosts_that_read_the_build_s_files_pass_every_header_check() {
    let site = Site::build("host01-headers", CONFIG, Options::default());
    let dist = Dist::read(&site.dist()).expect("the upload");
    let matrix = matrix::generate(&dist);
    for host in [Host::CloudflarePages, Host::Netlify, Host::Vercel] {
        for id in [
            "markdown-url-support",
            "http-status-codes",
            "redirect-behavior",
            "security-headers",
            "content-security-policy",
            "immutable-assets",
            "trailing-slash",
        ] {
            let cell = matrix.cell(id, host).expect("a cell");
            assert_eq!(
                cell.outcome,
                Outcome::Pass,
                "{}: {id}: {:?}",
                host.name(),
                cell.note
            );
        }
    }
    for host in Host::ALL {
        let status = matrix.cell("http-status-codes", host).expect("a cell");
        assert_ne!(status.outcome, Outcome::Fail, "{}", host.name());
        let negotiation = matrix.cell("content-negotiation", host).expect("a cell");
        assert_eq!(negotiation.outcome, Outcome::Partial, "{}", host.name());
    }
    let github = matrix
        .cell("security-headers", Host::GitHubPages)
        .expect("a cell");
    assert_eq!(github.outcome, Outcome::Manual);
}
