//! RX-80: whole-site PDF, rendered by the companion runtime from the print
//! stylesheet.
//!
//! The render needs a browser. Where there is none the command must say so and
//! exit 1 rather than write a broken file, and that half is asserted
//! unconditionally; the render itself is asserted only where a browser exists,
//! and the test says which of the two it did so a silent skip cannot be
//! mistaken for a pass.

use liyasa_cli::Exit;
use liyasa_cli::browser;

use crate::support::{Dir, Run};

fn scaffolded(name: &str) -> (Dir, std::path::PathBuf) {
    let root = Dir::new(name);
    let project = root.path().join("docs");
    let created = Run::new(["new", "docs", "--yes"]).cwd(root.path()).output();
    assert_eq!(created.code, Exit::Success.code(), "{}", created.all());
    let built = Run::new(["build"]).cwd(&project).output();
    assert_eq!(built.code, Exit::Success.code(), "{}", built.all());
    (root, project)
}

#[test]
fn a_whole_site_prints_to_one_pdf() {
    let Some(found) = browser::find() else {
        eprintln!("no browser on this machine; the render half of RX-80 was not exercised");
        return;
    };

    let (_root, project) = scaffolded("rx80-print");
    let outcome = Run::new(["export", "--pdf"]).cwd(&project).output();
    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());

    let pdf = project.join("export/site.pdf");
    assert!(
        pdf.is_file(),
        "no file at {}: {}",
        pdf.display(),
        outcome.all()
    );

    let bytes = std::fs::read(&pdf).expect("the pdf");
    assert!(
        bytes.starts_with(b"%PDF-"),
        "not a PDF, starts with {:?}",
        &bytes[..bytes.len().min(8)]
    );
    // A cover, a contents page, and five pages of content is not 2 KB.
    assert!(bytes.len() > 10_000, "{} bytes", bytes.len());

    // The browser it used is named, because an unpinned one changes the render.
    assert!(outcome.stdout.contains("printed"), "{}", outcome.stdout);
    if !found.source.is_pinned() {
        assert!(
            outcome.stdout.contains("does not pin"),
            "an unpinned browser was not reported: {}",
            outcome.stdout
        );
    }
}

/// The intermediate document is the site's own directory's business and must
/// not be left behind in it.
#[test]
fn printing_leaves_no_scratch_file_in_the_output() {
    if browser::find().is_none() {
        return;
    }
    let (_root, project) = scaffolded("rx80-scratch");
    Run::new(["export", "--pdf"]).cwd(&project).output();
    assert!(
        !project.join("dist/_liyasa/print.html").exists(),
        "the print document was left in dist/"
    );
}

#[test]
fn the_output_path_can_name_the_file_or_the_directory() {
    if browser::find().is_none() {
        return;
    }
    let (_root, project) = scaffolded("rx80-output");

    let named = Run::new(["export", "--pdf", "--output", "book.pdf"])
        .cwd(&project)
        .output();
    assert_eq!(named.code, Exit::Success.code(), "{}", named.all());
    assert!(project.join("book.pdf").is_file(), "{}", named.all());

    let directory = Run::new(["export", "--pdf", "--output", "out"])
        .cwd(&project)
        .output();
    assert_eq!(directory.code, Exit::Success.code(), "{}", directory.all());
    assert!(
        project.join("out/site.pdf").is_file(),
        "{}",
        directory.all()
    );
}

/// A dry run reports what it would print, finds the browser, and writes
/// nothing.
#[test]
fn a_dry_run_reports_the_browser_and_prints_nothing() {
    if browser::find().is_none() {
        return;
    }
    let (_root, project) = scaffolded("rx80-dry-run");
    let outcome = Run::new(["export", "--pdf", "--dry-run"])
        .cwd(&project)
        .output();

    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());
    assert!(outcome.stdout.contains("browser"), "{}", outcome.stdout);
    assert!(outcome.stdout.contains("pages"), "{}", outcome.stdout);
    assert!(!project.join("export/site.pdf").exists());
}

/// Without a browser the command explains itself rather than failing obscurely.
/// Forced by pointing the override at something that is not one, so the case is
/// exercised on a machine that does have a browser.
#[test]
fn without_a_browser_it_says_what_is_missing() {
    let (_root, project) = scaffolded("rx80-no-browser");
    let outcome = Run::new(["export", "--pdf"])
        .cwd(&project)
        .env("LIYASA_COMPANION_CHROME", "/nonexistent/chrome")
        .env("PLAYWRIGHT_BROWSERS_PATH", "/nonexistent")
        .env("LIYASA_CACHE_HOME", "/nonexistent")
        .env("PATH", "/nonexistent")
        .output();

    assert_eq!(outcome.code, Exit::Errors.code(), "{}", outcome.all());
    assert!(outcome.all().contains("E0003"), "{}", outcome.all());
    assert!(outcome.all().contains("companion"), "{}", outcome.all());
    assert!(!project.join("export/site.pdf").exists());
}

/// An unbuilt project has nothing to print, and that is a different failure
/// from having no browser.
#[test]
fn an_unbuilt_project_is_reported_as_such() {
    let root = Dir::new("rx80-unbuilt");
    let project = root.path().join("docs");
    Run::new(["new", "docs", "--yes"]).cwd(root.path()).output();

    let outcome = Run::new(["export", "--pdf"]).cwd(&project).output();
    assert_eq!(outcome.code, Exit::Errors.code(), "{}", outcome.all());
    assert!(outcome.all().contains("E0011"), "{}", outcome.all());
}

/// `liyasa doctor` reports the browser the export would use, so a person can
/// find out why a render differs without reading this code.
#[test]
fn doctor_names_the_browser_it_would_use() {
    let outcome = Run::new(["doctor", "--json"]).output();
    assert_eq!(outcome.code, Exit::Success.code(), "{}", outcome.all());

    let document: serde_json::Value =
        serde_json::from_str(&outcome.stdout).unwrap_or_else(|_| panic!("{}", outcome.stdout));
    let checks = document["checks"].as_array().expect("the checks");
    let runtime = checks
        .iter()
        .find(|check| check["name"] == "companion runtime")
        .expect("a companion runtime row");

    match browser::find() {
        Some(found) => {
            assert_eq!(runtime["state"], "ready", "{runtime}");
            assert!(
                runtime["detail"]
                    .as_str()
                    .is_some_and(|detail| detail.contains(&found.version)),
                "{runtime}"
            );
        }
        None => assert_eq!(runtime["state"], "missing", "{runtime}"),
    }
}
