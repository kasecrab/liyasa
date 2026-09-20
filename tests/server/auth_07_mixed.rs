//! AUTH-07: a mixed site, and the four answers a page route can give.
//!
//! One site is built once and served twice — public, then private — because
//! the difference between the two is the whole of what `SignIn` means. On a
//! public site a restricted page must be indistinguishable from one that does
//! not exist, so a reader without a session gets 404; sending them to a login
//! flow would confirm the page is there. On a private site there is nothing
//! to conceal and a 404 everywhere would leave a reader no way to discover
//! they can sign in, so it redirects.
//!
//! The build half of the file is the part that would not have been written if
//! the chain had been trusted rather than tested. `liyasa_server::auth::
//! groups::decide` has been correct and unit-tested since 2026-09-17; what was
//! missing was any way for the navigation's `groups:` to reach it. Two of the
//! cases below are for hazards in that path rather than for the decision:
//! `/internal/runbooks/failover` sits in a group nested inside a restricted
//! group, which the sidebar renderer drops entirely, and `/partners/hidden`
//! is `hidden: true`, which takes it out of the page list the sidebar is
//! resolved from. Either one inheriting nothing would make it public.

use std::collections::BTreeMap;
use std::path::PathBuf;

use axum::body::Body;
use http::{Request, StatusCode, header};
use liyasa_build::engine::{self, Options};
use liyasa_build::git::NoGit;
use liyasa_config::vfs::OsVfs;
use liyasa_server::auth::session::Principal;
use liyasa_tests::server::{Harness, Setup};
use serde_json::json;

/// `navigation` with a restriction at three depths: a plain group, a
/// restricted group, and a second restriction nested inside it.
const SITE: &str = r#"{
  "name": "Acme docs",
  "seo": { "canonicalOrigin": "https://docs.acme.com" },
  "navigation": [
    "index",
    { "group": "Guides", "pages": ["guides/install", "guides/open"] },
    { "group": "Internal", "groups": ["staff"], "pages": [
        "internal/overview",
        { "group": "Runbooks", "groups": ["sre", "oncall"], "pages": ["internal/runbooks/failover"] }
    ] },
    { "directory": "partners", "groups": ["partner"] }
  ]
}"#;

const PAGES: &[(&str, &str)] = &[
    ("index.md", "---\ntitle: Home\n---\n# Home\n"),
    ("guides/install.md", "---\ntitle: Install\n---\n# Install\n"),
    // `access: public` with no restricted ancestor: the one shape that opens
    // a page on a private site (§7.6).
    (
        "guides/open.md",
        "---\ntitle: Open\naccess: public\n---\n# Open\n",
    ),
    (
        "internal/overview.md",
        "---\ntitle: Overview\n---\n# Overview\n",
    ),
    (
        "internal/runbooks/failover.md",
        "---\ntitle: Failover\n---\n# Failover\n",
    ),
    (
        "partners/pricing.md",
        "---\ntitle: Pricing\n---\n# Pricing\n",
    ),
    // Out of the sidebar, still routable, still under the restricted
    // directory. Naming a hidden page in an explicit `pages` list is E0104 —
    // the renderer resolves those against the sidebar list, which excludes it
    // — so a `directory` node is the only way this shape exists at all.
    (
        "partners/hidden.md",
        "---\ntitle: Hidden\nhidden: true\n---\n# Hidden\n",
    ),
    // `access: public` UNDER a restriction: §7.6's exception is to the site
    // default, not to an ancestor that named groups.
    (
        "partners/open.md",
        "---\ntitle: Partner open\naccess: public\n---\n# Partner open\n",
    ),
];

fn build_site(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("liyasa-auth07-{name}-{}", std::process::id()));
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

/// Every route's access chain, as `[["staff"], []]` — one entry per level,
/// each the groups it declared. `public` is carried as the pseudo-group
/// `access:public` so one shape can assert on both.
fn chains(dist: &std::path::Path) -> BTreeMap<String, Vec<Vec<String>>> {
    let text = std::fs::read_to_string(dist.join("liyasa-manifest.json")).expect("a manifest");
    let manifest: serde_json::Value = serde_json::from_str(&text).expect("the manifest is JSON");
    manifest["routes"]
        .as_array()
        .expect("routes")
        .iter()
        .map(|entry| {
            let levels = entry["access"]
                .as_array()
                .unwrap_or_else(|| panic!("{} has no access chain", entry["route"]))
                .iter()
                .map(|level| {
                    let mut names: Vec<String> = level["groups"]
                        .as_array()
                        .map(|list| {
                            list.iter()
                                .filter_map(|g| g.as_str().map(str::to_owned))
                                .collect()
                        })
                        .unwrap_or_default();
                    if level["public"] == serde_json::Value::Bool(true) {
                        names.push("access:public".to_owned());
                    }
                    names
                })
                .collect();
            (entry["route"].as_str().expect("a route").to_owned(), levels)
        })
        .collect()
}

