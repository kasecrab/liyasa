//! Redirect rules as the host files spell them, under the base path
//! (CM-82, RX-111).
//!
//! The compiled table in [`crate::redirects`] is site-relative. A site hosted
//! under `/docs` needs `/docs/old` to send readers to `/docs/new`, so the
//! prefix is applied here, once, for every host spelling.

use crate::redirects::ManifestEntry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redirect {
    pub source: String,
    pub destination: String,
    pub status: u16,
}

pub fn from_manifest(entries: &[ManifestEntry], base_path: &str) -> Vec<Redirect> {
    entries
        .iter()
        .map(|entry| Redirect {
            source: prefixed(&entry.source, base_path),
            destination: prefixed(&entry.destination, base_path),
            status: entry.status,
        })
        .collect()
}

/// `dist/_redirects`: source, destination, status, one rule per line, first
/// match wins on both hosts that read it.
pub fn netlify(redirects: &[Redirect]) -> String {
    let mut out = String::new();
    for redirect in redirects {
        out.push_str(&format!(
            "{} {} {}\n",
            redirect.source, redirect.destination, redirect.status
        ));
    }
    out
}

/// A site path under the base path; an absolute URL is left alone.
pub fn prefixed(path: &str, base_path: &str) -> String {
    let base = base_path.trim_end_matches('/');
    if base.is_empty() || path.contains("://") || path.starts_with("//") {
        return path.to_owned();
    }
    if path == "/" {
        return format!("{base}/");
    }
    if path.starts_with(base) && path[base.len()..].starts_with('/') {
        return path.to_owned();
    }
    format!("{base}{path}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(source: &str, destination: &str) -> ManifestEntry {
        ManifestEntry {
            source: source.to_owned(),
            destination: destination.to_owned(),
            status: 301,
        }
    }

    #[test]
    fn the_base_path_prefixes_paths_and_leaves_urls_alone() {
        let redirects = from_manifest(
            &[
                entry("/old", "/new"),
                entry("/gone", "https://status.acme.com/"),
                entry("/v1/*", "/v2/:splat"),
            ],
            "/docs/",
        );
        assert_eq!(
            netlify(&redirects),
            "/docs/old /docs/new 301\n/docs/gone https://status.acme.com/ 301\n/docs/v1/* /docs/v2/:splat 301\n"
        );
        assert_eq!(prefixed("/docs/already", "/docs"), "/docs/already");
        assert_eq!(prefixed("/documents", "/docs"), "/docs/documents");
        assert_eq!(prefixed("/", "/docs"), "/docs/");
        assert_eq!(prefixed("/x", ""), "/x");
    }
}
