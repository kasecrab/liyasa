//! Routes and routability (CM-02, CM-03).
//!
//! A route is derived from where a file sits, never from its title: moving a
//! file is what changes its URL, which is what makes redirects mechanical.

use liyasa_core::frontmatter::FrontmatterFields;
use liyasa_core::ids::Route;
use liyasa_core::vfs::VfsPath;

/// Extensions that make a file a page.
pub const PAGE_EXTENSIONS: &[&str] = &["md", "mdx"];

/// Directories whose contents are never routable (CM-03).
pub const RESERVED_DIRECTORIES: &[&str] = &["snippets", "components", "facts", "theme", "assets"];

/// The route a page is served at, before `seo.trailingSlash` is applied.
///
/// `index.md` maps to its directory; front matter `slug` replaces the last
/// path segment.
pub fn route_of(path: &VfsPath, front: Option<&FrontmatterFields>) -> Route {
    let mut segments: Vec<String> = path
        .as_str()
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(str::to_owned)
        .collect();
    if let Some(last) = segments.last_mut()
        && let Some((stem, extension)) = last.rsplit_once('.')
        && PAGE_EXTENSIONS.contains(&extension)
    {
        *last = stem.to_owned();
    }

    match front.and_then(|front| front.slug.as_deref()) {
        Some(slug) => {
            let slug = slug.trim_matches('/');
            match segments.last_mut() {
                Some(last) => *last = slug.to_owned(),
                None => segments.push(slug.to_owned()),
            }
        }
        None => {
            if segments.last().is_some_and(|last| last == "index") {
                segments.pop();
            }
        }
    }
    let path = format!("/{}", segments.join("/"));
    let trimmed = path.trim_end_matches('/');
    Route::new(if trimmed.is_empty() { "/" } else { trimmed })
}

/// The external URL a page's `url` front matter points at. Such a page is a
/// navigation link and has no body (CM-02).
pub fn external_of(front: Option<&FrontmatterFields>) -> Option<&str> {
    front.and_then(|front| front.url.as_deref())
}

/// The href to emit for a route under `seo.trailingSlash` (CM-02).
pub fn href(route: &Route, trailing_slash: bool) -> String {
    let path = route.as_str();
    if !trailing_slash || path == "/" {
        return path.to_owned();
    }
    format!("{path}/")
}

/// Whether a file may be served as a page (CM-03).
pub fn is_routable(path: &VfsPath, ignore: &Ignore) -> bool {
    let text = path.as_str();
    if text.is_empty() {
        return false;
    }
    if !path
        .extension()
        .is_some_and(|extension| PAGE_EXTENSIONS.contains(&extension))
    {
        return false;
    }
    let segments: Vec<&str> = text.split('/').collect();
    if segments.iter().any(|segment| segment.starts_with('_')) {
        return false;
    }
    if segments
        .iter()
        .rev()
        .skip(1)
        .any(|segment| RESERVED_DIRECTORIES.contains(segment))
    {
        return false;
    }
    !ignore.matches(path)
}

// ---- .liyasaignore (CM-03, CM-83) ----

/// A `.liyasaignore` file: gitignore syntax, last matching pattern wins.
#[derive(Debug, Default, Clone)]
pub struct Ignore(Vec<Pattern>);

#[derive(Debug, Clone)]
struct Pattern {
    negated: bool,
    anchored: bool,
    directory_only: bool,
    segments: Vec<String>,
}

impl Ignore {
    pub fn parse(text: &str) -> Self {
        Self(text.lines().filter_map(Pattern::parse).collect())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether `path` is ignored. The last pattern that matches decides, so a
    /// `!` line can bring a file back.
    pub fn matches(&self, path: &VfsPath) -> bool {
        let segments: Vec<&str> = path.as_str().split('/').collect();
        self.0
            .iter()
            .rev()
            .find(|pattern| pattern.matches(&segments))
            .is_some_and(|pattern| !pattern.negated)
    }
}

impl Pattern {
    fn parse(line: &str) -> Option<Self> {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            return None;
        }
        let (negated, rest) = match line.strip_prefix('!') {
            Some(rest) => (true, rest),
            None => (false, line),
        };
        let directory_only = rest.ends_with('/');
        let rest = rest.trim_end_matches('/');
        let anchored = rest.contains('/');
        let rest = rest.trim_start_matches('/');
        if rest.is_empty() {
            return None;
        }
        Some(Self {
            negated,
            anchored,
            directory_only,
            segments: rest.split('/').map(str::to_owned).collect(),
        })
    }

    fn matches(&self, path: &[&str]) -> bool {
        if self.anchored {
            return match_segments(&self.segments, path, self.directory_only);
        }
        // An unanchored pattern matches at any depth.
        (0..path.len()).any(|at| match_segments(&self.segments, &path[at..], self.directory_only))
    }
}

/// Matches pattern segments against path segments, with `**` spanning any
/// number of them.
fn match_segments(pattern: &[String], path: &[&str], directory_only: bool) -> bool {
    let Some(first) = pattern.first() else {
        // The pattern ran out: it matched a prefix, which is a directory
        // match, and a directory match covers everything under it.
        return !path.is_empty() || !directory_only;
    };
    if first == "**" {
        return (0..=path.len())
            .any(|at| match_segments(&pattern[1..], &path[at..], directory_only));
    }
    let Some((head, rest)) = path.split_first() else {
        return false;
    };
    if !match_glob(first, head) {
        return false;
    }
    if pattern.len() == 1 {
        // The last pattern segment matched this one: a file pattern matches
        // only the file, a directory pattern everything below it.
        return if directory_only {
            !rest.is_empty()
        } else {
            true
        } || !rest.is_empty();
    }
    match_segments(&pattern[1..], rest, directory_only)
}