#[test]
fn the_navigation_tree_becomes_a_chain_per_route() {
    let chains = chains(&build_site("chain"));

    // An unrestricted page still carries one level — its own, empty. `decide`
    // reads `access: public` off `chain.last()`, so a page whose own level
    // were dropped would be judged by its ancestor's flag.
    assert_eq!(chains["/guides/install"], vec![Vec::<String>::new()]);

    assert_eq!(
        chains["/internal/overview"],
        vec![vec!["staff".to_owned()], Vec::new()],
        "one restricted ancestor, then the page"
    );

    // The renderer drops a group nested inside a group's `pages` — `item_of`
    // returns `None` for anything that is not a string. If the access walk
    // mirrored that, this page would inherit nothing at all.
    assert_eq!(
        chains["/internal/runbooks/failover"],
        vec![
            vec!["staff".to_owned()],
            vec!["oncall".to_owned(), "sre".to_owned()],
            Vec::new(),
        ],
        "both ancestors, outermost first, each level's groups sorted"
    );

    // `Indexing::navigation` is `!hidden`, so this page is absent from the
    // list the sidebar is built from. Resolving access against that list
    // would make `hidden: true` a way to publish a restricted page.
    assert_eq!(
        chains["/partners/hidden"],
        vec![vec!["partner".to_owned()], Vec::new()],
        "hidden from the sidebar, still behind the group"
    );

    assert_eq!(
        chains["/guides/open"],
        vec![vec!["access:public".to_owned()]],
        "the page's own level carries its `access: public`"
    );
    assert_eq!(
        chains["/partners/open"],
        vec![vec!["partner".to_owned()], vec!["access:public".to_owned()],],
        "`access: public` does not erase the ancestor that named groups"
    );
}

async fn harness(name: &str, dist: PathBuf, private: bool) -> Harness {
    let mut config = json!({
        "name": "Acme docs",
        "seo": { "canonicalOrigin": "https://docs.acme.com" }
    });
    if private {
        config["auth"] = json!({ "mode": "password" });
    }
    let (harness, _) = Harness::new(Setup {
        dist: Some(dist),
        site_config: Some(config),
        ..Setup::new(name)
    })
    .await;
    harness
}

/// A request already carrying a reader. `auth::layer::extract` inserts a
/// `Principal` only when none is present precisely so a test can decide.
async fn get_as(harness: &Harness, path: &str, groups: Option<&[&str]>) -> http::Response<Body> {
    let mut request = Request::builder()
        .method("GET")
        .uri(path)
        .body(Body::empty())
        .expect("a request");
    if let Some(groups) = groups {
        request.extensions_mut().insert(Principal {
            subject: "reader-1".to_owned(),
            groups: groups.iter().map(|g| (*g).to_owned()).collect(),
            ..Principal::default()
        });
    }
    harness.send(request).await
}

#[tokio::test]
async fn a_public_site_hides_a_restricted_page_rather_than_refusing_it() {
    let harness = harness("auth07-public", build_site("public"), false).await;

    assert_eq!(
        get_as(&harness, "/guides/install", None).await.status(),
        StatusCode::OK,
        "an unrestricted page is untouched"
    );

    // Not 403, and not a redirect to a login flow that does not exist on a
    // public site: either would confirm the page is there.
    assert_eq!(
        get_as(&harness, "/partners/pricing", None).await.status(),
        StatusCode::NOT_FOUND,
        "anonymous"
    );
    assert_eq!(
        get_as(&harness, "/partners/pricing", Some(&["other"]))
            .await
            .status(),
        StatusCode::NOT_FOUND,
        "a session in the wrong group"
    );
    assert_eq!(
        get_as(&harness, "/partners/pricing", Some(&["partner"]))
            .await
            .status(),
        StatusCode::OK,
        "a session in the right group"
    );
}

#[tokio::test]
async fn a_private_site_sends_a_reader_without_a_session_to_sign_in() {
    let harness = harness("auth07-private", build_site("private"), true).await;

    let response = get_as(&harness, "/guides/install", None).await;
    assert_eq!(
        response.status(),
        StatusCode::SEE_OTHER,
        "a private site has nothing to conceal by existing"
    );
    let location = response
        .headers()
        .get(header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    assert!(
        location.starts_with("/_liyasa/auth/login?returnTo="),
        "{location}"
    );
    assert!(
        location.contains("%2Fguides%2Finstall"),
        "the reader comes back to the page they asked for: {location}"
    );

    // A session, and the page restricts nothing.
    assert_eq!(
        get_as(&harness, "/guides/install", Some(&[]))
            .await
            .status(),
        StatusCode::OK
    );

    // A wrong group is a 404 even here: the redirect is for "you might be
    // allowed", and this reader is known not to be.
    assert_eq!(
        get_as(&harness, "/partners/pricing", Some(&["other"]))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get_as(&harness, "/partners/pricing", Some(&["partner"]))
            .await
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn access_public_opens_a_page_and_not_a_restricted_ancestor() {
    let harness = harness("auth07-open", build_site("open"), true).await;

    assert_eq!(
        get_as(&harness, "/guides/open", None).await.status(),
        StatusCode::OK,
        "§7.6: `access: public` serves without a session on a private site"
    );
    assert_eq!(
        get_as(&harness, "/partners/open", None).await.status(),
        StatusCode::SEE_OTHER,
        "the same flag under a `partner` group does not open it — the \
         exception is to the site default, not to an explicit restriction"
    );
    assert_eq!(
        get_as(&harness, "/partners/open", Some(&["other"]))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get_as(&harness, "/partners/open", Some(&["partner"]))
            .await
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn the_content_api_answers_the_same_decision() {
    let public = harness("auth07-api-public", build_site("api-public"), false).await;
    let path = "/_liyasa/api/v1/content?path=/partners/pricing";

    assert_eq!(
        get_as(&public, path, None).await.status(),
        StatusCode::NOT_FOUND,
        "a public site hides it from the API too"
    );
    assert_eq!(
        get_as(&public, path, Some(&["partner"])).await.status(),
        StatusCode::OK
    );

    let private = harness("auth07-api-private", build_site("api-private"), true).await;
    assert_eq!(
        get_as(&private, path, None).await.status(),
        StatusCode::UNAUTHORIZED,
        "a programmatic client cannot follow a login redirect, so it is told"
    );
}
