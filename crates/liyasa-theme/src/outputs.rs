//! What a build writes, and which of it the theme owns (RX-03).
//!
//! `liyasa-build` writes `dist/`; this is the inventory it writes against, kept
//! here because the theme is what decides the shape of the HTML, the 404 page,
//! and the asset URLs the rest of the output points at.

use serde::{Deserialize, Serialize};

/// Who produces an output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Producer {
    /// Rendered by `liyasa-theme`.
    Theme,
    /// Written by `liyasa-build` from the content and the config.
    Build,
    /// Written by `liyasa-search`.
    Search,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Output {
    /// The path in `dist/`, with `{route}` where a route is substituted.
    pub path: &'static str,
    pub producer: Producer,
    pub doc: &'static str,
}

/// Every file RX-03 requires in `dist/`.
pub const OUTPUTS: &[Output] = &[
    Output {
        path: "{route}/index.html",
        producer: Producer::Theme,
        doc: "One pre-rendered page per route and variant (RX-01).",
    },
    Output {
        path: "{route}.md",
        producer: Producer::Build,
        doc: "The page's Markdown, which the page actions link to (RX-100).",
    },
    Output {
        path: "{route}/index.md",
        producer: Producer::Build,
        doc: "The same Markdown under the directory form of the route.",
    },
    Output {
        path: "404.html",
        producer: Producer::Theme,
        doc: "The error page (CFG-71); the host or the server returns it with status 404.",
    },
    Output {
        path: "sitemap.xml",
        producer: Producer::Build,
        doc: "Every public route.",
    },
    Output {
        path: "robots.txt",
        producer: Producer::Build,
        doc: "Crawler policy, including the agent user agents (§24).",
    },
    Output {
        path: "llms.txt",
        producer: Producer::Build,
        doc: "The agent index (§11.8).",
    },
    Output {
        path: "llms-full.txt",
        producer: Producer::Build,
        doc: "The whole corpus as one document (§11.8).",
    },
    Output {
        path: "_llms/",
        producer: Producer::Build,
        doc: "Split indexes for large sites (§11.8).",
    },
    Output {
        path: "skill.md",
        producer: Producer::Build,
        doc: "The site as a skill (§24.3).",
    },
    Output {
        path: ".well-known/",
        producer: Producer::Build,
        doc: "Agent discovery files (§24.3).",
    },
    Output {
        path: "feed.xml",
        producer: Producer::Build,
        doc: "The changelog feed (§11.9).",
    },
    Output {
        path: "search-index/",
        producer: Producer::Search,
        doc: "The browser index (§12.2).",
    },
    Output {
        path: "liyasa-manifest.json",
        producer: Producer::Build,
        doc: "What was built, from what (§34.12).",
    },
    Output {
        path: "_redirects",
        producer: Producer::Build,
        doc: "Redirects for static hosts.",
    },
    Output {
        path: "_headers",
        producer: Producer::Build,
        doc: "Cache and security headers, including the CSP (RX-110).",
    },
];

/// The theme's own assets, which every page's `<head>` points at.
pub const ASSETS: &[Output] = &[
    Output {
        path: "_liyasa/theme.{hash}.css",
        producer: Producer::Theme,
        doc: "The one cached stylesheet (THM-30).",
    },
    Output {
        path: "_liyasa/theme.{hash}.js",
        producer: Producer::Theme,
        doc: "The base runtime bundle (THM-31).",
    },
    Output {
        path: "_liyasa/assistant.{hash}.js",
        producer: Producer::Theme,
        doc: "The assistant panel, loaded on demand (THM-31).",
    },
    Output {
        path: "og/{route}.png",
        producer: Producer::Theme,
        doc: "The generated Open Graph card (CFG-73), rasterized in the lazy image tier.",
    },
];

pub fn theme_owned() -> Vec<&'static Output> {
    OUTPUTS
        .iter()
        .chain(ASSETS)
        .filter(|output| output.producer == Producer::Theme)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_file_rx_03_lists_is_in_the_inventory() {
        for required in [
            "sitemap.xml",
            "robots.txt",
            "llms.txt",
            "llms-full.txt",
            "_llms/",
            "skill.md",
            ".well-known/",
            "feed.xml",
            "search-index/",
            "liyasa-manifest.json",
            "_redirects",
            "_headers",
        ] {
            assert!(
                OUTPUTS.iter().any(|output| output.path == required),
                "`{required}` is in RX-03 but not in the inventory"
            );
        }
        assert!(OUTPUTS.iter().any(|output| output.path.ends_with(".html")));
        assert!(OUTPUTS.iter().any(|output| output.path.ends_with(".md")));
    }

    #[test]
    fn the_theme_owns_the_html_the_error_page_and_its_assets() {
        let owned: Vec<&str> = theme_owned().iter().map(|output| output.path).collect();
        assert!(owned.contains(&"{route}/index.html"));
        assert!(owned.contains(&"404.html"));
        assert!(owned.contains(&"_liyasa/theme.{hash}.css"));
        assert!(owned.contains(&"_liyasa/theme.{hash}.js"));
    }

    #[test]
    fn every_entry_is_documented_and_unique() {
        let mut paths: Vec<&str> = OUTPUTS
            .iter()
            .chain(ASSETS)
            .map(|output| {
                assert!(
                    !output.doc.is_empty(),
                    "`{}` has no documentation",
                    output.path
                );
                output.path
            })
            .collect();
        let count = paths.len();
        paths.sort_unstable();
        paths.dedup();
        assert_eq!(paths.len(), count, "an output is listed twice");
    }
}
