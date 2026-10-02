//! VER-77's review reminders (the send side).
//!
//! The content is `liyasa-verify`'s; what is asserted here is the automation:
//! that it is registered, that it runs, that it reads the manifest rather than
//! an empty set, and that an instance which cannot read drift records says so
//! rather than reporting a clean site.

use liyasa_server::routes::{reviews, work};
use liyasa_tests::server::{Harness, Setup};

/// A send of zero names which of three things it was.
///
/// `"sent": 0, "reason": null` cannot distinguish them, and the set has grown
/// rather than shrunk now that `review::overdue` has a caller:
///
///   nothing was overdue          the good case
///   overdue but nobody owns it   today's case for every page, because
///                                nothing in the workspace reads a `DOCOWNERS`
///   overdue and owned, no mail   the case the second test below covers
///
/// This test used to assert the reason contained "records yet" — a disclaimer
/// that no production path wrote a `Review` record. WP-20c wrote that as
/// self-retiring and it has retired: the caller exists, so the wording is gone
/// and this asserts the distinction that replaced it. The fixture site's pages
/// carry no `reviewed:` front matter, so **the digest is no longer empty on
/// this instance** — every page is overdue and unowned, which is why the
/// assertions below are about `examined` and `unowned` rather than about a
/// zero.
#[tokio::test(flavor = "multi_thread")]
async fn a_send_of_zero_says_which_of_the_three_reasons_it_was() {
    let setup = Setup {
        drift: true,
        ..Setup::new("ver77-empty")
    };
    let (harness, _site) = Harness::new(setup).await;
    let kinds = [reviews::DIGEST];

    // Nothing written by hand: whatever the row holds, the production pass put
    // it there. That is the whole of what changed — this ran over an empty set
    // on every instance until the caller existed.
    work::fire_timers(&harness.state, &kinds)
        .await
        .expect("a tick");
    work::run_once(&harness.state, &kinds, "replica-a")
        .await
        .expect("a pass");

    let result = digest_result(&harness).await;

    // The denominator first. Without it `"owners": []` cannot be told apart
    // from a pass that read no pages, which is exactly what every run of this
    // job did before `routes::overdue::flag` existed — and it reported the
    // same `"sent": 0`.
    assert!(
        result["examined"].as_u64().is_some_and(|n| n > 0),
        "the pass read the manifest rather than nothing: {result}"
    );
    assert!(
        result["flagged"]["created"].as_u64().is_some_and(|n| n > 0),
        "and recorded what it found; the fixture's pages carry no `reviewed:` \
         date, so every one of them is past its cadence: {result}"
    );

    // Zero sent, and the reason is the second of the three: overdue pages
    // exist and are named, and none of them has an owner to send to.
    assert_eq!(result["sent"], 0, "{result}");
    assert!(
        result["owners"]
            .as_array()
            .is_some_and(|owners| owners.is_empty()),
        "nothing reads a `DOCOWNERS`, so no page resolves an owner: {result}"
    );
    assert!(
        !result["unowned"]
            .as_array()
            .expect("an unowned array")
            .is_empty(),
        "the overdue pages are reported rather than dropped — an owner-keyed \
         digest that folded them in would read as a reviewed site: {result}"
    );
    let reason = result["reason"].as_str().unwrap_or_default();
    assert!(
        reason.contains("overdue") && reason.contains("unowned"),
        "the reason names where the pages went rather than leaving a bare \
         zero: {result}"
    );
}

/// One open `Review` record for a page a month past its cadence, **with an
/// owner**.
///
/// Written through `RecordStore` rather than through `Engine`, and the reason
/// has changed: it used to be that `review::overdue` had no caller at all, so
/// nothing in production produced a candidate. The caller exists now and
/// produces candidates for every overdue page — but it cannot give any of them
/// an owner, because nothing in the workspace reads a `DOCOWNERS` file. So
/// this still stands in for a step nobody has written, and that step is now the
/// owner lookup rather than the flagging.
async fn write_overdue_review(harness: &Harness) {
    use liyasa_verify::core::config::DriftSeverity;
    use liyasa_verify::drift::record::{Candidate, DriftKind};

    let page = liyasa_core::ids::Route::new("/guides/install");
    let record = Candidate::new(
        DriftKind::Review {
            page: page.clone(),
            owners: vec!["docs@example.com".to_owned()],
            reviewed: None,
            cadence: std::time::Duration::from_secs(90 * 86_400),
            overdue_by: std::time::Duration::from_secs(30 * 86_400),
        },
        vec![page],
    )
    .opened(DriftSeverity::Medium, std::time::SystemTime::now());

    harness
        .state
        .drift_records()
        .expect("a record store")
        .save(&record)
        .expect("the record saves");
}

/// The job row this digest wrote, whichever way it ended.
async fn digest_result(harness: &Harness) -> serde_json::Value {
    let store = harness.state.store.clone().expect("a store");
    let rows = store
        .jobs_typed()
        .list(&Default::default(), liyasa_core::store::Page::default())
        .await
        .expect("the jobs are listable");
    rows.iter()
        .find(|job| job.name == reviews::DIGEST_JOB)
        .expect("the digest ran")
        .result
        .clone()
        .unwrap_or_default()
}

