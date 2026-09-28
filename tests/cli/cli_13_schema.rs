//! CLI-13: `liyasa schema [config|frontmatter|components]`.
//!
//! Driven through `Cli::parse_from` and `commands::dispatch`. The bytes the
//! command prints are `schema::named(..).json`, so the assertions about "the
//! output" are made against that string; what is not covered here is the
//! `println!` and the exit code a shell sees, which only a test inside
//! `crates/liyasa-cli/` can spawn a process to check.

use clap::Parser;
use liyasa_cli::cli::Cli;
use liyasa_cli::{Exit, commands};
use liyasa_config::schema::{self, SCHEMAS};
use serde_json::Value;

/// PRD §34.2 with the two keys RFC 0101 records as drift written the way the
/// schema accepts them.
const EXAMPLE: &str = include_str!("../../crates/liyasa-config/tests/fixtures/example.json");
/// §34.2 byte for byte.
const EXAMPLE_PRD: &str =
    include_str!("../../crates/liyasa-config/tests/fixtures/example-prd.json");

fn run(arguments: &[&str]) -> Exit {
    let mut argv = vec!["liyasa"];
    argv.extend_from_slice(arguments);
    let cli = Cli::parse_from(argv);
    commands::dispatch(&cli.global, cli.command)
}

fn validator(schema: &str) -> jsonschema::Validator {
    let parsed: Value = serde_json::from_str(schema).expect("the emitted schema is valid JSON");
    jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .build(&parsed)
        .expect("the emitted schema compiles as a 2020-12 schema")
}

#[test]
fn every_schema_this_build_can_print_is_printable() {
    assert_eq!(run(&["schema"]), Exit::Success, "with no argument it lists");
    for named in SCHEMAS {
        assert_eq!(
            run(&["schema", named.name]),
            Exit::Success,
            "`liyasa schema {}`",
            named.name
        );
    }
}

#[test]
fn a_name_this_build_does_not_have_is_a_usage_error() {
    assert_eq!(
        run(&["schema", "nonsense"]),
        Exit::Usage,
        "naming a schema that does not exist is a mistake in the command line, \
         not a problem with the project"
    );
    assert_eq!(
        run(&["schema", "components"]),
        Exit::Usage,
        "the component schema arrives with the component registry"
    );
}

#[test]
fn what_schema_config_prints_validates_the_example_config() {
    let printed = schema::named("config").expect("config is printable").json;
    let example: Value = serde_json::from_str(EXAMPLE).expect("the example is valid JSON");
    let errors: Vec<String> = validator(printed)
        .iter_errors(&example)
        .map(|error| error.to_string())
        .collect();
    assert_eq!(errors, Vec::<String>::new());
}

/// §34.2 verbatim still does not validate, in exactly one place, and it is
/// prose rather than a schema gap: the example writes `weight` as an array
/// where the schema says string or integer. RFC 0101 carries it; this asserts
/// the count so a second drift cannot appear unnoticed.
#[test]
fn the_prd_example_still_drifts_in_one_place() {
    let printed = schema::named("config").expect("config is printable").json;
    let prd: Value = serde_json::from_str(EXAMPLE_PRD).expect("valid JSON");
    let errors: Vec<String> = validator(printed)
        .iter_errors(&prd)
        .map(|error| error.to_string())
        .collect();
    assert_eq!(errors.len(), 1, "RFC 0101: {errors:?}");
    assert!(
        errors[0].contains("600"),
        "the array weight is the drift: {errors:?}"
    );
}

#[test]
fn the_frontmatter_schema_is_printable_and_compiles() {
    let printed = schema::named("frontmatter")
        .expect("frontmatter is printable")
        .json;
    let _ = validator(printed);
}
