//! What a build calls to produce the search index (SRC-01, SRC-05, SRC-11).
//!
//! The rest of the crate takes section documents and gives back an index. This
//! module is the step before that: it takes what a build has — a Rendered AST
//! per page, the page's facets, and the site's `search` settings — and gives
//! back the files to write under [`writer::DIRECTORY`].
//!
//! It exists because that step had no owner. `liyasa-build` never depended on
//! this crate, so `liyasa search` reported `E0016` on every site that had ever
//! been built. The decisions behind the shape of this function — the rendered
//! AST rather than the source, one artefact per build, an empty index rather
//! than none — are in `plan/rfcs/0705-who-builds-the-search-index.md`.
// TODO(rfc-0705): unused until WP-06 calls it from `engine::build`.

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::document::Document;

use crate::config::SearchSettings;
use crate::doc::{PageMeta, SectionDocument};
use crate::idx::writer::{self, BuiltIndex};
use crate::section;

/// One page offered to the index.
///
/// `document` is the **anonymous** render (§6.6.4), the same one the HTML was
/// serialized from. SRC-12 turns on this: a personalized render would put
/// `{{reader.` values into snippets that every reader can search.
#[derive(Debug)]
pub struct IndexPage<'a> {
    pub document: &'a Document,
    pub meta: PageMeta,
    /// `tree::Indexing::search` — whether the build decided this page belongs
    /// in the index at all. `hidden`, `noindex` and `draft` have already been
    /// folded into it by the time a build calls here.
    pub indexed: bool,
}

/// The index for one site, and what the build should report about it.
#[derive(Debug)]
pub struct SiteIndex {
    pub index: BuiltIndex,
    /// Section documents across every indexed page.
    pub sections: usize,
    pub pages_indexed: usize,
    /// Pages the build offered and the index declined: `indexing.search` off,
    /// or a `search.exclude` glob.
    pub pages_skipped: usize,
    /// `W1005` for a `search.boost` or `search.exclude` pattern that matched no
    /// route. Only a whole-site pass can know this, which is why it is raised
    /// here rather than in `config`.
    pub diagnostics: Vec<Diagnostic>,
}

impl SiteIndex {
    /// An index with no documents in it. Still an index, and still written:
    /// the CLI's `E0016` means *no index*, and a site with nothing indexable
    /// is not that.
    pub fn is_empty(&self) -> bool {
        self.sections == 0
    }

    /// Every file to write, at its path relative to the output directory, so a
    /// caller loops once and writes.
    pub fn output_files(&self) -> impl Iterator<Item = (String, &[u8])> {
        self.index
            .files
            .iter()
            .map(|(name, bytes)| (format!("{}/{name}", writer::DIRECTORY), bytes.as_slice()))
    }
}

/// Builds the site's index from every page the build offers.
pub fn index_site(pages: &[IndexPage<'_>], settings: &SearchSettings) -> SiteIndex {
    let mut documents: Vec<SectionDocument> = Vec::new();
    let mut routes: Vec<String> = Vec::with_capacity(pages.len());
    let mut pages_indexed = 0;
    let mut pages_skipped = 0;

    for page in pages {
        let route = page.meta.route.as_str().to_owned();
        // Every route the site has, matched or not: a glob that matches a page
        // the build kept out of search still matched something, and warning
        // about it would send an author looking for a typo that is not there.
        routes.push(route.clone());

        if !page.indexed || settings.excludes(&route) {
            pages_skipped += 1;
            continue;
        }

        let mut meta = page.meta.clone();
        meta.boost *= settings.boost_for(&route, &meta.groups);
        documents.extend(section::extract(page.document, &meta));
        pages_indexed += 1;
    }

    let diagnostics = settings
        .unmatched(&routes)
        .into_iter()
        .map(|pattern| {
            Diagnostic::new(
                code::W1005,
                format!("`{pattern}` in `search.boost` or `search.exclude` matched no route"),
            )
            .help("Check the pattern against the routes this site builds; a leading `/` and a trailing `**` are both significant.")
        })
        .collect();

    SiteIndex {
        sections: documents.len(),
        index: writer::build(&documents, &settings.writer_options()),
        pages_indexed,
        pages_skipped,
        diagnostics,
    }
}

/// The `search` object of `liyasa.json`, or the defaults when it is absent.
///
/// Lenient on purpose: `schemas/liyasa.schema.json` is what rejects a malformed
/// `search` object (CFG-94), and it runs before a build reaches here, so a
/// second rejection at this depth would report the same fault twice with less
/// context than the first.
pub fn settings_from_config(config: &serde_json::Value) -> SearchSettings {
    config
        .get("search")
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_config_whose_search_object_is_the_wrong_shape_falls_back() {
        let config = serde_json::json!({ "search": { "maxResults": "eight" } });
        assert_eq!(settings_from_config(&config), SearchSettings::default());
    }

    #[test]
    fn the_output_paths_are_relative_to_the_output_directory() {
        let built = index_site(&[], &SearchSettings::default());
        for (path, _) in built.output_files() {
            assert!(!path.starts_with('/'), "{path} must be relative");
            assert!(path.starts_with("search-index/"), "{path}");
        }
    }
}
