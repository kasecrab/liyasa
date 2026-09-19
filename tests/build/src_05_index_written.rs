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
