//! The research phase's `search_docs`, proved to return a passage (AGT-02,
//! AGT-03) and proved to withhold a restricted one (AGT-04).
//!
//! **Why this file exists.** Every `search_docs` assertion in the suite —
//! `tests/server/agt_02_research_job.rs`, `tests/server/agt_02_research.rs`, and
//! my own `agt_02_research_seam.rs` — asserted the SHAPE of the answer:
//! `answer["passages"].is_array()`. I probed it on 2026-10-02 and the array was
//! empty in every one. The search path had never returned a passage in any test,
//! and three tests passed anyway, because `{"passages": []}` satisfies all three.
//! That is the same shape as the search endpoint that answered 503 for weeks
//! against a test asserting only "not 404, JSON, no-store".
//!
//! The cause is not a forgotten call. `ServerTools::new` sets
//! `index: None, embed: None`; `ServerTools::with_index` takes an
//! `Arc<dyn Embed>`, and **nothing in the workspace implements `Embed`** — so its
//! argument was unconstructable and the builder had no caller. `search` returns
//! `Ok(Vec::new())` whenever the index is absent, which is the right answer for
//! an unindexed site and indistinguishable from a working search over an empty
//! one.
//!
//! `Embed`'s own doc says it is "the seam so that `ServerTools` can be built and
//! tested without one". This is the first test to use it.
//!
//! No provider key is needed: the embedding is fixed and the store is
//! `liyasa-ai`'s in-memory one, so this proves the retrieval path rather than a
//! model's quality. AGT-02's golden replay against a live model is still
//! escalated and still the one row that needs keys.

use std::sync::Arc;

use liyasa_agent::record::Phase;
use liyasa_agent::tools;
use liyasa_ai::index::{ChunkKind, ChunkQuery, ChunkRecord, MemoryStore, VectorStore};
use liyasa_core::ids::{ChunkId, IndexId, Locale, Route};
use liyasa_core::net::BoxFut;
use liyasa_server::routes::research;
use liyasa_server::routes::tools::{Embed, ServerTools};
use liyasa_tests::server::Harness;
use serde_json::json;

/// One fixed vector for every query, so a hit depends on the index holding the
/// chunk rather than on an embedding's quality. Cosine similarity against an
/// identical stored vector is 1.0.
struct FixedEmbedding(Vec<f32>);

impl Embed for FixedEmbedding {
    fn embed<'a>(&'a self, _query: &'a str) -> BoxFut<'a, Result<Vec<f32>, String>> {
        Box::pin(std::future::ready(Ok(self.0.clone())))
    }
}

const DIMS: usize = 4;

fn vector() -> Vec<f32> {
    vec![1.0, 0.0, 0.0, 0.0]
}

fn chunk(id: &str, route: &str, title: &str, groups: &[&str]) -> ChunkRecord {
    ChunkRecord {
        id: ChunkId::new(id),
        route: Route::new(route),
        anchor: String::new(),
        title: title.to_owned(),
        breadcrumb: Vec::new(),
        version: None,
        locale: Locale::new("en"),
        groups: groups.iter().map(|g| (*g).to_owned()).collect(),
        regions: Vec::new(),
        product: None,
        last_verified: None,
        kind: ChunkKind::Prose,
        ordinal: 0,
        tokens: 12,
        content_hash: format!("blake3:{id}"),
        text: format!("{title} — the body of {route}."),
    }
}

/// A store holding one public chunk and one restricted to `staff`.
async fn indexed_store() -> (Arc<MemoryStore>, IndexId) {
    let store = Arc::new(MemoryStore::new());
    let model = "fake:embed-1".parse().expect("a model ref");
    let descriptor = store.create(&model, DIMS).await.expect("an index");
    store
        .upsert(
            &descriptor.id,
            &[
                (chunk("c-public", "/pricing", "Pricing", &[]), vector()),
                (
                    chunk("c-staff", "/internal/margins", "Margins", &["staff"]),
                    vector(),
                ),
            ],
        )
        .await
        .expect("the chunks are written");
    store
        .swap_active(&descriptor.id)
        .await
        .expect("the index becomes active");
    (store, descriptor.id)
}

/// `ServerTools` with a real index behind it, filtered as `filter` says.
async fn searching_tools(filter: ChunkQuery) -> ServerTools {
    let (harness, _site) = Harness::serving("agt02-retrieval").await;
    let bundle = harness.state.bundle.clone().expect("the fixture bundle");
    let (store, _) = indexed_store().await;
    ServerTools::new(bundle, filter.clone()).with_index(store, Arc::new(FixedEmbedding(vector())))
}

