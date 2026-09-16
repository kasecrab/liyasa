//! MIG-01's quality bar, which is the reason this importer exists (§5.4).
//!
//! At least ten public Mintlify-built documentation repositories are imported
//! on every change, and at least 99% of their pages must convert with no
//! manual-attention item.
//!
//! The corpus is a fixture set of other people's repositories, so like the
//! Markdown corpus it lives outside this one and is never committed. It is
//! found only through `LIYASA_MINTLIFY_CORPUS`, never by guessing at a path:
//! it is shared mutable state that every worktree can see, and a test that
//! picked it up on its own would turn a neighbouring package's gate red for
//! content that package never touched. CI sets the variable; so does anyone
//! running the bar by hand:
//!
//! ```text
//! LIYASA_MINTLIFY_CORPUS=$PREP/spec/mintlify cargo test -p liyasa-import --test it -- --nocapture corpus
//! ```

use std::path::{Path, PathBuf};

use liyasa_config::vfs::OsVfs;
use liyasa_core::vfs::VfsPath;
use liyasa_import::mintlify;
use liyasa_import::report::Kind;
use liyasa_import::stubs::Stubs;

use crate::support::{Builtins, scan_errors};

/// What MIG-01 requires of the corpus.
const MIN_PROJECTS: usize = 10;
const MIN_CLEAN_PERCENT: f64 = 99.0;

fn corpus() -> Option<PathBuf> {
    let named = std::env::var("LIYASA_MINTLIFY_CORPUS").ok()?;
    let path = PathBuf::from(named);
    path.is_dir().then_some(path)
}

fn projects(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.join("docs.json").is_file() || path.join("mint.json").is_file())
        .collect();
    out.sort();
    out
}

#[test]
fn every_public_mintlify_project_converts_with_nothing_left_to_do() {
    let Some(root) = corpus() else {
        println!(
            "MIG-01 corpus not run: set LIYASA_MINTLIFY_CORPUS to the directory of \
             Mintlify projects. This test asserted nothing."
        );
        return;
    };
    let projects = projects(&root);

    let components = Builtins::default();
    let mut total = 0usize;
    let mut clean = 0usize;
    let mut worst: Vec<(String, f64, usize)> = Vec::new();
    let mut reasons: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();

    for project in &projects {
        let vfs = OsVfs::new(project);
        let plan = mintlify::import(
            &vfs,
            &VfsPath::new(""),
            &mintlify::Options {
                components: &components,
                directives: false,
                mapping: &Stubs,
            },
        );
        let name = project
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        assert!(
            !plan.report.diagnostics.has_errors(),
            "{name} failed to import: {:?}",
            plan.report
                .diagnostics
                .iter()
                .map(|d| format!("{}: {}", d.code, d.message))
                .collect::<Vec<_>>()
        );
        assert!(
            !plan.report.pages.is_empty(),
            "{name} produced no pages at all"
        );

        // A page the report calls clean has to be one: the importer scans its
        // own output, so a clean page that does not scan is a bug in the check
        // rather than in the page.
        for page in plan.report.pages.iter().filter(|page| page.is_clean()) {
            let Some(text) = plan.text_at(page.to.as_str()) else {
                continue;
            };
            let errors = scan_errors(text);
            assert!(
                errors.is_empty(),
                "{name}: {} is reported clean but does not scan: {errors:?}",
                page.to
            );
        }

        for page in &plan.report.pages {
            for item in &page.attention {
                *reasons
                    .entry(format!("{}: {}", kind_name(item.kind), item.what))
                    .or_default() += 1;
            }
        }
        total += plan.report.pages.len();
        clean += plan.report.clean_pages();
        worst.push((name, plan.report.clean_percent(), plan.report.pages.len()));
    }

    worst.sort_by(|a, b| a.1.total_cmp(&b.1));
    println!("MIG-01 corpus: {} projects, {total} pages", projects.len());
    for (name, percent, pages) in &worst {
        println!("  {percent:>6.2}%  {pages:>5} pages  {name}");
    }
    let mut top: Vec<(&String, &usize)> = reasons.iter().collect();
    top.sort_by(|a, b| b.1.cmp(a.1));
    for (reason, count) in top.iter().take(15) {
        println!("  {count:>5}x {reason}");
    }

    let percent = clean as f64 * 100.0 / total as f64;
    println!("  clean: {clean}/{total} ({percent:.2}%)");

    // The count is asserted after the breakdown is printed: a corpus that is
    // short of projects should still show what the ones it has did.
    assert!(
        projects.len() >= MIN_PROJECTS,
        "MIG-01 wants at least {MIN_PROJECTS} projects; {} found under {}",
        projects.len(),
        root.display()
    );
    assert!(
        percent >= MIN_CLEAN_PERCENT,
        "MIG-01 requires at least {MIN_CLEAN_PERCENT}% of pages to convert with no \
         manual-attention item; {percent:.2}% did ({clean} of {total})"
    );
}

fn kind_name(kind: Kind) -> &'static str {
    kind.as_str()
}
