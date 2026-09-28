use super::*;
use crate::lock::{Runner as LockedRunner, Tool};

fn lock(runners: Vec<LockedRunner>) -> Lock {
    Lock {
        version: 1,
        liyasa: Tool {
            version: "0.1.0".to_owned(),
        },
        theme: None,
        components: Vec::new(),
        runners,
        fonts: Default::default(),
        companion: None,
    }
}

fn locked(id: &str, image: &str, digest: &str) -> LockedRunner {
    LockedRunner {
        id: id.to_owned(),
        image: image.to_owned(),
        digest: digest.to_owned(),
    }
}

const DIGEST: &str = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

#[test]
fn a_locked_runner_becomes_a_pinned_image() {
    let lock = lock(vec![locked(
        "python",
        "docker.io/library/python:3.12",
        DIGEST,
    )]);
    let mut config = VerifyConfig::default();

    pin_from_lock(&mut config, Some(&lock));

    assert_eq!(
        config.runners.images.get("python").map(String::as_str),
        Some(format!("docker.io/library/python:3.12@{DIGEST}").as_str())
    );
}

#[test]
fn without_a_lock_a_language_has_no_pin() {
    let mut config = VerifyConfig::default();

    pin_from_lock(&mut config, None);

    assert!(
        config.runners.images.is_empty(),
        "an unpinned image is E0610, not a silent `latest`"
    );
}

/// The lock fills what the config left unset and never the other way round:
/// the config is what the operator is editing now.
#[test]
fn the_config_wins_over_the_lock() {
    let mut config = VerifyConfig::default();
    config.runners.images.insert(
        "python".to_owned(),
        format!("internal.example/python@{DIGEST}"),
    );
    let lock = lock(vec![locked(
        "python",
        "docker.io/library/python:3.12",
        DIGEST,
    )]);

    pin_from_lock(&mut config, Some(&lock));

    assert_eq!(
        config.runners.images.get("python").map(String::as_str),
        Some(format!("internal.example/python@{DIGEST}").as_str())
    );
}

/// `Images` lower-cases what it looks up, so a lock entry written `Python`
/// has to arrive lower-cased or it is never found.
#[test]
fn a_lock_entry_is_keyed_the_way_images_looks_it_up() {
    let lock = lock(vec![locked(
        "Python",
        "docker.io/library/python:3.12",
        DIGEST,
    )]);
    let mut config = VerifyConfig::default();

    pin_from_lock(&mut config, Some(&lock));

    assert!(
        config.runners.images.contains_key("python"),
        "{:?}",
        config.runners.images
    );
}
