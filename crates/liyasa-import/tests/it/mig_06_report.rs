//! MIG-06. An importer runs in dry-run mode first and produces a migration
//! report with a confidence score per page. The dry run is not a mode here: an
//! importer's only product is a plan, and nothing reaches the disk until the
//! caller applies it.

use liyasa_config::vfs::MemVfs;
use liyasa_core::vfs::VfsPath;
use liyasa_import::stubs::Stubs;
use liyasa_import::{Apply, mintlify};

use crate::support::{Builtins, text_at};

const DOCS_JSON: &str = r#"{ "name": "Acme", "navigation": { "pages": ["index", "pricing"] } }"#;

fn project() -> MemVfs {
    MemVfs::new()
        .with("docs.json", DOCS_JSON.as_bytes().to_vec())
        .with("index.mdx", b"---\ntitle: Home\n---\n\nHi.\n".to_vec())
        .with(
            "pricing.mdx",
            b"---\ntitle: Pricing\n---\n\n<PricingTable plan=\"team\" />\n\n{rows.map(r => r)}\n"
                .to_vec(),
        )
        .with("images/logo.svg", b"<svg/>".to_vec())
}

fn plan() -> liyasa_import::Plan {
    mintlify::import(
        &project(),
        &VfsPath::new(""),
        &mintlify::Options {
            components: &Builtins::default(),
            directives: false,
            mapping: &Stubs,
        },
    )
}

#[test]
fn the_report_scores_every_page() {
    let plan = plan();
    assert_eq!(plan.report.pages.len(), 2);
    for page in &plan.report.pages {
        let score = page.confidence();
        assert!(score <= 100);
        match page.from.as_str() {
            "index.mdx" => assert_eq!(score, 100),
            // TODO(rfc-2902): the custom component became a stub; the
            // JavaScript expression is what is left for a human.
            "pricing.mdx" => assert_eq!(score, 70),
            other => panic!("unexpected page {other}"),
        }
    }
    assert_eq!(plan.report.clean_pages(), 1);
    assert_eq!(plan.report.clean_percent(), 50.0);
    assert_eq!(plan.report.mean_confidence(), 85);
}

#[test]
fn a_dry_run_names_every_file_and_writes_none_of_them() {
    let plan = plan();
    let summary = plan.summary();

    assert!(summary.contains("write liyasa.json"), "{summary}");
    assert!(summary.contains("write pricing.md"), "{summary}");
    assert!(summary.contains("write images/logo.svg (copy of images/logo.svg)"));
    assert!(
        summary.contains("write components/pricing-table.jinja"),
        "{summary}"
    );
    assert!(summary.ends_with("6 files, nothing written\n"), "{summary}");
}

#[test]
fn the_report_is_a_file_of_the_imported_project() {
    let plan = plan();
    let report = text_at(&plan, "migration-report.md");
    assert!(report.contains("2 pages, 1 clean (50.0%), mean confidence 85."));
    assert!(report.contains("### pricing.mdx (70)"), "{report}");
    assert!(report.contains("JavaScript expression"), "{report}");
    // The component Liyasa cannot render is named once, under the project,
    // with the stub written for it (RFC 2902).
    assert!(report.contains("## The project"), "{report}");
    assert!(
        report.contains("custom component: `PricingTable`"),
        "{report}"
    );
    assert!(report.contains("used 1 times"), "{report}");
}

#[test]
fn nothing_reaches_the_disk_until_the_plan_is_applied() {
    let source = project();
    let plan = plan();
    let root = scratch("dry-run");
    assert!(
        std::fs::read_dir(&root)
            .expect("the scratch directory")
            .next()
            .is_none(),
        "building a plan wrote something"
    );

    let diagnostics = plan.apply(&source, &root, &Apply::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics.as_slice());
    assert!(root.join("liyasa.json").is_file());
    assert!(root.join("pricing.md").is_file());
    assert_eq!(
        std::fs::read(root.join("images/logo.svg")).unwrap_or_default(),
        b"<svg/>"
    );
}

#[test]
fn applying_into_a_project_that_already_has_files_refuses_before_writing_any() {
    let source = project();
    let plan = plan();
    let root = scratch("occupied");
    std::fs::write(root.join("liyasa.json"), "mine").expect("the fixture writes");

    let diagnostics = plan.apply(&source, &root, &Apply::default());
    assert_eq!(
        diagnostics
            .iter()
            .map(|d| d.code.as_str())
            .collect::<Vec<_>>(),
        ["E1103"]
    );
    assert_eq!(
        std::fs::read_to_string(root.join("liyasa.json")).unwrap_or_default(),
        "mine"
    );
    assert!(!root.join("pricing.md").exists());
}

fn scratch(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("liyasa-mig-06-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("a scratch directory");
    path
}
