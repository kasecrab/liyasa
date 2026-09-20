//! The deployment a request is served from (PRD §6.4, RX-13, RX-60).
//!
//! A bundle is `dist/` plus its manifest: what the build produced, read once
//! at startup and then only from the page cache. The headers come from the
//! bundle's own `_headers`, parsed with the same reader the host emulators
//! use, so `liyasa serve` and a static host send the same policy by
//! construction rather than by a second implementation of it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use liyasa_build::hosting::{self, Rules};
use liyasa_build::manifest::{self, AssetEntry, Manifest, RouteEntry, ServedFile};
use liyasa_build::redirects::ManifestEntry as RedirectEntry;

use crate::auth::groups::Declared;

pub const HTML_TYPE: &str = "text/html; charset=utf-8";
pub const MARKDOWN_TYPE: &str = hosting::headers::MARKDOWN_TYPE;

/// What a request resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Page {
        route: String,
        /// The file under `dist/`.
        path: String,
        format: Format,
        /// Whether the choice came from `Accept` rather than the URL, which
        /// decides `Vary: Accept` (RX-60).
        negotiated: bool,
    },
    Asset {
        path: String,
        content_type: String,
    },
    Redirect {
        location: String,
        status: u16,
    },
    NotFound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Html,
    Markdown,
}

impl Format {
    pub fn content_type(self) -> &'static str {
        match self {
            Self::Html => HTML_TYPE,
            Self::Markdown => MARKDOWN_TYPE,
        }
    }
}

#[derive(Debug)]
pub struct Bundle {
    root: PathBuf,
    manifest: Manifest,
    rules: Rules,
    routes: HashMap<String, RouteEntry>,
    assets: HashMap<String, AssetEntry>,
    /// Theme CSS and JS, the agent surfaces, the search index — keyed by the
    /// request path they answer to (defect 145).
    served: HashMap<String, ServedFile>,
    redirects: Vec<RedirectEntry>,
}

/// `/a/b/` and `/a/b` are the same route; `/` stays `/`.
fn normalize(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_owned()
    } else {
        trimmed.to_owned()
    }
}

impl Bundle {
    pub fn open(root: &Path) -> std::io::Result<Self> {
        let manifest: Manifest =
            serde_json::from_str(&std::fs::read_to_string(root.join(manifest::FILE))?)
                .map_err(std::io::Error::other)?;
        let rules = std::fs::read_to_string(root.join(hosting::HEADERS_FILE))
            .map(|text| hosting::emulate::parse_headers_file(&text))
            .unwrap_or_default();
        Ok(Self::new(root.to_owned(), manifest, rules))
    }

