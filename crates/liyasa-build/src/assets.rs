//! The asset pipeline (PRD §7.9 CM-84, §11.1 RX-02).
//!
//! Everything under `assets/` and every file a page links to with a non-page
//! extension is copied to `dist/` at a stable URL. Hashing is optional and
//! configured once for the whole site; `Content-Disposition` is configured per
//! extension. Nothing here encodes an image — that is the lazy tier in
//! [`crate::images`].

use std::collections::BTreeMap;

use liyasa_core::ids::Fingerprint;
use liyasa_core::vfs::VfsPath;
use serde::{Deserialize, Serialize};

/// `build.hashing`. Absent in config means [`Hashing::None`]
/// (`plan/rfcs/0601-config-keys-the-build-needs.md`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Hashing {
    /// The URL is the path: caching is the host's problem, and a rebuild does
    /// not churn every link.
    #[default]
    None,
    /// `/assets/manual.pdf?v=<digest>`: one file on disk, a new URL per change.
    Query,
    /// `/assets/manual.<digest>.pdf`: immutable, which is what a CDN wants.
    Filename,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Disposition {
    Inline,
    Attachment,
}

/// Types served as downloads unless config says otherwise (RFC 0601 item 4).
const ALWAYS_DOWNLOAD: &[&str] = &["zip", "gz", "tgz", "tar", "7z", "exe", "dmg", "msi"];

/// Enough of a MIME table for what documentation links to; anything else is
/// served as bytes and sniffing is refused by the header block.
const CONTENT_TYPES: &[(&str, &str)] = &[
    ("avif", "image/avif"),
    ("csv", "text/csv"),
    ("gif", "image/gif"),
    ("gz", "application/gzip"),
    ("ico", "image/x-icon"),
    ("jpeg", "image/jpeg"),
    ("jpg", "image/jpeg"),
    ("json", "application/json"),
    ("mp3", "audio/mpeg"),
    ("mp4", "video/mp4"),
    ("pdf", "application/pdf"),
    ("png", "image/png"),
    ("svg", "image/svg+xml"),
    ("txt", "text/plain; charset=utf-8"),
    ("webm", "video/webm"),
    ("webp", "image/webp"),
    ("woff2", "font/woff2"),
    ("yaml", "application/yaml"),
    ("zip", "application/zip"),
];

pub const DEFAULT_CONTENT_TYPE: &str = "application/octet-stream";

/// Bytes of the digest that reach a URL. 64 bits is far past collision risk for
/// one site's assets and keeps a link readable.
const DIGEST_HEX: usize = 16;

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub hashing: Hashing,
    /// `build.downloads`, keyed by extension without the dot.
    pub downloads: BTreeMap<String, Disposition>,
    /// `build.basePath`, prefixed to every URL and to nothing on disk.
    pub base_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    pub source: VfsPath,
    /// Where the file lands under `dist/`.
    pub output: String,
    /// What a page links to.
    pub url: String,
    pub fingerprint: Fingerprint,
    pub content_type: &'static str,
    pub disposition: Disposition,
}

#[derive(Debug, Clone, Default)]
pub struct Plan {
    assets: Vec<Asset>,
}

impl Plan {
    pub fn entries(&self) -> &[Asset] {
        &self.assets
    }

    pub fn by_source(&self, path: &str) -> Option<&Asset> {
        self.assets
            .iter()
            .find(|asset| asset.source.as_str() == path)
    }

    /// The URL to write into a page for a file it links to.
    pub fn url_of(&self, path: &VfsPath) -> Option<&str> {
        self.assets
            .iter()
            .find(|asset| &asset.source == path)
            .map(|asset| asset.url.as_str())
    }

    pub fn is_empty(&self) -> bool {
        self.assets.is_empty()
    }
}

/// Plans the copy for everything under `assets/` and everything a page linked
/// to. A path in both lists is copied once.
pub fn plan(
    assets: &[(VfsPath, Fingerprint)],
    referenced: &[(VfsPath, Fingerprint)],
    options: &Options,
) -> Plan {
    let mut seen: BTreeMap<VfsPath, Fingerprint> = BTreeMap::new();
    for (path, fingerprint) in assets.iter().chain(referenced) {
        seen.insert(path.clone(), *fingerprint);
    }

    let assets = seen
        .into_iter()
        .map(|(source, fingerprint)| {
            let extension = source.extension().unwrap_or_default().to_ascii_lowercase();
            let output = output_path(&source, fingerprint, options.hashing);
            Asset {
                url: url_of(&output, fingerprint, options),
                output,
                content_type: content_type(&extension),
                disposition: disposition(&extension, &options.downloads),
                source,
                fingerprint,
            }
        })
        .collect();
    Plan { assets }
}

