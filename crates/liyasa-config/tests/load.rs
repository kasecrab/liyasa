//! Reading `liyasa.json` through a `Vfs`: parse failures (`E0101`), env
//! overlays (CFG-93), and split navigation files (CFG-34).

use liyasa_config::load::{self, Options};
use liyasa_config::model::{Navigation, NavigationNode};
use liyasa_config::vfs::MemVfs;
use liyasa_core::source_map::SourceMap;
use liyasa_core::vfs::VfsPath;

fn read(files: &[(&str, &str)], env: Option<&str>) -> (load::Load, SourceMap) {
    let vfs: MemVfs = files
        .iter()
        .map(|(p, t)| (*p, t.as_bytes().to_vec()))
        .collect();
    let mut sources = SourceMap::new();
    let options = Options {
        root: VfsPath::new(""),
        env: env.map(str::to_owned),
    };
    let load = load::load(&vfs, &mut sources, &options);
    (load, sources)
}

fn codes(load: &load::Load) -> Vec<&str> {
    load.diagnostics.iter().map(|d| d.code.as_str()).collect()
}

#[test]
fn a_minimal_config_loads() {
    let (load, _) = read(&[("liyasa.json", r#"{ "name": "Acme" }"#)], None);
    assert_eq!(codes(&load), Vec::<&str>::new());
    assert_eq!(load.config.expect("a config").name, "Acme");
}

#[test]
fn a_missing_config_is_e0001() {
    let (load, _) = read(&[("readme.md", "hello")], None);
    assert_eq!(codes(&load), vec!["E0001"]);
    assert!(load.config.is_none());
}

#[test]
fn malformed_json_is_e0101_with_a_span() {
    let text = "{\n  \"name\": \"Acme\",\n}";
    let (load, sources) = read(&[("liyasa.json", text)], None);
    assert_eq!(codes(&load), vec!["E0101"]);
    let span = load
        .diagnostics
        .iter()
        .next()
        .and_then(|d| d.span)
        .expect("located");
    assert_eq!(
        sources.line_col(span).0.line,
        3,
        "the trailing comma is on line 3"
    );
}

#[test]
fn an_env_overlay_is_deep_merged() {
    let base = r#"{ "name": "Acme", "seo": { "indexing": "navigable", "trailingSlash": false },
                   "build": { "output": "dist" } }"#;
    let overlay = r#"{ "seo": { "trailingSlash": true }, "build": { "drafts": true } }"#;
    let (load, _) = read(
        &[("liyasa.json", base), ("liyasa.preview.json", overlay)],
        Some("preview"),
    );
    assert_eq!(codes(&load), Vec::<&str>::new());

    let config = load.config.expect("a config");
    let seo = config.seo.expect("seo survives the merge");
    assert!(seo.trailing_slash, "the overlay wins");
    assert_eq!(
        seo.indexing.expect("the base key survives"),
        liyasa_config::model::SeoIndexing::Navigable
    );
    let build = config.build.expect("build survives");
    assert_eq!(build.output, "dist");
    assert!(build.drafts, "the overlay adds a key the base did not set");
}

#[test]
fn an_overlay_replaces_an_array_rather_than_appending() {
    let base = r#"{ "name": "Acme", "navigation": ["a", "b"] }"#;
    let overlay = r#"{ "navigation": ["c"] }"#;
    let (load, _) = read(
        &[("liyasa.json", base), ("liyasa.staging.json", overlay)],
        Some("staging"),
    );
    let Navigation::List(nodes) = load
        .config
        .expect("a config")
        .navigation
        .expect("navigation")
    else {
        panic!("an array of nodes");
    };
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0], NavigationNode::Text("c".to_owned()));
}

#[test]
fn an_overlay_diagnostic_points_into_the_overlay_file() {
    let base = r#"{ "name": "Acme" }"#;
    let overlay = r#"{ "seo": { "trailingSlash": "yes" } }"#;
    let (load, sources) = read(
        &[("liyasa.json", base), ("liyasa.preview.json", overlay)],
        Some("preview"),
    );
    assert_eq!(codes(&load), vec!["E0102"]);
    let span = load
        .diagnostics
        .iter()
        .next()
        .and_then(|d| d.span)
        .expect("located");
    assert_eq!(
        sources.get(span.source).path,
        VfsPath::new("liyasa.preview.json")
    );
}

#[test]
fn a_missing_env_overlay_is_not_an_error() {
    let (load, _) = read(&[("liyasa.json", r#"{ "name": "Acme" }"#)], Some("preview"));
    assert_eq!(codes(&load), Vec::<&str>::new());
}

#[test]
fn navigation_may_live_in_its_own_file() {
    let (load, sources) = read(
        &[
            (
                "liyasa.json",
                r#"{ "name": "Acme", "navigation": "navigation.json" }"#,
            ),
            (
                "navigation.json",
                r#"[{ "tab": "Guides", "pages": ["index"] }]"#,
            ),
        ],
        None,
    );
    assert_eq!(codes(&load), Vec::<&str>::new());
    let Navigation::List(nodes) = load
        .config
        .expect("a config")
        .navigation
        .expect("navigation")
    else {
        panic!("the file's array replaced the path");
    };
    assert!(matches!(&nodes[0], NavigationNode::Tab { tab, .. } if tab == "Guides"));
    assert!(
        sources.find(&VfsPath::new("navigation.json")).is_some(),
        "the navigation file is interned so its spans resolve"
    );
}

#[test]
fn a_navigation_file_diagnostic_points_into_that_file() {
    let (load, sources) = read(
        &[
            (
                "liyasa.json",
                r#"{ "name": "Acme", "navigation": "navigation.json" }"#,
            ),
            ("navigation.json", r#"[{ "tab": "Guides", "nope": 1 }]"#),
        ],
        None,
    );
    assert_eq!(codes(&load), vec!["E0103"]);
    let span = load
        .diagnostics
        .iter()
        .next()
        .and_then(|d| d.span)
        .expect("located");
    assert_eq!(
        sources.get(span.source).path,
        VfsPath::new("navigation.json")
    );
}

#[test]
fn a_missing_navigation_file_is_e0002() {
    let (load, _) = read(
        &[(
            "liyasa.json",
            r#"{ "name": "Acme", "navigation": "navigation.json" }"#,
        )],
        None,
    );
    assert_eq!(codes(&load), vec!["E0002"]);
}

#[test]
fn an_unknown_key_is_dropped_so_the_config_still_builds() {
    let (load, _) = read(&[("liyasa.json", r#"{ "name": "Acme", "nope": 1 }"#)], None);
    assert_eq!(codes(&load), vec!["E0103"]);
    assert_eq!(
        load.config.expect("a config despite the warning").name,
        "Acme"
    );
}

#[test]
fn a_config_from_an_older_schema_is_told_to_migrate() {
    let text = r#"{ "$schema": "https://liyasa.dev/schema/v0/liyasa.json", "name": "Acme" }"#;
    let (load, _) = read(&[("liyasa.json", text)], None);
    assert_eq!(codes(&load), vec!["E0102"]);
    assert!(
        load.diagnostics
            .iter()
            .any(|d| d.help.as_deref() == Some("run `liyasa migrate-config` to upgrade it"))
    );
}
