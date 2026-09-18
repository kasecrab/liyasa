//! Every endpoint the dashboard declares `dashboard-read` should sit behind a
//! guarded subtree (defect 65).
//!
//! Two failure modes are live at once and they look like opposites: endpoints
//! open to anyone, and endpoints nobody can ever reach. One cause — nothing
//! inserts a `Principal`, so `routes::mount::guarded` answers 401 to every
//! caller, and a route that is not registered at all answers 404. Neither
//! fails any test, which is why this one exists.
//!
//! `web/dashboard/src/api.ts` is the source of truth on purpose: it is the
//! declaration the routes are supposed to satisfy, written by the package that
//! consumes them. A test that read the router would only ever agree with
//! itself.
//!
//! This is a ratchet, not a boolean. The ungated set cannot reach zero today,
//! so a pass/fail assertion would have to be `false` and would say nothing
//! about progress. Instead the set is pinned: it may only be edited downward,
//! and an endpoint that becomes ungated fails this test loudly.

use std::path::PathBuf;

use http::StatusCode;
use liyasa_tests::server::Harness;

/// One row of `ENDPOINTS` in `web/dashboard/src/api.ts`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Declared {
    id: String,
    method: String,
    path: String,
}

/// How an unauthenticated request to a declared endpoint was answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reached {
    /// Behind a permission: 401 without a session. What every row here should
    /// eventually be.
    Guarded,
    /// Served to anyone. The set this test ratchets down.
    Ungated,
    /// No route at all — the request fell through to the site handler. Not a
    /// disclosure, and not reachable either.
    NotRouted,
}

fn api_ts() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("web/dashboard/src/api.ts");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// Pulls the declared rows out of the TypeScript rather than duplicating them.
/// A row this cannot parse is a failure, not a silent omission: a declaration
/// that slips past the parser would look like compliance.
fn declared_dashboard_read(source: &str) -> Vec<Declared> {
    let mut out = Vec::new();
    for line in source.lines() {
        if !line.contains("auth: \"dashboard-read\"") {
            continue;
        }
        let field = |name: &str| -> Option<String> {
            let at = line.find(&format!("{name}: "))? + name.len() + 2;
            let rest = &line[at..];
            let quote = rest.chars().next()?;
            let (open, close) = match quote {
                '"' => ('"', '"'),
                '`' => ('`', '`'),
                _ => return None,
            };
            let start = rest.find(open)? + 1;
            let end = rest[start..].find(close)? + start;
            Some(rest[start..end].to_owned())
        };
        let id = field("id").unwrap_or_else(|| panic!("no `id` in: {line}"));
        let method = field("method").unwrap_or_else(|| panic!("no `method` in: {line}"));
        let path = field("path")
            .unwrap_or_else(|| panic!("no `path` in: {line}"))
            // `${API_BASE}` is the one interpolation the table uses.
            .replace("${API_BASE}", "/_liyasa/api/v1");
        out.push(Declared { id, method, path });
    }
    assert!(
        !out.is_empty(),
        "no `dashboard-read` rows parsed out of api.ts; the table's shape changed \
         and this test would otherwise pass by finding nothing"
    );
    out
}

/// A concrete path: the declaration carries `{env}` and similar.
fn concrete(path: &str) -> String {
    let mut out = String::new();
    let mut skipping = false;
    for c in path.chars() {
        match c {
            '{' => {
                skipping = true;
                out.push('x');
            }
            '}' => skipping = false,
            _ if !skipping => out.push(c),
            _ => {}
        }
    }
    out
}

async fn classify(harness: &Harness, endpoint: &Declared) -> Reached {
    let response = harness
        .request(&endpoint.method, &concrete(&endpoint.path))
        .await;
    let status = response.status();
    let content_type = liyasa_tests::server::header(&response, "content-type")
        .unwrap_or_default()
        .to_owned();

    if status == StatusCode::UNAUTHORIZED {
        return Reached::Guarded;
    }
    // A 404 from a handler is problem details; a 404 from the site fallback is
    // the 404 page. Only the second means nothing is routed there.
    if status == StatusCode::NOT_FOUND && !content_type.starts_with("application/problem+json") {
        return Reached::NotRouted;
    }
    Reached::Ungated
}

