//! A publish, end to end: real Markdown, the exclusion rules, the chunker, the
//! index, and a query that comes back with the right passage.

use liyasa_ai::chunk::{ChunkOptions, PageContext};
use liyasa_ai::config::{AiConfig, ModelRef};
use liyasa_ai::exclude::{Environment, Excluded, PageFacts};
use liyasa_ai::index::{ChunkQuery, MemoryStore, VectorStore};
use liyasa_ai::indexing::{JOB_NAME, JobPayload, PageInput, page_delta, records_for, withdraw};
use liyasa_core::document::Document;
use liyasa_core::frontmatter::{AiSetting, FrontmatterFields};
use liyasa_core::ids::{Locale, Route};

use crate::page::parse;

const PAGE: &str = "\
# Authentication

Every request carries a key.

## Bearer tokens

Send the key in the `Authorization` header as a bearer token.

## Rotating a key

Create the new key first, deploy it, then revoke the old one.
";

fn context(route: &str) -> PageContext {
    PageContext {
        route: Route::new(route),
        title: "Authentication".to_owned(),
        breadcrumb: vec!["Guides".to_owned()],
        version: None,
        locale: Locale::new("en"),
        groups: Vec::new(),
        regions: Vec::new(),
        product: Some("api".to_owned()),
        last_verified: Some(1_700_000_000_000),
    }
}

fn input<'a>(front: &'a FrontmatterFields, document: &'a Document, route: &str) -> PageInput<'a> {
    PageInput {
        facts: PageFacts {
            front,
            draft: false,
            ignored: false,
            ai_ignored: false,
        },
        context: context(route),
        document,
    }
}

/// A vector that puts two routes at measurably different angles, so a query can
/// be checked without an embedding model.
fn vector(record: &liyasa_ai::index::ChunkRecord) -> Vec<f32> {
    match record.anchor.as_str() {
        "bearer-tokens" => vec![1.0, 0.0],
        "rotating-a-key" => vec![0.0, 1.0],
        _ => vec![0.7, 0.7],
    }
}

#[tokio::test]
async fn a_published_page_becomes_answerable() {
    let document = parse(PAGE);
    let front = FrontmatterFields::default();
    let page = input(&front, &document, "/guides/auth");
    let records = records_for(
        &page,
        Environment::Production,
        &AiConfig::default(),
        &ChunkOptions::default(),
    )
    .expect("an ordinary page is indexed");

    assert!(records.len() >= 3, "{} records", records.len());
    assert!(records.iter().any(|r| r.anchor == "bearer-tokens"));
    assert!(records.iter().all(|r| r.product.as_deref() == Some("api")));
    assert!(records.iter().all(|r| r.last_verified.is_some()));

    let store = MemoryStore::new();
    let model: ModelRef = "openai:text-embedding-3-small".parse().expect("model");
    let index = store.create(&model, 2).await.expect("create");
    let rows: Vec<_> = records.iter().map(|r| (r.clone(), vector(r))).collect();
    store.upsert(&index.id, &rows).await.expect("upsert");
    store.swap_active(&index.id).await.expect("swap");

    let hits = store
        .query(&[1.0, 0.0], 1, &ChunkQuery::default())
        .await
        .expect("query");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].record.anchor, "bearer-tokens");
    assert_eq!(hits[0].record.citation(), "/guides/auth#bearer-tokens");
    assert!(
        hits[0].record.text.contains("Authorization"),
        "{}",
        hits[0].record.text
    );
}

#[tokio::test]
async fn a_second_publish_re_embeds_only_what_changed() {
    let document = parse(PAGE);
    let front = FrontmatterFields::default();
    let page = input(&front, &document, "/guides/auth");
    let first = records_for(
        &page,
        Environment::Production,
        &AiConfig::default(),
        &ChunkOptions::default(),
    )
    .expect("records");

    let store = MemoryStore::new();
    let model: ModelRef = "openai:text-embedding-3-small".parse().expect("model");
    let index = store.create(&model, 2).await.expect("create");
    let rows: Vec<_> = first.iter().map(|r| (r.clone(), vector(r))).collect();
    store.upsert(&index.id, &rows).await.expect("upsert");

    // Republish with one section edited.
    let edited = PAGE.replace(
        "Create the new key first, deploy it, then revoke the old one.",
        "Create the new key, deploy it, wait an hour, then revoke the old one.",
    );
    let document = parse(&edited);
    let page = input(&front, &document, "/guides/auth");
    let second = records_for(
        &page,
        Environment::Production,
        &AiConfig::default(),
        &ChunkOptions::default(),
    )
    .expect("records");

    let delta = page_delta(&store, &index.id, &Route::new("/guides/auth"), second)
        .await
        .expect("delta");

    assert_eq!(
        delta.embed.len(),
        1,
        "{:?}",
        delta.embed.iter().map(|r| &r.anchor).collect::<Vec<_>>()
    );
    assert_eq!(delta.embed[0].anchor, "rotating-a-key");
    assert_eq!(delta.unchanged, first.len() - 1);
    assert!(delta.delete.is_empty());
}