#[test]
fn the_digest_is_registered_and_spelled_once() {
    let kind = work::kinds()
        .iter()
        .find(|kind| kind.name == reviews::DIGEST_JOB)
        .expect("the review digest is registered; a job nothing registers never runs");
    assert!(
        matches!(kind.trigger, work::Trigger::Scheduled(_)),
        "nothing enqueues this but the worker's own timer"
    );
}

/// Every replica fires its timer; the day bucket is what leaves one row.
#[tokio::test]
async fn a_second_replicas_tick_adds_no_second_digest() {
    let (harness, _site) = Harness::serving("ver77-timer").await;
    let kinds = [reviews::DIGEST];

    let first = work::fire_timers(&harness.state, &kinds)
        .await
        .expect("a tick");
    let second = work::fire_timers(&harness.state, &kinds)
        .await
        .expect("a second replica's tick");
    assert_eq!((first, second), (1, 0), "rows added, not ticks taken");
}

/// With a store, the job opens a digest rather than skipping.
///
/// This is the half that makes the clause met: the job existing and running is
/// not the automation opening a digest, and until `main` constructed a
/// `RecordStore` every run of it reported "this instance keeps no drift
/// records" — a skip that is a pass in a green gate.
///
/// Multi-threaded on purpose: `RecordStore` is synchronous and `SqliteDrift`
/// bridges to the async pool with `block_in_place`, which panics on a
/// current-thread runtime.
#[tokio::test(flavor = "multi_thread")]
async fn with_a_record_store_the_digest_opens_rather_than_skipping() {
    let setup = Setup {
        drift: true,
        ..Setup::new("ver77-withstore")
    };
    let (harness, _site) = Harness::new(setup).await;
    let kinds = [reviews::DIGEST];

    // One overdue page with an owner, written directly into the store. The
    // production pass flags the fixture's pages too, but every one of them
    // comes out unowned, so without this the `owners` assertions below would
    // pass on an empty array — the shape this file was already repaired for
    // once.
    write_overdue_review(&harness).await;

    work::fire_timers(&harness.state, &kinds)
        .await
        .expect("a tick");
    work::run_once(&harness.state, &kinds, "replica-a")
        .await
        .expect("a pass");

    let result = digest_result(&harness).await;
    assert!(
        result.get("skipped").is_none(),
        "an instance that keeps records does not skip: {result}"
    );
    // The owner that was written above must be NAMED, not merely counted. An
    // earlier version asserted `owners` and `unowned` were merely present,
    // which an empty digest satisfies — and an empty digest was what this
    // instance produced, because nothing in production flagged an overdue
    // page. That assertion was passing on nothing, the same shape as
    // `passages.is_array()` passing on an index that cannot exist. The
    // flagging is wired now; the owner lookup is not, which is why the owned
    // record is still supplied by hand.
    let owners = result["owners"].as_array().expect("an owners array");
    assert_eq!(owners.len(), 1, "one record, one owner: {result}");
    assert_eq!(owners[0]["owner"], "docs@example.com", "{result}");
    assert_eq!(owners[0]["pages"], 1, "{result}");
    // The site configures no `mail` block, so nothing was sent and the row
    // says which of the two it was. "Zero delivered" and "no sender" are
    // different facts and must not read alike.
    assert_eq!(result["sent"], 0, "{result}");
    assert!(
        result["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("mail")),
        "the row names the missing sender rather than reporting a clean send: {result}"
    );
    // An empty site has no owners and nothing unowned, and both fields are
    // present rather than absent — "no overdue pages" and "the digest did not
    // look" must not serialise the same.
    assert!(result.get("owners").is_some(), "{result}");
    assert!(result.get("unowned").is_some(), "{result}");
}

/// An instance with no record store reports that it cannot see drift. It must
/// not report an empty digest: a site nobody can read and a site with nothing
/// overdue serialise to the same thing otherwise, and the second reads as
/// "fully reviewed".
#[tokio::test]
async fn an_instance_that_keeps_no_records_says_so_rather_than_sending_nothing() {
    let (harness, _site) = Harness::serving("ver77-norecords").await;
    let kinds = [reviews::DIGEST];

    work::fire_timers(&harness.state, &kinds)
        .await
        .expect("a tick");
    let ran = work::run_once(&harness.state, &kinds, "replica-a")
        .await
        .expect("a pass");
    assert_eq!(ran, 1);

    let store = harness.state.store.clone().expect("a store");
    let rows = store
        .jobs_typed()
        .list(&Default::default(), liyasa_core::store::Page::default())
        .await
        .expect("the jobs are listable");
    let digest = rows
        .iter()
        .find(|job| job.name == reviews::DIGEST_JOB)
        .expect("the digest ran");

    // A skip completes rather than retrying: a missing precondition would
    // otherwise burn the backoff ladder to `dead` and tell nobody anything.
    assert_eq!(digest.state, liyasa_core::store::JobState::Done);
    let result = digest.result.clone().unwrap_or_default();
    let reason = result
        .get("skipped")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    assert!(
        reason.contains("drift records"),
        "the skip names what is absent rather than reporting a clean site: {result}"
    );
}
