use liyasa_core::diagnostics::code;

use super::*;
use crate::core::config::{CustomRunner, RunnersConfig};

const DIGEST: &str = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn config(custom: Vec<CustomRunner>) -> VerifyConfig {
    VerifyConfig {
        runners: RunnersConfig {
            images: [("shell".to_owned(), format!("busybox@{DIGEST}"))]
                .into_iter()
                .collect(),
            custom,
            ..RunnersConfig::default()
        },
        ..VerifyConfig::default()
    }
}

#[test]
fn the_registry_claims_every_language_ver_02_names() {
    let (registry, problems) = sandboxed(&config(Vec::new()));
    assert!(problems.is_empty());
    for language in [
        "bash",
        "sh",
        "zsh",
        "fish",
        "powershell",
        "python",
        "node",
        "typescript",
        "go",
        "rust",
    ] {
        assert!(
            registry.for_language(language).is_some(),
            "no runner claims `{language}`"
        );
    }
}

#[test]
fn every_sandboxed_runner_says_it_needs_a_sandbox() {
    let (registry, _) = sandboxed(&config(Vec::new()));
    for language in ["bash", "python", "go", "rust", "typescript"] {
        assert_eq!(
            registry
                .for_language(language)
                .expect("a runner")
                .isolation(),
            liyasa_core::verify::Isolation::Sandbox
        );
    }
}

#[test]
fn a_declared_runner_takes_a_language_from_a_built_in_one() {
    let (registry, problems) = sandboxed(&config(vec![CustomRunner {
        id: "my-python".to_owned(),
        languages: vec!["python".to_owned()],
        image: None,
        command: vec!["pypy".to_owned(), "{file}".to_owned()],
        timeout: None,
        network: false,
    }]));
    assert!(problems.is_empty());
    assert_eq!(
        registry.for_language("python").expect("a runner").id(),
        "my-python"
    );
    // The built-in is still there under its own id.
    assert!(registry.by_id("python").is_some());
}

#[test]
fn a_declaration_liyasa_cannot_use_costs_only_itself() {
    let (registry, problems) = sandboxed(&config(vec![CustomRunner {
        id: "broken".to_owned(),
        languages: Vec::new(),
        image: None,
        command: vec!["run".to_owned()],
        timeout: None,
        network: false,
    }]));
    assert_eq!(problems.len(), 1);
    assert_eq!(problems.iter().next().expect("one").code, code::E0613);
    assert!(registry.for_language("bash").is_some());
}

#[test]
fn the_configured_hide_prefix_reaches_the_runners() {
    let mut config = config(Vec::new());
    config.hide_prefix = "#~ ".to_owned();
    let (registry, _) = sandboxed(&config);
    assert!(registry.for_language("bash").is_some());
}
