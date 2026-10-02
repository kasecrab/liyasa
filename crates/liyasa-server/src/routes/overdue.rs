//! VER-77's caller: the thing that decides a page is past its review cadence.
//!
//! `liyasa_verify::drift::review::overdue` has had no caller outside its own
//! module since it was written, so no production path flagged an overdue page,
//! no `Review` record existed, and `reviews.rs`'s digest ran over an empty set
//! and reported `{"owners": [], "unowned": [], "sent": false}` on every
//! instance. This is that caller.
//!
//! **It runs on elapsed time, not on deploy.** WP-16 offered its build worker,
//! which had five of the six inputs in one function; WP-20c declined on the
//! grounds that a cadence measures elapsed time and so must run on elapsed
//! time — flagging at deploy means the overdue set only moves when somebody
//! deploys, so a site nobody has deployed in six months accrues no flags, and
//! that is exactly the site VER-77 exists to catch. The failure is invisible
//! on every site anybody tests on, because a test site deploys: that is how it
//! gets content.
//!
//! So the caller lives beside the scheduled digest, which already has the
//! right trigger and the manifest in process.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::SystemTime;

use liyasa_core::diagnostics::Diagnostic;
use liyasa_core::document::{Edge, EdgeOrigin};
use liyasa_core::ids::Route;
use liyasa_core::verify::{DriftReport, StoreError};
use liyasa_verify::core::policy::Policy;
use liyasa_verify::drift::engine::{Coverage, Engine, Routes};
use liyasa_verify::drift::owners::Docowners;
use liyasa_verify::drift::record::{DriftKey, DriftKind};
use liyasa_verify::drift::review::{self, Cadence, PageReview};
use liyasa_verify::drift::store::RecordStore;

use super::AppState;

/// What one pass over the manifest produced.
pub struct Flagged {
    pub report: DriftReport,
    /// `W0639` per page whose `reviewed:` is not a date. Carried rather than
    /// logged: such a page is *also* flagged, so a reader who sees only the
    /// count of records would not learn that some of them are there because
    /// Liyasa could not read the date.
    pub problems: Vec<Diagnostic>,
    /// Routes considered, which is the denominator for everything above and
    /// the set `Coverage::Subjects` was built from.
    pub examined: usize,
}

/// A `Routes` the engine never consults, and an error rather than an empty
/// answer if that ever stops being true.
///
/// `Engine::new` requires one, but `Routes::routes` maps a candidate's
/// *blocks* to the pages they render on, and a review candidate is
/// `Candidate::new(kind, pages)` with no blocks — the pages come from the
/// candidate itself. `engine.rs` reads `self.routes` only in `facts::candidates`.
///
/// Returning `Ok(Vec::new())` would be the quiet version: a future caller that
/// did pass blocks would get no routes and no complaint, which is the shape
/// this package keeps finding in other people's code. So an empty block list
/// answers empty and anything else fails by name.
struct NoBlocks;

impl Routes for NoBlocks {
    fn routes(&self, blocks: &[(EdgeOrigin, Vec<Edge>)]) -> Result<Vec<Route>, StoreError> {
        if blocks.is_empty() {
            return Ok(Vec::new());
        }
        Err(StoreError::Io(
            "the review pass has no route pairing: it supplies candidates whose pages are \
             already known and passes no blocks, so a block reaching this point means a \
             caller changed and needs a real `Routes`"
                .to_owned(),
        ))
    }
}

