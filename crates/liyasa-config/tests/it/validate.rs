//! The semantic rules `liyasa validate` runs after the schema (CFG-90).

use liyasa_config::json::SpanIndex;
use liyasa_config::pages::Pages;
use liyasa_config::validate::{self, Context, Mode};
use liyasa_core::span::SourceId;

fn check(text: &str, pages: &[&str], mode: Mode) -> Vec<String> {
    let value: serde_json::Value = serde_json::from_str(text).expect("the fixture is valid JSON");
    let spans = SpanIndex::scan(SourceId(0), text);
    let pages: Pages = pages.iter().collect();
    let context = Context {
        pages: &pages,
        mode,
    };
    validate::validate(&value, &spans, &context)
        .iter()
        .map(|d| d.code.as_str().to_owned())
        .collect()
}

/// The diagnostics' messages rather than their codes, for a rule whose point
/// is what it says.
fn messages(text: &str) -> Vec<String> {
    let value: serde_json::Value = serde_json::from_str(text).expect("the fixture is valid JSON");
    let spans = SpanIndex::scan(SourceId(0), text);
    let pages = Pages::new();
    validate::validate(
        &value,
        &spans,
        &Context {
            pages: &pages,
            mode: Mode::Build,
        },
    )
    .iter()
    .map(|diagnostic| diagnostic.message.clone())
    .collect()
}

fn build(text: &str, pages: &[&str]) -> Vec<String> {
    check(text, pages, Mode::Build)
}

const PAGES: &[&str] = &["index", "guides/one", "guides/two", "api/intro"];

#[test]
fn a_navigation_that_names_every_page_is_clean() {
    let config = r##"{
      "name": "Acme",
      "seo": { "canonicalOrigin": "https://docs.acme.com" },
      "navigation": ["index", { "group": "Guides", "pages": ["guides/one", "guides/two"] },
                     "api/intro"]
    }"##;
    assert_eq!(build(config, PAGES), Vec::<String>::new());
}

#[test]
fn a_page_that_does_not_exist_is_e0104() {
    let config = r##"{ "name": "Acme", "navigation": ["index", "guides/missing"],
                      "seo": { "canonicalOrigin": "https://x.dev" } }"##;
    // Sorted by span: the three unreachable pages are reported at `navigation`,
    // which starts before the element that names the missing page.
    assert_eq!(build(config, PAGES), ["W0130", "W0130", "W0130", "E0104"]);
}

#[test]
fn a_page_outside_the_navigation_is_w0130() {
    let config = r##"{ "name": "Acme", "navigation": ["index"],
                      "seo": { "canonicalOrigin": "https://x.dev" } }"##;
    assert_eq!(build(config, PAGES), ["W0130", "W0130", "W0130"]);
}

#[test]
fn autofill_replaces_the_warning_during_dev() {
    let config = r##"{ "name": "Acme",
                      "navigation": { "autofill": true, "pages": ["index"] },
                      "seo": { "canonicalOrigin": "https://x.dev" } }"##;
    assert_eq!(check(config, PAGES, Mode::Dev), Vec::<String>::new());
    assert_eq!(
        check(config, PAGES, Mode::Build),
        ["W0130", "W0130", "W0130"]
    );
}

#[test]
fn a_glob_expands_and_an_empty_one_is_e0104() {
    let config = r##"{ "name": "Acme", "seo": { "canonicalOrigin": "https://x.dev" },
      "navigation": ["index", "api/intro", { "group": "Guides", "pages": ["guides/*"] }] }"##;
    assert_eq!(build(config, PAGES), Vec::<String>::new());

    let empty = r##"{ "name": "Acme", "seo": { "canonicalOrigin": "https://x.dev" },
      "navigation": [{ "group": "Nope", "pages": ["nothing/*"] }] }"##;
    assert!(build(empty, PAGES).contains(&"E0104".to_owned()));
}

