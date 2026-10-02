//! AST-11 and defect 146: the reader the assistant answers, and what they may
//! be shown.
//!
//! Defect 146 reads "`ReaderContext` is constructed only in tests, because no
//! handler builds one", and states the retrieval filter as already correct.
//! Building the handler showed the second half is narrower than that: the
//! filter is applied BY THE VECTOR STORE, so it covers `search` and nothing
//! else. `get_page`, `get_openapi` and `list_navigation` read the bundle
//! directly, never reach the store, and had no entitlement check at all —
//! `ChunkQuery.routes` was their only gate and `ReaderContext::query()` leaves
//! it empty, which `ServerTools` reads as "no restriction".
//!
//! So writing the missing caller naively would have served every restricted
//! page's Markdown and the whole navigation to anyone who asked, which is the
//! disclosure `routes::search` exists to avoid. `Visibility` is the gate, and
//! these are the tests that keep it.

use std::path::PathBuf;
use std::sync::Arc;

use liyasa_ai::assistant::tools::Tools;
use liyasa_ai::index::ChunkQuery;
use liyasa_build::engine::{self, Options};
use liyasa_build::git::NoGit;
use liyasa_config::vfs::OsVfs;
use liyasa_core::ids::Route;
use liyasa_server::assistant::gate::GatedTools;
use liyasa_server::auth::groups::SiteDefault;
use liyasa_server::auth::session::Principal;
use liyasa_server::routes::bundle::Bundle;
use liyasa_server::routes::tools::ServerTools;
use liyasa_tests::server::{Harness, Setup};
use serde_json::json;

const SITE: &str = r#"{
  "name": "Acme docs",
  "seo": { "canonicalOrigin": "https://docs.acme.com" },
  "navigation": [
    "index",
    { "group": "Guides", "pages": ["guides/install"] },
    { "group": "Internal", "groups": ["staff"], "pages": ["internal/overview"] }
  ]
}"#;

const PAGES: &[(&str, &str)] = &[
    ("index.md", "---\ntitle: Home\n---\n# Home\n"),
    ("guides/install.md", "---\ntitle: Install\n---\n# Install\n"),
    (
        "internal/overview.md",
        "---\ntitle: Overview\n---\n# Overview\nThe on-call rota is here.\n",
    ),
];

/// Keyed on the test's own name as well as the pid: the gate runs nextest,
/// which gives each test its own process, and CI runs `cargo test`, which does
/// not — a pid-only key collides there and nowhere the gate would show it.
fn build_site(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("liyasa-ast11-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a project directory");
    write(&root, "liyasa.json", SITE);
    for (path, body) in PAGES {
        write(&root, path, body);
    }
    let options = Options {
        build_time: Some(1_789_473_600),
        ..Options::default()
    };
    let report = engine::build(&OsVfs::new(&root), &NoGit, &root, &options);
    assert!(
        !report.failed(false),
        "the fixture failed to build: {:?}",
        report.diagnostics
    );
    root.join("dist")
}

fn write(root: &std::path::Path, path: &str, body: &str) {
    let full = root.join(path);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).expect("a directory");
    }
    std::fs::write(full, body).expect("a fixture file");
}

fn reader(groups: &[&str]) -> Principal {
    Principal {
        subject: "reader-1".to_owned(),
        groups: groups.iter().map(|g| (*g).to_owned()).collect(),
        ..Principal::default()
    }
}

fn tools_for(dist: &std::path::Path, principal: Option<Principal>) -> GatedTools {
    let bundle = Arc::new(Bundle::open(dist).expect("the fixture bundle"));
    let inner = ServerTools::new(bundle.clone(), ChunkQuery::default());
    GatedTools::new(inner, bundle, SiteDefault::Public, principal)
}

#[tokio::test]
async fn a_page_the_reader_may_not_browse_is_not_readable_through_get_page() {
    let dist = build_site("get-page");
    let restricted = Route::new("/internal/overview");

    let anonymous = tools_for(&dist, None);
    assert!(
        anonymous
            .get_page(&restricted, None)
            .await
            .expect("the tool answers")
            .is_none(),
        "a reader with no groups read a staff-only page through the assistant"
    );

    // The same call for somebody entitled to it, so the assertion above is
    // about the entitlement rather than about the page being unreadable for
    // some other reason. Without this the test would pass on a typo in the
    // route.
    let staff = tools_for(&dist, Some(reader(&["staff"])));
    let page = staff
        .get_page(&restricted, None)
        .await
        .expect("the tool answers")
        .expect("staff may read the staff page");
    assert!(page.markdown.contains("on-call rota"), "{page:?}");
}

