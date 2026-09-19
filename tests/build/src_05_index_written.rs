//! SRC-05: a built site carries the search index the reader queries.
//!
//! This is the test whose absence let `liyasa search` report `E0016` on every
//! site ever built. Every other search test constructs its own index and then
//! searches it, so all of them passed while no build wrote one — the tell is a
//! test that calls a constructor the binary never calls. This one asks the
//! built site instead, and calls nothing.

use liyasa_search::idx::writer;

use liyasa_tests::docs::Docs;

/// The directory `liyasa search` reads, relative to `dist/`.
fn index_path(name: &str) -> String {
    format!("{}/{name}", writer::DIRECTORY)
}

#[test]
fn a_built_site_has_a_search_index() {
    let docs = Docs::build("src_05_index");
    assert!(
        docs.exists(&index_path(writer::MANIFEST)),
        "a build writes `dist/{}`; without it `liyasa search` reports E0016 on \
         every query",
        index_path(writer::MANIFEST)
    );
}

#[test]
fn the_index_a_build_writes_answers_a_query_about_the_site() {
    use std::collections::BTreeMap;

    use liyasa_search::idx::Index;
    use liyasa_search::idx::manifest::Context;
    use liyasa_search::idx::query;
    use liyasa_search::idx::search::SearchOptions;

    let docs = Docs::build("src_05_index_query");
    let directory = docs.dist().join(writer::DIRECTORY);

    let mut files = BTreeMap::new();
    for entry in std::fs::read_dir(&directory)
        .expect("the build writes the index directory")
        .flatten()
    {
        if let (Ok(bytes), Some(name)) = (
            std::fs::read(entry.path()),
            entry.file_name().to_str().map(str::to_owned),
        ) {
            files.insert(name, bytes);
        }
    }

    let index = Index::open(files).expect("the index the build wrote opens");
    let parsed = query::parse("search", "en").expect("valid query");
    let hits = index
        .search(&parsed, &Context::default(), &SearchOptions::default())
        .expect("searches");
    assert!(
        !hits.is_empty(),
        "the documentation site says the word `search` somewhere"
    );
}

// ---- defect 72: neither gate may reach the site-wide index ----

