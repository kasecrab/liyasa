//! The routes a project has, which is what navigation is checked against
//! (CFG-30).
//!
//! Discovery walks the project tree through the `Vfs` and turns every Markdown
//! file into the route it will be served at. Callers that already know the
//! content tree — `liyasa build` does — pass their own set instead.

use std::collections::BTreeSet;

use liyasa_core::vfs::{Vfs, VfsKind, VfsPath};

/// Directories that never hold pages (PRD §34.4 and the conventional ones).
const SKIPPED: &[&str] = &[
    ".git",
    "node_modules",
    "snippets",
    "target",
    "dist",
    "automations",
    "assets",
    "public",
];

const PAGE_EXTENSIONS: &[&str] = &["md", "mdx"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pages {
    routes: BTreeSet<String>,
}

impl Pages {
    pub fn new() -> Self {
        Self::default()
    }

    /// Every Markdown file under `root`, as a route.
    pub fn discover(vfs: &dyn Vfs, root: &VfsPath, output: &str) -> Self {
        let mut pages = Self::new();
        pages.walk(vfs, root, root, output, 0);
        pages
    }

    fn walk(&mut self, vfs: &dyn Vfs, root: &VfsPath, dir: &VfsPath, output: &str, depth: u8) {
        if depth > 32 {
            return;
        }
        let Ok(entries) = vfs.list(dir) else {
            return;
        };
        for entry in entries {
            let Some(name) = entry.file_name() else {
                continue;
            };
            if name.starts_with('.') || SKIPPED.contains(&name) || name == output {
                continue;
            }
            match vfs.metadata(&entry).map(|meta| meta.kind) {
                Ok(VfsKind::Dir) => self.walk(vfs, root, &entry, output, depth + 1),
                Ok(VfsKind::File)
                    if entry
                        .extension()
                        .is_some_and(|e| PAGE_EXTENSIONS.contains(&e)) =>
                {
                    self.routes.insert(route_of(root, &entry));
                }
                _ => {}
            }
        }
    }

    pub fn insert(&mut self, route: impl AsRef<str>) -> &mut Self {
        self.routes.insert(normalize(route.as_ref()));
        self
    }

    pub fn contains(&self, route: &str) -> bool {
        self.routes.contains(&normalize(route))
    }

    /// Whether any route sits directly or indirectly under `dir`, which is what
    /// a `{ "directory": … }` node needs.
    pub fn has_directory(&self, dir: &str) -> bool {
        let prefix = format!("{}/", normalize(dir));
        self.routes.iter().any(|route| route.starts_with(&prefix))
    }

    /// Routes matching a navigation glob: `*` spans one segment, `**` spans
    /// any number.
    pub fn matching(&self, pattern: &str) -> Vec<&str> {
        let pattern = normalize(pattern);
        self.routes
            .iter()
            .filter(|route| glob(&pattern, route))
            .map(String::as_str)
            .collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.routes.iter().map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.routes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }
}

impl<S: AsRef<str>> FromIterator<S> for Pages {
    fn from_iter<I: IntoIterator<Item = S>>(iter: I) -> Self {
        Self {
            routes: iter.into_iter().map(|s| normalize(s.as_ref())).collect(),
        }
    }
}

/// `guides/index.md` under `docs/` is the route `guides`; `index.md` at the
/// root is the empty route.
fn route_of(root: &VfsPath, page: &VfsPath) -> String {
    let full = page.as_str();
    let relative = match root.as_str() {
        "" => full,
        prefix => full
            .strip_prefix(prefix)
            .unwrap_or(full)
            .trim_start_matches('/'),
    };
    let stem = relative.rsplit_once('.').map_or(relative, |(head, _)| head);
    normalize(stem)
}

fn normalize(route: &str) -> String {
    let route = route.trim_matches('/');
    let route = route
        .rsplit_once('.')
        .filter(|(_, ext)| PAGE_EXTENSIONS.contains(ext))
        .map_or(route, |(head, _)| head);
    route
        .strip_suffix("index")
        .map_or(route, |head| head.trim_end_matches('/'))
        .to_owned()
}

/// Segment-wise glob. Only `*` and `**` are supported, which is what §8.4's
/// `"getting-started/*"` needs.
fn glob(pattern: &str, route: &str) -> bool {
    let pattern: Vec<&str> = pattern.split('/').collect();
    let route: Vec<&str> = route.split('/').collect();
    matches_from(&pattern, &route)
}

fn matches_from(pattern: &[&str], route: &[&str]) -> bool {
    match pattern.first() {
        None => route.is_empty(),
        Some(&"**") => (0..=route.len()).any(|skip| matches_from(&pattern[1..], &route[skip..])),
        Some(&"*") => !route.is_empty() && matches_from(&pattern[1..], &route[1..]),
        Some(segment) => route.first() == Some(segment) && matches_from(&pattern[1..], &route[1..]),
    }
}
