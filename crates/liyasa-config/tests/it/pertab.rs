//! CFG-34: a subtree may be a path to the file holding it (RFC 0111).

use liyasa_config::vfs::MemVfs;
use liyasa_config::{Options, load};
use liyasa_core::source_map::SourceMap;
use serde_json::Value;

fn read(files: &[(&str, &str)]) -> (liyasa_config::load::Load, SourceMap) {
    let vfs: MemVfs = files
        .iter()
        .map(|(path, text)| (*path, text.as_bytes().to_vec()))
        .collect();
    let mut sources = SourceMap::new();
    let load = load(&vfs, &mut sources, &Options::default());
    (load, sources)
}

fn codes(load: &liyasa_config::load::Load) -> Vec<&str> {
    load.diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect()
}

const ROOT: &str = r#"{
  "name": "Acme",
  "seo": { "canonicalOrigin": "https://acme.dev" },
  "navigation": [
    { "tab": "Guides", "pages": "nav/guides.json" },
    { "tab": "API", "pages": ["api/intro"] }
  ]
}"#;

#[test]
fn a_tab_reads_its_pages_from_the_file_it_names() {
    let (load, _) = read(&[
        ("liyasa.json", ROOT),
        (
            "nav/guides.json",
            r#"["index", { "group": "Start", "pages": ["guides/install"] }]"#,
        ),
    ]);
    assert_eq!(codes(&load), Vec::<&str>::new());

    let pages = load
        .value
        .pointer("/navigation/0/pages")
        .expect("the tab still has pages");
    assert!(
        pages.is_array(),
        "the path was replaced by what the file held: {pages}"
    );
    assert_eq!(pages[0], Value::String("index".to_owned()));
    assert_eq!(
        pages.pointer("/1/group").and_then(Value::as_str),
        Some("Start")
    );
    // The tab written inline is untouched.
    assert_eq!(
        load.value.pointer("/navigation/1/pages/0"),
        Some(&Value::String("api/intro".to_owned()))
    );
}

#[test]
fn a_diagnostic_inside_the_file_points_into_that_file() {
    // `expanded` is not a string, which the schema refuses — and the span has
    // to land in `nav/guides.json`, not in `liyasa.json`.
    let (load, sources) = read(&[
        ("liyasa.json", ROOT),
        (
            "nav/guides.json",
            "[\n  { \"group\": \"Start\", \"expanded\": \"yes\", \"pages\": [\"guides/install\"] }\n]\n",
        ),
    ]);
    assert_eq!(codes(&load), ["E0102"]);
    let span = load
        .diagnostics
        .iter()
        .next()
        .and_then(|diagnostic| diagnostic.span)
        .expect("the diagnostic is located");
    let file = sources.get(span.source).path.to_string();
    assert!(file.ends_with("nav/guides.json"), "pointed at {file}");
}

#[test]
fn a_file_may_name_another_file() {
    let (load, _) = read(&[
        ("liyasa.json", ROOT),
        (
            "nav/guides.json",
            r#"[{ "group": "Start", "pages": "nav/start.json" }]"#,
        ),
        ("nav/start.json", r#"["guides/install", "guides/upgrade"]"#),
    ]);
    assert_eq!(codes(&load), Vec::<&str>::new());
    assert_eq!(
        load.value
            .pointer("/navigation/0/pages/0/pages/1")
            .and_then(Value::as_str),
        Some("guides/upgrade")
    );
}

#[test]
fn a_file_that_names_itself_is_e0136_and_keeps_the_path() {
    let (load, _) = read(&[
        ("liyasa.json", ROOT),
        (
            "nav/guides.json",
            r#"[{ "tab": "Loop", "pages": "nav/guides.json" }]"#,
        ),
    ]);
    let codes = codes(&load);
    assert!(codes.contains(&"E0136"), "{codes:?}");

    // The path is left as written rather than replaced by an empty array.
    assert_eq!(
        load.value
            .pointer("/navigation/0/pages/0/pages")
            .and_then(Value::as_str),
        Some("nav/guides.json")
    );
    // And the refusal is an error, which is the only thing between a cycle and
    // a tab that renders empty: since RFC 0111 the schema accepts a string
    // here, so `E0102` no longer objects to the leftover path. The first draft
    // of this rule leaned on exactly that, and the schema branch this same
    // change added is what took it away.
    assert!(
        load.diagnostics.has_errors(),
        "a cycle fails the load: {codes:?}"
    );
}

#[test]
fn deeper_than_four_files_is_e0136() {
    // Five splices: the tab's file, then four more. Four are allowed, so the
    // fifth is the one that is refused — the cap counts splices, and the test
    // that proves it has to cross it rather than reach it.
    let mut files: Vec<(String, String)> = vec![
        ("liyasa.json".to_owned(), ROOT.to_owned()),
        (
            "nav/guides.json".to_owned(),
            r#"[{ "group": "A", "pages": "nav/a.json" }]"#.to_owned(),
        ),
        (
            "nav/a.json".to_owned(),
            r#"[{ "group": "B", "pages": "nav/b.json" }]"#.to_owned(),
        ),
        (
            "nav/b.json".to_owned(),
            r#"[{ "group": "C", "pages": "nav/c.json" }]"#.to_owned(),
        ),
        (
            "nav/c.json".to_owned(),
            r#"[{ "group": "D", "pages": "nav/d.json" }]"#.to_owned(),
        ),
        ("nav/d.json".to_owned(), r#"["guides/install"]"#.to_owned()),
    ];
    let borrowed: Vec<(&str, &str)> = files
        .iter_mut()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let (load, _) = read(&borrowed);
    let codes = codes(&load);
    assert!(
        codes.contains(&"E0136"),
        "the fourth splice is past the cap: {codes:?}"
    );
}

#[test]
fn a_subtree_file_that_is_not_there_is_e0002() {
    let (load, _) = read(&[("liyasa.json", ROOT)]);
    assert!(codes(&load).contains(&"E0002"), "{:?}", codes(&load));
}
