//! `dist/liyasa-manifest.json` (PRD §6.6, RX-03).
//!
//! Every route, its variants, its Markdown, its hashes, and the source file it
//! came from, so the server, the MCP server, and the verifier never rescan the
//! file system. Everything in it is sorted: the manifest is part of the output
//! a determinism check diffs (§6.6.2 rule 5).

use std::collections::BTreeMap;

use liyasa_core::ids::{BuildId, Fingerprint, Route};
use serde::{Deserialize, Serialize};

use crate::assets::Disposition;
use crate::redirects::ManifestEntry as RedirectEntry;

pub const FILE: &str = "liyasa-manifest.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub build_id: BuildId,
    pub liyasa_version: String,
    /// The build clock, in seconds since the Unix epoch (§6.6.2 rule 1).
    pub built_at: i64,
    pub base_path: String,
    pub routes: Vec<RouteEntry>,
    pub assets: Vec<AssetEntry>,
    pub images: Vec<ImageEntry>,
    pub redirects: Vec<RedirectEntry>,
    /// Files the build wrote that the server must serve as-is: the theme's
    /// CSS and JS, the agent surfaces, the search index (defect 145).
    ///
    /// `Bundle` indexes the manifest and nothing else, so anything absent
    /// from it is unreachable through `liyasa serve` however plainly it sits
    /// in `dist/`. Before this existed every page the server returned was
    /// unstyled, because `_liyasa/theme.<hash>.css` is written by the theme
    /// pipeline rather than from an `assets/` source directory and so never
    /// became an `AssetEntry`.
    pub served: Vec<ServedFile>,
    /// What the build ID was computed over, so a rebuild can say what moved.
    pub inputs: BTreeMap<String, Fingerprint>,
}

/// A file served byte for byte from `dist/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServedFile {
    /// Path under `dist/`, no leading slash — the spelling `write_file`
    /// records and `Bundle::read` expects.
    pub path: String,
    pub content_type: String,
    /// For a listing — `llms.txt`, `llms-full.txt`, `sitemap.xml` — which
    /// route occupies which bytes, so the server can drop the entries a
    /// reader may not see (AUTH-10). Empty for a file that lists nothing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<ListingEntry>,
    /// Groups this whole file is for; empty is everyone (RX-73). A skill
    /// written for one group is a file, not a listing — there is nothing to
    /// filter inside it, so the decision is taken on the file.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<String>,
}

/// One route's bytes inside a listing.
///
/// The generator emits these as it writes the body, so the offsets cannot
/// drift from the text: a change to a heading level or a separator moves both
/// together. The alternative was three parsers in the server — `llms.txt` is
/// lines, `llms-full.txt` is sections, `sitemap.xml` is blocks — each of them
/// silently wrong the day its generator changed, and silent wrongness is the
/// failure mode this project keeps paying for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListingEntry {
    /// The route this span lists, or `None` for structural text — a section
    /// heading — which survives only if something under it does. Without that
    /// distinction, filtering every page out of a restricted section leaves
    /// `## Internal` standing on its own, which still names the section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    /// The section a span belongs to. A heading and the pages beneath it share
    /// one, and that is how a heading knows whether it has been emptied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    /// Byte offsets into the file, `start..end`, half open. Bytes outside
    /// every span — the preamble, the closing tag — are always kept.
    pub start: usize,
    pub end: usize,
}

impl ListingEntry {
    pub fn page(route: &str, section: Option<&str>, start: usize, end: usize) -> Self {
        Self {
            route: Some(route.to_owned()),
            section: section.map(str::to_owned),
            start,
            end,
        }
    }

    pub fn heading(section: &str, start: usize, end: usize) -> Self {
        Self {
            route: None,
            section: Some(section.to_owned()),
            start,
            end,
        }
    }
}