/// The endpoints served to anyone today.
///
/// EDIT THIS DOWNWARD ONLY. An entry leaves when its route moves behind a
/// guarded subtree; nothing should ever be added. Sorted so a diff is legible.
const UNGATED_TODAY: &[&str] = &[
    // WP-16's deploy subtree, registered with `permission: None`.
    "builds.activate",
    "builds.queue",
    "builds.status",
    "builds.trigger",
    // This package's own base router, which carries no permission layer.
    "content.tree",
    "deployments.current",
    "deployments.history",
    "deployments.latest",
    "deployments.list",
    "deployments.retained",
    "deployments.rollback",
    "feedback.list",
    "feedback.status",
    "feedback.summary",
    "jobs.cancel",
    "jobs.list",
    "jobs.retry",
];

#[tokio::test]
async fn no_endpoint_becomes_ungated_that_was_not_already() {
    let (harness, _site) = Harness::serving("ratchet").await;
    let declared = declared_dashboard_read(&api_ts());

    let mut ungated = Vec::new();
    let mut not_routed = Vec::new();
    for endpoint in &declared {
        match classify(&harness, endpoint).await {
            Reached::Ungated => ungated.push(endpoint.id.clone()),
            Reached::NotRouted => not_routed.push(endpoint.id.clone()),
            Reached::Guarded => {}
        }
    }
    ungated.sort();
    not_routed.sort();

    let pinned: Vec<String> = UNGATED_TODAY.iter().map(|s| (*s).to_owned()).collect();
    assert_eq!(
        ungated,
        pinned,
        "\nthe set of `dashboard-read` endpoints served to anyone has CHANGED.\n\
         If it grew, an endpoint lost its guard and that is the defect this test \
         exists to catch.\n\
         If it shrank, edit UNGATED_TODAY down to match and say so in the commit.\n\
         For context, {} of {} declared endpoints are not routed at all: {:?}\n",
        not_routed.len(),
        declared.len(),
        not_routed
    );
}

#[test]
fn the_pin_is_sorted_and_has_no_duplicates() {
    // A pin that drifts out of order makes its own diffs unreadable, which is
    // how a ratchet stops being read.
    let mut sorted: Vec<&str> = UNGATED_TODAY.to_vec();
    sorted.sort_unstable();
    assert_eq!(
        UNGATED_TODAY,
        sorted.as_slice(),
        "UNGATED_TODAY is not sorted"
    );
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        UNGATED_TODAY.len(),
        "UNGATED_TODAY repeats an entry"
    );
}

#[test]
fn every_pinned_endpoint_is_still_declared() {
    // An entry that outlives its declaration would sit here for ever looking
    // like outstanding work.
    let declared = declared_dashboard_read(&api_ts());
    for id in UNGATED_TODAY {
        assert!(
            declared.iter().any(|d| d.id == *id),
            "`{id}` is pinned as ungated and api.ts no longer declares it"
        );
    }
}

#[test]
fn a_declaration_the_parser_cannot_read_is_a_failure() {
    // The parser is the weak point: a table shape it silently skipped would
    // read as compliance. This pins what it must be able to read.
    let rows = declared_dashboard_read(&api_ts());
    assert!(
        rows.len() >= 30,
        "only {} `dashboard-read` rows parsed; api.ts declares far more",
        rows.len()
    );
    for row in &rows {
        assert!(
            row.path.starts_with('/'),
            "`{}` has no path: {:?}",
            row.id,
            row
        );
        assert!(
            !row.path.contains("${"),
            "`{}` kept an uninterpolated expression: {}",
            row.id,
            row.path
        );
        assert!(
            ["GET", "POST", "PATCH", "PUT", "DELETE"].contains(&row.method.as_str()),
            "`{}` has an odd method: {}",
            row.id,
            row.method
        );
    }
}
