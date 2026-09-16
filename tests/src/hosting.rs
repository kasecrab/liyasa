//! A project built by the engine and given its host files, the way
//! `liyasa build` does once the engine calls `hosting::generate`
//! (`plan/rfcs/1200-hosting-engine-seam.md`). The tests of RX-13, RX-110,
//! RX-111, RX-112, and HOST-01 all start from this upload.

use std::fs;
use std::path::{Path, PathBuf};

use liyasa_build::engine::{self, Options, Report};
use liyasa_build::git::NoGit;
use liyasa_build::hosting::{self, Inputs, Output};
use liyasa_config::vfs::OsVfs;
use liyasa_core::ids::Route;
use liyasa_core::source_map::SourceMap;
use liyasa_core::vfs::VfsPath;

/// The fixture's `liyasa.json`: a redirect, a frame-ancestors list, an extra
/// image host, and one analytics integration.
pub const CONFIG: &str = r#"{
  "name": "Acme docs",
  "description": "How Acme works",
  "seo": { "canonicalOrigin": "https://docs.acme.com" },
  "redirects": { "rules": [{ "source": "/old", "destination": "/guides/install" }] },
  "security": {
    "frameAncestors": ["https://app.acme.com"],
    "csp": { "extraImgSrc": ["images.acme.com"] }
  },
  "integrations": { "plausible": { "id": "docs.acme.com", "consent": "none" } }
}"#;

/// The same without the integration, for the "adds exactly its sources" check.
pub const CONFIG_BARE: &str = r#"{
  "name": "Acme docs",
  "description": "How Acme works",
  "seo": { "canonicalOrigin": "https://docs.acme.com" },
  "redirects": { "rules": [{ "source": "/old", "destination": "/guides/install" }] },
  "security": {
    "frameAncestors": ["https://app.acme.com"],
    "csp": { "extraImgSrc": ["images.acme.com"] }
  }
}"#;

/// The route rendered in `frame` mode.
pub const FRAME_ROUTE: &str = "/embed/widget";

pub struct Site {
    root: PathBuf,
    pub report: Report,
    pub output: Output,
    /// `liyasa.json` after overlays, as the engine read it.
    pub config: serde_json::Value,
}

impl Site {
    /// Builds the fixture with the given config and options, then generates
    /// and writes the host files into `dist/`.
    pub fn build(name: &str, config: &str, options: Options) -> Self {
        let root =
            std::env::temp_dir().join(format!("liyasa-hosting-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("a project directory");
        write(&root, "liyasa.json", config);
        write(
            &root,
            "index.md",
            "---\ntitle: Home\n---\n# Home\n\nWelcome.\n",
        );
        write(
            &root,
            "guides/install.md",
            "---\ntitle: Install\n---\n# Install\n\nRun it.\n",
        );
        write(
            &root,
            "embed/widget.md",
            "---\ntitle: Widget\nmode: frame\n---\n# Widget\n\nEmbedded.\n",
        );
        write(&root, "assets/manual.pdf", "%PDF-1.7 fixture");

        let options = Options {
            build_time: Some(1_789_473_600),
            ..options
        };
        let vfs = OsVfs::new(&root);
        let report = engine::build(&vfs, &NoGit, &root, &options);
        assert!(
            !report.failed(false),
            "the build failed: {:?}",
            report.diagnostics
        );

        let mut sources = SourceMap::new();
        let load = liyasa_config::load(
            &vfs,
            &mut sources,
            &liyasa_config::Options {
                root: VfsPath::new(""),
                env: options.env.clone(),
            },
        );
        let manifest = report.manifest.as_ref().expect("a manifest");
        let home =
            fs::read_to_string(root.join("dist").join("index.html")).expect("dist/index.html");
        let critical = critical_block(&home);
        let frame_routes = [Route::new(FRAME_ROUTE)];
        let output = hosting::generate(&Inputs {
            config: &load.value,
            manifest,
            critical_css: &critical,
            frame_routes: &frame_routes,
            image_hosts: &[],
            media_hosts: &[],
            hsts_preload: false,
        });
        let diagnostics = hosting::write(&output, &root.join("dist"));
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        Self {
            root,
            report,
            output,
            config: load.value,
        }
    }

    pub fn dist(&self) -> PathBuf {
        self.root.join("dist")
    }

    pub fn read(&self, path: &str) -> String {
        fs::read_to_string(self.dist().join(path))
            .unwrap_or_else(|error| panic!("dist/{path} is missing: {error}"))
    }

    pub fn headers_file(&self) -> String {
        self.read(hosting::HEADERS_FILE)
    }

    pub fn vercel(&self) -> serde_json::Value {
        serde_json::from_str(&self.read(hosting::VERCEL_FILE)).expect("vercel.json is JSON")
    }

    /// The headers `vercel.json` gives a `source`.
    pub fn vercel_headers(&self, source: &str) -> Vec<(String, String)> {
        self.vercel()["headers"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|rule| rule["source"] == source)
            .flat_map(|rule| {
                rule["headers"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|h| {
                        (
                            h["key"].as_str().unwrap_or_default().to_owned(),
                            h["value"].as_str().unwrap_or_default().to_owned(),
                        )
                    })
            })
            .collect()
    }
}

impl Drop for Site {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// The inline critical CSS exactly as the page carries it, which is what the
/// CSP hash has to be computed over.
pub fn critical_block(html: &str) -> String {
    let start = html
        .find("data-liyasa=\"critical\">")
        .map(|i| i + "data-liyasa=\"critical\">".len())
        .expect("the page inlines the critical block");
    let end = html[start..]
        .find("</style>")
        .expect("the critical block closes");
    html[start..start + end].to_owned()
}

/// The value of one header in a `name: value` list.
pub fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

fn write(root: &Path, path: &str, text: &str) {
    let full = root.join(path);
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent).expect("a directory");
    }
    fs::write(full, text).expect("a file");
}
