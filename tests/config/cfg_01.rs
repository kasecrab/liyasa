//! CFG-01: `name` is required, everything else has a default, and `public:
//! false` needs a server this edition does not ship.

use liyasa_tests::config::{codes, one, project};

const PAGES: &[(&str, &str)] = &[("index.md", "# Home")];

fn with_config(config: &str) -> Vec<(&str, &str)> {
    let mut files = vec![("liyasa.json", config)];
    files.extend_from_slice(PAGES);
    files
}

#[test]
fn a_config_without_a_name_is_e0102() {
    let checked = project(&with_config(
        r#"{ "description": "docs", "seo": { "canonicalOrigin": "https://acme.dev" } }"#,
    ));
    assert_eq!(codes(&checked), ["E0102"]);
    let diagnostic = one(&checked, "E0102");
    assert!(
        diagnostic.message.contains("name"),
        "the message names the missing key: {}",
        diagnostic.message
    );
}

#[test]
fn a_name_is_the_only_key_a_config_needs() {
    let checked = project(&with_config(
        r#"{ "name": "Acme", "seo": { "canonicalOrigin": "https://acme.dev" } }"#,
    ));
    assert_eq!(codes(&checked), Vec::<&str>::new());
    let config = checked.config.expect("the defaults fill the rest in");
    assert_eq!(config.name, "Acme");
    assert!(config.public, "`public` defaults to true");
}

#[test]
fn a_private_site_is_e0120_until_the_server_ships() {
    let checked = project(&with_config(
        r#"{ "name": "Acme", "public": false,
             "seo": { "canonicalOrigin": "https://acme.dev" } }"#,
    ));
    assert_eq!(codes(&checked), ["E0120"]);
    let diagnostic = one(&checked, "E0120");
    assert!(
        diagnostic.message.contains("serve"),
        "the message points at `liyasa serve`: {}",
        diagnostic.message
    );
}
