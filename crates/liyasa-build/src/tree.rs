//! `content_tree`: what the project holds and where each page is served
//! (PRD §6.6 query 1, CM-80, CM-83).
//!
//! The walk is the one place that decides whether a file is a page, an asset,
//! or nothing at all, and the one place that reads `.liyasaignore` and
//! `.liyasa-aiignore`. Routing rules themselves belong to
//! `liyasa_markdown::source::route`; this module calls them.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use liyasa_core::diagnostics::Diagnostics;
use liyasa_core::frontmatter::{AiSetting, FrontmatterFields, SearchSetting};
use liyasa_core::ids::{Fingerprint, Route, Version};
use liyasa_core::source_map::SourceMap;
use liyasa_core::vfs::{Vfs, VfsKind, VfsPath};
use liyasa_markdown::source::route::{Ignore, PAGE_EXTENSIONS, is_routable, route_of};

/// `.liyasaignore` (CM-83): excluded from the build, search, AI indexing, and
/// `llms.txt`.
pub const IGNORE_FILE: &str = ".liyasaignore";
/// `.liyasa-aiignore` (CM-83): excluded from AI indexing only.
pub const AI_IGNORE_FILE: &str = ".liyasa-aiignore";

/// Directories the walk never descends into. `assets/` is not here: its files
/// are collected, just never as pages.
const NEVER_WALKED: &[&str] = &[".git", ".liyasa", "node_modules", "target"];

/// Where uploaded and referenced files live (CM-84, CM-131).
pub const ASSET_DIRECTORIES: &[&str] = &["assets", "public"];

/// Where includable fragments live (CM-70). They are interned into the build's
/// source map because that is how `liyasa_markdown::source::expand` resolves an
/// `{% include %}`: by looking the name up in the map.
pub const SNIPPET_DIR: &str = "snippets";

const MAX_DEPTH: u8 = 32;

#[derive(Debug, Clone)]
pub struct Options {
    /// `build.output`, which the walk must not read back in.
    pub output: String,
    /// `--drafts`: whether `draft: true` pages are part of this build.
    pub drafts: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            output: "dist".to_owned(),
            drafts: false,
        }
    }
}

/// Which shared surfaces a page appears in (CM-80).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Indexing {
    pub navigation: bool,
    pub sitemap: bool,
    pub search: bool,
    pub ai: bool,
}

