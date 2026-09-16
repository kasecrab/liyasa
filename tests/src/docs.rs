//! Building `docs/`, the Liyasa documentation site (MIG-22).
//!
//! The docs are a real Liyasa project in this repository, built by the engine
//! this repository ships. There is no CLI yet (WP-09), so the tests call
//! [`liyasa_build::engine::build`] the way `liyasa build` will.
//!
//! Every build runs against a copy in a temporary directory: the source tree
//! keeps no `dist/` or `.liyasa/`, and two tests can build the same site at
//! once.

pub mod generate;

use std::fs;
use std::path::{Path, PathBuf};

use liyasa_build::engine::{self, Options, Report};
use liyasa_build::git::NoGit;
use liyasa_build::hosting::{self, Inputs};
use liyasa_config::vfs::OsVfs;
use liyasa_core::source_map::SourceMap;
use liyasa_core::vfs::VfsPath;

/// A fixed clock, so a build is reproducible and a test can assert on dates.
pub const BUILD_TIME: i64 = 1_789_473_600;

/// Where the docs are published (§33.1 item 8: there is no domain).
pub const ORIGIN: &str = "https://kasecrab.github.io";

/// The path the site is served under on GitHub Pages.
pub const BASE_PATH: &str = "/liyasa/docs";

/// `docs/` in this repository.
pub fn source() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the tests crate sits under the repository root")
        .join("docs")
}

/// A built copy of `docs/`, removed when it is dropped.
pub struct Docs {
    root: PathBuf,
    pub report: Report,
}

impl Docs {
    /// Copies `docs/` into a temporary directory and builds it there.
    pub fn build(name: &str) -> Self {
        Self::build_with(name, Options::default())
    }

    pub fn build_with(name: &str, options: Options) -> Self {
        let root = std::env::temp_dir().join(format!(
            "liyasa-docs-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&root);
        copy(&source(), &root).expect("docs/ copies into a temporary directory");

        let options = Options {
            build_time: Some(BUILD_TIME),
            ..options
        };
        let vfs = OsVfs::new(&root);
        let report = engine::build(&vfs, &NoGit, &root, &options);

        // `engine::build` writes only the asset-derived `_headers`; the
        // security headers, the content security policy, `_redirects`, and
        // `vercel.json` come from `hosting::generate`, which nothing in the
        // engine calls (see NEEDS-INPUT.md [WP-31]). The release pipeline has
        // to run both, so the harness does what the pipeline does.
        if let Some(manifest) = report.manifest.as_ref() {
            let mut sources = SourceMap::new();
            let load = liyasa_config::load(
                &vfs,
                &mut sources,
                &liyasa_config::Options {
                    root: VfsPath::new(""),
                    env: options.env.clone(),
                },
            );
            let home = fs::read_to_string(root.join("dist").join("index.html")).unwrap_or_default();
            let output = hosting::generate(&Inputs {
                config: &load.value,
                manifest,
                critical_css: &crate::hosting::critical_block(&home),
                frame_routes: &[],
                image_hosts: &[],
                media_hosts: &[],
                hsts_preload: false,
            });
            let diagnostics = hosting::write(&output, &root.join("dist"));
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
        }

        Self { root, report }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn dist(&self) -> PathBuf {
        self.root.join("dist")
    }

    /// The bytes at a path under `dist/`, or a panic naming the path.
    pub fn read(&self, path: &str) -> String {
        fs::read_to_string(self.dist().join(path))
            .unwrap_or_else(|error| panic!("dist/{path} is missing: {error}"))
    }

    pub fn exists(&self, path: &str) -> bool {
        self.dist().join(path).exists()
    }

    /// Every error-severity diagnostic, formatted for an assertion message.
    pub fn errors(&self) -> Vec<String> {
        self.report
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.is_error())
            .map(|diagnostic| format!("{} {}", diagnostic.code, diagnostic.message))
            .collect()
    }

    /// Every warning, formatted the same way.
    pub fn warnings(&self) -> Vec<String> {
        self.report
            .diagnostics
            .iter()
            .filter(|diagnostic| !diagnostic.is_error())
            .map(|diagnostic| format!("{} {}", diagnostic.code, diagnostic.message))
            .collect()
    }
}

impl Drop for Docs {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn copy(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        match entry.file_type()? {
            kind if kind.is_dir() => {
                // A previous build's output is not part of the source.
                let name = entry.file_name();
                if name == "dist" || name == ".liyasa" {
                    continue;
                }
                copy(&entry.path(), &target)?;
            }
            _ => {
                fs::copy(entry.path(), &target)?;
            }
        }
    }
    Ok(())
}