    pub fn new(root: PathBuf, manifest: Manifest, rules: Rules) -> Self {
        // The manifest records URLs for the Markdown twin and for assets, so
        // both carry `build.basePath`; routes and variant paths do not. The
        // maps are keyed the way a stripped request path is spelled.
        let base = manifest.base_path.trim_end_matches('/').to_owned();
        let without_base = |url: &str| -> String {
            match base.is_empty() {
                true => url.to_owned(),
                false => url.strip_prefix(&base).unwrap_or(url).to_owned(),
            }
        };
        let routes = manifest
            .routes
            .iter()
            .map(|entry| {
                let mut entry = entry.clone();
                entry.markdown = without_base(&entry.markdown);
                (normalize(entry.route.as_str()), entry)
            })
            .collect();
        let assets = manifest
            .assets
            .iter()
            .map(|asset| (without_base(&asset.url), asset.clone()))
            .collect();
        // `served` records paths under `dist/` rather than URLs, so unlike
        // the assets these carry no base path to strip — `strip_base` has
        // already run by the time one is looked up.
        let served = manifest
            .served
            .iter()
            .map(|file| {
                (
                    format!("/{}", file.path.trim_start_matches('/')),
                    file.clone(),
                )
            })
            .collect();
        Self {
            redirects: manifest.redirects.clone(),
            root,
            rules,
            routes,
            assets,
            served,
            manifest,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn rules(&self) -> &Rules {
        &self.rules
    }

    pub fn base_path(&self) -> &str {
        &self.manifest.base_path
    }

    /// Strips `build.basePath` so one instance can serve `acme.com/docs` and
    /// `docs.acme.com` from the same bundle (HOST-22). `None` when the request
    /// is outside the prefix.
    pub fn strip_base(&self, path: &str) -> Option<String> {
        let base = self.base_path().trim_end_matches('/');
        if base.is_empty() {
            return Some(path.to_owned());
        }
        if path == base {
            return Some("/".to_owned());
        }
        path.strip_prefix(base)
            .filter(|rest| rest.starts_with('/'))
            .map(str::to_owned)
    }

    pub fn route(&self, route: &str) -> Option<&RouteEntry> {
        self.routes.get(&normalize(route))
    }

    /// Who may see this route (AUTH-07, AUTH-10, §7.6), as
    /// [`crate::auth::groups::decide`] wants it: navigation ancestors
    /// root-first, then the page's own level.
    ///
    /// The build resolves the chain, because the hierarchy it comes from is
    /// the config's `navigation` tree and the server never sees that. A route
    /// the manifest does not know returns an empty chain, which `decide`
    /// reads as unrestricted — correct, because there is no page there and
    /// the caller has already resolved it to `NotFound`.
    pub fn access_chain(&self, route: &str) -> Vec<Declared> {
        self.route(route)
            .map(|entry| entry.access.iter().map(declared_of).collect())
            .unwrap_or_default()
    }

    /// What `path` resolves to for a client that did or did not ask for
    /// Markdown. `wants_markdown` is the parsed `Accept` header.
    pub fn resolve(&self, path: &str, wants_markdown: bool) -> Target {
        let Some(path) = self.strip_base(path) else {
            return Target::NotFound;
        };
        if let Some(hit) = self
            .redirects
            .iter()
            .find(|rule| normalize(&rule.source) == normalize(&path))
        {
            return Target::Redirect {
                location: hit.destination.clone(),
                status: hit.status,
            };
        }
        if let Some(asset) = self.assets.get(&path) {
            return Target::Asset {
                path: asset.path.clone(),
                content_type: asset.content_type.clone(),
            };
        }
        // Ahead of the `.md` twin, because `skill.md` is a served file and not
        // a page's Markdown: reaching the twin branch first would strip the
        // suffix and look for a route called `/skill`.
        if let Some(file) = self.served.get(&path) {
            return Target::Asset {
                path: file.path.clone(),
                content_type: file.content_type.clone(),
            };
        }

        // `<route>.md` and `<route>/index.md` are the Markdown twin (RX-60).
        if let Some(stem) = path.strip_suffix("/index.md").or(path.strip_suffix(".md")) {
            let route = if stem.is_empty() { "/" } else { stem };
            if let Some(entry) = self.route(route) {
                return Target::Page {
                    route: entry.route.as_str().to_owned(),
                    path: entry.markdown.clone(),
                    format: Format::Markdown,
                    negotiated: false,
                };
            }
            return Target::NotFound;
        }

        let Some(entry) = self.route(&path) else {
            return Target::NotFound;
        };
        if wants_markdown {
            return Target::Page {
                route: entry.route.as_str().to_owned(),
                path: entry.markdown.clone(),
                format: Format::Markdown,
                negotiated: true,
            };
        }
        let Some(variant) = entry.variants.first() else {
            return Target::NotFound;
        };
        Target::Page {
            route: entry.route.as_str().to_owned(),
            path: variant.path.clone(),
            format: Format::Html,
            negotiated: false,
        }
    }

    /// The headers `_headers` gives this path, with the base path put back.
    pub fn headers_for(&self, path: &str) -> Vec<(String, String)> {
        self.rules.resolve(path)
    }

    /// Reads a file from the bundle. The path comes from the manifest, never
    /// from the request, so there is no traversal to defend against; the
    /// check below is belt and braces for a manifest written by hand.
    pub fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        let relative = path.trim_start_matches('/');
        if relative.split('/').any(|part| part == "..") {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "a bundle path may not climb out of the bundle",
            ));
        }
        std::fs::read(self.root.join(relative))
    }

    /// `404.html`, which every listed host serves by name.
    pub fn not_found_body(&self) -> Option<Vec<u8>> {
        self.read("404.html").ok()
    }
}

/// Whether the client asked for Markdown (RX-60, §6.4 step 3).
///
/// `text/markdown` must beat `text/html` on quality, so a browser sending
/// `text/html,*/*;q=0.8` still gets HTML and an agent sending
/// `text/markdown` gets Markdown.
pub fn prefers_markdown(accept: Option<&str>) -> bool {
    let Some(accept) = accept else {
        return false;
    };
    let mut markdown = None::<f32>;
    let mut html = None::<f32>;
    for part in accept.split(',') {
        let mut fields = part.split(';').map(str::trim);
        let Some(media) = fields.next() else {
            continue;
        };
        let quality = fields
            .find_map(|f| f.strip_prefix("q="))
            .and_then(|q| q.parse::<f32>().ok())
            .unwrap_or(1.0);
        let slot = match media.to_ascii_lowercase().as_str() {
            "text/markdown" | "text/x-markdown" => &mut markdown,
            "text/html" | "application/xhtml+xml" | "*/*" | "text/*" => &mut html,
            _ => continue,
        };
        if slot.is_none_or(|current| quality > current) {
            *slot = Some(quality);
        }
    }
    match (markdown, html) {
        (Some(md), Some(html)) => md > html,
        (Some(md), None) => md > 0.0,
        _ => false,
    }
}