/// `dist/_headers`: one block per asset that needs one. Every block refuses
/// content sniffing (CM-131's rule, applied to built assets as well).
pub fn headers(plan: &Plan) -> String {
    let mut out = String::new();
    for asset in plan.entries() {
        out.push_str(&asset.url);
        out.push('\n');
        if asset.disposition == Disposition::Attachment {
            out.push_str("  Content-Disposition: attachment\n");
        }
        out.push_str("  X-Content-Type-Options: nosniff\n");
    }
    out
}

pub fn content_type(extension: &str) -> &'static str {
    CONTENT_TYPES
        .iter()
        .find(|(known, _)| *known == extension)
        .map(|(_, mime)| *mime)
        .unwrap_or(DEFAULT_CONTENT_TYPE)
}

fn disposition(extension: &str, downloads: &BTreeMap<String, Disposition>) -> Disposition {
    if let Some(configured) = downloads.get(extension) {
        return *configured;
    }
    if ALWAYS_DOWNLOAD.contains(&extension) {
        return Disposition::Attachment;
    }
    Disposition::Inline
}

fn output_path(source: &VfsPath, fingerprint: Fingerprint, hashing: Hashing) -> String {
    let path = source.as_str();
    if hashing != Hashing::Filename {
        return path.to_owned();
    }
    let digest = digest(fingerprint);
    match path.rsplit_once('.') {
        Some((stem, extension)) => format!("{stem}.{digest}.{extension}"),
        None => format!("{path}.{digest}"),
    }
}

fn url_of(output: &str, fingerprint: Fingerprint, options: &Options) -> String {
    let base = options.base_path.trim_end_matches('/');
    let url = format!("{base}/{output}");
    match options.hashing {
        Hashing::Query => format!("{url}?v={}", digest(fingerprint)),
        Hashing::None | Hashing::Filename => url,
    }
}

fn digest(fingerprint: Fingerprint) -> String {
    fingerprint.to_hex()[..DIGEST_HEX].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(path: &str, hashing: Hashing) -> Asset {
        let files = [(VfsPath::new(path), Fingerprint::of("bytes"))];
        plan(
            &files,
            &[],
            &Options {
                hashing,
                ..Options::default()
            },
        )
        .entries()
        .first()
        .cloned()
        .expect("one asset")
    }

    #[test]
    fn a_file_with_no_extension_still_gets_a_url() {
        let asset = one("assets/LICENSE", Hashing::None);
        assert_eq!(asset.url, "/assets/LICENSE");
        assert_eq!(asset.content_type, DEFAULT_CONTENT_TYPE);
    }

    #[test]
    fn filename_hashing_of_a_file_with_no_extension_appends_the_digest() {
        let asset = one("assets/LICENSE", Hashing::Filename);
        assert!(
            asset.output.starts_with("assets/LICENSE."),
            "{}",
            asset.output
        );
    }

    #[test]
    fn an_extension_is_matched_case_insensitively() {
        let asset = one("assets/MANUAL.PDF", Hashing::None);
        assert_eq!(asset.content_type, "application/pdf");
    }

    #[test]
    fn a_file_listed_twice_is_copied_once() {
        let files = [(VfsPath::new("assets/a.png"), Fingerprint::of("bytes"))];
        let plan = plan(&files, &files, &Options::default());
        assert_eq!(plan.entries().len(), 1);
    }

    #[test]
    fn changing_the_bytes_changes_a_hashed_url() {
        let path = VfsPath::new("assets/a.png");
        let options = Options {
            hashing: Hashing::Filename,
            ..Options::default()
        };
        let before = plan(&[(path.clone(), Fingerprint::of("one"))], &[], &options);
        let after = plan(&[(path.clone(), Fingerprint::of("two"))], &[], &options);
        assert_ne!(before.url_of(&path), after.url_of(&path));
    }

    #[test]
    fn a_base_path_with_a_trailing_slash_does_not_double_it() {
        let files = [(VfsPath::new("assets/a.png"), Fingerprint::of("b"))];
        let plan = plan(
            &files,
            &[],
            &Options {
                base_path: "/docs/".to_owned(),
                ..Options::default()
            },
        );
        assert_eq!(plan.entries()[0].url, "/docs/assets/a.png");
    }
}
