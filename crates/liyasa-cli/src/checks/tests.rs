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

    let (registry, problems) = registry(&VerifyConfig::default(), Some(&lock));

    assert!(problems.is_empty(), "{problems:?}");
    assert!(
        registry.for_language("python").is_some(),
        "the built-in runners are still there"
    );
    let images = Images::new(&VerifyConfig::default().runners).with_lock(pins(Some(&lock)));
    let pin = images.pin_any(&["python"]).expect("the lock pinned python");
    assert_eq!(pin.image, "docker.io/library/python:3.12");
    assert_eq!(pin.digest, DIGEST);
}

#[test]
fn without_a_lock_a_language_has_no_pin() {
    assert!(pins(None).is_empty());
    let images = Images::new(&VerifyConfig::default().runners).with_lock(pins(None));
    assert!(
        images.pin_any(&["python"]).is_err(),
        "an unpinned image is E0610, not a silent `latest`"
    );
}

/// `with_lock` fills what the config left unset and never the other way
/// round: the config is what the operator is editing now.
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

    let images = Images::new(&config.runners).with_lock(pins(Some(&lock)));

    assert_eq!(
        images.pin_any(&["python"]).expect("pinned").image,
        "internal.example/python"
    );
}