/// Flags every page in the bundle's manifest that is past its cadence, and
/// reconciles the result against what is already recorded.
///
/// `None` when this instance serves no bundle, which is the collector shape:
/// there is no manifest, so there are no pages to have an opinion about.
pub fn flag(
    state: &Arc<AppState>,
    records: &Arc<dyn RecordStore>,
    now: SystemTime,
) -> Option<Result<Flagged, StoreError>> {
    let bundle = state.bundle.clone()?;
    let pages: Vec<PageReview> = bundle
        .manifest()
        .routes
        .iter()
        .map(|entry| PageReview {
            route: entry.route.clone(),
            // WP-06's `RouteEntry.reviewed`, straight off the front matter.
            // This was the last missing input and landed in this morning's
            // chain; before it, the build dropped the key before the manifest
            // and nothing downstream could recover it.
            reviewed: entry.reviewed.clone(),
            // No source reachable from here, and this is not an oversight to
            // paper over. `liyasa_git::history::History::last_author` needs a
            // git repository; a server holds the built `dist/`, not the
            // project, and `AppState` has no source root. The fallback owner
            // therefore stays `None`, which makes a page with no `DOCOWNERS`
            // rule *unowned* rather than silently attributed.
            last_author: None,
            // Likewise. VER-77's weight orders an owner's reminder worst
            // first, and `liyasa-analytics` exposes no per-route traffic
            // figure. `body_for` already prints "no traffic data" for `None`
            // rather than a zero it does not know.
            weight: None,
        })
        .collect();

    // Empty, because nothing in the workspace reads a `DOCOWNERS` file:
    // `Docowners::parse` has only test callers, the build neither records it
    // in the manifest nor copies it into the bundle, and the schema has no key
    // for it. So every overdue page comes out unowned today, which
    // `review::digest` reports in `unowned` and `summarise` prints beside the
    // owners rather than folding in — absent and empty must not serialise to
    // the same thing. Swapping this for a real file is a one-line change here
    // once something carries the content.
    let owners = Docowners::parse("");
    let cadence = Cadence::from_config(&liyasa_config::review::review_cadence(
        &state.config.site_config,
    ));
    let mut over = review::overdue(&pages, &owners, &cadence, now);

    // **This pass must not destroy owner information it cannot produce.**
    //
    // `Engine::updated` replaces a stored record's kind with the candidate's
    // wholesale (`kind: candidate.kind.clone()`), and
    // `DriftKind::Review.owners` is a `Vec<String>` with no way to say
    // "unknown" — so a pass that resolves no owner asserts *no owners* rather
    // than *I could not tell*, and the second observation of a page silently
    // blanked an owner something else had recorded. Caught by
    // `ver_77_reminders::with_a_record_store_the_digest_opens_rather_than_skipping`,
    // whose owned record this pass wiped on its first real run.
    //
    // Carried forward rather than guessed: the owners come from the stored
    // record for the same page, and only when this pass found none. Once
    // something reads a `DOCOWNERS` the lookup wins and this becomes dead
    // weight — a `find` per overdue page, which is the overdue set and not the
    // site.
    for candidate in &mut over.candidates {
        let DriftKind::Review { page, owners, .. } = &mut candidate.kind else {
            continue;
        };
        if !owners.is_empty() {
            continue;
        }
        let stored = match records.find(&DriftKey::Review(page.clone())) {
            Ok(stored) => stored,
            Err(error) => return Some(Err(error)),
        };
        if let Some(DriftKind::Review {
            owners: recorded, ..
        }) = stored.map(|record| record.kind)
            && !recorded.is_empty()
        {
            *owners = recorded;
        }
    }

    // Every route examined, not every route flagged. This is what lets a page
    // that has since been reviewed close its record: `close_absent` closes an
    // open record whose key the run covered and which produced no candidate,
    // and `Coverage::Observed` — the default — covers nothing and so closes
    // nothing ever. A key is `DriftKey::Review(route)`, on the page alone, so
    // it is stable while `overdue_by` grows and a daily pass updates one
    // record rather than opening a new one each morning.
    let subjects: BTreeSet<DriftKey> = pages
        .iter()
        .map(|page| DriftKey::Review(page.route.clone()))
        .collect();

    let (verify, _) = liyasa_verify::core::config::VerifyConfig::from_value(
        state
            .config
            .site_config
            .get("verify")
            .unwrap_or(&serde_json::Value::Null),
    );
    let routes = NoBlocks;
    let engine = Engine::new(records.as_ref(), &verify.drift, &routes)
        .at(now)
        .covering(Coverage::Subjects(subjects));

    // `Policy::new()` is correct rather than a placeholder: `class_of` maps
    // `DriftKind::Review` to `None`, and `watched` reads
    // `class_of(..).is_none_or(..)`, so a review candidate is watched whatever
    // `verify.policy` says. There is no class to switch off, which is why the
    // off-class freeze WP-20c found cannot reach this kind.
    let report = match engine.record(&over.candidates, &Policy::new()) {
        Ok(report) => report,
        Err(error) => return Some(Err(error)),
    };

    Some(Ok(Flagged {
        report,
        problems: over.problems,
        examined: pages.len(),
    }))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use liyasa_build::hosting::Rules;
    use liyasa_build::manifest::{Manifest, RouteEntry};
    use liyasa_core::ids::{BuildId, Fingerprint, PageId};
    use liyasa_verify::drift::record::DriftKind;
    use liyasa_verify::drift::store::MemoryDrift;

    use super::*;
    use crate::routes::ServerConfig;
    use crate::routes::bundle::Bundle;

    /// A fixed `now`, read through the same parser the production path uses
    /// rather than written as an epoch literal: a magic number here would be a
    /// second implementation of the date format, and the test would pass while
    /// disagreeing with `parse_date` about what the front matter means.
    fn now() -> SystemTime {
        review::parse_date("2026-10-02").expect("the fixture date parses")
    }

    fn page(route: &str, reviewed: Option<&str>) -> RouteEntry {
        RouteEntry {
            reviewed: reviewed.map(str::to_owned),
            ..RouteEntry::new(
                Route::new(route),
                format!("{}.md", route.trim_start_matches('/')),
            )
        }
    }

    fn state(pages: Vec<RouteEntry>, site_config: serde_json::Value) -> Arc<AppState> {
        let manifest = Manifest {
            routes: pages,
            ..Manifest::new(BuildId(Fingerprint::of("build")), "0.1.0", 0)
        };
        let config = ServerConfig {
            site_config: Arc::new(site_config),
            ..ServerConfig::default()
        };
        let mut app = AppState::new(config);
        app.bundle = Some(Arc::new(Bundle::new(
            std::path::PathBuf::from("/nonexistent"),
            manifest,
            Rules::default(),
        )));
        Arc::new(app)
    }

    fn store() -> Arc<dyn RecordStore> {
        Arc::new(MemoryDrift::new())
    }

    fn reviewed_routes(records: &Arc<dyn RecordStore>) -> Vec<String> {
        let mut out: Vec<String> = records
            .open_records()
            .expect("the records are readable")
            .iter()
            .filter_map(|record| match &record.kind {
                DriftKind::Review { page, .. } => Some(page.as_str().to_owned()),
                _ => None,
            })
            .collect();
        out.sort();
        out
    }

    /// The clause: a page past its cadence is flagged and one inside it is not.
    ///
    /// Asserted as the exact set rather than a count. A count of one would pass
    /// if the wrong page were flagged, and "the wrong page" is the live failure
    /// here — the whole pass turns on comparing `reviewed` against a window, so
    /// a sign error or a swapped comparison produces one record either way.
    #[test]
    fn a_page_past_its_cadence_is_flagged_and_a_fresh_one_is_not() {
        let app = state(
            vec![
                page("/stale", Some("2020-01-01")),
                page("/fresh", Some("2026-10-01")),
            ],
            serde_json::json!({ "name": "docs" }),
        );
        let records = store();
        let flagged = flag(&app, &records, now())
            .expect("an instance with a bundle has pages to read")
            .expect("the pass records");

        assert_eq!(flagged.examined, 2, "both pages were read");
        assert_eq!(flagged.report.created, 1);
        assert_eq!(
            reviewed_routes(&records),
            ["/stale"],
            "the page six years past a 180-day cadence, and only that one"
        );
    }

    /// A page that has never been reviewed is flagged, because no date is not
    /// evidence of freshness.
    #[test]
    fn a_page_with_no_reviewed_date_is_flagged() {
        let app = state(
            vec![page("/undated", None)],
            serde_json::json!({ "name": "docs" }),
        );
        let records = store();
        flag(&app, &records, now()).expect("a bundle").expect("ok");
        assert_eq!(reviewed_routes(&records), ["/undated"]);
    }

    /// An unreadable date flags the page *and* reports `W0639`.
    ///
    /// Both halves, because either alone is a defect: reporting without
    /// flagging treats a date Liyasa cannot read as a date it believes, and
    /// flagging without reporting gives the author no way to learn the key is
    /// malformed.
    #[test]
    fn an_unreadable_date_is_both_flagged_and_reported() {
        let app = state(
            vec![page("/bad", Some("last Tuesday"))],
            serde_json::json!({ "name": "docs" }),
        );
        let records = store();
        let flagged = flag(&app, &records, now()).expect("a bundle").expect("ok");

        assert_eq!(reviewed_routes(&records), ["/bad"], "flagged");
        assert_eq!(flagged.problems.len(), 1, "and reported");
        assert!(
            flagged.problems[0].message.contains("/bad"),
            "the message names the route, because there is no span to point at: {}",
            flagged.problems[0].message
        );
    }

    /// `content.reviewCadence` is read, so an operator can shorten the window.
    ///
    /// The same page and the same `now` as the fresh case above, which passes
    /// under the 180-day default and fails under a 7-day cadence. Without this
    /// the config read could be deleted and every other test here would still
    /// pass.
    #[test]
    fn a_shorter_configured_cadence_flags_a_page_the_default_would_not() {
        let pages = vec![page("/weekly", Some("2026-09-01"))];
        let default = store();
        flag(
            &state(pages.clone(), serde_json::json!({ "name": "docs" })),
            &default,
            now(),
        )
        .expect("a bundle")
        .expect("ok");
        assert!(
            reviewed_routes(&default).is_empty(),
            "a month old is inside VER-77's 180-day default"
        );

        let weekly = store();
        flag(
            &state(
                pages,
                serde_json::json!({ "content": { "reviewCadence": "7d" } }),
            ),
            &weekly,
            now(),
        )
        .expect("a bundle")
        .expect("ok");
        assert_eq!(
            reviewed_routes(&weekly),
            ["/weekly"],
            "and outside a configured seven days"
        );
    }

    /// A daily pass updates one record rather than opening a new one each
    /// morning.
    ///
    /// `DriftKey::Review` is keyed on the page alone, so the key is stable
    /// while `overdue_by` grows. Were it keyed on the kind's fields the second
    /// morning would open a second record and an owner would be reminded about
    /// the same page once per day forever.
    #[test]
    fn a_second_pass_updates_the_same_record() {
        let app = state(
            vec![page("/stale", Some("2020-01-01"))],
            serde_json::json!({ "name": "docs" }),
        );
        let records = store();
        let first = flag(&app, &records, now()).expect("a bundle").expect("ok");
        let second = flag(&app, &records, now() + Duration::from_secs(86_400))
            .expect("a bundle")
            .expect("ok");

        assert_eq!(first.report.created, 1);
        assert_eq!(second.report.created, 0, "no second record for one page");
        assert_eq!(reviewed_routes(&records), ["/stale"]);
    }

    /// A page since reviewed stops being reported as overdue.
    ///
    /// This is the test for `Coverage::Subjects`. The default `Observed`
    /// closes nothing, ever — so without the subject set a record opened once
    /// would stay open after the page was reviewed, and the owner would be
    /// reminded about work they had already done. `verify.drift.autoResolve`
    /// is false by default, so the record is marked `gone_since` and waits for
    /// approval rather than closing itself: the assertion is that the engine
    /// *noticed*, not that it resolved.
    #[test]
    fn a_page_reviewed_since_is_marked_gone_rather_than_left_open() {
        let records = store();
        flag(
            &state(
                vec![page("/stale", Some("2020-01-01"))],
                serde_json::json!({ "name": "docs" }),
            ),
            &records,
            now(),
        )
        .expect("a bundle")
        .expect("ok");
        assert_eq!(reviewed_routes(&records), ["/stale"]);

        // The same route, now carrying a fresh date.
        flag(
            &state(
                vec![page("/stale", Some("2026-10-01"))],
                serde_json::json!({ "name": "docs" }),
            ),
            &records,
            now(),
        )
        .expect("a bundle")
        .expect("ok");

        let record = records
            .all()
            .expect("readable")
            .into_iter()
            .find(|record| matches!(&record.kind, DriftKind::Review { .. }))
            .expect("the record survives rather than being deleted");
        assert!(
            record.gone_since.is_some(),
            "the pass covered this page and it produced no candidate, so the condition \
             no longer holds: {record:?}"
        );
    }

    /// A stored owner survives a pass that cannot resolve one.
    ///
    /// `Engine::updated` replaces the stored kind with the candidate's
    /// wholesale, and `DriftKind::Review.owners` cannot say "unknown" — so
    /// without carrying it forward the second observation of an owned page
    /// blanks the owner, and the reminder that page existed to send stops
    /// being sent. There is no error and no count: `owners` is still a valid
    /// empty list.
    ///
    /// Found by the integration test rather than here, on this pass's first
    /// real run against a store that already held an owned record.
    #[test]
    fn a_stored_owner_survives_a_pass_that_resolves_none() {
        let records = store();
        let route = Route::new("/owned");
        let owned = liyasa_verify::drift::record::Candidate::new(
            DriftKind::Review {
                page: route.clone(),
                owners: vec!["docs@example.com".to_owned()],
                reviewed: None,
                cadence: Duration::from_secs(90 * 86_400),
                overdue_by: Duration::from_secs(30 * 86_400),
            },
            vec![route],
        )
        .opened(
            liyasa_verify::core::config::DriftSeverity::Medium,
            now() - Duration::from_secs(86_400),
        );
        records.save(&owned).expect("the record saves");

        let app = state(
            vec![page("/owned", Some("2020-01-01"))],
            serde_json::json!({ "name": "docs" }),
        );
        flag(&app, &records, now()).expect("a bundle").expect("ok");

        let stored = records
            .find(&DriftKey::Review(Route::new("/owned")))
            .expect("readable")
            .expect("the record survives");
        let DriftKind::Review { owners, .. } = &stored.kind else {
            panic!("a review record: {stored:?}");
        };
        assert_eq!(
            owners,
            &["docs@example.com".to_owned()],
            "the pass resolves no owner and must not overwrite one that is \
             recorded: {stored:?}"
        );
    }

    /// A collector serves no bundle, which is not a failure and not an empty
    /// result.
    #[test]
    fn an_instance_with_no_bundle_has_nothing_to_read() {
        let config = ServerConfig {
            site_config: Arc::new(serde_json::json!({ "name": "docs" })),
            ..ServerConfig::default()
        };
        let app = Arc::new(AppState::new(config));
        assert!(flag(&app, &store(), now()).is_none());
    }

    /// The `Routes` the engine never consults answers empty for no blocks and
    /// fails by name for any, rather than quietly returning no routes.
    #[test]
    fn the_route_pairing_refuses_a_block_it_cannot_place() {
        assert_eq!(NoBlocks.routes(&[]).expect("no blocks"), Vec::new());
        let error = NoBlocks
            .routes(&[(
                EdgeOrigin::Page(
                    PageId::parse("00000000000000000000000000").expect("the nil ULID parses"),
                ),
                Vec::new(),
            )])
            .expect_err("a block has no pairing here");
        assert!(
            format!("{error}").contains("no route pairing"),
            "the failure says why rather than returning an empty answer: {error}"
        );
    }
}
