//! The hosting fixture gives each `Site` its own directory.
//!
//! `Site::build` opens with `remove_dir_all`, so two live `Site`s rooted at one
//! path are a pair that deletes each other's files: the second build wipes the
//! first's `dist/`, and whichever drops first takes the other's project with
//! it. The failure is a race, so it shows up as `tests/src/hosting.rs`
//! panicking somewhere unrelated — a missing `dist/index.html`, a `_headers`
//! that "differs from what the tests generate", a `fs::write` with no parent
//! directory — in a test that has nothing to do with the one it collided with.
//!
//! Nothing requires two call sites to pass the same `name` for this to happen.
//! `agt_02_research_seam.rs` has one call site, in a helper five tests call, so
//! a search for a duplicated literal finds nothing; under `cargo test` those
//! five run as threads in one process and share a root. That shape failed 7
//! times in 25 runs of the file alone, and it is what put `main` red on
//! 76cef5a. `bin/gate` runs nextest, which gives every test its own process and
//! its own pid, so the local gate cannot see any of it.
//!
//! The uniqueness therefore has to come from the harness rather than from each
//! caller remembering to pick an unused name.

use liyasa_build::engine::Options;
use liyasa_tests::hosting::{CONFIG, Site};

/// Two fixtures built under one name do not share a directory.
#[test]
fn two_sites_under_one_name_get_their_own_directories() {
    let first = Site::build("isolation", CONFIG, Options::default());
    let second = Site::build("isolation", CONFIG, Options::default());

    assert_ne!(
        first.dist(),
        second.dist(),
        "both fixtures are rooted at the same path, so the second build's \
         `remove_dir_all` has already deleted the first's files"
    );

    // The damage the shared root does is not only at build time: `Drop` removes
    // the root, so one fixture going out of scope would take a still-live one's
    // project with it. That is the half that strikes a test which never touched
    // this name.
    drop(second);
    assert!(
        first.read("index.html").contains("Home"),
        "the first fixture lost its `dist/` when the second was dropped"
    );
}