/// One manifest level as the access module's own type. They are separate
/// types on purpose: `liyasa-build` cannot depend on `liyasa-server`, so the
/// serialized shape and the decision's input cannot be one struct.
fn declared_of(level: &manifest::AccessLevel) -> Declared {
    Declared {
        groups: level.groups.iter().cloned().collect(),
        public: level.public,
    }
}

#[cfg(test)]
mod tests {
    use liyasa_build::manifest::{AccessLevel, RouteEntry, VariantEntry};
    use liyasa_core::ids::{BuildId, Fingerprint, Route};

    use super::*;
    use crate::auth::session::Principal;

    fn route(path: &str) -> RouteEntry {
        RouteEntry {
            route: Route::new(path),
            source: format!("{}.md", path.trim_start_matches('/')),
            markdown: format!("{path}.md"),
            hidden: false,
            dynamic: false,
            variants: vec![VariantEntry {
                key: String::new(),
                path: format!("{}/index.html", path.trim_end_matches('/')),
                hash: Fingerprint::of(path),
            }],
            access: vec![AccessLevel::new([], false)],
        }
    }

    /// The same page behind a navigation ancestor and its own declaration.
    fn restricted(path: &str, ancestor: &[&str], own: &[&str]) -> RouteEntry {
        RouteEntry {
            access: vec![
                AccessLevel::new(ancestor.iter().map(|g| (*g).to_owned()), false),
                AccessLevel::new(own.iter().map(|g| (*g).to_owned()), false),
            ],
            ..route(path)
        }
    }

    fn bundle(base_path: &str) -> Bundle {
        let with_base = |entry: RouteEntry| RouteEntry {
            markdown: format!("{base_path}{}", entry.markdown),
            ..entry
        };
        let manifest = Manifest {
            build_id: BuildId(Fingerprint::of("build")),
            liyasa_version: "0.1.0".to_owned(),
            built_at: 0,
            base_path: base_path.to_owned(),
            routes: vec![with_base(route("/")), with_base(route("/guides/install"))],
            assets: Vec::new(),
            images: Vec::new(),
            redirects: vec![RedirectEntry {
                source: "/old".to_owned(),
                destination: "/guides/install".to_owned(),
                status: 301,
            }],
            served: Vec::new(),
            inputs: Default::default(),
        };
        Bundle::new(PathBuf::from("/nonexistent"), manifest, Rules::default())
    }

    fn restricted_bundle() -> Bundle {
        let manifest = Manifest {
            build_id: BuildId(Fingerprint::of("build")),
            liyasa_version: "0.1.0".to_owned(),
            built_at: 0,
            base_path: String::new(),
            routes: vec![
                route("/"),
                restricted("/internal/failover", &["staff"], &["sre", "oncall"]),
            ],
            assets: Vec::new(),
            images: Vec::new(),
            redirects: Vec::new(),
            served: Vec::new(),
            inputs: Default::default(),
        };
        Bundle::new(PathBuf::from("/nonexistent"), manifest, Rules::default())
    }

    #[test]
    fn the_access_chain_keeps_the_manifest_order() {
        let bundle = restricted_bundle();
        let chain = bundle.access_chain("/internal/failover");
        assert_eq!(chain.len(), 2, "the ancestor and the page itself");
        assert_eq!(
            chain[0].groups.iter().cloned().collect::<Vec<_>>(),
            ["staff"],
            "the ancestor comes first"
        );
        assert_eq!(
            chain[1].groups.iter().cloned().collect::<Vec<_>>(),
            ["oncall", "sre"],
            "the page's own level is last"
        );

        // A page that restricts nothing still carries its own level, because
        // `decide` reads `access: public` off `chain.last()` — drop it and an
        // ancestor's flag is read as the page's.
        assert_eq!(bundle.access_chain("/").len(), 1);
        assert!(bundle.access_chain("/nope").is_empty());
    }

