//! Documentation versions (PRD §7.10, CM-90..CM-93).
//!
//! Two content models, which may be mixed: a full tree under
//! `versions/<name>/`, and a shared tree whose pages name the versions they
//! belong to in front matter. Either way the default version is served at the
//! un-prefixed route and every other version under `/<name>/`.

use std::collections::{BTreeMap, BTreeSet};

use liyasa_core::ids::{Route, Version};
use liyasa_core::vfs::VfsPath;

use crate::engine::settings::{Tag, VersionDecl};

/// The directory a full version tree lives in.
pub const VERSIONS_DIR: &str = "versions";

/// One entry of the navbar's version switcher (CM-91, CM-93).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherEntry {
    pub version: Version,
    pub label: String,
    pub href: String,
    pub current: bool,
    pub tag: Option<Tag>,
    /// The target route does not exist in that version, so the switcher points
    /// at the closest ancestor that does and the page says so.
    pub fallback: bool,
}

/// The banner a deprecated version carries (CM-93).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Banner {
    pub message: String,
    pub href: String,
    pub dismissible: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Versions {
    decls: Vec<VersionDecl>,
}

impl Versions {
    pub fn new(decls: &[VersionDecl]) -> Self {
        Self {
            decls: decls.to_vec(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.decls.is_empty()
    }

    pub fn names(&self) -> Vec<Version> {
        self.decls
            .iter()
            .map(|decl| Version::new(decl.name.clone()))
            .collect()
    }

    pub fn default_version(&self) -> Option<Version> {
        self.decls
            .iter()
            .find(|decl| decl.default)
            .or_else(|| self.decls.first())
            .map(|decl| Version::new(decl.name.clone()))
    }

    pub fn declaration(&self, version: &Version) -> Option<&VersionDecl> {
        self.decls.iter().find(|decl| decl.name == version.as_str())
    }

    /// The version a file belongs to by where it sits, and the path within that
    /// version's tree (CM-90, full-tree model).
    pub fn of_path(&self, path: &VfsPath) -> Option<(Version, VfsPath)> {
        let text = path.as_str();
        for decl in &self.decls {
            let prefix = match &decl.path {
                Some(custom) => format!("{}/", custom.trim_matches('/')),
                None => format!("{VERSIONS_DIR}/{}/", decl.name),
            };
            if let Some(rest) = text.strip_prefix(&prefix) {
                return Some((Version::new(decl.name.clone()), VfsPath::new(rest)));
            }
        }
        None
    }

    /// Where a page is served: the default version keeps the bare route, every
    /// other version is prefixed (CM-91).
    pub fn route_of(&self, base: &Route, version: Option<&Version>) -> Route {
        let Some(version) = version else {
            return base.clone();
        };
        if self.default_version().as_ref() == Some(version) {
            return base.clone();
        }
        let trimmed = base.as_str().trim_matches('/');
        match trimmed.is_empty() {
            true => Route::new(format!("/{version}")),
            false => Route::new(format!("/{version}/{trimmed}")),
        }
    }

    /// `/<version>/llms.txt` and the rest of the per-version surfaces (CM-92).
    pub fn surface(&self, version: Option<&Version>, name: &str) -> String {
        self.route_of(&Route::new(format!("/{name}")), version)
            .as_str()
            .to_owned()
    }

    /// The switcher for one page. A version that does not have the page falls
    /// back to the closest ancestor route it does have (CM-91).
    pub fn switcher(
        &self,
        base: &Route,
        current: Option<&Version>,
        routes: &BTreeMap<Version, BTreeSet<Route>>,
    ) -> Vec<SwitcherEntry> {
        self.decls
            .iter()
            .map(|decl| {
                let version = Version::new(decl.name.clone());
                let known = routes.get(&version);
                let (target, fallback) = match known {
                    Some(known) if known.contains(base) => (base.clone(), false),
                    Some(known) => (closest_ancestor(base, known), true),
                    None => (Route::new("/"), true),
                };
                SwitcherEntry {
                    href: self.route_of(&target, Some(&version)).as_str().to_owned(),
                    current: current == Some(&version),
                    label: decl.label.clone(),
                    tag: decl.tag,
                    fallback,
                    version,
                }
            })
            .collect()
    }

    /// The dismissible banner a deprecated version shows, pointing at the
    /// default (CM-93).
    pub fn banner(&self, version: Option<&Version>) -> Option<Banner> {
        let version = version?;
        let decl = self.declaration(version)?;
        if decl.tag != Some(Tag::Deprecated) {
            return None;
        }
        let default = self.default_version()?;
        let default_label = self
            .declaration(&default)
            .map(|decl| decl.label.clone())
            .unwrap_or_else(|| default.as_str().to_owned());
        Some(Banner {
            message: format!(
                "{} is no longer maintained. The current documentation is {default_label}.",
                decl.label
            ),
            href: self
                .route_of(&Route::new("/"), Some(&default))
                .as_str()
                .to_owned(),
            dismissible: true,
        })
    }

    /// The badge a version carries in the switcher (CM-93).
    pub fn badge(&self, version: &Version) -> Option<&'static str> {
        self.declaration(version)?.tag.map(Tag::label)
    }

    /// The versions a page in the shared tree belongs to: what its front matter
    /// says, or every declared version when it says nothing (CM-90).
    pub fn of_frontmatter(&self, declared: &[Version]) -> Vec<Version> {
        match declared.is_empty() {
            true => self.names(),
            false => declared
                .iter()
                .filter(|version| self.declaration(version).is_some())
                .cloned()
                .collect(),
        }
    }
}

/// Turns the discovered pages into the pages each version serves (CM-90).
///
/// A page under a version's own tree belongs to that version; a page in the
/// shared tree belongs to the versions its front matter names, or to all of
/// them. Without declared versions the list comes back untouched.
pub fn expand(pages: Vec<crate::tree::Page>, versions: &Versions) -> Vec<crate::tree::Page> {
    if versions.is_empty() {
        return pages;
    }
    let mut out = Vec::with_capacity(pages.len());
    for page in pages {
        match versions.of_path(&page.path) {
            Some((version, within)) => {
                let base = liyasa_markdown::source::route::route_of(&within, Some(&page.front));
                out.push(crate::tree::Page {
                    route: versions.route_of(&base, Some(&version)),
                    base_route: base,
                    version: Some(version),
                    ..page
                });
            }
            None => {
                for version in versions.of_frontmatter(&page.front.versions) {
                    out.push(crate::tree::Page {
                        route: versions.route_of(&page.base_route, Some(&version)),
                        version: Some(version),
                        ..page.clone()
                    });
                }
            }
        }
    }
    out
}

/// Every route each version serves, which is what the switcher is built from.
pub fn routes_by_version(pages: &[crate::tree::Page]) -> BTreeMap<Version, BTreeSet<Route>> {
    let mut out: BTreeMap<Version, BTreeSet<Route>> = BTreeMap::new();
    for page in pages {
        if let Some(version) = &page.version {
            out.entry(version.clone())
                .or_default()
                .insert(page.base_route.clone());
        }
    }
    out
}

/// The deepest route in `known` that is a prefix of `base`, or `/`.
fn closest_ancestor(base: &Route, known: &BTreeSet<Route>) -> Route {
    let mut segments: Vec<&str> = base
        .as_str()
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    while !segments.is_empty() {
        segments.pop();
        let candidate = match segments.is_empty() {
            true => Route::new("/"),
            false => Route::new(format!("/{}", segments.join("/"))),
        };
        if known.contains(&candidate) {
            return candidate;
        }
    }
    Route::new("/")
}

#[cfg(test)]
pub(crate) mod tests_support {
    use super::*;