#[test]
fn a_directory_node_needs_pages_under_it() {
    let good = r##"{ "name": "Acme", "seo": { "canonicalOrigin": "https://x.dev" },
      "navigation": ["index", "api/intro", { "directory": "guides" }] }"##;
    assert_eq!(build(good, PAGES), Vec::<String>::new());

    let bad = r##"{ "name": "Acme", "seo": { "canonicalOrigin": "https://x.dev" },
      "navigation": [{ "directory": "absent" }] }"##;
    assert!(build(bad, PAGES).contains(&"E0104".to_owned()));
}

#[test]
fn the_same_page_twice_is_e0105() {
    let config = r##"{ "name": "Acme", "seo": { "canonicalOrigin": "https://x.dev" },
      "navigation": ["index", "guides/one", "guides/one", "guides/two", "api/intro"] }"##;
    assert_eq!(build(config, PAGES), ["E0105"]);
}

#[test]
fn a_subtree_bound_to_an_undeclared_version_is_e0133() {
    let config = r##"{ "name": "Acme", "seo": { "canonicalOrigin": "https://x.dev" },
      "versions": [{ "name": "v1", "default": true }],
      "navigation": [{ "version": "v9", "pages": ["index"] }] }"##;
    assert!(build(config, PAGES).contains(&"E0133".to_owned()));
}

#[test]
fn an_openapi_node_must_name_a_declared_spec() {
    let config = r##"{ "name": "Acme", "seo": { "canonicalOrigin": "https://x.dev" },
      "openapi": [{ "id": "api", "source": "openapi/api.yaml" }],
      "navigation": [{ "openapi": "api" }, { "openapi": "ghost" }] }"##;
    let codes = build(config, PAGES);
    assert_eq!(codes.iter().filter(|c| *c == "E0133").count(), 1);
}

#[test]
fn versions_and_locales_need_exactly_one_default() {
    let none = r##"{ "name": "Acme", "seo": { "canonicalOrigin": "https://x.dev" },
      "versions": [{ "name": "v1" }, { "name": "v2" }] }"##;
    assert!(build(none, &[]).contains(&"E0108".to_owned()));

    let two = r##"{ "name": "Acme", "seo": { "canonicalOrigin": "https://x.dev" },
      "locales": [{ "code": "en", "default": true }, { "code": "de", "default": true }] }"##;
    assert!(build(two, &[]).contains(&"E0108".to_owned()));

    let plain = r##"{ "name": "Acme", "seo": { "canonicalOrigin": "https://x.dev" },
      "versions": ["v2", "v1"] }"##;
    assert_eq!(build(plain, &[]), Vec::<String>::new());
}

#[test]
fn a_repeated_version_name_is_e0105() {
    let config = r##"{ "name": "Acme", "seo": { "canonicalOrigin": "https://x.dev" },
      "versions": [{ "name": "v1", "default": true }, { "name": "v1" }] }"##;
    assert!(build(config, &[]).contains(&"E0105".to_owned()));
}

#[test]
fn conflicting_redirects_are_e0106() {
    let duplicate = r##"{ "name": "Acme", "seo": { "canonicalOrigin": "https://x.dev" },
      "redirects": { "rules": [{ "source": "/old", "destination": "/index" },
                               { "source": "/old", "destination": "/guides/one" }] } }"##;
    assert!(build(duplicate, PAGES).contains(&"E0106".to_owned()));

    let shadows = r##"{ "name": "Acme", "seo": { "canonicalOrigin": "https://x.dev" },
      "redirects": [{ "source": "/guides/one", "destination": "/guides/two" }] }"##;
    assert!(build(shadows, PAGES).contains(&"E0106".to_owned()));
}

