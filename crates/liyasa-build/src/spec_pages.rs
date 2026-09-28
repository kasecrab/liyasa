//! Pages that have no source file (API-10, API-11, RFC 0608).
//!
//! `liyasa_openapi::build::surface` renders one whole Markdown source document
//! per operation and per schema, front matter and component directives included.
//! They go through the same pipeline as a written page rather than being
//! serialized separately, so the HTML, the `<route>.md` twin, the search index
//! and `llms.txt` all come from one parse — which is the property a second
//! renderer would lose.
//!
//! The pipeline needs three things a file supplies and a spec does not: a
//! `VfsPath` to key the `SourceMap` by, a `Fingerprint` for the cache, and a
//! route. See RFC 0608 for why each is what it is; the short version is that
//! `SourceMap::intern` is keyed by `VfsPath` and lives in frozen `liyasa-core`,
//! so a synthetic path is not a preference.

use std::sync::Arc;

use liyasa_core::ids::{Fingerprint, Route};
use liyasa_core::source_map::SourceMap;
use liyasa_core::vfs::VfsPath;
use liyasa_openapi::build::SpecPage;

use crate::tree::{Indexing, Origin, Page, Tree};

/// Where a generated page is interned. Underscore-prefixed because
/// `is_routable` withholds any path with a `_` segment and `W0723` deliberately
/// does not warn about one, so an author cannot collide with it and the warning
/// needs no exception.
pub fn synthetic_path(route: &str) -> VfsPath {
    // The route's own segments, not a flattened form. Replacing `/` with `-`
    // is not injective — `/api/pets/list` and `/api/pets-list` collapse to one
    // path — and two pages sharing a `SourceMap` key is the exact failure
    // WP-08 warned about for `operationId`, reintroduced one layer down.
    VfsPath::new(format!("_openapi/{}.md", route.trim_matches('/')))
}

/// The cache key input for a generated page.
///
/// The source string and not the spec bytes: a page is built from the
/// *processed* document, so an overlay edit changes the page while the bytes are
/// identical, and hashing the bytes would serve a stale page — and rebuild every
/// sibling when one operation changes.
fn fingerprint_of(route: &str, source: &str) -> Fingerprint {
    Fingerprint::of_parts([route.as_bytes(), source.as_bytes()])
}

/// Adds every generated page to the tree, interning its source.
///
/// Returns the diagnostics scanning them produced, which are the author's own —
/// a spec whose description holds a malformed directive reports here.
pub fn inject(tree: &mut Tree, map: &mut SourceMap, pages: &[SpecPage]) {
    for page in pages {
        let path = synthetic_path(&page.route);
        let id = map.intern(path.clone(), Arc::from(page.source.as_str()));
        let (document, diagnostics) = liyasa_markdown::scan(&page.source, id);
        tree.diagnostics.extend(diagnostics.into_vec());

        let front = document
            .frontmatter
            .map(|front| front.typed)
            .unwrap_or_default();
        let hidden = front.hidden.unwrap_or(false);
        let indexing = Indexing::of(&front, false);
        // The route is the spec's, never the path's: `route_of` would answer
        // `/_openapi/...`, and the path exists only to key the source map.
        let route = Route::new(&page.route);
        tree.files.insert(path.clone());
        tree.origins.insert(
            route.clone(),
            Origin::Spec {
                spec: page.spec.clone(),
                operation: operation_of(&page.selector),
            },
        );
        tree.pages.push(Page {
            path,
            base_route: route.clone(),
            version: None,
            route,
            fingerprint: fingerprint_of(&page.route, &page.source),
            front,
            indexing,
            hidden,
            draft: false,
        });
    }
}

/// The operation id out of a selector such as `api GET /widgets/{id}`, when the
/// selector names one rather than a method and path.
fn operation_of(selector: &str) -> Option<String> {
    let mut parts = selector.split_whitespace();
    let _spec = parts.next()?;
    let second = parts.next()?;
    match parts.next().is_none() && !second.starts_with('/') {
        true => Some(second.to_owned()),
        false => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_generated_path_cannot_collide_with_an_author_s_page() {
        let path = synthetic_path("/api-reference/getwidget");
        assert_eq!(path.as_str(), "_openapi/api-reference/getwidget.md");
        // The property that makes the namespace safe, rather than the spelling:
        // `is_routable` withholds it, so no author file can route here.
        assert!(!liyasa_markdown::source::route::is_routable(
            &path,
            &liyasa_markdown::source::route::Ignore::default()
        ));
    }

    /// Flattening `/` to `-` looks harmless and is not injective. This is the
    /// test that caught it.
    #[test]
    fn two_routes_that_differ_only_by_depth_get_different_paths() {
        assert_ne!(
            synthetic_path("/api/pets/list").as_str(),
            synthetic_path("/api/pets-list").as_str()
        );
    }

    #[test]
    fn the_fingerprint_follows_the_source_rather_than_the_route() {
        let one = fingerprint_of("/api/get", "---\ntitle: A\n---\nbody\n");
        let two = fingerprint_of("/api/get", "---\ntitle: A\n---\nbody changed\n");
        assert_ne!(one, two, "an overlay edit changes the page");
        assert_eq!(
            one,
            fingerprint_of("/api/get", "---\ntitle: A\n---\nbody\n"),
            "the same page hashes the same twice"
        );
    }

    #[test]
    fn a_selector_naming_a_method_and_path_has_no_operation_id() {
        assert_eq!(operation_of("api getWidget"), Some("getWidget".to_owned()));
        assert_eq!(operation_of("api GET /widgets/{id}"), None);
        assert_eq!(operation_of("api"), None);
    }
}
