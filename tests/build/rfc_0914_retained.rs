//! RFC 0914: the build hands back each page's render, so a caller does not
//! re-render it through the lenient context.
//!
//! The defect this closes is not performance. `liyasa verify` re-renders every
//! page through `liyasa_lsp::analysis`, which uses `Options::anonymous`, so
//! `site.*`, `nav.*` and `env.*` are unexpanded inside a fence — a verified code
//! fence runs code that differs from what the page shows.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use liyasa_build::engine::{self, Options};
use liyasa_build::git::NoGit;
use liyasa_build::retained::{PageHook, Retain};
use liyasa_config::vfs::OsVfs;
use liyasa_core::ids::Route;

/// Beside the pid: the gate runs nextest, one process per test; CI runs
/// `cargo test`, where a binary's tests are threads sharing one pid.
static COUNTER: AtomicU64 = AtomicU64::new(0);

struct Project(PathBuf);

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn site() -> Project {
    let root = std::env::temp_dir().join(format!(
        "liyasa-rfc-0914-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("guides")).expect("a project directory");
    fs::write(
        root.join("liyasa.json"),
        r#"{"name":"Acme docs","seo":{"canonicalOrigin":"https://docs.acme.com"}}"#,
    )
    .expect("config");
    fs::write(
        root.join("index.md"),
        "---\ntitle: Home\n---\n# Home\n\nThe landing page.\n",
    )
    .expect("a page");
    fs::write(
        root.join("guides/install.md"),
        "---\ntitle: Install\n---\n# Install\n\nRun the installer.\n",
    )
    .expect("a second page");
    Project(root)
}

fn with(project: &Project, retain: Retain) -> engine::Report {
    let vfs = OsVfs::new(&project.0);
    let report = engine::build_retaining(
        &vfs,
        &NoGit,
        &project.0,
        &Options {
            build_time: Some(1_789_473_600),
            ..Options::default()
        },
        &retain,
    );
    assert!(!report.failed(false), "{:?}", report.diagnostics);
    report
}

fn keeping(documents: bool) -> Retain {
    Retain {
        documents,
        on_page: None,
    }
}

/// `None` and `Some(empty)` are different answers. A plain map would make "I did
/// not ask" indistinguishable from "there was nothing", and a caller reading the
/// second as the first reports a clean site for a build that retained nothing.
#[test]
fn not_asking_is_a_different_answer_from_nothing_to_give() {
    let project = site();
    let silent = with(&project, keeping(false));
    assert!(
        silent.documents.is_none(),
        "a build nobody asked holds nothing"
    );

    let project = site();
    let asked = with(&project, keeping(true));
    let documents = asked.documents.expect("asking is answered");
    assert_eq!(
        documents.keys().map(Route::as_str).collect::<Vec<_>>(),
        ["/", "/guides/install"]
    );
}

/// The property the whole thing lives on. A warm build renders nothing, so a
/// hook on the render loop would hand a full set cold and an empty one warm —
/// and both callers run warm by default.
#[test]
fn a_warm_build_hands_back_the_same_set_as_a_cold_one() {
    let project = site();
    let cold = with(&project, keeping(true)).documents.expect("cold");
    let warm_report = with(&project, keeping(true));
    assert_eq!(warm_report.cache_misses, 0, "the second build is warm");
    let warm = warm_report.documents.expect("warm");

    // Equality, not containment: a subset assertion passes for exactly the
    // defect — a warm run that hands back fewer pages than it placed.
    assert_eq!(
        warm.keys().collect::<Vec<_>>(),
        cold.keys().collect::<Vec<_>>()
    );
    for (route, record) in &cold {
        let warmed = warm.get(route).expect("the same route");
        assert_eq!(&warmed.document, &record.document, "{route}");
        assert_eq!(&warmed.expansion, &record.expansion, "{route}");
    }
}

/// The hook is the primitive: a caller that wants three pages of five thousand
/// discards on arrival rather than holding the set.
#[test]
fn the_hook_sees_every_page_without_the_map() {
    let project = site();
    let seen: Arc<Mutex<BTreeMap<String, bool>>> = Arc::new(Mutex::new(BTreeMap::new()));
    let sink = Arc::clone(&seen);
    let report = with(
        &project,
        Retain {
            on_page: Some(PageHook::new(move |route, record| {
                sink.lock().expect("the sink").insert(
                    route.as_str().to_owned(),
                    !record.document.root.children.is_empty(),
                );
            })),
            ..keeping(false)
        },
    );
    assert!(
        report.documents.is_none(),
        "the hook does not imply the map"
    );
    let seen = seen.lock().expect("the sink");
    assert_eq!(
        seen.keys().map(String::as_str).collect::<Vec<_>>(),
        ["/", "/guides/install"]
    );
    assert!(
        seen.values().all(|had_content| *had_content),
        "each page arrives with its rendered body, not an empty shell"
    );
}