impl Indexing {
    /// A `hidden: true` page is routable and in none of them; `search: true`,
    /// `ai: true`, and `noindex: false` each bring one back on its own.
    pub fn of(front: &FrontmatterFields, ai_ignored: bool) -> Self {
        let hidden = front.hidden.unwrap_or(false);
        let search = match &front.search {
            Some(SearchSetting::Enabled(on)) => *on,
            Some(SearchSetting::Options { exclude, .. }) => !exclude.unwrap_or(false),
            None => !hidden,
        };
        let ai = match &front.ai {
            Some(AiSetting::Enabled(on)) => *on,
            Some(AiSetting::Options { .. }) => true,
            None => !hidden,
        };
        let sitemap = match front.noindex {
            Some(noindex) => !noindex,
            None => !hidden,
        };
        Self {
            navigation: !hidden,
            sitemap,
            search,
            ai: ai && !ai_ignored,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Page {
    pub path: VfsPath,
    /// Where the page is served, version prefix and all.
    pub route: Route,
    /// The route without a version prefix, which is what the version switcher
    /// and the truth graph compare across versions (CM-91).
    pub base_route: Route,
    /// The version this page belongs to, once `versions::expand` has run.
    pub version: Option<Version>,
    pub fingerprint: Fingerprint,
    pub front: FrontmatterFields,
    pub indexing: Indexing,
    pub hidden: bool,
    pub draft: bool,
}

#[derive(Debug, Clone)]
pub struct Asset {
    pub path: VfsPath,
    pub fingerprint: Fingerprint,
}

/// Everything one walk found, in a stable order (§6.6.2 rule 5).
#[derive(Debug, Default)]
pub struct Tree {
    pub pages: Vec<Page>,
    pub assets: Vec<Asset>,
    /// Every file the walk saw that the ignore file did not remove, whether or
    /// not it is a page. A link to one of these is a download, not a broken
    /// route (CM-84).
    pub files: BTreeSet<VfsPath>,
    /// Files `.liyasaignore` removed, kept so `--profile` and the manifest can
    /// say why a page is missing.
    pub ignored: Vec<VfsPath>,
    /// Pages a build without `--drafts` left out.
    pub drafts: Vec<VfsPath>,
    /// Template name to fingerprint for every file under `snippets/`, each of
    /// them interned into the source map (CM-70).
    pub snippets: BTreeMap<String, Fingerprint>,
    pub diagnostics: Diagnostics,
}

impl Tree {
    pub fn page(&self, route: &Route) -> Option<&Page> {
        self.pages.iter().find(|page| &page.route == route)
    }

    pub fn routes(&self) -> Vec<Route> {
        self.pages.iter().map(|page| page.route.clone()).collect()
    }
}

/// Walks the project and reads every page's front matter.
///
/// `map` is filled with the page sources, so a diagnostic raised later in the
/// build can still point at a line.
pub fn discover(vfs: &dyn Vfs, map: &mut SourceMap, options: &Options) -> Tree {
    let ignore = read_ignore(vfs, IGNORE_FILE);
    let ai_ignore = read_ignore(vfs, AI_IGNORE_FILE);

    let mut files = Vec::new();
    walk(vfs, &VfsPath::new(""), options, &mut files, 0);
    files.sort();

    let mut tree = Tree::default();
    for path in files {
        if ignore.matches(&path) {
            tree.ignored.push(path);
            continue;
        }
        tree.files.insert(path.clone());
        if is_snippet(&path) {
            // An include names the file by its project path, so the map has to
            // hold it under exactly that name.
            if let Some((text, fingerprint)) = read_text(vfs, &path) {
                map.intern(path.clone(), text);
                tree.snippets.insert(path.as_str().to_owned(), fingerprint);
            }
            continue;
        }
        if is_asset(&path) {
            if let Ok(fingerprint) = vfs.fingerprint(&path) {
                tree.assets.push(Asset { path, fingerprint });
            }
            continue;
        }
        if !is_routable(&path, &Ignore::default()) {
            continue;
        }
        let Ok(bytes) = vfs.read(&path) else { continue };
        let Ok(text) = std::str::from_utf8(&bytes) else {
            continue;
        };
        let fingerprint = Fingerprint::of(&bytes);
        let id = map.intern(path.clone(), Arc::from(text));
        let (document, diagnostics) = liyasa_markdown::scan(text, id);
        tree.diagnostics.extend(diagnostics.into_vec());

        let front = document
            .frontmatter
            .map(|front| front.typed)
            .unwrap_or_default();
        let draft = front.draft.unwrap_or(false);
        if draft && !options.drafts {
            tree.drafts.push(path);
            continue;
        }
        let hidden = front.hidden.unwrap_or(false);
        let indexing = Indexing::of(&front, ai_ignore.matches(&path));
        let route = route_of(&path, Some(&front));
        tree.pages.push(Page {
            path,
            base_route: route.clone(),
            version: None,
            route,
            fingerprint,
            front,
            indexing,
            hidden,
            draft,
        });
    }
    tree
}

/// Whether a file is an includable fragment rather than content of its own
/// (CM-70, CM-74).
pub fn is_snippet(path: &VfsPath) -> bool {
    path.as_str()
        .split('/')
        .next()
        .is_some_and(|first| first == SNIPPET_DIR)
}

/// Whether a file is copied to `dist/` rather than rendered (CM-84).
pub fn is_asset(path: &VfsPath) -> bool {
    let mut segments = path.as_str().split('/');
    let first = segments.next().unwrap_or_default();
    if !ASSET_DIRECTORIES.contains(&first) {
        return false;
    }
    // A `.md` under `assets/` is documentation for the assets, not an asset.
    !path
        .extension()
        .is_some_and(|extension| PAGE_EXTENSIONS.contains(&extension))
}

/// A file's text and the fingerprint of its bytes, or `None` when it is not
/// UTF-8 — an image under `snippets/` is not a template.
fn read_text(vfs: &dyn Vfs, path: &VfsPath) -> Option<(Arc<str>, Fingerprint)> {
    let bytes = vfs.read(path).ok()?;
    let text = std::str::from_utf8(&bytes).ok()?;
    Some((Arc::from(text), Fingerprint::of(&bytes)))
}

fn read_ignore(vfs: &dyn Vfs, name: &str) -> Ignore {
    vfs.read(&VfsPath::new(name))
        .ok()
        .and_then(|bytes| String::from_utf8(bytes.to_vec()).ok())
        .map(|text| Ignore::parse(&text))
        .unwrap_or_default()
}

fn walk(vfs: &dyn Vfs, dir: &VfsPath, options: &Options, out: &mut Vec<VfsPath>, depth: u8) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = vfs.list(dir) else { return };
    for entry in entries {
        let Some(name) = entry.file_name() else {
            continue;
        };
        if NEVER_WALKED.contains(&name) || name == options.output {
            continue;
        }
        match vfs.metadata(&entry).map(|meta| meta.kind) {
            Ok(VfsKind::Dir) => walk(vfs, &entry, options, out, depth + 1),
            Ok(VfsKind::File) => out.push(entry),
            // A symlink is not followed: the `Vfs` already refuses one that
            // leaves the project, and following one inside it would double a
            // page's route.
            _ => {}
        }
    }
}

/// The set of routes, for the duplicate check and for navigation resolution.
pub fn routes(tree: &Tree) -> BTreeSet<String> {
    tree.pages
        .iter()
        .map(|page| page.route.as_str().to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use liyasa_config::vfs::MemVfs;

    use super::*;

    fn vfs(files: &[(&str, &str)]) -> MemVfs {
        files
            .iter()
            .map(|(path, text)| (*path, text.as_bytes().to_vec()))
            .collect()
    }

    fn discovered(files: &[(&str, &str)], options: &Options) -> Tree {
        let mut map = SourceMap::new();
        discover(&vfs(files), &mut map, options)
    }

    #[test]
    fn a_walk_finds_pages_and_assets_and_nothing_else() {
        let tree = discovered(
            &[
                ("index.md", "# home"),
                ("guides/install.md", "# install"),
                ("assets/manual.pdf", "%PDF"),
                ("snippets/note.md", "a shared note"),
                ("liyasa.json", "{}"),
                ("dist/index.html", "<!doctype html>"),
            ],
            &Options::default(),
        );
        assert_eq!(
            tree.routes().iter().map(Route::as_str).collect::<Vec<_>>(),
            ["/guides/install", "/"]
        );
        assert_eq!(
            tree.assets
                .iter()
                .map(|asset| asset.path.as_str())
                .collect::<Vec<_>>(),
            ["assets/manual.pdf"]
        );
    }

    #[test]
    fn cm_83_an_ignored_file_is_not_built() {
        let tree = discovered(
            &[
                (IGNORE_FILE, "drafts/\ninternal.md\n"),
                ("index.md", "# home"),
                ("drafts/next.md", "# next"),
                ("internal.md", "# internal"),
            ],
            &Options::default(),
        );
        assert_eq!(tree.pages.len(), 1);
        assert_eq!(
            tree.ignored.iter().map(VfsPath::as_str).collect::<Vec<_>>(),
            ["drafts/next.md", "internal.md"]
        );
    }

    #[test]
    fn cm_83_the_ai_ignore_file_only_takes_a_page_out_of_ai_indexing() {
        let tree = discovered(
            &[
                (AI_IGNORE_FILE, "internal/\n"),
                ("internal/runbook.md", "# runbook"),
            ],
            &Options::default(),
        );
        let page = tree.page(&Route::new("/internal/runbook")).expect("routed");
        assert!(!page.indexing.ai);
        assert!(page.indexing.search);
        assert!(page.indexing.sitemap);
    }

    #[test]
    fn cm_80_a_hidden_page_is_routable_and_in_nothing_else() {
        let tree = discovered(
            &[("secret.md", "---\nhidden: true\n---\n# secret")],
            &Options::default(),
        );
        let page = tree.page(&Route::new("/secret")).expect("routed");
        assert!(page.hidden);
        assert_eq!(
            page.indexing,
            Indexing {
                navigation: false,
                sitemap: false,
                search: false,
                ai: false,
            }
        );
    }

    #[test]
    fn cm_80_each_surface_can_be_turned_back_on_by_itself() {
        let tree = discovered(
            &[
                ("a.md", "---\nhidden: true\nsearch: true\n---\n#a"),
                ("b.md", "---\nhidden: true\nai: true\n---\n#b"),
                ("c.md", "---\nhidden: true\nnoindex: false\n---\n#c"),
            ],
            &Options::default(),
        );
        let of = |route: &str| tree.page(&Route::new(route)).expect("routed").indexing;
        assert!(of("/a").search && !of("/a").ai && !of("/a").sitemap);
        assert!(of("/b").ai && !of("/b").search && !of("/b").sitemap);
        assert!(of("/c").sitemap && !of("/c").search && !of("/c").ai);
        assert!(!of("/a").navigation && !of("/b").navigation && !of("/c").navigation);
    }

    #[test]
    fn a_draft_needs_the_flag() {
        let files = [("draft.md", "---\ndraft: true\n---\n# wip")];
        let without = discovered(&files, &Options::default());
        assert!(without.pages.is_empty());
        assert_eq!(without.drafts.len(), 1);

        let with = discovered(
            &files,
            &Options {
                drafts: true,
                ..Options::default()
            },
        );
        assert_eq!(with.pages.len(), 1);
    }

    #[test]
    fn a_slug_moves_the_route() {
        let tree = discovered(
            &[("guides/install.md", "---\nslug: setup\n---\n# install")],
            &Options::default(),
        );
        assert_eq!(
            tree.routes().first().map(Route::as_str),
            Some("/guides/setup")
        );
    }

    #[test]
    fn the_output_directory_is_never_read_back_in() {
        let tree = discovered(
            &[("out/index.md", "# built")],
            &Options {
                output: "out".to_owned(),
                ..Options::default()
            },
        );
        assert!(tree.pages.is_empty());
    }

    #[test]
    fn every_file_the_walk_kept_is_listed() {
        let tree = discovered(
            &[
                (IGNORE_FILE, "drafts/\n"),
                ("index.md", "# home"),
                ("guides/schema.json", "{}"),
                ("assets/manual.pdf", "%PDF"),
                ("drafts/next.md", "# next"),
            ],
            &Options::default(),
        );
        assert!(tree.files.contains(&VfsPath::new("guides/schema.json")));
        assert!(tree.files.contains(&VfsPath::new("assets/manual.pdf")));
        assert!(tree.files.contains(&VfsPath::new("index.md")));
        assert!(!tree.files.contains(&VfsPath::new("drafts/next.md")));
    }

    #[test]
    fn a_snippet_is_interned_rather_than_routed() {
        let files = vfs(&[
            ("index.md", "# home"),
            ("snippets/note.md", "Mind the gap."),
            ("snippets/nested/warning.md", "Careful."),
        ]);
        let mut map = SourceMap::new();
        let tree = discover(&files, &mut map, &Options::default());

        assert_eq!(tree.pages.len(), 1, "a snippet is not a page");
        assert_eq!(tree.snippets.len(), 2);
        assert_eq!(
            tree.snippets.get("snippets/note.md"),
            Some(&Fingerprint::of("Mind the gap."))
        );
        // Interned under the name an include uses, which is what makes
        // `expand`'s `map.find` succeed.
        assert!(map.find(&VfsPath::new("snippets/note.md")).is_some());
        assert!(
            map.find(&VfsPath::new("snippets/nested/warning.md"))
                .is_some()
        );
    }

    #[test]
    fn a_binary_file_under_snippets_is_not_a_template() {
        let vfs: MemVfs = [
            ("index.md", vec![b'#']),
            ("snippets/logo.png", vec![0x89, b'P', 0xff, 0xfe]),
        ]
        .into_iter()
        .collect();
        let mut map = SourceMap::new();
        let tree = discover(&vfs, &mut map, &Options::default());
        assert!(tree.snippets.is_empty());
    }

    #[test]
    fn a_page_carries_the_fingerprint_of_its_bytes() {
        let tree = discovered(&[("index.md", "# home")], &Options::default());
        let page = tree.page(&Route::new("/")).expect("routed");
        assert_eq!(page.fingerprint, Fingerprint::of("# home"));
    }
}
