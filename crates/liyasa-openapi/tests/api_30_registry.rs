//! API-30 umbrella: the registry lists each generator with its template path
//! and its corpus, and every P0 generator is in it.

use std::path::Path;

use liyasa_openapi::codegen::{DEFAULT_LANGUAGES, GENERATORS, Registry, corpus, generator};
use liyasa_openapi::load;
use liyasa_openapi::sample::{Options, Request};

/// API-30.1, 30.2, 30.4, 30.6.
const REQUIRED: &[&str] = &["curl", "javascript", "python", "go"];

#[test]
fn every_p0_generator_is_registered_with_a_template_and_a_corpus() {
    for language in REQUIRED {
        let found = generator(language).unwrap_or_else(|| panic!("`{language}` is not registered"));
        assert!(!found.template_path.is_empty());
        assert!(!found.corpus.is_empty(), "`{language}` lists no corpus");
        assert!(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(found.template_path)
                .is_file(),
            "`{}` names a template that is not there",
            found.template_path
        );
    }
}

#[test]
fn the_registry_has_no_repeated_or_empty_rows() {
    for (index, generator) in GENERATORS.iter().enumerate() {
        assert!(!generator.id.is_empty());
        assert!(!generator.label.is_empty());
        assert!(!generator.highlight.is_empty());
        assert!(
            !GENERATORS[..index]
                .iter()
                .any(|other| other.id == generator.id),
            "`{}` is registered twice",
            generator.id
        );
        assert!(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(generator.template_path)
                .is_file(),
            "`{}` names a template that is not there",
            generator.template_path
        );
    }
}

#[test]
fn the_default_language_list_is_the_p0_set() {
    assert_eq!(DEFAULT_LANGUAGES, REQUIRED);
}

#[test]
fn an_operator_replaces_one_generators_template_by_name() {
    let loaded =
        load::from_bytes("api", "corpus.yaml", corpus::SPEC.as_bytes()).expect("the corpus loads");
    let spec = loaded.spec;
    let operation = spec
        .by_operation_id("pathParams")
        .expect("the case is there");
    let request = Request::build(&spec, &operation, &Options::default());

    let mut registry = Registry::new();
    registry
        .override_template("curl", "http {{ req.method }} {{ req.url }}".to_owned())
        .expect("the replacement compiles");

    let sample = registry.render("curl", &request).expect("it renders");
    assert_eq!(
        sample.source,
        "http GET https://api.example.com/v1/users/u_1/notes/7"
    );
    assert_eq!(sample.highlight, "bash", "the row's metadata is unchanged");
}

#[test]
fn a_template_that_does_not_compile_is_refused_rather_than_panicking() {
    let mut registry = Registry::new();
    let error = registry
        .override_template("curl", "{% for %}".to_owned())
        .expect_err("a broken template is refused");
    assert!(!error.is_empty());
}