#[tokio::test]
async fn a_section_that_is_deleted_loses_its_row() {
    let document = parse(PAGE);
    let front = FrontmatterFields::default();
    let page = input(&front, &document, "/guides/auth");
    let first = records_for(
        &page,
        Environment::Production,
        &AiConfig::default(),
        &ChunkOptions::default(),
    )
    .expect("records");

    let store = MemoryStore::new();
    let model: ModelRef = "openai:text-embedding-3-small".parse().expect("model");
    let index = store.create(&model, 2).await.expect("create");
    let rows: Vec<_> = first.iter().map(|r| (r.clone(), vector(r))).collect();
    store.upsert(&index.id, &rows).await.expect("upsert");

    let shortened = PAGE
        .split("## Rotating a key")
        .next()
        .expect("the page splits")
        .to_owned();
    let document = parse(&shortened);
    let page = input(&front, &document, "/guides/auth");
    let second = records_for(
        &page,
        Environment::Production,
        &AiConfig::default(),
        &ChunkOptions::default(),
    )
    .expect("records");

    let delta = page_delta(&store, &index.id, &Route::new("/guides/auth"), second)
        .await
        .expect("delta");
    assert_eq!(delta.delete.len(), 1);
    assert!(delta.delete[0].as_str().contains("rotating-a-key"));
}

#[tokio::test]
async fn a_page_that_turns_hidden_loses_every_row() {
    let document = parse(PAGE);
    let front = FrontmatterFields::default();
    let page = input(&front, &document, "/guides/auth");
    let records = records_for(
        &page,
        Environment::Production,
        &AiConfig::default(),
        &ChunkOptions::default(),
    )
    .expect("records");

    let store = MemoryStore::new();
    let model: ModelRef = "openai:text-embedding-3-small".parse().expect("model");
    let index = store.create(&model, 2).await.expect("create");
    let rows: Vec<_> = records.iter().map(|r| (r.clone(), vector(r))).collect();
    store.upsert(&index.id, &rows).await.expect("upsert");
    store.swap_active(&index.id).await.expect("swap");

    // The republished page is `ai: false`.
    let hidden = FrontmatterFields {
        ai: Some(AiSetting::Enabled(false)),
        ..Default::default()
    };
    let page = input(&hidden, &document, "/guides/auth");
    let reason = records_for(
        &page,
        Environment::Production,
        &AiConfig::default(),
        &ChunkOptions::default(),
    )
    .expect_err("an opted-out page produces nothing");
    assert_eq!(reason, Excluded::OptedOut);

    let doomed = withdraw(&store, &index.id, &Route::new("/guides/auth"))
        .await
        .expect("withdraw");
    assert_eq!(doomed.len(), records.len());
    store.delete(&index.id, &doomed).await.expect("delete");

    let hits = store
        .query(&[1.0, 0.0], 5, &ChunkQuery::default())
        .await
        .expect("query");
    assert!(hits.is_empty(), "an opted-out page kept answering");
}

#[test]
fn a_preview_deployment_indexes_nothing_unless_it_was_turned_on() {
    let document = parse(PAGE);
    let front = FrontmatterFields::default();
    let page = input(&front, &document, "/guides/auth");
    assert_eq!(
        records_for(
            &page,
            Environment::Preview { assistant: false },
            &AiConfig::default(),
            &ChunkOptions::default(),
        )
        .expect_err("previews are out"),
        Excluded::Preview
    );
    assert!(
        records_for(
            &page,
            Environment::Preview { assistant: true },
            &AiConfig::default(),
            &ChunkOptions::default(),
        )
        .is_ok()
    );
}

#[test]
fn the_job_payload_round_trips_under_the_name_the_server_enqueues() {
    // The value the deploy queue enqueues. Pinned against
    // liyasa_server::deploy::queue::EMBED_JOB in tests/, which is the only
    // place that can see both crates.
    assert_eq!(JOB_NAME, "assistant.embed");
    let payload = JobPayload {
        project: "p_1".to_owned(),
        deployment: "d_1".to_owned(),
        routes: vec![Route::new("/guides/auth")],
    };
    let json = serde_json::to_value(&payload).expect("serialize");
    assert_eq!(json["buildId"], "d_1", "the wire name is the queue's, not ours");
    assert_eq!(json["routes"][0], "/guides/auth");
    assert_eq!(
        serde_json::from_value::<JobPayload>(json).expect("deserialize"),
        payload
    );
}
