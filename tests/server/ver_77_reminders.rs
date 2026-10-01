//! VER-77's review reminders (the send side).
//!
//! The content is `liyasa-verify`'s; what is asserted here is the automation:
//! that it is registered, that it runs, and that an instance which cannot read
//! drift records says so rather than reporting a clean site.

use liyasa_server::routes::{reviews, work};
use liyasa_tests::server::{Harness, Setup};

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
