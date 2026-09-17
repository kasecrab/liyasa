//! Static hosting output (PRD §18.1 HOST-01; RX-13, RX-110, RX-111, RX-112).
//!
//! One policy, several spellings. The security and cache headers are computed
//! once from the build and written as `_headers` (Cloudflare Pages, Netlify)
//! and `vercel.json` (Vercel), the redirects as `_redirects` and inside
//! `vercel.json`; hosts that read none of them are listed in [`matrix`] with
//! what they send on their own. `.nojekyll` goes out with them, because a host
//! that reads none of the others still has an opinion about which files it
//! publishes at all. The 404 body is `404.html`, which
//! [`crate::agents::notfound`] writes and every listed host serves by name.
//!
//! The engine calls [`generate`] after the manifest exists and [`write`] with
//! the output directory; the build nonce it renders pages with is
//! [`build_nonce`] of the build ID, so header and markup agree
//! (`plan/rfcs/1200-hosting-engine-seam.md`).

pub mod csp;
pub mod digest;
pub mod emulate;
pub mod fallback;
pub mod headers;
pub mod integrations;
pub mod matrix;
pub mod redirects;
pub mod vercel;

use std::path::Path;

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::ids::{BuildId, Route};
use serde_json::Value;

pub use csp::Policy;
pub use headers::Rules;
pub use redirects::Redirect;

use crate::manifest::Manifest;

pub const HEADERS_FILE: &str = "_headers";
/// GitHub Pages runs Jekyll over an uploaded site unless this empty file sits
/// at its root, and Jekyll publishes nothing whose name begins with `_` or
/// `.` — which is the theme, the image variants, and the agent surfaces. It is
/// written on every build rather than for one host: it is inert everywhere
/// else, and a `dist/` is uploaded by hand as often as by a configured deploy
/// (`plan/rfcs/1202-nojekyll-is-unconditional.md`).
pub const NOJEKYLL_FILE: &str = ".nojekyll";
pub const REDIRECTS_FILE: &str = "_redirects";
pub const VERCEL_FILE: &str = "vercel.json";

/// What the host files are computed from.
#[derive(Debug, Clone, Copy)]
pub struct Inputs<'a> {
    /// `liyasa.json` after overlays.
    pub config: &'a Value,
    pub manifest: &'a Manifest,
    /// The theme's inline critical block, byte for byte as the `<style>`
    /// element carries it.
    pub critical_css: &'a str,
    /// Routes rendered in `frame` mode (RX-112).
    pub frame_routes: &'a [Route],
    /// Remote hosts content loads images and media from (CM-35).
    pub image_hosts: &'a [String],
    pub media_hosts: &'a [String],
    /// Submit the site to the HSTS preload list. Opt-in and not a config key
    /// yet (RFC 1200).
    pub hsts_preload: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    /// Relative to the output directory.
    pub path: String,
    pub contents: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub policy: Policy,
    pub rules: Rules,
    pub redirects: Vec<Redirect>,
    pub files: Vec<File>,
    pub diagnostics: Diagnostics,
}

impl Output {
    pub fn file(&self, path: &str) -> Option<&str> {
        self.files
            .iter()
            .find(|file| file.path == path)
            .map(|file| file.contents.as_str())
    }
}

/// The nonce of every cacheable page of one build: 144 bits of the build ID,
/// Base64. It is the same in the header and in every `<script nonce>` the
/// theme renders, so a `304` that re-sends the header still matches the
/// cached body.
pub fn build_nonce(build_id: &BuildId) -> String {
    digest::base64(&build_id.0.0[..18])
}

