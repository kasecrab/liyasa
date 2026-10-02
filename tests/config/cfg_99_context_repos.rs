//! CFG-99 and GIT-11, across the seam: what `liyasa-config` reads out of
//! `contextRepos[]` is what `liyasa-git` plans a clone from.
//!
//! Three separate claims live here because they can only be made from a
//! package that sees both crates:
//!
//!   * the handoff composes — a config value becomes a `CloneSpec` with no
//!     decision taken in between;
//!   * the duplicated constant agrees with the one it duplicates;
//!   * the two size parsers agree with the ones already in the workspace,
//!     which is the price of `liyasa-config` owning the spelling.
//!
//! Without the first of those, "a reader exists" is a claim about structure:
//! the reader could return numbers the clone policy refuses and every
//! single-crate test would still pass.

use liyasa_config::context_repos::{self, ContextRepoConfig, context_repos};
use liyasa_git::clone::{CloneRefusal, CloneSpec, ContextRepo, DEFAULT_DEPTH, DEFAULT_MAX_BYTES};
use serde_json::json;

/// The mapping a consumer writes, and the only one: every field is carried,
/// nothing is defaulted here. If this function ever needs a judgement, the
/// reader is the wrong shape.
fn as_context_repo(entry: &ContextRepoConfig) -> ContextRepo {
    let mut repo = ContextRepo::new(&entry.repo);
    repo.r#ref = entry.r#ref.clone();
    repo.paths = entry.paths.clone();
    repo.depth = entry.depth;
    repo.max_bytes = entry.max_bytes;
    repo.refresh = entry.refresh_seconds;
    repo
}

#[test]
fn a_configured_repository_becomes_a_clone_spec() {
    let repos = context_repos(&json!({
        "contextRepos": [{
            "repo": "https://github.com/acme/api.git",
            "ref": "v2",
            "paths": ["openapi.yaml", "/spec/"],
            "depth": 1,
            "maxBytes": "512MB",
            "refresh": "6h"
        }]
    }));
    let entry = repos.first().expect("one entry");
    let spec = CloneSpec::plan(&as_context_repo(entry), None).expect("GIT-11 allows it");

    assert_eq!(spec.repo, "https://github.com/acme/api.git");
    assert_eq!(spec.r#ref.as_deref(), Some("v2"));
    assert_eq!(spec.depth, 1);
    assert!(spec.blob_filter, "GIT-11 clones with `--filter=blob:none`");
    assert_eq!(spec.max_bytes, 512 * 1024 * 1024);
    assert_eq!(spec.refresh.as_secs(), 6 * 60 * 60);
    // `/spec/` and `spec` are one path to a sparse checkout, and the file in
    // the allow list survives the trip.
    assert_eq!(spec.sparse_paths, vec!["openapi.yaml", "spec"]);
    assert!(spec.allows("openapi.yaml"));
    assert!(spec.allows("spec/api.yaml"));
    assert!(!spec.allows("src/main.rs"));
}

#[test]
fn what_the_config_leaves_out_is_what_git_defaults() {
    let repos = context_repos(&json!({
        "contextRepos": [{ "repo": "acme/api", "paths": ["openapi.yaml"] }]
    }));
    let spec = CloneSpec::plan(&as_context_repo(repos.first().expect("one entry")), None)
        .expect("GIT-11 allows it");
    assert_eq!(spec.depth, DEFAULT_DEPTH);
    assert_eq!(spec.max_bytes, DEFAULT_MAX_BYTES);
    assert_eq!(
        spec.refresh.as_secs(),
        24 * 60 * 60,
        "GIT-11's daily refresh, invented by the package that owns it"
    );
}

#[test]
fn the_entry_validate_warns_about_is_the_one_a_clone_refuses() {
    // W0142 and `CloneRefusal::NoPaths` are the same mistake seen twice, and
    // the point of the warning is that it arrives first. If these two ever
    // disagree, the warning is lying about what a deploy will do.
    let repos = context_repos(&json!({ "contextRepos": [{ "repo": "acme/api" }] }));
    let refusal = CloneSpec::plan(&as_context_repo(repos.first().expect("one entry")), None)
        .expect_err("no paths is refused");
    assert_eq!(refusal, CloneRefusal::NoPaths("acme/api".to_owned()));

    let repos = context_repos(&json!({
        "contextRepos": [{ "repo": "acme/api", "paths": ["../secrets"] }]
    }));
    let refusal = CloneSpec::plan(&as_context_repo(repos.first().expect("one entry")), None)
        .expect_err("a path that climbs out is refused");
    assert!(
        matches!(refusal, CloneRefusal::PathEscapes { .. }),
        "{refusal:?}"
    );
    assert!(
        context_repos::escapes("../secrets"),
        "and the config-side check agrees about which paths those are"
    );
}

#[test]
fn the_limit_is_one_number_in_two_crates() {
    assert_eq!(
        context_repos::MAX_CONTEXT_REPOS,
        liyasa_git::clone::MAX_CONTEXT_REPOS,
        "`liyasa-config` duplicates this constant because it does not depend on \
         `liyasa-git`; the duplicate has to agree"
    );
}

#[test]
fn the_byte_parser_agrees_with_the_one_search_already_had() {
    // `liyasa-config` parses `maxBytes` because it owns the pattern that
    // admits `1.5 GB`. `liyasa-search` has parsed the same spelling since
    // before this reader existed, and two parsers for one syntax are a
    // disagreement waiting to happen unless something asserts otherwise.
    for text in [
        "512MB",
        "1.5 GB",
        "900",
        "1KB",
        "0.5MB",
        "2GB",
        "100 B",
        "12 flagons",
        "",
        "MB",
    ] {
        assert_eq!(
            context_repos::parse_bytes(text),
            liyasa_search::config::parse_bytes(text),
            "`{text}` is read differently by the two parsers"
        );
    }
}

#[test]
fn the_refresh_parser_agrees_with_the_duration_setting() {
    // Same argument for `refresh`, against VER-77's parser — the one
    // `review.rs` deliberately defers to rather than duplicating.
    for text in ["500ms", "30s", "5m", "6h", "180d", "0s", "1w", "d", ""] {
        let theirs = liyasa_verify::core::duration::DurationSetting::parse(text)
            .ok()
            .map(|setting| setting.as_duration().as_secs());
        assert_eq!(
            context_repos::parse_refresh_seconds(text),
            theirs,
            "`{text}` is read differently by the two parsers"
        );
    }
}
