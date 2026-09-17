//! The actions the cards link to (ANA-20, ANA-30), against the real job table.

use liyasa_analytics::actions;
use liyasa_core::store::{JobQuery, JobState, Page};
use liyasa_store::jobs::Jobs;

use crate::support::app;

#[tokio::test]
async fn creating_a_page_for_a_query_queues_one_job_however_often_it_is_asked() {
    let (_dir, pool) = app("actions-create-page").await;
    let jobs = Jobs::new(pool);

    let first = jobs
        .enqueue(&actions::create_page_for_query("sso saml", None))
        .await
        .expect("a job");
    let again = jobs
        .enqueue(&actions::create_page_for_query("sso saml", None))
        .await
        .expect("a job");
    assert_eq!(
        first.id(),
        again.id(),
        "two people asking for the same page is one task for the agent"
    );

    let other = jobs
        .enqueue(&actions::create_page_for_query("terraform", None))
        .await
        .expect("a job");
    assert_ne!(
        first.id(),
        other.id(),
        "a different query is a different task"
    );

    let queued = jobs
        .list(
            &JobQuery {
                name: Some(actions::CREATE_PAGE.to_owned()),
                state: Some(JobState::Queued),
                ..JobQuery::default()
            },
            Page::default(),
        )
        .await
        .expect("a list");
    assert_eq!(queued.len(), 2);
    let payload = &queued
        .iter()
        .find(|j| j.payload["query"] == "sso saml")
        .expect("the query's job")
        .payload;
    assert_eq!(payload["reason"], "no_result_search");
}

#[tokio::test]
async fn asking_the_agent_to_fix_a_page_is_one_job_per_page_not_per_report() {
    let (_dir, pool) = app("actions-fix-page").await;
    let jobs = Jobs::new(pool);

    let first = jobs
        .enqueue(&actions::fix_page("/payments/create", "f1", None))
        .await
        .expect("a job");
    let second = jobs
        .enqueue(&actions::fix_page("/payments/create", "f2", None))
        .await
        .expect("a job");
    assert_eq!(
        first.id(),
        second.id(),
        "five reports of one broken page are one job"
    );

    let other = jobs
        .enqueue(&actions::fix_page("/payments/refund", "f3", None))
        .await
        .expect("a job");
    assert_ne!(first.id(), other.id());
}

#[tokio::test]
async fn the_scheduled_passes_are_keyed_so_a_refresh_does_not_queue_a_second() {
    let (_dir, pool) = app("actions-scheduled").await;
    let jobs = Jobs::new(pool);
    let day = 20_710i64;
    assert_eq!(
        jobs.enqueue(&actions::refresh_insights(day, None))
            .await
            .expect("a job")
            .id(),
        jobs.enqueue(&actions::refresh_insights(day, None))
            .await
            .expect("a job")
            .id()
    );
    assert_ne!(
        jobs.enqueue(&actions::refresh_insights(day, None))
            .await
            .expect("a job")
            .id(),
        jobs.enqueue(&actions::refresh_insights(day + 1, None))
            .await
            .expect("a job")
            .id()
    );
    // The other two scheduled passes enqueue and are distinct jobs.
    assert_ne!(
        jobs.enqueue(&actions::send_digest(day, None))
            .await
            .expect("a job")
            .id(),
        jobs.enqueue(&actions::run_retention(day))
            .await
            .expect("a job")
            .id()
    );
}
