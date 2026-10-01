//! VER-77's review reminders (the send side).
//!
//! The content is `liyasa-verify`'s; what is asserted here is the automation:
//! that it is registered, that it runs, and that an instance which cannot read
//! drift records says so rather than reporting a clean site.

use liyasa_server::routes::{reviews, work};
use liyasa_tests::server::Harness;

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
