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