#[test]
fn an_absolute_redirect_needs_an_allowed_host() {
    let blocked = r##"{ "name": "Acme", "seo": { "canonicalOrigin": "https://x.dev" },
      "redirects": { "rules": [{ "source": "/out", "destination": "https://evil.test/x" }] } }"##;
    assert!(build(blocked, PAGES).contains(&"E0109".to_owned()));

    let allowed = r##"{ "name": "Acme", "seo": { "canonicalOrigin": "https://x.dev" },
      "redirects": { "rules": [{ "source": "/out", "destination": "https://acme.com/x" }],
                     "externalAllow": ["acme.com"] } }"##;
    assert!(!build(allowed, PAGES).contains(&"E0109".to_owned()));

    let parameterized = r##"{ "name": "Acme", "seo": { "canonicalOrigin": "https://x.dev" },
      "redirects": { "rules": [{ "source": "/out/:host", "destination": "https://:host/x" }],
                     "externalAllow": ["acme.com"] } }"##;
    assert!(build(parameterized, PAGES).contains(&"E0109".to_owned()));
}

#[test]
fn a_low_contrast_primary_is_a_warning_naming_the_ratio() {
    let config = r##"{ "name": "Acme", "seo": { "canonicalOrigin": "https://x.dev" },
      "theme": { "colors": { "primary": "#818CF8" } } }"##;
    let value: serde_json::Value = serde_json::from_str(config).expect("valid JSON");
    let spans = SpanIndex::scan(SourceId(0), config);
    let pages = Pages::new();
    let diagnostics = validate::validate(
        &value,
        &spans,
        &Context {
            pages: &pages,
            mode: Mode::Build,
        },
    );
    let contrast = diagnostics
        .iter()
        .find(|d| d.code.as_str() == "E0107")
        .expect("the pale primary is reported");
    assert_eq!(
        contrast.severity,
        liyasa_core::diagnostics::Severity::Warning,
        "RFC 0102"
    );
    assert!(contrast.message.contains("2.98"), "{}", contrast.message);
    assert!(contrast.message.contains("4.5"), "{}", contrast.message);
}

#[test]
fn a_colour_liyasa_cannot_read_is_e0132() {
    let config = r##"{ "name": "Acme", "seo": { "canonicalOrigin": "https://x.dev" },
      "theme": { "colors": { "primary": "#ggg" } } }"##;
    assert!(build(config, &[]).contains(&"E0132".to_owned()));

    let custom = r##"{ "name": "Acme", "seo": { "canonicalOrigin": "https://x.dev" },
      "theme": { "colors": { "primary": "var(--brand)" } } }"##;
    assert_eq!(build(custom, &[]), Vec::<String>::new());
}

#[test]
fn a_private_site_is_e0120_until_the_server_ships() {
    let config = r##"{ "name": "Acme", "public": false,
                      "seo": { "canonicalOrigin": "https://x.dev" } }"##;
    assert!(build(config, &[]).contains(&"E0120".to_owned()));
}

#[test]
fn a_missing_canonical_origin_is_w0131() {
    assert_eq!(build(r##"{ "name": "Acme" }"##, &[]), ["W0131"]);
}

#[test]
fn an_origin_that_already_carries_the_base_path_is_w0136() {
    let doubled = r##"{ "name": "Acme", "seo": { "canonicalOrigin": "https://acme.dev/docs" },
      "build": { "basePath": "/docs" } }"##;
    assert_eq!(build(doubled, &[]), ["W0136"]);

    let once = r##"{ "name": "Acme", "seo": { "canonicalOrigin": "https://acme.dev" },
      "build": { "basePath": "/docs" } }"##;
    assert_eq!(build(once, &[]), Vec::<String>::new());

    // An origin with a path of its own and no `basePath` is how the reference
    // site is published; it is not this warning.
    let project_page =
        r##"{ "name": "Acme", "seo": { "canonicalOrigin": "https://acme.github.io/docs" } }"##;
    assert_eq!(build(project_page, &[]), Vec::<String>::new());
}

