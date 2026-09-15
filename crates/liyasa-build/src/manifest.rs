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
    /// What the build ID was computed over, so a rebuild can say what moved.
    pub inputs: BTreeMap<String, Fingerprint>,
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
        }
        self.assets.sort_by(|a, b| a.source.cmp(&b.source));
        self.images.sort_by(|a, b| a.source.cmp(&b.source));
        for image in &mut self.images {
            image
                .variants
                .sort_by_key(|entry| (entry.width, entry.format));
        }
        self.redirects.sort_by(|a, b| a.source.cmp(&b.source));
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
                },
                RouteEntry {
                    route: Route::new("/"),
                    source: "index.md".to_owned(),
                    markdown: "/index.md".to_owned(),
                    hidden: false,
                    dynamic: false,
                    variants: Vec::new(),
                },
            ],
            assets: Vec::new(),
            images: Vec::new(),
            redirects: Vec::new(),
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
