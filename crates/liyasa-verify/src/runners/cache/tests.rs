use std::time::Duration;

use liyasa_core::ids::{BlockId, CheckId, Route};
use liyasa_core::verify::{CheckInput, CheckOutcome, Expectation};
use liyasa_core::vfs::{Bytes, VfsPath};

use super::*;

const DIGEST: &str = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const OTHER: &str = "sha256:fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

fn pin(digest: &str) -> ImagePin {
    ImagePin {
        image: "busybox".to_owned(),
        digest: digest.to_owned(),
    }
}

fn spec(source: &str) -> CheckSpec {
    CheckSpec {
        id: CheckId::new("/guide#block#0"),
        page: Route::new("/guide"),
        block: BlockId::explicit("block"),
        runner: "shell".to_owned(),
        input: CheckInput::Code {
            lang: "bash".to_owned(),
            source: source.to_owned(),
            hidden_lines: Vec::new(),
        },
        expect: vec![Expectation::Exit(0)],
        timeout: Duration::from_secs(30),
        needs_network: false,
        needs_secrets: Vec::new(),
    }
}

fn result(id: &str) -> CheckResult {
    CheckResult {
        id: CheckId::new(id),
        outcome: CheckOutcome::Pass,
        duration: Duration::from_millis(1),
        digest: Fingerprint::of(b"x"),
    }
}

#[test]
fn the_same_check_keys_the_same_twice() {
    let binding = Binding::default();
    assert_eq!(
        key(&spec("echo hi\n"), &binding, &pin(DIGEST)),
        key(&spec("echo hi\n"), &binding, &pin(DIGEST))
    );
}

#[test]
fn a_changed_block_is_a_different_check() {
    let binding = Binding::default();
    assert_ne!(
        key(&spec("echo hi\n"), &binding, &pin(DIGEST)),
        key(&spec("echo bye\n"), &binding, &pin(DIGEST))
    );
}

#[test]
fn a_changed_image_digest_is_a_different_check() {
    let binding = Binding::default();
    assert_ne!(
        key(&spec("echo hi\n"), &binding, &pin(DIGEST)),
        key(&spec("echo hi\n"), &binding, &pin(OTHER))
    );
}

#[test]
fn a_changed_setup_is_a_different_check() {
    let with = Binding {
        setup: Some("export A=1".to_owned()),
        ..Binding::default()
    };
    assert_ne!(
        key(&spec("echo hi\n"), &Binding::default(), &pin(DIGEST)),
        key(&spec("echo hi\n"), &with, &pin(DIGEST))
    );
}

#[test]
fn a_fixture_whose_contents_changed_is_a_different_check() {
    let fixture = |body: &[u8]| Binding {
        fixtures: vec![(VfsPath::new("data.json"), Bytes::from(body.to_vec()))],
        ..Binding::default()
    };
    assert_ne!(
        key(&spec("echo hi\n"), &fixture(b"[]"), &pin(DIGEST)),
        key(&spec("echo hi\n"), &fixture(b"[1]"), &pin(DIGEST))
    );
}

#[test]
fn a_changed_environment_is_a_different_check() {
    let env = Binding {
        env: vec![("TOKEN".to_owned(), "abc".to_owned())],
        ..Binding::default()
    };
    assert_ne!(
        key(&spec("echo hi\n"), &Binding::default(), &pin(DIGEST)),
        key(&spec("echo hi\n"), &env, &pin(DIGEST))
    );
}

#[test]
fn the_key_does_not_move_when_only_the_check_id_does() {
    // VER-06 keys on the block, not on where it sits: the same sample on two
    // pages is one run.
    let mut moved = spec("echo hi\n");
    moved.id = CheckId::new("/elsewhere#other#3");
    moved.page = Route::new("/elsewhere");
    assert_eq!(
        key(&spec("echo hi\n"), &Binding::default(), &pin(DIGEST)),
        key(&moved, &Binding::default(), &pin(DIGEST))
    );
}

#[test]
fn a_stored_result_comes_back() {
    let mut cache = ResultCache::new();
    let k = key(&spec("echo hi\n"), &Binding::default(), &pin(DIGEST));
    assert!(cache.get(&k).is_none());
    cache.put(k, result("/guide#block#0"));
    assert_eq!(cache.get(&k).map(|r| r.id.as_str()), Some("/guide#block#0"));
    assert_eq!(cache.len(), 1);
}

#[test]
fn no_cache_stores_nothing_and_answers_nothing() {
    let mut cache = ResultCache::disabled();
    assert!(!cache.is_enabled());
    let k = key(&spec("echo hi\n"), &Binding::default(), &pin(DIGEST));
    cache.put(k, result("/guide#block#0"));
    assert!(cache.get(&k).is_none());
    assert!(cache.is_empty());
}
