//! What every importer test needs: the real component registry, and a way to
//! hold a converted project up against the real config schema.

use liyasa_components::Registry;
use liyasa_config::vfs::MemVfs;
use liyasa_core::components::ComponentRegistry;
use liyasa_core::vfs::VfsPath;
use liyasa_import::plan::Content;
use liyasa_import::{Components, Plan};

/// The component set the shipped registry really has. A test that says a page
/// converted cleanly is then saying it about the product, not about a list.
pub struct Builtins(Registry);

impl Default for Builtins {
    fn default() -> Self {
        Self(Registry::builtins())
    }
}

impl Components for Builtins {
    fn known(&self, name: &str) -> bool {
        self.0.get(name).is_some()
    }

    fn suggest(&self, name: &str) -> Option<String> {
        self.0.suggest(name).map(str::to_owned)
    }
}

/// The text a plan would write at a path.
pub fn text_at<'a>(plan: &'a Plan, path: &str) -> &'a str {
    plan.text_at(path).unwrap_or_else(|| {
        panic!(
            "the plan writes {path}; it writes {}",
            paths(plan).join(", ")
        )
    })
}

pub fn paths(plan: &Plan) -> Vec<&str> {
    plan.writes().iter().map(|w| w.path.as_str()).collect()
}

/// The config a plan produced, parsed.
pub fn config(plan: &Plan) -> serde_json::Value {
    serde_json::from_str(text_at(plan, "liyasa.json")).expect("liyasa.json is valid JSON")
}

/// Loads the planned project the way `liyasa build` would and returns every
/// diagnostic. An importer whose output does not load has not migrated
/// anything, so this is what an acceptance test asserts on.
pub fn validate(plan: &Plan, source: &MemVfs) -> Vec<String> {
    let mut vfs = MemVfs::new();
    for write in plan.writes() {
        let bytes = match &write.content {
            Content::Text(text) => text.clone().into_bytes(),
            Content::Copy(from) => match liyasa_core::vfs::Vfs::read(source, from) {
                Ok(bytes) => bytes.to_vec(),
                Err(error) => panic!("the plan carries {from}, which does not read: {error}"),
            },
        };
        vfs.insert(write.path.clone(), bytes);
    }

    let root = VfsPath::new("");
    let mut sources = liyasa_core::SourceMap::new();
    let load = liyasa_config::load(&vfs, &mut sources, &liyasa_config::Options::default());
    let pages = liyasa_config::Pages::discover(&vfs, &root, "dist");
    let mut out: Vec<String> = load
        .diagnostics
        .iter()
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    let context = liyasa_config::Context {
        pages: &pages,
        mode: liyasa_config::Mode::Build,
    };
    out.extend(
        liyasa_config::validate_load(&load, &context)
            .iter()
            .filter(|d| d.is_error())
            .map(|d| format!("{}: {}", d.code, d.message)),
    );
    out
}

/// Every error the Markdown scanner raises on a converted page.
///
/// MIG-01 asks for pages that convert with nothing left to do *and* render with
/// no diagnostics. Scanning is the layer that answers the second half here: it
/// settles front matter, fences, directives, and template well-formedness
/// (CM-21) without needing a snippet loader or a fact table, neither of which an
/// importer can supply.
pub fn scan_errors(text: &str) -> Vec<String> {
    let (_, diagnostics) = liyasa_markdown::source::scan(text, liyasa_core::span::SourceId(0));
    diagnostics
        .iter()
        .filter(|d| d.is_error())
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect()
}

/// Asserts that every page a plan wrote scans cleanly.
pub fn pages_scan(plan: &Plan) {
    for write in plan.writes() {
        let Content::Text(text) = &write.content else {
            continue;
        };
        if !write.path.as_str().ends_with(".md") {
            continue;
        }
        let errors = scan_errors(text);
        assert!(
            errors.is_empty(),
            "{} does not scan cleanly: {errors:?}",
            write.path
        );
    }
}
