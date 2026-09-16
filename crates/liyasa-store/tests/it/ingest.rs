//! The bounded ingest queue and its batch writer (ANA-08).

use std::sync::atomic::Ordering;

use liyasa_store::ingest::{IngestOptions, IngestQueue, RawSink, Writer};
use liyasa_store::records::EventRecord;
use sqlx::Row;

use crate::support::TempDir;

fn event(kind: &str, route: &str) -> EventRecord {
    EventRecord {
        ts: liyasa_store::now_ms(),
        site: "acme-docs".to_owned(),
        env: "production".to_owned(),
        route: route.to_owned(),
        kind: kind.to_owned(),
        caller: serde_json::json!({"kind": "agent", "agent_name": "claude-code"}),
        format: "markdown".to_owned(),
        ..EventRecord::default()
    }
}

#[test]
fn a_full_queue_drops_interaction_events_before_views_and_never_drops_the_rest() {
    let queue = IngestQueue::new(3, 3);
    for _ in 0..3 {
        queue.push(event("scroll_depth", "/a")).expect("room");
    }
    assert_eq!(queue.depth(), 3);

    // A page view evicts an interaction event rather than being refused.
    queue.push(event("page_view", "/a")).expect("a view wins");
    assert_eq!(queue.depth(), 3);
    assert_eq!(queue.metrics.dropped_interaction.load(Ordering::Relaxed), 1);

    // A deployment event evicts a view, never the other way round.
    for _ in 0..2 {
        queue.push(event("page_view", "/a")).expect("a view");
    }
    queue.push(event("deployment", "/")).expect("a deployment");
    assert_eq!(
        queue.metrics.dropped_search_or_view.load(Ordering::Relaxed),
        1
    );

    // With nothing below it to evict, a full queue refuses the lowest class.
    let queue = IngestQueue::new(1, 1);
    queue.push(event("deployment", "/")).expect("room");
    assert!(queue.push(event("scroll_depth", "/a")).is_err());
    assert_eq!(queue.metrics.dropped_interaction.load(Ordering::Relaxed), 1);
}

#[test]
fn the_highest_class_leaves_the_queue_first() {
    let queue = IngestQueue::new(10, 10);
    queue.push(event("scroll_depth", "/a")).expect("room");
    queue.push(event("deployment", "/")).expect("room");
    queue.push(event("page_view", "/a")).expect("room");
    let drained = queue.drain(10);
    let kinds: Vec<&str> = drained.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(kinds, ["deployment", "page_view", "scroll_depth"]);
    assert_eq!(queue.depth(), 0);
}

#[tokio::test]
async fn a_batch_writes_raw_rows_and_aggregates_in_one_transaction() {
    let dir = TempDir::new("ingest-batch");
    let queue = IngestQueue::new(100, 10);
    let mut writer = Writer::open(
        &dir.join("analytics.db"),
        queue.clone(),
        IngestOptions::default(),
    )
    .await
    .expect("an analytics database");

    for _ in 0..3 {
        queue.push(event("page_view", "/payments")).expect("room");
    }
    queue.push(event("page_view", "/install")).expect("room");
    assert_eq!(writer.flush().await.expect("a batch"), 4);
    assert_eq!(writer.flush().await.expect("an empty batch"), 0);

    let raw: i64 = sqlx::query("SELECT COUNT(*) AS n FROM event")
        .fetch_one(writer.pool())
        .await
        .expect("a row")
        .try_get("n")
        .expect("a count");
    assert_eq!(raw, 4);

    let series = writer
        .hourly("acme-docs", "/payments")
        .await
        .expect("a series");
    assert_eq!(series.len(), 1, "one hour");
    assert_eq!(series[0].1, 3, "three views of that route");
    assert_eq!(queue.metrics.written.load(Ordering::Relaxed), 4);

    writer.checkpoint().await.expect("a checkpoint");
}

#[tokio::test]
async fn the_file_sink_keeps_raw_events_out_of_the_database() {
    let dir = TempDir::new("ingest-files");
    let segments = dir.join("segments");
    let queue = IngestQueue::new(100, 10);
    let mut writer = Writer::open(
        &dir.join("analytics.db"),
        queue.clone(),
        IngestOptions {
            raw_sink: RawSink::Files(segments.clone()),
            ..IngestOptions::default()
        },
    )
    .await
    .expect("an analytics database");

    queue.push(event("page_view", "/install")).expect("room");
    writer.flush().await.expect("a batch");

    let raw: i64 = sqlx::query("SELECT COUNT(*) AS n FROM event")
        .fetch_one(writer.pool())
        .await
        .expect("a row")
        .try_get("n")
        .expect("a count");
    assert_eq!(
        raw, 0,
        "raw rows go to the segments, aggregates to the table"
    );
    assert_eq!(
        writer
            .hourly("acme-docs", "/install")
            .await
            .expect("a series")[0]
            .1,
        1
    );

    let written: Vec<String> = std::fs::read_dir(&segments)
        .expect("a segment directory")
        .filter_map(|e| Some(e.ok()?.file_name().to_string_lossy().into_owned()))
        .collect();
    assert_eq!(written.len(), 1, "{written:?}");
    assert!(written[0].starts_with("events-") && written[0].ends_with(".ndjson"));
    let lines = std::fs::read_to_string(segments.join(&written[0])).expect("a segment");
    assert_eq!(lines.lines().count(), 1);
    let parsed: EventRecord = serde_json::from_str(lines.lines().next().expect("a line"))
        .expect("a segment line is one event");
    assert_eq!(parsed.route, "/install");
}

#[tokio::test]
async fn drops_are_counted_in_the_database_as_well_as_the_metric() {
    let dir = TempDir::new("ingest-drops");
    let queue = IngestQueue::new(1, 1);
    let mut writer = Writer::open(
        &dir.join("analytics.db"),
        queue.clone(),
        IngestOptions::default(),
    )
    .await
    .expect("an analytics database");

    queue.push(event("deployment", "/")).expect("room");
    assert!(queue.push(event("scroll_depth", "/a")).is_err());
    writer.flush().await.expect("a batch");

    let dropped: i64 = sqlx::query("SELECT SUM(count) AS n FROM ingest_drops")
        .fetch_one(writer.pool())
        .await
        .expect("a row")
        .try_get("n")
        .expect("a count");
    assert_eq!(dropped, 1);
    assert_eq!(queue.metrics.dropped(), 1, "the metric keeps counting up");
}

#[tokio::test]
async fn the_writer_loop_drains_the_queue_on_shutdown() {
    let dir = TempDir::new("ingest-drain");
    let queue = IngestQueue::new(100, 1000);
    let writer = Writer::open(
        &dir.join("analytics.db"),
        queue.clone(),
        IngestOptions::default(),
    )
    .await
    .expect("an analytics database");
    let pool = writer.pool().clone();
    let (tx, rx) = tokio::sync::watch::channel(false);
    let task = tokio::spawn(writer.run(rx));

    for _ in 0..5 {
        queue.push(event("page_view", "/a")).expect("room");
    }
    tx.send(true).expect("a shutdown signal");
    task.await.expect("the writer stops");

    let raw: i64 = sqlx::query("SELECT COUNT(*) AS n FROM event")
        .fetch_one(&pool)
        .await
        .expect("a row")
        .try_get("n")
        .expect("a count");
    assert_eq!(raw, 5, "nothing buffered is lost on a clean shutdown");
}