    #[test]
    fn a_chain_from_the_manifest_means_all_levels_and_any_group() {
        use crate::auth::groups::{Decision, SiteDefault, decide};

        let bundle = restricted_bundle();
        let chain = bundle.access_chain("/internal/failover");
        let reader = |groups: &[&str]| Principal {
            groups: groups.iter().map(|g| (*g).to_owned()).collect(),
            ..Principal::default()
        };

        // `staff` AND (`sre` OR `oncall`). If the two levels were ever
        // flattened into one set this would read `staff` OR `sre` OR
        // `oncall`, and the two Deny cases below would come back Allow.
        assert_eq!(decide(SiteDefault::Public, &chain, None), Decision::SignIn);
        assert_eq!(
            decide(SiteDefault::Public, &chain, Some(&reader(&["staff"]))),
            Decision::Deny,
            "satisfies the ancestor and neither of the page's own groups"
        );
        assert_eq!(
            decide(SiteDefault::Public, &chain, Some(&reader(&["sre"]))),
            Decision::Deny,
            "satisfies the page and not the ancestor"
        );
        assert_eq!(
            decide(
                SiteDefault::Public,
                &chain,
                Some(&reader(&["staff", "oncall"]))
            ),
            Decision::Allow
        );
        assert_eq!(
            decide(
                SiteDefault::Public,
                &chain,
                Some(&reader(&["staff", "sre"]))
            ),
            Decision::Allow
        );
    }

    #[test]
    fn a_page_is_reachable_as_html_markdown_and_index_markdown() {
        let bundle = bundle("");
        let html = bundle.resolve("/guides/install", false);
        assert!(matches!(
            &html,
            Target::Page {
                format: Format::Html,
                negotiated: false,
                ..
            }
        ));

        for path in ["/guides/install.md", "/guides/install/index.md"] {
            match bundle.resolve(path, false) {
                Target::Page {
                    format,
                    path,
                    negotiated,
                    ..
                } => {
                    assert_eq!(format, Format::Markdown, "{path}");
                    assert_eq!(path, "/guides/install.md");
                    assert!(!negotiated, "the URL said so, not the header");
                }
                other => panic!("{path}: {other:?}"),
            }
        }
    }

    #[test]
    fn accept_chooses_markdown_and_marks_the_response_as_negotiated() {
        let bundle = bundle("");
        match bundle.resolve("/guides/install", true) {
            Target::Page {
                format, negotiated, ..
            } => {
                assert_eq!(format, Format::Markdown);
                assert!(negotiated, "the response varies on Accept");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_trailing_slash_is_the_same_route() {
        let bundle = bundle("");
        assert_eq!(
            bundle.resolve("/guides/install/", false),
            bundle.resolve("/guides/install", false)
        );
        assert!(matches!(bundle.resolve("/", false), Target::Page { .. }));
    }

    #[test]
    fn a_base_path_is_stripped_and_a_request_outside_it_is_a_miss() {
        let bundle = bundle("/docs");
        assert!(matches!(
            bundle.resolve("/docs/guides/install", false),
            Target::Page { .. }
        ));
        // The manifest writes the Markdown twin as a URL, so it carries the
        // base path; the file under `dist/` does not.
        match bundle.resolve("/docs/guides/install.md", false) {
            Target::Page { path, format, .. } => {
                assert_eq!(format, Format::Markdown);
                assert_eq!(
                    path, "/guides/install.md",
                    "the path is under dist/, not a URL"
                );
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            bundle.resolve("/docs", false),
            Target::Page { .. }
        ));
        assert_eq!(bundle.resolve("/guides/install", false), Target::NotFound);
        assert_eq!(bundle.resolve("/docsother", false), Target::NotFound);
    }

    #[test]
    fn a_redirect_in_the_manifest_is_served_before_the_route_is_looked_up() {
        assert_eq!(
            bundle("").resolve("/old", false),
            Target::Redirect {
                location: "/guides/install".to_owned(),
                status: 301
            }
        );
    }

    #[test]
    fn an_unknown_route_is_a_miss_in_both_formats() {
        let bundle = bundle("");
        assert_eq!(bundle.resolve("/absent", false), Target::NotFound);
        assert_eq!(bundle.resolve("/absent.md", false), Target::NotFound);
    }

    #[test]
    fn a_browsers_accept_header_still_gets_html() {
        assert!(!prefers_markdown(Some(
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"
        )));
        assert!(!prefers_markdown(None));
        assert!(!prefers_markdown(Some("*/*")));
        assert!(prefers_markdown(Some("text/markdown")));
        assert!(prefers_markdown(Some("text/markdown, text/html;q=0.5")));
        assert!(!prefers_markdown(Some("text/markdown;q=0.4, text/html")));
        assert!(prefers_markdown(Some("text/markdown;q=0.9, */*;q=0.8")));
    }

    #[test]
    fn a_bundle_path_cannot_climb_out_of_the_bundle() {
        let bundle = bundle("");
        assert!(bundle.read("../../etc/passwd").is_err());
    }
}
