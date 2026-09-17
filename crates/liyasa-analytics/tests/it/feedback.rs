//! ANA-30, against a real application database.
//!
//! Rows go in through `liyasa_store::repos::Feedback`, which is the same
//! writer the feedback endpoint uses, so a schema change that broke this
//! reading would break it there first.

use liyasa_analytics::feedback::{self, FeedbackFilters};
use liyasa_analytics::query::{Grain, Range};
use liyasa_store::records::{FeedbackKind, FeedbackRecord, FeedbackStatus};
use liyasa_store::repos::Feedback;

use crate::support::{DAY, HOUR, T0, app};

fn row(id: &str, route: &str, kind: FeedbackKind, rating: Option<i32>, at: i64) -> FeedbackRecord {
    FeedbackRecord {
        id: id.to_owned(),
        project: None,
        route: route.to_owned(),
        kind,
        rating,
        category: None,
        text: None,
        block_id: None,
        task: None,
        status: FeedbackStatus::Open,
        notes: String::new(),
        created_at: at,
        updated_at: at,
    }
}

async fn seeded(
    name: &str,
    rows: Vec<FeedbackRecord>,
) -> (crate::support::TempDir, Feedback, sqlx::sqlite::SqlitePool) {
    let (dir, pool) = app(name).await;
    let repo = Feedback::new(pool.clone());
    for record in &rows {
        repo.insert(record).await.expect("a feedback row");
    }
    (dir, repo, pool)
}

fn day() -> Range {
    Range::new(T0, T0 + DAY)
}

#[tokio::test]
async fn a_pages_score_is_ups_over_votes_and_agent_reports_are_not_votes() {
    let (_dir, _repo, pool) = seeded(
        "feedback-score",
        vec![
            row("f1", "/a", FeedbackKind::Page, Some(1), T0 + HOUR),
            row("f2", "/a", FeedbackKind::Page, Some(1), T0 + HOUR),
            row("f3", "/a", FeedbackKind::Page, Some(1), T0 + HOUR),
            row("f4", "/a", FeedbackKind::Page, Some(-1), T0 + HOUR),
            row("f5", "/a", FeedbackKind::Agent, None, T0 + HOUR),
            row("f6", "/b", FeedbackKind::Page, Some(-1), T0 + HOUR),
        ],
    )
    .await;

    let pages = feedback::by_page(&pool, day(), 10).await.expect("pages");
    let a = pages.iter().find(|p| p.route == "/a").expect("/a");
    assert_eq!(a.up, 3);
    assert_eq!(a.down, 1);
    assert_eq!(a.score(), Some(0.75));
    assert_eq!(a.agent_reports, 1);
    assert_eq!(
        a.total(),
        4,
        "an agent report is counted beside the score, never inside it"
    );

    let b = pages.iter().find(|p| p.route == "/b").expect("/b");
    assert_eq!(b.score(), Some(0.0));
}

#[tokio::test]
async fn a_page_nobody_rated_has_no_score() {
    let (_dir, _repo, pool) = seeded(
        "feedback-unrated",
        vec![row("f1", "/a", FeedbackKind::Agent, None, T0 + HOUR)],
    )
    .await;
    let pages = feedback::by_page(&pool, day(), 10).await.expect("pages");
    assert_eq!(pages[0].score(), None, "no votes is not a score of zero");
    assert_eq!(pages[0].agent_reports, 1);
}

#[tokio::test]
async fn ratings_over_time_have_a_bucket_for_every_day_in_the_range() {
    let (_dir, _repo, pool) = seeded(
        "feedback-over-time",
        vec![
            row("f1", "/a", FeedbackKind::Page, Some(1), T0 + HOUR),
            row("f2", "/a", FeedbackKind::Page, Some(-1), T0 + HOUR),
            row("f3", "/a", FeedbackKind::Page, Some(1), T0 + 2 * DAY + HOUR),
            row("f4", "/b", FeedbackKind::Page, Some(-1), T0 + HOUR),
        ],
    )
    .await;

    let points =
        feedback::ratings_over_time(&pool, Range::new(T0, T0 + 3 * DAY), Grain::Day, Some("/a"))
            .await
            .expect("points");
    assert_eq!(points.len(), 3);
    assert_eq!(points[0].up, 1);
    assert_eq!(points[0].down, 1);
    assert_eq!(points[0].score(), Some(0.5));
    assert_eq!(points[1].total(), 0);
    assert_eq!(points[1].score(), None, "a day with no votes has no score");
    assert_eq!(points[2].up, 1);

    // Site-wide includes /b.
    let all = feedback::ratings_over_time(&pool, Range::new(T0, T0 + 3 * DAY), Grain::Day, None)
        .await
        .expect("points");
    assert_eq!(all[0].down, 2);
}