const ORIGIN: &str = r#""seo": { "canonicalOrigin": "https://acme.dev" }"#;

fn auth_config(auth: &str, extra: &str) -> String {
    format!(r#"{{ "name": "Acme", {ORIGIN}, "auth": {auth}{extra} }}"#)
}

#[test]
fn an_operator_under_a_mode_that_names_nobody_is_w0137() {
    let password = auth_config(
        r#"{ "mode": "password", "operators": [{ "subject": "ana", "role": "owner" }] }"#,
        "",
    );
    assert_eq!(build(&password, &[]), ["W0139", "W0137"]);

    let public = auth_config(
        r#"{ "mode": "public", "operators": [{ "subject": "ana", "role": "owner" }] }"#,
        "",
    );
    assert_eq!(build(&public, &[]), ["W0139", "W0137"]);

    // The mode is the default when nothing says otherwise, and the default is
    // public, so an operator with no mode is the same mistake.
    let no_mode = auth_config(
        r#"{ "operators": [{ "subject": "ana", "role": "owner" }] }"#,
        "",
    );
    assert_eq!(build(&no_mode, &[]), ["W0139", "W0137"]);
}

#[test]
fn the_shared_password_subject_is_w0137_under_any_mode() {
    let config = auth_config(
        r#"{ "mode": "oidc", "operators": [{ "subject": "password:production", "role": "owner" }] }"#,
        "",
    );
    let codes = build(&config, &[]);
    assert_eq!(codes, ["W0139", "W0137"]);
}

#[test]
fn an_address_under_magic_link_sign_in_is_w0137() {
    let config = auth_config(
        r#"{ "mode": "managed", "operators": [{ "subject": "ana@acme.dev", "role": "owner" }] }"#,
        r#", "mail": { "from": "docs@acme.dev", "smtp": { "host": "smtp.acme.dev" } }"#,
    );
    assert_eq!(build(&config, &[]), ["W0139", "W0137"]);
}

#[test]
fn a_subject_a_provider_really_issues_is_accepted() {
    let config = auth_config(
        r#"{ "mode": "oidc", "operators": [
             { "subject": "8f14e45fce", "role": "owner" },
             { "subject": "https://idp.acme.dev/users/42", "role": "admin" }] }"#,
        "",
    );
    assert_eq!(
        build(&config, &[]),
        ["W0139"],
        "a well-formed operator list is still a standing grant, and says so"
    );
}

/// The warning is about the grant outliving the membership row, so it names
/// the subjects rather than counting them: somebody reading a build log has to
/// be able to tell whether the person who just left is one of them.
#[test]
fn the_standing_grant_names_who_holds_it() {
    let config = auth_config(
        r#"{ "mode": "oidc", "operators": [
             { "subject": "8f14e45fce", "role": "owner" },
             { "subject": "c9a1b2", "role": "admin" }] }"#,
        "",
    );
    let text = messages(&config);
    let warning = text
        .iter()
        .find(|message| message.contains("no membership change"))
        .expect("the standing grant is reported");
    assert!(warning.contains("2 subjects"), "{warning}");
    assert!(
        warning.contains("8f14e45fce") && warning.contains("c9a1b2"),
        "{warning}"
    );

    let none = auth_config(r#"{ "mode": "oidc" }"#, "");
    assert_eq!(
        build(&none, &[]),
        Vec::<String>::new(),
        "no entries, nothing to say"
    );
}

#[test]
fn magic_link_sign_in_with_nowhere_to_send_is_w0138() {
    let without = auth_config(r#"{ "mode": "managed" }"#, "");
    assert_eq!(build(&without, &[]), ["W0138"]);

    let with = auth_config(
        r#"{ "mode": "managed" }"#,
        r#", "mail": { "from": "docs@acme.dev", "smtp": { "host": "smtp.acme.dev", "port": 587 } }"#,
    );
    assert_eq!(build(&with, &[]), Vec::<String>::new());
}