#[tokio::test]
async fn search_docs_returns_an_actual_passage_and_not_an_empty_array() {
    // The assertion the other three should have made.
    let filter = ChunkQuery::default();
    let tools = searching_tools(filter.clone()).await;
    let answer = research::execute(
        &tools,
        &filter,
        tools::SEARCH_DOCS,
        &json!({ "query": "what does it cost" }),
    )
    .await
    .expect("search_docs is served");

    let passages = answer["passages"]
        .as_array()
        .expect("search_docs answers passages");
    assert!(
        !passages.is_empty(),
        "the search path still returns nothing: {answer}"
    );
    assert_eq!(passages[0]["route"], "/pricing");
    assert_eq!(passages[0]["title"], "Pricing");
    assert!(
        passages[0]["score"].as_f64().is_some_and(|s| s > 0.0),
        "a hit carries its score: {answer}"
    );
}

#[tokio::test]
async fn an_unauthenticated_filter_withholds_a_restricted_chunk() {
    // The empirical basis for RFC 2503. The research pass runs under
    // `ChunkQuery::default()`, and this is what that excludes: the store applies
    // the filter during retrieval, so a restricted chunk is never scored rather
    // than scored and dropped (RFC 1807).
    let filter = ChunkQuery::default();
    let tools = searching_tools(filter.clone()).await;
    let answer = research::execute(
        &tools,
        &filter,
        tools::SEARCH_DOCS,
        &json!({ "query": "margins" }),
    )
    .await
    .expect("served");

    let routes: Vec<&str> = answer["passages"]
        .as_array()
        .expect("passages")
        .iter()
        .filter_map(|p| p["route"].as_str())
        .collect();
    assert!(
        routes.contains(&"/pricing"),
        "the public chunk should be found: {answer}"
    );
    assert!(
        !routes.contains(&"/internal/margins"),
        "a restricted chunk reached an unauthenticated research pass: {answer}"
    );
}

#[tokio::test]
async fn a_staff_filter_does_reach_the_restricted_chunk() {
    // Falsifies the test above: the restricted chunk is retrievable, so its
    // absence there is the filter working and not the chunk being unindexed.
    // This is also what RFC 2503 decides NOT to do by default.
    let filter = ChunkQuery {
        groups: vec!["staff".to_owned()],
        ..ChunkQuery::default()
    };
    let tools = searching_tools(filter.clone()).await;
    let answer = research::execute(
        &tools,
        &filter,
        tools::SEARCH_DOCS,
        &json!({ "query": "margins" }),
    )
    .await
    .expect("served");

    let routes: Vec<&str> = answer["passages"]
        .as_array()
        .expect("passages")
        .iter()
        .filter_map(|p| p["route"].as_str())
        .collect();
    assert!(
        routes.contains(&"/internal/margins"),
        "the restricted chunk is not retrievable at all, so the test above \
         proves nothing: {answer}"
    );
}

#[tokio::test]
async fn a_retrieved_passage_reaches_the_run_as_member_data() {
    // The whole seam with real data in it: retrieve, record, and the finding
    // arrives in the next request as a delimited block at the tool's trust.
    use liyasa_core::ai::{ChatEvent, TrustLevel};

    let filter = ChunkQuery::default();
    let tools = searching_tools(filter.clone()).await;
    let answer = research::execute(
        &tools,
        &filter,
        tools::SEARCH_DOCS,
        &json!({ "query": "cost" }),
    )
    .await
    .expect("served");
    assert!(
        !answer["passages"].as_array().expect("passages").is_empty(),
        "this test is about a NON-empty retrieval: {answer}"
    );

    let mut run = liyasa_agent::run::start(
        crate::agent_support::request(
            crate::agent_support::drift_trigger(),
            "the pricing page disagrees with the plan table",
        ),
        crate::agent_support::config(),
        crate::agent_support::layout(),
        crate::agent_support::agents_md(),
        crate::agent_support::known_hosts(),
    );
    run.enter(Phase::Research).expect("research");
    run.authorise(tools::SEARCH_DOCS, &json!({ "query": "cost" }))
        .expect("permitted");
    run.record_result(tools::SEARCH_DOCS, "cost", &answer);

    let pages = crate::agent_support::pages();
    let model = liyasa_agent::testing::ScriptedModel::new([vec![ChatEvent::Done]]);
    run.enter(Phase::Plan).expect("plan");
    run.enter(Phase::Write).expect("write");
    run.write_turn(&model, &pages, "reconcile them")
        .await
        .expect("answered");

    let block = model.seen()[0]
        .data
        .iter()
        .find(|b| b.label.starts_with(tools::SEARCH_DOCS))
        .expect("the finding did not reach the model")
        .clone();
    assert_eq!(block.trust, TrustLevel::Member);
    assert!(
        block.content.contains("/pricing"),
        "the retrieved route is not in the block the model was shown: {}",
        block.content
    );
}