/// `*` and `?` within one path segment.
fn match_glob(pattern: &str, text: &str) -> bool {
    let (pattern, text) = (pattern.as_bytes(), text.as_bytes());
    let (mut p, mut t) = (0usize, 0usize);
    let (mut star, mut mark) = (None, 0usize);
    while t < text.len() {
        match pattern.get(p) {
            Some(b'*') => {
                star = Some(p);
                mark = t;
                p += 1;
            }
            Some(b'?') => {
                p += 1;
                t += 1;
            }
            Some(byte) if *byte == text[t] => {
                p += 1;
                t += 1;
            }
            _ => match star {
                Some(at) => {
                    p = at + 1;
                    mark += 1;
                    t = mark;
                }
                None => return false,
            },
        }
    }
    pattern[p..].iter().all(|byte| *byte == b'*')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn front(slug: Option<&str>) -> FrontmatterFields {
        FrontmatterFields {
            slug: slug.map(str::to_owned),
            ..FrontmatterFields::default()
        }
    }

    fn route(path: &str) -> String {
        route_of(&VfsPath::new(path), None).as_str().to_owned()
    }

    #[test]
    fn a_path_becomes_a_route() {
        assert_eq!(
            route("getting-started/install.md"),
            "/getting-started/install"
        );
        assert_eq!(route("install.md"), "/install");
    }

    #[test]
    fn an_index_maps_to_its_directory() {
        assert_eq!(route("guides/index.md"), "/guides");
        assert_eq!(route("index.md"), "/");
        assert_eq!(route("a/b/index.md"), "/a/b");
    }

    #[test]
    fn a_slug_replaces_the_last_segment() {
        let path = VfsPath::new("getting-started/install.md");
        let found = route_of(&path, Some(&front(Some("setup"))));
        assert_eq!(found.as_str(), "/getting-started/setup");
    }

    #[test]
    fn mdx_is_a_page_extension_too() {
        assert_eq!(route("guides/legacy.mdx"), "/guides/legacy");
    }

    #[test]
    fn a_trailing_slash_is_applied_at_the_link() {
        let found = route_of(&VfsPath::new("guides/index.md"), None);
        assert_eq!(href(&found, false), "/guides");
        assert_eq!(href(&found, true), "/guides/");
        let root = route_of(&VfsPath::new("index.md"), None);
        assert_eq!(href(&root, true), "/");
    }

    #[test]
    fn an_external_url_overrides_the_whole_route() {
        let fields = FrontmatterFields {
            url: Some("https://status.acme.com".to_owned()),
            ..FrontmatterFields::default()
        };
        assert_eq!(external_of(Some(&fields)), Some("https://status.acme.com"));
        assert_eq!(external_of(None), None);
    }

    // ---- CM-03 ----

    fn routable(path: &str) -> bool {
        is_routable(&VfsPath::new(path), &Ignore::default())
    }

    #[test]
    fn reserved_directories_are_not_routable() {
        for path in [
            "snippets/note.md",
            "components/card.md",
            "facts/pricing.md",
            "theme/layout.md",
            "assets/readme.md",
            "guides/snippets/note.md",
        ] {
            assert!(!routable(path), "{path} should not be routable");
        }
    }

    #[test]
    fn an_underscore_prefix_is_not_routable() {
        assert!(!routable("_drafts/post.md"));
        assert!(!routable("guides/_wip.md"));
        assert!(routable("guides/wip_notes.md"));
    }

    #[test]
    fn only_page_extensions_are_routable() {
        assert!(routable("guides/install.md"));
        assert!(routable("guides/install.mdx"));
        assert!(!routable("guides/data.json"));
        assert!(!routable("guides"));
    }

    #[test]
    fn a_file_named_snippets_is_still_routable() {
        // Only the directory is reserved, not a page that shares its name.
        assert!(routable("snippets.md"));
    }

    #[test]
    fn the_ignore_file_removes_pages() {
        let ignore = Ignore::parse("# comment\n\ndrafts/\n*.tmp.md\n/private.md\n");
        let ignored = |path: &str| ignore.matches(&VfsPath::new(path));
        assert!(ignored("drafts/post.md"));
        assert!(ignored("a/drafts/post.md"));
        assert!(ignored("guides/notes.tmp.md"));
        assert!(ignored("private.md"));
        assert!(!ignored("guides/private.md"));
        assert!(!ignored("guides/install.md"));
    }

    #[test]
    fn a_negation_brings_a_page_back() {
        let ignore = Ignore::parse("drafts/\n!drafts/keep.md\n");
        assert!(ignore.matches(&VfsPath::new("drafts/post.md")));
        assert!(!ignore.matches(&VfsPath::new("drafts/keep.md")));
    }

    #[test]
    fn double_star_spans_directories() {
        let ignore = Ignore::parse("docs/**/internal.md\n");
        assert!(ignore.matches(&VfsPath::new("docs/internal.md")));
        assert!(ignore.matches(&VfsPath::new("docs/a/b/internal.md")));
        assert!(!ignore.matches(&VfsPath::new("other/internal.md")));
    }

    #[test]
    fn an_ignored_page_is_not_routable() {
        let ignore = Ignore::parse("drafts/\n");
        assert!(!is_routable(&VfsPath::new("drafts/post.md"), &ignore));
        assert!(is_routable(&VfsPath::new("guides/post.md"), &ignore));
    }

    #[test]
    fn globs_match_within_one_segment() {
        assert!(match_glob("*.md", "install.md"));
        assert!(match_glob("in?tall.md", "install.md"));
        assert!(!match_glob("*.md", "install.mdx"));
        assert!(match_glob("*", "anything"));
        assert!(match_glob("a*b*c", "axxbyyc"));
        assert!(!match_glob("a*b*c", "axxb"));
    }
}