#[tokio::test]
async fn the_written_list_filters_by_type_status_and_text() {
    let mut with_text = row("f1", "/a", FeedbackKind::Page, Some(-1), T0 + HOUR);
    with_text.text = Some("the curl example 404s".to_owned());
    with_text.category = Some("incorrect".to_owned());
    let mut agent = row("f2", "/a", FeedbackKind::Agent, None, T0 + 2 * HOUR);
    agent.task = Some("create a payment".to_owned());
    let mut resolved = row("f3", "/b", FeedbackKind::Page, Some(-1), T0 + 3 * HOUR);
    resolved.text = Some("fixed now".to_owned());
    resolved.status = FeedbackStatus::Resolved;
    let (_dir, _repo, pool) = seeded(
        "feedback-list",
        vec![
            with_text,
            agent,
            resolved,
            row("f4", "/c", FeedbackKind::Page, Some(1), T0 + 4 * HOUR),
        ],
    )
    .await;

    let all = feedback::list(&pool, day(), &FeedbackFilters::default(), 50)
        .await
        .expect("a list");
    assert_eq!(all.len(), 4);
    assert_eq!(all[0].id, "f4", "newest first");

    let written = feedback::list(
        &pool,
        day(),
        &FeedbackFilters {
            with_text: true,
            ..FeedbackFilters::default()
        },
        50,
    )
    .await
    .expect("a list");
    assert_eq!(written.len(), 2);

    let agents = feedback::list(
        &pool,
        day(),
        &FeedbackFilters {
            kind: Some(FeedbackKind::Agent),
            ..FeedbackFilters::default()
        },
        50,
    )
    .await
    .expect("a list");
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].id, "f2");
    assert_eq!(agents[0].task.as_deref(), Some("create a payment"));

    let open = feedback::list(
        &pool,
        day(),
        &FeedbackFilters {
            status: Some(FeedbackStatus::Open),
            ..FeedbackFilters::default()
        },
        50,
    )
    .await
    .expect("a list");
    assert_eq!(open.len(), 3, "the resolved one is not open");

    let incorrect = feedback::list(
        &pool,
        day(),
        &FeedbackFilters {
            category: Some("incorrect".to_owned()),
            ..FeedbackFilters::default()
        },
        50,
    )
    .await
    .expect("a list");
    assert_eq!(incorrect.len(), 1);
}

#[tokio::test]
async fn the_status_workflow_and_internal_notes_survive_the_reading() {
    let (_dir, repo, pool) = seeded(
        "feedback-workflow",
        vec![row("f1", "/a", FeedbackKind::Page, Some(-1), T0 + HOUR)],
    )
    .await;
    repo.set_status(
        "f1",
        FeedbackStatus::Triaged,
        Some("assigned to the payments team"),
    )
    .await
    .expect("a status change");

    let listed = feedback::list(&pool, day(), &FeedbackFilters::default(), 50)
        .await
        .expect("a list");
    assert_eq!(listed[0].status, FeedbackStatus::Triaged);
    assert!(listed[0].notes.contains("assigned to the payments team"));

    let counts = feedback::status_counts(&pool, day(), &FeedbackFilters::default())
        .await
        .expect("counts");
    assert_eq!(counts, vec![(FeedbackStatus::Triaged, 1)]);
}

#[tokio::test]
async fn a_route_prefix_filter_means_the_prefix() {
    let (_dir, _repo, pool) = seeded(
        "feedback-prefix",
        vec![
            row("f1", "/guides/a", FeedbackKind::Page, Some(1), T0 + HOUR),
            row("f2", "/guides/b", FeedbackKind::Page, Some(1), T0 + HOUR),
            row("f3", "/reference/c", FeedbackKind::Page, Some(1), T0 + HOUR),
            row("f4", "/g_ides/d", FeedbackKind::Page, Some(1), T0 + HOUR),
        ],
    )
    .await;
    let guides = feedback::list(
        &pool,
        day(),
        &FeedbackFilters {
            route_prefix: Some("/guides".to_owned()),
            ..FeedbackFilters::default()
        },
        50,
    )
    .await
    .expect("a list");
    assert_eq!(guides.len(), 2);

    let literal = feedback::list(
        &pool,
        day(),
        &FeedbackFilters {
            route_prefix: Some("/g_ides".to_owned()),
            ..FeedbackFilters::default()
        },
        50,
    )
    .await
    .expect("a list");
    assert_eq!(literal.len(), 1, "`_` is a literal underscore");
}

#[tokio::test]
async fn a_row_outside_the_range_is_outside_the_range() {
    let (_dir, repo, pool) = seeded(
        "feedback-range",
        vec![row("f1", "/a", FeedbackKind::Page, Some(1), T0 - HOUR)],
    )
    .await;
    assert!(
        feedback::list(&pool, day(), &FeedbackFilters::default(), 50)
            .await
            .expect("a list")
            .is_empty()
    );
}