pub fn generate(inputs: &Inputs<'_>) -> Output {
    let mut diagnostics = Diagnostics::new();
    let base_path = inputs.manifest.base_path.as_str();

    let mut policy = Policy::new(&build_nonce(&inputs.manifest.build_id), inputs.critical_css);
    diagnostics.extend(policy.extend_from_config(inputs.config).into_vec());
    diagnostics.extend(
        policy
            .add_content_hosts(
                inputs.image_hosts.iter().map(String::as_str),
                inputs.media_hosts.iter().map(String::as_str),
                inputs.config,
            )
            .into_vec(),
    );

    let hashed_assets = inputs
        .config
        .get("build")
        .and_then(|build| build.get("hashing"))
        .and_then(Value::as_str)
        == Some("filename");
    let rules = headers::rules(&headers::Options {
        policy: &policy,
        base_path,
        hsts_preload: inputs.hsts_preload,
        hashed_assets,
        frame_routes: inputs.frame_routes,
    });
    let redirects = redirects::from_manifest(&inputs.manifest.redirects, base_path);

    let mut files = vec![
        File {
            path: NOJEKYLL_FILE.to_owned(),
            contents: String::new(),
        },
        File {
            path: HEADERS_FILE.to_owned(),
            contents: rules.render(),
        },
        File {
            path: VERCEL_FILE.to_owned(),
            contents: vercel::render(&rules, &redirects),
        },
    ];
    if !redirects.is_empty() {
        files.push(File {
            path: REDIRECTS_FILE.to_owned(),
            contents: redirects::netlify(&redirects),
        });
    }

    Output {
        policy,
        rules,
        redirects,
        files,
        diagnostics,
    }
}

/// Writes every file under `dist`. A file that cannot be written is `E0002`,
/// the same code the engine gives any output it cannot place.
pub fn write(output: &Output, dist: &Path) -> Diagnostics {
    let mut diagnostics = Diagnostics::new();
    for file in &output.files {
        let path = dist.join(&file.path);
        if let Some(parent) = path.parent()
            && let Err(error) = std::fs::create_dir_all(parent)
        {
            diagnostics.push(Diagnostic::new(
                code::E0002,
                format!("could not create {}: {error}", parent.display()),
            ));
            continue;
        }
        if let Err(error) = std::fs::write(&path, &file.contents) {
            diagnostics.push(Diagnostic::new(
                code::E0002,
                format!("could not write {}: {error}", path.display()),
            ));
        }
    }
    diagnostics
}

#[cfg(test)]
pub(crate) mod fixture {
    //! A manifest small enough to read, for the tests of every submodule.

    use std::collections::BTreeMap;

    use liyasa_core::ids::{Fingerprint, Route};

    use crate::manifest::{Manifest, RouteEntry, VariantEntry};
    use crate::redirects::ManifestEntry;

    pub fn route(route: &str) -> RouteEntry {
        let trimmed = route.trim_matches('/');
        let (source, markdown, path) = match trimmed.is_empty() {
            true => (
                "index.md".to_owned(),
                "/index.md".to_owned(),
                "index.html".to_owned(),
            ),
            false => (
                format!("{trimmed}.md"),
                format!("/{trimmed}.md"),
                format!("{trimmed}/index.html"),
            ),
        };
        RouteEntry {
            route: Route::new(route),
            source,
            markdown,
            hidden: false,
            dynamic: false,
            variants: vec![VariantEntry {
                key: "default".to_owned(),
                path,
                hash: Fingerprint::of(route),
            }],
        }
    }

