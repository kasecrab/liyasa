use super::*;

const DIGEST: &str = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn config(pairs: &[(&str, &str)], registry: Option<&str>) -> RunnersConfig {
    RunnersConfig {
        registry: registry.map(str::to_owned),
        images: pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
        ..RunnersConfig::default()
    }
}

#[test]
fn a_reference_with_a_digest_is_a_pin() {
    let pin = ImagePin::parse(&format!("docker.io/library/rust@{DIGEST}")).expect("a pin");
    assert_eq!(pin.image, "docker.io/library/rust");
    assert_eq!(pin.digest, DIGEST);
    assert_eq!(pin.reference(), format!("docker.io/library/rust@{DIGEST}"));
}

#[test]
fn a_tag_is_not_a_pin() {
    assert!(ImagePin::parse("docker.io/library/rust:1.90").is_none());
    assert!(ImagePin::parse("rust").is_none());
    assert!(ImagePin::parse("rust@sha256:short").is_none());
    assert!(ImagePin::parse("rust@md5:0123").is_none());
    assert!(ImagePin::parse(&format!("@{DIGEST}")).is_none());
}

#[test]
fn a_language_nothing_pins_is_e0610() {
    let images = Images::new(&config(&[], None));
    let problem = images.pin("rust").expect_err("no pin");
    assert_eq!(problem.code, code::E0610);
}

#[test]
fn a_language_pinned_only_by_a_tag_is_e0610() {
    let images = Images::new(&config(&[("rust", "docker.io/library/rust:1.90")], None));
    assert_eq!(images.pin("rust").expect_err("no digest").code, code::E0610);
}

#[test]
fn the_lock_supplies_a_digest_config_does_not() {
    let images = Images::new(&config(&[], None)).with_lock([(
        "rust".to_owned(),
        format!("docker.io/library/rust@{DIGEST}"),
    )]);
    assert_eq!(images.pin("rust").expect("a pin").digest, DIGEST);
}

#[test]
fn config_wins_over_the_lock() {
    let images = Images::new(&config(&[("rust", &format!("mine/rust@{DIGEST}"))], None))
        .with_lock([("rust".to_owned(), format!("theirs/rust@{DIGEST}"))]);
    assert_eq!(images.pin("rust").expect("a pin").image, "mine/rust");
}

#[test]
fn a_private_registry_replaces_the_host() {
    let images = Images::new(&config(
        &[("rust", &format!("docker.io/library/rust@{DIGEST}"))],
        Some("registry.acme.internal"),
    ));
    assert_eq!(
        images.pin("rust").expect("a pin").image,
        "registry.acme.internal/library/rust"
    );
}

#[test]
fn a_private_registry_prefixes_an_image_that_names_no_host() {
    let images = Images::new(&config(
        &[("go", &format!("golang@{DIGEST}"))],
        Some("registry.acme.internal/"),
    ));
    assert_eq!(
        images.pin("go").expect("a pin").image,
        "registry.acme.internal/golang"
    );
}

#[test]
fn a_language_is_matched_without_regard_to_case() {
    let images = Images::new(&config(&[("Python", &format!("python@{DIGEST}"))], None));
    assert!(images.pin("PYTHON").is_ok());
}

#[test]
fn a_runner_finds_its_image_under_any_of_the_names_it_claims() {
    let images = Images::new(&config(&[("shell", &format!("busybox@{DIGEST}"))], None));
    assert_eq!(
        images.pin_any(&["bash", "shell"]).expect("a pin").image,
        "busybox"
    );
    assert_eq!(
        images.pin_any(&["bash", "sh"]).expect_err("nothing").code,
        code::E0610
    );
    assert_eq!(images.pin_any(&[]).expect_err("nothing").code, code::E0610);
}
