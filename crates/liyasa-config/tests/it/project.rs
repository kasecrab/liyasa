//! `liyasa validate` end to end (CFG-90).

use liyasa_config::vfs::MemVfs;
use liyasa_config::{Options, check, validate::Mode};
use liyasa_core::source_map::SourceMap;

fn project(files: &[(&str, &str)]) -> liyasa_config::Checked {
    let vfs: MemVfs = files
        .iter()
        .map(|(p, t)| (*p, t.as_bytes().to_vec()))
        .collect();
    let mut sources = SourceMap::new();
    check(&vfs, &mut sources, &Options::default(), Mode::Build)
}

fn codes(checked: &liyasa_config::Checked) -> Vec<&str> {
    checked
        .diagnostics
        .iter()
        .map(|d| d.code.as_str())
        .collect()
}

const CONFIG: &str = r##"{
  "name": "Acme Docs",
  "seo": { "canonicalOrigin": "https://docs.acme.com" },
  "navigation": ["index", { "group": "Guides", "pages": ["guides/*"] }]
}"##;

#[test]
fn a_whole_project_validates() {
    let checked = project(&[
        ("liyasa.json", CONFIG),
        ("index.md", "# Home"),
        ("guides/one.md", "# One"),
        ("guides/two.md", "# Two"),
    ]);
    assert_eq!(codes(&checked), Vec::<&str>::new());
    assert_eq!(checked.pages.len(), 3);
    assert_eq!(checked.config.expect("a config").name, "Acme Docs");
}

#[test]
fn a_page_nobody_linked_is_reported() {
    let checked = project(&[
        ("liyasa.json", CONFIG),
        ("index.md", "# Home"),
        ("guides/one.md", "# One"),
        ("orphan.md", "# Orphan"),
    ]);
    assert_eq!(codes(&checked), vec!["W0130"]);
    assert!(
        checked
            .diagnostics
            .iter()
            .any(|d| d.message.contains("orphan"))
    );
}

#[test]
fn the_output_directory_is_not_content() {
    let checked = project(&[
        ("liyasa.json", CONFIG),
        ("index.md", "# Home"),
        ("guides/one.md", "# One"),
        ("dist/index.md", "built output"),
    ]);
    assert_eq!(codes(&checked), Vec::<&str>::new());
    assert!(!checked.pages.contains("dist"));
}

#[test]
fn the_content_root_key_moves_discovery() {
    let config = r##"{ "name": "Acme", "root": "docs",
      "seo": { "canonicalOrigin": "https://x.dev" },
      "navigation": ["index"] }"##;
    let checked = project(&[("liyasa.json", config), ("docs/index.md", "# Home")]);
    assert_eq!(codes(&checked), Vec::<&str>::new());
    assert!(checked.pages.contains("index"));
}

#[test]
fn a_broken_project_reports_everything_in_one_pass() {
    let config = r##"{ "name": "Acme", "nvaigation": [],
      "theme": { "colors": { "primary": "#818CF8" } } }"##;
    let checked = project(&[("liyasa.json", config), ("index.md", "# Home")]);
    let mut codes = codes(&checked);
    codes.sort_unstable();
    assert_eq!(codes, ["E0103", "E0107", "W0131"]);
    assert!(!checked.has_errors(), "none of the three is fatal");
}