    pub fn decls() -> Vec<VersionDecl> {
        vec![
            VersionDecl {
                name: "v2".to_owned(),
                label: "2.x".to_owned(),
                default: true,
                path: None,
                tag: Some(Tag::Latest),
            },
            VersionDecl {
                name: "v1".to_owned(),
                label: "1.x".to_owned(),
                default: false,
                path: None,
                tag: Some(Tag::Deprecated),
            },
        ]
    }

    pub fn versions() -> Versions {
        Versions::new(&decls())
    }

    pub fn routes(pairs: &[(&str, &[&str])]) -> BTreeMap<Version, BTreeSet<Route>> {
        pairs
            .iter()
            .map(|(version, routes)| {
                (
                    Version::new((*version).to_owned()),
                    routes.iter().map(|route| Route::new(*route)).collect(),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::*;
    use super::*;

    #[test]
    fn a_full_tree_names_its_version_by_its_directory() {
        let found = versions().of_path(&VfsPath::new("versions/v1/guides/install.md"));
        let (version, rest) = found.expect("a versioned page");
        assert_eq!(version.as_str(), "v1");
        assert_eq!(rest.as_str(), "guides/install.md");
        assert!(
            versions()
                .of_path(&VfsPath::new("guides/install.md"))
                .is_none()
        );
    }

    #[test]
    fn a_custom_directory_is_honoured() {
        let mut decls = decls();
        decls[1].path = Some("legacy".to_owned());
        let versions = Versions::new(&decls);
        let (version, rest) = versions
            .of_path(&VfsPath::new("legacy/guides/install.md"))
            .expect("a versioned page");
        assert_eq!(version.as_str(), "v1");
        assert_eq!(rest.as_str(), "guides/install.md");
    }

    #[test]
    fn the_default_version_keeps_the_bare_route() {
        let versions = versions();
        let base = Route::new("/guides/install");
        assert_eq!(
            versions.route_of(&base, Some(&Version::new("v2"))).as_str(),
            "/guides/install"
        );
        assert_eq!(
            versions.route_of(&base, Some(&Version::new("v1"))).as_str(),
            "/v1/guides/install"
        );
        assert_eq!(versions.route_of(&base, None).as_str(), "/guides/install");
    }

    #[test]
    fn the_root_of_a_version_is_the_version_itself() {
        assert_eq!(
            versions()
                .route_of(&Route::new("/"), Some(&Version::new("v1")))
                .as_str(),
            "/v1"
        );
    }

    #[test]
    fn per_version_surfaces_are_prefixed_too() {
        let versions = versions();
        assert_eq!(
            versions.surface(Some(&Version::new("v1")), "llms.txt"),
            "/v1/llms.txt"
        );
        assert_eq!(
            versions.surface(Some(&Version::new("v2")), "llms.txt"),
            "/llms.txt"
        );
    }

    #[test]
    fn the_switcher_points_at_the_same_page_when_it_exists() {
        let known = routes(&[
            ("v2", &["/guides/install", "/"]),
            ("v1", &["/guides/install", "/"]),
        ]);
        let entries = versions().switcher(
            &Route::new("/guides/install"),
            Some(&Version::new("v2")),
            &known,
        );
        let v1 = entries
            .iter()
            .find(|entry| entry.version.as_str() == "v1")
            .expect("v1 is in the switcher");
        assert_eq!(v1.href, "/v1/guides/install");
        assert!(!v1.fallback);
        assert!(entries[0].current);
    }

    #[test]
    fn a_missing_page_falls_back_to_the_closest_ancestor() {
        let known = routes(&[
            ("v2", &["/guides/install/advanced", "/guides/install", "/"]),
            ("v1", &["/guides", "/"]),
        ]);
        let entries = versions().switcher(
            &Route::new("/guides/install/advanced"),
            Some(&Version::new("v2")),
            &known,
        );
        let v1 = entries
            .iter()
            .find(|entry| entry.version.as_str() == "v1")
            .expect("v1 is in the switcher");
        assert_eq!(v1.href, "/v1/guides");
        assert!(v1.fallback, "the page is missing, so the reader is told");
    }

    #[test]
    fn a_deprecated_version_carries_a_banner_to_the_default() {
        let banner = versions()
            .banner(Some(&Version::new("v1")))
            .expect("a deprecation banner");
        assert!(banner.message.contains("1.x"));
        assert!(banner.message.contains("2.x"));
        assert_eq!(banner.href, "/");
        assert!(banner.dismissible);
        assert!(versions().banner(Some(&Version::new("v2"))).is_none());
    }

    #[test]
    fn a_version_badge_is_its_tag() {
        assert_eq!(versions().badge(&Version::new("v2")), Some("latest"));
        assert_eq!(versions().badge(&Version::new("v1")), Some("deprecated"));
        assert_eq!(versions().badge(&Version::new("v0")), None);
    }

    #[test]
    fn a_shared_tree_page_belongs_to_what_it_names_or_to_everything() {
        let versions = versions();
        assert_eq!(versions.of_frontmatter(&[]).len(), 2);
        assert_eq!(
            versions.of_frontmatter(&[Version::new("v1")]),
            vec![Version::new("v1")]
        );
        assert!(versions.of_frontmatter(&[Version::new("v9")]).is_empty());
    }

    #[test]
    fn no_versions_declared_means_no_prefixes_at_all() {
        let versions = Versions::default();
        assert!(versions.is_empty());
        assert_eq!(
            versions
                .route_of(&Route::new("/guides"), Some(&Version::new("v1")))
                .as_str(),
            "/v1/guides"
        );
        assert!(versions.banner(Some(&Version::new("v1"))).is_none());
    }
}

#[cfg(test)]
mod expansion_tests {
    use liyasa_core::frontmatter::FrontmatterFields;
    use liyasa_core::ids::Fingerprint;

    use super::tests_support::*;
    use super::*;
    use crate::tree::{Indexing, Page};

    fn page(path: &str, versions: &[&str]) -> Page {
        let front = FrontmatterFields {
            versions: versions.iter().map(|v| Version::new(*v)).collect(),
            ..FrontmatterFields::default()
        };
        let route = liyasa_markdown::source::route::route_of(&VfsPath::new(path), Some(&front));
        Page {
            path: VfsPath::new(path),
            base_route: route.clone(),
            route,
            version: None,
            fingerprint: Fingerprint::of(path),
            front,
            indexing: Indexing {
                navigation: true,
                sitemap: true,
                search: true,
                ai: true,
            },
            hidden: false,
            draft: false,
        }
    }

    #[test]
    fn a_full_tree_page_is_routed_under_its_version() {
        let pages = expand(
            vec![page("versions/v1/guides/install.md", &[])],
            &versions(),
        );
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].route.as_str(), "/v1/guides/install");
        assert_eq!(pages[0].base_route.as_str(), "/guides/install");
        assert_eq!(pages[0].version.as_ref().map(Version::as_str), Some("v1"));
    }

    #[test]
    fn a_shared_tree_page_is_served_once_per_version() {
        let pages = expand(vec![page("guides/install.md", &[])], &versions());
        let routes: Vec<&str> = pages.iter().map(|page| page.route.as_str()).collect();
        assert_eq!(routes, ["/guides/install", "/v1/guides/install"]);
    }

    #[test]
    fn front_matter_narrows_which_versions_a_page_appears_in() {
        let pages = expand(vec![page("guides/install.md", &["v1"])], &versions());
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].route.as_str(), "/v1/guides/install");
    }

    #[test]
    fn without_declared_versions_nothing_moves() {
        let pages = expand(vec![page("guides/install.md", &[])], &Versions::default());
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].route.as_str(), "/guides/install");
        assert!(pages[0].version.is_none());
    }

    #[test]
    fn the_route_table_is_grouped_by_version() {
        let pages = expand(
            vec![
                page("guides/install.md", &[]),
                page("versions/v1/guides/legacy.md", &[]),
            ],
            &versions(),
        );
        let table = routes_by_version(&pages);
        assert_eq!(table[&Version::new("v2")].len(), 1);
        assert_eq!(table[&Version::new("v1")].len(), 2);
    }
}