/// Whether a written file belongs in [`Manifest::served`].
///
/// **An allow list, and deliberately not "everything the build wrote except
/// the pages".** The build knows every path it wrote, so the short version of
/// defect 145's fix is to record that list — and it is a security regression,
/// because the list holds every page's rendered `index.html` and `.md`.
/// Serving those by file path walks straight past `groups::decide`, which is
/// taken on a `Target::Page` resolved from the route table: a reader would
/// fetch `/partners/pricing/index.html` and receive a page their groups
/// forbid.
///
/// So the rule names what is static rather than what is not a page. A new
/// kind of build output is unreachable until it is added here, which is the
/// right way round for a boundary: forgetting to add one costs a 404, and
/// forgetting to exclude one costs a leak.
pub fn is_servable(path: &str) -> bool {
    /// Whole subtrees that never contain a page.
    ///
    /// `openapi/` holds the processed spec documents, written for
    /// `Audience::public()` precisely so that serving them to anyone is
    /// right (API-50). Per-reader filtering of a spec is API-52 and would
    /// have to happen at request time, not by handing over this file.
    ///
    /// `search-index/` is deliberately NOT here. The index holds the text of
    /// every indexed page, restricted ones included, in binary shards that
    /// cannot be filtered per reader the way a listing can — and nothing
    /// fetches it: there is no client-side search and no `/_liyasa/search`
    /// route (defect 146). Serving it would be a leak in exchange for no
    /// feature. When the search endpoint exists it answers with filtered
    /// results rather than handing over the index.
    const DIRECTORIES: &[&str] = &["_liyasa/", ".well-known/", "openapi/"];
    /// Named files at the root. `404.html` is not here: it is served as the
    /// body of a 404 rather than at its own path, and `liyasa-manifest.json`,
    /// `_headers`, `vercel.json` and `.nojekyll` are the server's own index
    /// and the host's configuration.
    const FILES: &[&str] = &[
        "llms.txt",
        "llms-full.txt",
        "skill.md",
        "sitemap.xml",
        "robots.txt",
        "favicon.ico",
    ];
    let path = path.trim_start_matches('/');
    DIRECTORIES.iter().any(|dir| path.starts_with(dir)) || FILES.contains(&path)
}