    pub fn manifest(base_path: &str) -> Manifest {
        Manifest {
            build_id: crate::manifest::build_id(&BTreeMap::new(), 1_789_473_600, None),
            liyasa_version: crate::cache::VERSION.to_owned(),
            built_at: 1_789_473_600,
            base_path: base_path.to_owned(),
            routes: vec![route("/"), route("/guides/install"), route("/embed/widget")],
            assets: Vec::new(),
            images: Vec::new(),
            redirects: vec![ManifestEntry {
                source: "/old".to_owned(),
                destination: "/guides/install".to_owned(),
                status: 301,
            }],
            inputs: BTreeMap::new(),
        }
        .sorted()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn inputs<'a>(config: &'a Value, manifest: &'a Manifest) -> Inputs<'a> {
        Inputs {
            config,
            manifest,
            critical_css: ":root{--x:1}",
            frame_routes: &[],
            image_hosts: &[],
            media_hosts: &[],
            hsts_preload: false,
        }
    }

    #[test]
    fn the_nonce_is_stable_per_build_and_differs_between_builds() {
        let a = fixture::manifest("");
        let nonce = build_nonce(&a.build_id);
        assert_eq!(nonce.len(), 24);
        assert!(!nonce.contains('='));
        assert_eq!(nonce, build_nonce(&a.build_id));
        let other = crate::manifest::build_id(&std::collections::BTreeMap::new(), 1, None);
        assert_ne!(nonce, build_nonce(&other));
    }

    #[test]
    fn the_header_and_the_markup_share_one_nonce() {
        let config = json!({});
        let manifest = fixture::manifest("");
        let output = generate(&inputs(&config, &manifest));
        let expected = format!("'nonce-{}'", build_nonce(&manifest.build_id));
        assert!(output.policy.header().contains(&expected));
        assert!(
            output
                .file(HEADERS_FILE)
                .expect("_headers")
                .contains(&expected)
        );
        assert!(
            output
                .file(VERCEL_FILE)
                .expect("vercel.json")
                .contains(&expected)
        );
    }

    #[test]
    fn every_host_file_is_written_and_redirects_only_when_there_are_any() {
        let config = json!({});
        let manifest = fixture::manifest("");
        let output = generate(&inputs(&config, &manifest));
        let paths: Vec<&str> = output.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            [NOJEKYLL_FILE, HEADERS_FILE, VERCEL_FILE, REDIRECTS_FILE]
        );
        assert_eq!(output.file(NOJEKYLL_FILE), Some(""));
        assert_eq!(
            output.file(REDIRECTS_FILE),
            Some("/old /guides/install 301\n")
        );

        let mut empty = fixture::manifest("");
        empty.redirects.clear();
        let output = generate(&inputs(&config, &empty));
        assert!(output.file(REDIRECTS_FILE).is_none());
        assert!(output.file(VERCEL_FILE).is_some());
    }

    #[test]
    fn the_base_path_reaches_every_file() {
        let config = json!({});
        let manifest = fixture::manifest("/docs");
        let output = generate(&inputs(&config, &manifest));
        let headers = output.file(HEADERS_FILE).expect("_headers");
        assert!(headers.starts_with("/docs/*\n"));
        assert!(headers.contains("\n/docs/_liyasa/*\n"));
        assert_eq!(
            output.file(REDIRECTS_FILE),
            Some("/docs/old /docs/guides/install 301\n")
        );
        let vercel: Value =
            serde_json::from_str(output.file(VERCEL_FILE).expect("vercel.json")).expect("JSON");
        assert_eq!(vercel["headers"][0]["source"], "/docs/(.*)");
        assert_eq!(vercel["redirects"][0]["source"], "/docs/old");
    }

    #[test]
    fn config_diagnostics_surface_and_hashing_switches_the_asset_rule() {
        let config = json!({
            "build": { "hashing": "filename" },
            "security": { "csp": { "extraScriptSrc": ["bad host"] } }
        });
        let manifest = fixture::manifest("");
        let output = generate(&inputs(&config, &manifest));
        assert!(output.diagnostics.has_errors());
        assert!(
            output
                .file(HEADERS_FILE)
                .expect("_headers")
                .contains("\n/assets/*\n")
        );
    }

    #[test]
    fn write_puts_the_files_under_dist() {
        let config = json!({});
        let manifest = fixture::manifest("");
        let output = generate(&inputs(&config, &manifest));
        let dist = std::env::temp_dir().join(format!("liyasa-hosting-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dist);
        let diagnostics = write(&output, &dist);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(
            std::fs::read_to_string(dist.join(HEADERS_FILE)).expect("_headers"),
            output.file(HEADERS_FILE).expect("_headers")
        );
        let _ = std::fs::remove_dir_all(&dist);
    }
}
