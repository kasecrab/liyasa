//! AUTH-53 for the `except` spelling of a region gate (defect 157).
//!
//! `regions` is availability rather than access: AUTH-54 has a region-gated page
//! publicly indexable in its default variant, and `section::gates` already keeps
//! every gated *block* out of the index, so what a region gate scopes here is
//! the title, the route and the snippet.
//!
//! The defect is that one gate gives two answers depending on how the author
//! spelt it. `only` reaches the index; `except` did not, so a page withheld from
//! a region was offered to a reader in it.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use liyasa_build::engine::{self, Options};
use liyasa_build::git::NoGit;
use liyasa_config::vfs::OsVfs;
use liyasa_search::idx::Index;
use liyasa_search::idx::manifest::Context;
use liyasa_search::idx::query::{self, ReaderScope};
use liyasa_search::idx::search::SearchOptions;
use liyasa_search::idx::writer;

struct Project(PathBuf);

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A site with three regions declared and one page per gate spelling.
fn site(name: &str) -> Project {
    let root = std::env::temp_dir().join(format!("liyasa-auth-53-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("a project directory");
    fs::write(
        &root.join("liyasa.json"),
        r#"{
          "name": "Acme docs",
          "seo": { "canonicalOrigin": "https://docs.acme.com" },
          "regions": { "enabled": true, "list": ["us", "eu", "apac"] }
        }"#,
    )
    .expect("config");
    fs::write(
        root.join("index.md"),
        "---\ntitle: Home\n---\n# Home\n\nThe landing page.\n",
    )
    .expect("a home page");
    fs::write(
        root.join("except-eu.md"),
        "---\ntitle: Not in Europe\nregions:\n  except: [eu]\n---\n# Not in Europe\n\n\
         This page says exceptword and is withheld from one region.\n",
    )
    .expect("an except-gated page");
    fs::write(
        root.join("only-us.md"),
        "---\ntitle: Only America\nregions:\n  only: [us]\n---\n# Only America\n\n\
         This page says onlyword and names the one region it is for.\n",
    )
    .expect("an only-gated page");
    Project(root)
}

fn index_of(project: &Project) -> Index {
    let vfs = OsVfs::new(&project.0);
    let report = engine::build(
        &vfs,
        &NoGit,
        &project.0,
        &Options {
            build_time: Some(1_789_473_600),
            ..Options::default()
        },
    );
    assert!(!report.failed(false), "{:?}", report.diagnostics);

    let mut files = BTreeMap::new();
    for entry in fs::read_dir(project.0.join("dist").join(writer::DIRECTORY))
        .expect("the build writes the index directory")
        .flatten()
    {
        if let (Ok(bytes), Some(name)) = (
            fs::read(entry.path()),
            entry.file_name().to_str().map(str::to_owned),
        ) {
            files.insert(name, bytes);
        }
    }
    Index::open(files).expect("the index the build wrote opens")
}

fn hits(index: &Index, term: &str, region: &str) -> usize {
    let parsed = query::parse(term, "en").expect("valid query");
    let options = SearchOptions {
        reader: ReaderScope {
            groups: Vec::new(),
            region: Some(region.to_owned()),
        },
        ..SearchOptions::default()
    };
    index
        .search(&parsed, &Context::default(), &options)
        .expect("searches")
        .len()
}

#[test]
fn a_reader_in_an_excepted_region_does_not_match_the_page() {
    let project = site("except");
    let index = index_of(&project);

    // The control, and the reason the assertion below means anything: a reader
    // outside the exception finds the page, so a zero for `eu` is the gate
    // working rather than the page missing from the index altogether.
    assert!(
        hits(&index, "exceptword", "us") > 0,
        "a reader in a region the page does not except still finds it"
    );
    assert_eq!(
        hits(&index, "exceptword", "eu"),
        0,
        "`regions: {{ except: [eu] }}` reached the index as no regions at all, \
         so `ReaderScope::admits` opened on an empty list and offered the page \
         to the one region it withholds itself from"
    );
}

/// The spelling that already worked, kept so a fix to `except` cannot quietly
/// change `only`. This passes before the fix as well as after it.
#[test]
fn a_reader_outside_an_only_list_does_not_match_the_page() {
    let project = site("only");
    let index = index_of(&project);

    assert!(
        hits(&index, "onlyword", "us") > 0,
        "the region the page names finds it"
    );
    assert_eq!(
        hits(&index, "onlyword", "eu"),
        0,
        "a region outside `only` does not match"
    );
}