/// Neither kind of gated text may reach `search-index/`, which is one artefact
/// for every reader.
///
/// The two words are gated by different mechanisms, and what each assertion is
/// worth was measured by deleting the thing it is meant to catch rather than
/// inferred from its passing:
///
/// - `componentonly` sits in a `:::visibility` block, which stays in the AST
///   for every variant because the component gate runs at render time. Only
///   `liyasa_search::section`'s own check keeps it out. Disabling that check
///   (`if gates(props)` in section.rs) makes this query return 1 hit, so this
///   assertion is live.
/// - `templateonly` sits behind `{% if %}`, resolved before the page is parsed.
///   It is absent because the AST handed to the indexer came from an anonymous
///   expansion where `reader.groups` is empty and the branch is not taken. It
///   pins that property. It does **not** exercise `engine::build`'s
///   `variant == Variant::default()` guard — see the test below for why that
///   guard cannot be observed today.
#[test]
fn neither_gate_lets_its_text_into_the_search_index() {
    use std::collections::BTreeMap;
    use std::fs;

    use liyasa_search::idx::Index;
    use liyasa_search::idx::manifest::Context;
    use liyasa_search::idx::query;
    use liyasa_search::idx::search::SearchOptions;

    use liyasa_build::engine::{self, Options};
    use liyasa_build::git::NoGit;
    use liyasa_config::vfs::OsVfs;

    let root = std::env::temp_dir().join(format!("liyasa-defect-72-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("a project directory");
    fs::write(
        root.join("liyasa.json"),
        r#"{"name":"Acme docs","seo":{"canonicalOrigin":"https://docs.acme.com"}}"#,
    )
    .expect("config");
    fs::write(
        root.join("index.md"),
        "---\ntitle: Home\npersonalized: true\n---\n# Home\n\n\
         Everyone may read the word publicword on this page.\n\n\
         {% if \"admin\" in reader.groups %}\n\
         The console lives behind templateonly for staff.\n\
         {% endif %}\n\n\
         :::visibility{groups=[\"admin\"]}\n\
         The console also mentions componentonly for staff.\n\
         :::\n",
    )
    .expect("a page");
    fs::write(
        root.join("plain.md"),
        "---\ntitle: Plain\n---\n# Plain\n\nAn ordinary page saying plainword.\n",
    )
    .expect("a second page");

    let vfs = OsVfs::new(&root);
    let report = engine::build(
        &vfs,
        &NoGit,
        &root,
        &Options {
            build_time: Some(1_789_473_600),
            ..Options::default()
        },
    );
    assert!(!report.failed(false), "{:?}", report.diagnostics);

    let mut files = BTreeMap::new();
    for entry in fs::read_dir(root.join("dist").join(writer::DIRECTORY))
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
    let index = Index::open(files).expect("the index the build wrote opens");

    let hits_for = |term: &str| {
        let parsed = query::parse(term, "en").expect("valid query");
        index
            .search(&parsed, &Context::default(), &SearchOptions::default())
            .expect("searches")
            .len()
    };

    // The control, and the reason the two assertions below mean anything: the
    // same query path over the same index finds an ungated word on this page.
    // Without it, "no hits" would also be what a broken query, an empty index
    // or a tokenizer that split the word differently looks like.
    assert!(
        hits_for("plainword") > 0,
        "an ordinary page is indexed at all; if this fails the index is empty \
         and nothing below means anything"
    );
    assert!(
        hits_for("publicword") > 0,
        "the ungated sentence is indexed, so this query path can find text on \
         this page at all"
    );
    assert_eq!(
        hits_for("templateonly"),
        0,
        "a `{{% if %}}` branch only the admin variant takes reached the \
         site-wide index: `engine::build` is indexing a populated variant's \
         render rather than the default one"
    );
    assert_eq!(
        hits_for("componentonly"),
        0,
        "a `:::visibility` block's text reached the site-wide index: the \
         section walk in `liyasa_search` is descending into a gated component"
    );

    let _ = fs::remove_dir_all(&root);
}

/// One group may not search out what a template showed another.
///
/// A page whose front matter names groups is withheld from a reader in none of
/// them by `ReaderScope::admits`, so nothing on it leaks to the public. The
/// leak this pins is between two groups that can both read the page: if the
/// build indexed the admin variant's render, a partner would find text
/// `{% if %}` showed only to admins. `index_site` receives one AST per page and
/// cannot know which variant produced it, so only `engine::build` can prevent
/// it.
///
/// **This cannot fail today, and that is worth knowing rather than hiding.**
/// Deleting `engine::build`'s `variant == Variant::default()` guard leaves it
/// green, because a page that reads `reader.groups` takes the free-form branch
/// of `variants::of_page` — the one that raises `E0208` — which returns
/// `Mode::Dynamic` with the coordinate variants only. No group variant is ever
/// built, so there is no populated render for the capture to take. The guard is
/// defence against a future `of_page` that does emit one, and this test is what
/// would notice.
#[test]
fn one_group_cannot_search_out_what_a_template_showed_another() {
    use std::collections::BTreeMap;
    use std::fs;

    use liyasa_search::idx::Index;
    use liyasa_search::idx::manifest::Context;
    use liyasa_search::idx::query::{self, ReaderScope};
    use liyasa_search::idx::search::SearchOptions;

    use liyasa_build::engine::{self, Options};
    use liyasa_build::git::NoGit;
    use liyasa_config::vfs::OsVfs;

    let root = std::env::temp_dir().join(format!("liyasa-defect-72b-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("a project directory");
    fs::write(
        root.join("liyasa.json"),
        r#"{"name":"Acme docs","seo":{"canonicalOrigin":"https://docs.acme.com"}}"#,
    )
    .expect("config");
    fs::write(
        root.join("index.md"),
        "---\ntitle: Home\n---\n# Home\n\nThe public landing page.\n",
    )
    .expect("a home page");
    fs::write(
        root.join("internal.md"),
        "---\ntitle: Internal\ngroups: [admin, partner]\npersonalized: true\n---\n# Internal\n\n\
         Both groups may read the word sharedword here.\n\n\
         {% if \"admin\" in reader.groups %}\n\
         Only admins may read the word adminonlyword.\n\
         {% endif %}\n",
    )
    .expect("a two-group page");

    let vfs = OsVfs::new(&root);
    let report = engine::build(
        &vfs,
        &NoGit,
        &root,
        &Options {
            build_time: Some(1_789_473_600),
            ..Options::default()
        },
    );
    assert!(!report.failed(false), "{:?}", report.diagnostics);

    let mut files = BTreeMap::new();
    for entry in fs::read_dir(root.join("dist").join(writer::DIRECTORY))
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
    let index = Index::open(files).expect("the index the build wrote opens");

    let as_partner = SearchOptions {
        reader: ReaderScope {
            groups: vec!["partner".to_owned()],
            region: None,
        },
        ..SearchOptions::default()
    };
    let hits_for = |term: &str| {
        let parsed = query::parse(term, "en").expect("valid query");
        index
            .search(&parsed, &Context::default(), &as_partner)
            .expect("searches")
            .len()
    };

    // A partner may read this page, so the shared sentence is theirs to find.
    // Without this the assertion below would pass for a reader who cannot see
    // the page at all, which is a different rule working.
    assert!(
        hits_for("sharedword") > 0,
        "a partner can search the page they are allowed to read"
    );
    assert_eq!(
        hits_for("adminonlyword"),
        0,
        "a partner searched out text `{{% if %}}` showed only to admins: the \
         build indexed a populated variant's render instead of the default one"
    );

    let _ = fs::remove_dir_all(&root);
}