/// The content type a served file answers with.
///
/// `assets::content_type` is the table for what an author puts in `assets/` —
/// images, fonts, archives — and it has no `css`, `js`, `xml` or `md` because
/// those are not asset types. These are build OUTPUT, so they get their own
/// mapping rather than widening that table, which would also change the
/// disposition and caching rules an authored file of the same extension gets.
/// Without this the theme's stylesheet served as `application/octet-stream`
/// and every browser ignored it, which looks exactly like the 404 it
/// replaced.
pub fn served_content_type(path: &str) -> String {
    let extension = path.rsplit('.').next().unwrap_or_default();
    match extension {
        "css" => "text/css; charset=utf-8".to_owned(),
        "js" | "mjs" => "text/javascript; charset=utf-8".to_owned(),
        "map" => "application/json".to_owned(),
        "xml" => "application/xml; charset=utf-8".to_owned(),
        "md" => crate::hosting::headers::MARKDOWN_TYPE.to_owned(),
        "wasm" => "application/wasm".to_owned(),
        _ => crate::assets::content_type(extension).to_owned(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteEntry {
    pub route: Route,
    /// The file the page was rendered from.
    pub source: String,
    /// `<route>.md`, the agent-facing Markdown.
    pub markdown: String,
    pub hidden: bool,
    /// Rendered per request rather than written as files (§6.6.4).
    pub dynamic: bool,
    pub variants: Vec<VariantEntry>,
    /// Who may see this page: the navigation ancestors that declared a
    /// restriction, root-first, and then the page itself as the last element
    /// (AUTH-07, AUTH-10, §7.6).
    ///
    /// The server has no navigation tree — the hierarchy exists only in the
    /// config, at build time — so the chain is resolved here and carried
    /// across. Without it `liyasa_server::auth::groups::decide` has no input
    /// and every restricted page serves to everyone.
    ///
    /// The last element is ALWAYS the page's own level, present even when it
    /// declares nothing, because `decide` reads the `access: public` flag off
    /// `chain.last()`. Ancestors that declare nothing are omitted; `decide`
    /// skips them anyway.
    pub access: Vec<AccessLevel>,
}

/// One level of a page's access chain.
///
/// Within a level any one group is enough; across levels every level that
/// declares groups must be satisfied. That is why this is a list and not a
/// single set — flattening `[staff]` and `[sre, oncall]` into one set turns
/// `staff AND (sre OR oncall)` into `staff OR sre OR oncall` (RFC 1503).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessLevel {
    /// `groups:` on the navigation node or in the page's front matter. Empty
    /// means this level places no restriction of its own.
    pub groups: Vec<String>,
    /// `access: public` (§7.6). Only a page sets this; a navigation node has
    /// no such key, so an ancestor's flag is always false.
    pub public: bool,
}

impl AccessLevel {
    /// Sorts and deduplicates as it builds: the manifest is part of the output
    /// a determinism check diffs, so two builds of one input must produce the
    /// same bytes here (§6.6.2 rule 5).
    pub fn new(groups: impl IntoIterator<Item = String>, public: bool) -> Self {
        let mut groups: Vec<String> = groups.into_iter().collect();
        groups.sort();
        groups.dedup();
        Self { groups, public }
    }

    /// Places no restriction, so `decide` would skip it. An ancestor like this
    /// is left out of the chain; the page's own level is kept regardless.
    pub fn is_open(&self) -> bool {
        self.groups.is_empty() && !self.public
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariantEntry {
    /// `variants::key`, the server's lookup key.
    pub key: String,
    /// Path under `dist/`.
    pub path: String,
    pub hash: Fingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetEntry {
    pub source: String,
    pub path: String,
    pub url: String,
    pub hash: Fingerprint,
    pub content_type: String,
    pub disposition: Disposition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageEntry {
    pub source: String,
    pub original_url: String,
    pub variants: Vec<ImageVariantEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageVariantEntry {
    pub key: Fingerprint,
    pub url: String,
    pub width: u32,
    pub format: crate::images::Format,
}

impl Manifest {
    pub fn route(&self, route: &Route) -> Option<&RouteEntry> {
        self.routes.iter().find(|entry| &entry.route == route)
    }

    /// Sorts everything the manifest holds, which is what makes two builds of
    /// the same inputs byte-identical.
    pub fn sorted(mut self) -> Self {
        self.routes.sort_by(|a, b| a.route.cmp(&b.route));
        for route in &mut self.routes {
            route.variants.sort_by(|a, b| a.key.cmp(&b.key));
            // The groups within a level sort; the levels themselves do NOT.
            // Their order is the access semantics — root-first, page last —
            // and `decide` reads the page's `public` flag off the last one.
            for level in &mut route.access {
                level.groups.sort();
                level.groups.dedup();
            }
        }
        self.assets.sort_by(|a, b| a.source.cmp(&b.source));
        self.images.sort_by(|a, b| a.source.cmp(&b.source));
        for image in &mut self.images {
            image
                .variants
                .sort_by_key(|entry| (entry.width, entry.format));
        }
        self.redirects.sort_by(|a, b| a.source.cmp(&b.source));
        self.served.sort_by(|a, b| a.path.cmp(&b.path));
        self
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_owned())
    }

    pub fn parse(text: &str) -> Option<Self> {
        serde_json::from_str(text).ok()
    }
}

/// `hash(all input fingerprints, build clock, liyasa version, lockfile)`
/// (§6.6.2).
pub fn build_id(
    inputs: &BTreeMap<String, Fingerprint>,
    clock_unix: i64,
    lockfile: Option<Fingerprint>,
) -> BuildId {
    let mut parts: Vec<Vec<u8>> = Vec::with_capacity(inputs.len() * 2 + 3);
    for (path, fingerprint) in inputs {
        parts.push(path.as_bytes().to_vec());
        parts.push(fingerprint.0.to_vec());
    }
    parts.push(clock_unix.to_le_bytes().to_vec());
    parts.push(crate::cache::VERSION.as_bytes().to_vec());
    parts.push(lockfile.map(|fp| fp.0.to_vec()).unwrap_or_default());
    BuildId(Fingerprint::of_parts(parts.iter().map(Vec::as_slice)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> Manifest {
        Manifest {
            build_id: build_id(&BTreeMap::new(), 0, None),
            liyasa_version: crate::cache::VERSION.to_owned(),
            built_at: 1_789_473_600,
            base_path: String::new(),
            routes: vec![
                RouteEntry {
                    route: Route::new("/guides/install"),
                    source: "guides/install.md".to_owned(),
                    markdown: "/guides/install.md".to_owned(),
                    hidden: false,
                    dynamic: false,
                    variants: vec![
                        VariantEntry {
                            key: "g=admin".to_owned(),
                            path: "guides/install/index.g-admin.html".to_owned(),
                            hash: Fingerprint::of("b"),
                        },
                        VariantEntry {
                            key: "default".to_owned(),
                            path: "guides/install/index.html".to_owned(),
                            hash: Fingerprint::of("a"),
                        },
                    ],
                    // A navigation ancestor restricts it, and the page itself
                    // declares nothing — the shape the `access` docs describe.
                    access: vec![
                        AccessLevel::new(["staff".to_owned()], false),
                        AccessLevel::new([], false),
                    ],
                },
                RouteEntry {
                    route: Route::new("/"),
                    source: "index.md".to_owned(),
                    markdown: "/index.md".to_owned(),
                    hidden: false,
                    dynamic: false,
                    variants: Vec::new(),
                    access: vec![AccessLevel::new([], false)],
                },
            ],
            assets: Vec::new(),
            images: Vec::new(),
            redirects: Vec::new(),
            served: Vec::new(),
            inputs: BTreeMap::new(),
        }
    }

    #[test]
    fn sorting_puts_routes_and_variants_in_one_order() {
        let sorted = manifest().sorted();
        assert_eq!(sorted.routes[0].route.as_str(), "/");
        assert_eq!(sorted.routes[1].variants[0].key, "default");
    }

    #[test]
    fn a_manifest_round_trips() {
        let manifest = manifest().sorted();
        let back = Manifest::parse(&manifest.to_json()).expect("the manifest parses");
        assert_eq!(back, manifest);
        assert!(back.route(&Route::new("/")).is_some());
    }

    #[test]
    fn a_manifest_without_an_access_chain_is_not_a_manifest() {
        // `access` is deliberately not `#[serde(default)]`. A defaulted empty
        // chain parses cleanly and then serves every restricted page to
        // everyone, so a bundle from before the field existed has to fail
        // loudly rather than fail open. Nothing legitimate produces one.
        let mut value: serde_json::Value =
            serde_json::from_str(&manifest().sorted().to_json()).expect("the manifest is JSON");
        for route in value["routes"].as_array_mut().expect("routes") {
            route.as_object_mut().expect("an object").remove("access");
        }
        assert!(
            Manifest::parse(&value.to_string()).is_none(),
            "a manifest with no access chain must not parse"
        );
    }

    #[test]
    fn sorting_orders_the_groups_within_a_level_and_never_the_levels() {
        let mut manifest = manifest();
        // Built by hand rather than through `AccessLevel::new`, which sorts:
        // the point is what `sorted()` does to a manifest assembled anywhere
        // else, since that is what the determinism check diffs.
        manifest.routes[0].access = vec![
            AccessLevel {
                groups: vec!["sre".to_owned(), "admin".to_owned()],
                public: false,
            },
            AccessLevel {
                groups: vec!["zeta".to_owned(), "alpha".to_owned()],
                public: false,
            },
        ];
        let sorted = manifest.sorted();
        let entry = sorted
            .route(&Route::new("/guides/install"))
            .expect("the route survived sorting");
        assert_eq!(entry.access[0].groups, ["admin", "sre"]);
        assert_eq!(
            entry.access[1].groups,
            ["alpha", "zeta"],
            "the levels keep their order — it is the access semantics, and \
             `decide` reads the page's own flag off the last one"
        );
    }

    #[test]
    fn the_build_id_covers_the_inputs_the_clock_and_the_lockfile() {
        let mut inputs = BTreeMap::new();
        inputs.insert("index.md".to_owned(), Fingerprint::of("one"));
        let base = build_id(&inputs, 10, None);
        assert_ne!(base, build_id(&inputs, 11, None));
        assert_ne!(base, build_id(&inputs, 10, Some(Fingerprint::of("lock"))));

        let mut moved = BTreeMap::new();
        moved.insert("index.md".to_owned(), Fingerprint::of("two"));
        assert_ne!(base, build_id(&moved, 10, None));
        assert_eq!(base, build_id(&inputs, 10, None));
    }
}
