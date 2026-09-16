//! CFG-90 and CFG-30 in a build: which of the config's semantic rules a
//! `liyasa build` actually reports.
//!
//! `liyasa-build` runs the rules as well as loading the config (RFC 0106, now
//! closed), so a build and `liyasa validate` report the same set. This file
//! pins that they agree: a rule that only one of them reports is the defect
//! this file exists to catch.

use std::fs;
use std::path::{Path, PathBuf};

use liyasa_build::engine::{self, Options};
use liyasa_build::git::NoGit;
use liyasa_config::vfs::{MemVfs, OsVfs};
use liyasa_config::{Mode, check};
use liyasa_core::source_map::SourceMap;

/// One config with something wrong for each rule that can coexist: a page named
/// twice, a primary no label clears, two versions and no default, a subtree on
/// a version nobody declared, a colour that is not a colour, and a page no
/// navigation reaches.
const BROKEN: &str = r##"{
  "name": "Acme docs",
  "seo": { "canonicalOrigin": "https://docs.acme.com" },
  "theme": { "colors": { "primary": "#818CF8", "accent": "#ggg" } },
  "versions": [{ "name": "v2" }, { "name": "v1" }],
  "navigation": [
    "index",
    "index",
    { "version": "v9", "pages": ["guides/install"] }
  ]
}"##;

const PAGES: &[(&str, &str)] = &[
    ("index.md", "---\ntitle: Home\n---\n# Home\n"),
    ("guides/install.md", "---\ntitle: Install\n---\n# Install\n"),
    ("guides/orphan.md", "---\ntitle: Orphan\n---\n# Orphan\n"),
];

struct Project(PathBuf);

impl Project {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("liyasa-cfg-90-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("a project directory");
        let project = Self(root);
        project.write("liyasa.json", BROKEN);
        for (path, text) in PAGES {
            project.write(path, text);
        }
        project
    }

    fn write(&self, path: &str, text: &str) -> &Self {
        let full = self.0.join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).expect("a directory");
        }
        fs::write(full, text).expect("a file");
        self
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// What `liyasa build` reports for that project.
fn built() -> Vec<String> {
    let project = Project::new("build");
    let vfs = OsVfs::new(project.path());
    let report = engine::build(
        &vfs,
        &NoGit,
        project.path(),
        &Options {
            build_time: Some(1_789_473_600),
            ..Options::default()
        },
    );
    report
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str().to_owned())
        .collect()
}

/// What `liyasa validate` reports for the same project.
fn validated() -> Vec<String> {
    let mut files: Vec<(&str, Vec<u8>)> = vec![("liyasa.json", BROKEN.as_bytes().to_vec())];
    files.extend(
        PAGES
            .iter()
            .map(|(path, text)| (*path, text.as_bytes().to_vec())),
    );
    let vfs: MemVfs = files.into_iter().collect();
    let mut sources = SourceMap::new();
    check(&vfs, &mut sources, &Default::default(), Mode::Build)
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str().to_owned())
        .collect()
}

#[test]
fn validate_reports_every_rule_the_config_is_breaking() {
    let codes = validated();
    for code in ["E0105", "E0107", "E0108", "E0132", "E0133", "W0130"] {
        assert!(
            codes.contains(&code.to_owned()),
            "{code} is not among {codes:?}"
        );
    }
}

#[test]
fn a_build_reports_them_too() {
    let codes = built();
    let missing: Vec<&str> = ["E0105", "E0107", "E0108", "E0132", "E0133", "W0130"]
        .into_iter()
        .filter(|code| !codes.contains(&(*code).to_owned()))
        .collect();
    assert_eq!(
        missing,
        Vec::<&str>::new(),
        "a build runs the config's semantic rules (RFC 0106). Got {codes:?}"
    );
}

/// One mistake, one diagnostic. The engine's own modules check redirects and
/// navigation for callers that never load a config — the server, the dev loop —
/// so a build drops their copies of the codes the config owns rather than
/// reporting the same defect under two wordings (RFC 0106).
#[test]
fn a_rule_both_halves_check_is_reported_once() {
    let project = Project::new("redirects");
    project.write(
        "liyasa.json",
        r##"{ "name": "Acme docs", "seo": { "canonicalOrigin": "https://docs.acme.com" },
              "navigation": ["index", "guides/missing"],
              "redirects": { "rules": [{ "source": "/a", "destination": "/b" },
                                       { "source": "/a", "destination": "/c" }] } }"##,
    );
    let vfs = OsVfs::new(project.path());
    let report = engine::build(
        &vfs,
        &NoGit,
        project.path(),
        &Options {
            build_time: Some(1_789_473_600),
            ..Options::default()
        },
    );
    let codes: Vec<&str> = report
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect();
    for code in ["E0104", "E0106"] {
        let times = codes.iter().filter(|found| **found == code).count();
        assert_eq!(times, 1, "{code} was reported {times} times: {codes:?}");
    }
}