#[tokio::test]
async fn the_navigation_omits_what_the_reader_cannot_open() {
    let dist = build_site("navigation");

    let anonymous = tools_for(&dist, None)
        .list_navigation()
        .await
        .expect("the tool answers");
    let routes: Vec<&str> = anonymous.iter().map(|e| e.route.as_str()).collect();
    assert!(
        !routes.contains(&"/internal/overview"),
        "the navigation named a staff-only route to an anonymous reader: {routes:?}"
    );
    assert!(
        routes.contains(&"/guides/install"),
        "the filter removed a page everyone may read: {routes:?}"
    );

    let staff = tools_for(&dist, Some(reader(&["staff"])))
        .list_navigation()
        .await
        .expect("the tool answers");
    let staff_routes: Vec<&str> = staff.iter().map(|e| e.route.as_str()).collect();
    assert!(
        staff_routes.contains(&"/internal/overview"),
        "the page is missing for the reader entitled to it, so the assertion above \
         proves nothing: {staff_routes:?}"
    );
}

#[tokio::test]
async fn the_ungated_reader_is_what_the_indexer_still_gets() {
    // `ServerTools` on its own reads every page, which is correct for the
    // indexing side: AST-01 embeds the neutral rendering of the whole site.
    // Asserted so the gate's value is unambiguous — it is the wrapper that
    // restricts, and anything composing the bare reader for a REQUEST is the
    // disclosure, not a missing feature of `ServerTools`.
    let dist = build_site("ungated");
    let bundle = Arc::new(Bundle::open(&dist).expect("the fixture bundle"));
    let tools = ServerTools::new(bundle, ChunkQuery::default());
    assert!(
        tools
            .get_page(&Route::new("/internal/overview"), None)
            .await
            .expect("the tool answers")
            .is_some(),
        "the bare bundle reader could not read a restricted page, so the gated \
         assertions above may be measuring something other than the gate"
    );
}

// ---- the composed application, which is the other half of retiring a pin ----

async fn harness(name: &str, dist: PathBuf) -> Harness {
    let (harness, _) = Harness::new(Setup {
        dist: Some(dist),
        site_config: Some(json!({
            "name": "Acme docs",
            "seo": { "canonicalOrigin": "https://docs.acme.com" },
            "ai": { "assistant": { "enabled": true } }
        })),
        ..Setup::new(name)
    })
    .await;
    harness
}

#[tokio::test]
async fn the_application_the_binary_runs_serves_the_assistant_endpoint() {
    // Retiring the census pin proves a `.route("/_liyasa/assistant")` literal
    // exists somewhere in the crate. It does NOT prove `routes::application`
    // composes it, and main has already carried a complete MCP server that
    // nothing mounted for a full merge round on exactly that gap. This is the
    // half that asks the composed object.
    let harness = harness("ast11-mounted", build_site("mounted")).await;

    // This used to skip while `subtrees()` had no `assistant` entry, because
    // that line is in `routes/mount.rs` and not WP-18's to write. The entry is
    // there now, so the skip branch was dead code reading as coverage — a
    // skip-with-reason is a claim about the present and nothing in its format
    // marks it expired. Converted to the assertion rather than deleted, so the
    // reason stays loud if the entry is ever dropped.
    let record = harness
        .mounted
        .iter()
        .find(|m| m.name == "assistant")
        .unwrap_or_else(|| {
            panic!(
                "`routes::mount::subtrees()` carries no `assistant` entry, so \
                 `routes::application` does not compose this endpoint however well the \
                 router works — the state main was in with the MCP server for a full \
                 merge round. The line is\n\n    \
                 Subtree {{ name: \"assistant\", permission: None, mount: crate::assistant::mount }},\n\n\
                 in crates/liyasa-server/src/routes/mount.rs. Mounted: {:?}",
                harness.mounted.iter().map(|m| m.name).collect::<Vec<_>>()
            )
        });
    assert!(
        record.mounted,
        "the assistant subtree is registered and did not mount: {:?}",
        record.skipped
    );

    let response = harness
        .post_json(
            "/_liyasa/assistant",
            json!({ "question": "how do I install" }),
        )
        .await;
    assert_ne!(
        response.status(),
        http::StatusCode::NOT_FOUND,
        "the subtree mounted and the endpoint is still not served"
    );
}
