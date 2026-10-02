//! AST-10's effect: a question returns a passage from this site's own index.
//!
//! The defect these exist for is that **every `ServerTools::search` in the
//! workspace returned an empty vector.** `search` returns early unless it
//! holds both an index and an `Embed`, `with_index` takes an `Arc<dyn Embed>`,
//! and the workspace's only `impl Embed` was a fixed four-dimensional vector
//! in WP-25's test — so the builder was unconstructable from production and
//! had no caller.
//!
//! Three tests asserted `passages` was an array and all three passed, because
//! `is_array()` holds for an empty array exactly as for a populated one. So
//! these assert on the CONTENTS, and `a_question_with_no_index_says_so_rather_
//! than_answering_nothing` pins the distinction the shape could not make.
//!
//! `ModelEmbed` is the production adapter under test. The `EmbeddingModel`
//! behind it is scripted here rather than a provider, because a test that
//! needs a key tests the key: what is under test is the narrowing from the
//! frozen batch contract to one query, and that a vector reaches the store.

use std::sync::Arc;

use liyasa_ai::index::{ChunkKind, ChunkRecord, MemoryStore, VectorStore};
use liyasa_core::ai::{AiError, EmbeddingModel};
use liyasa_core::ids::{ChunkId, Locale, Route};
use liyasa_core::net::BoxFut;
use liyasa_server::assistant::embed::ModelEmbed;
use liyasa_server::assistant::http::{AssistantState, Retrieval, router};
use liyasa_server::auth::session::Principal;
use liyasa_server::routes::tools::Embed;
use liyasa_tests::server::{Harness, Setup};
use serde_json::{Value, json};

const DIMS: usize = 4;

/// Every input embeds to the same vector, so cosine similarity is 1.0 against
/// every stored chunk and ordering is not what is under test — reaching the
/// store at all is.
struct ScriptedModel {
    dims: usize,
    /// How many vectors to answer with, so the adapter's own guard is
    /// reachable rather than only its happy path.
    vectors: usize,
}

impl ScriptedModel {
    fn good() -> Self {
        Self {
            dims: DIMS,
            vectors: 1,
        }
    }
}

impl EmbeddingModel for ScriptedModel {
    fn id(&self) -> &str {
        "scripted:embed-1"
    }

    fn dims(&self) -> usize {
        self.dims
    }

    fn embed<'a>(&'a self, inputs: &'a [String]) -> BoxFut<'a, Result<Vec<Vec<f32>>, AiError>> {
        let answer = (0..self.vectors).map(|_| vec![0.5f32; DIMS]).collect();
        let _ = inputs;
        Box::pin(async move { Ok(answer) })
    }
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

/// One public chunk and one restricted to `staff`, in an active index.
async fn indexed_store() -> (Arc<MemoryStore>, usize) {
    let store = Arc::new(MemoryStore::new());
    let model = "scripted:embed-1".parse().expect("a model ref");
    let descriptor = store.create(&model, DIMS).await.expect("an index");
    store
        .upsert(
            &descriptor.id,
            &[
                (
                    chunk("c-public", "/pricing", "Pricing", &[]),
                    vec![0.5; DIMS],
                ),
                (
                    chunk("c-staff", "/internal/margins", "Margins", &["staff"]),
                    vec![0.5; DIMS],
                ),
            ],
        )
        .await
        .expect("the chunks are written");
    store
        .swap_active(&descriptor.id)
        .await
        .expect("the index becomes active");
    (store, descriptor.dims)
}

async fn harness(name: &str) -> Harness {
    let (harness, _site) = Harness::new(Setup {
        site_config: Some(json!({
            "name": "Acme docs",
            "seo": { "canonicalOrigin": "https://docs.acme.com" },
            "ai": { "assistant": { "enabled": true } }
        })),
        ..Setup::new(name)
    })
    .await;
    harness
}

/// The endpoint's own router, with retrieval supplied — which is what a served
/// instance cannot do yet (defect 52) and what `liyasa build`'s static export
/// and these tests legitimately can.
async fn serving(name: &str, with_index: bool) -> (axum::Router, Arc<MemoryStore>) {
    let harness = harness(name).await;
    let bundle = harness.state.bundle.clone().expect("the fixture bundle");
    let config =
        liyasa_ai::config::AiConfig::from_site(&harness.state.config.site_config).expect("ai");
    let (store, dims) = indexed_store().await;
    let retrieval = with_index.then(|| {
        Retrieval::new(
            store.clone() as Arc<dyn VectorStore>,
            ModelEmbed::new(Arc::new(ScriptedModel::good())),
            dims,
        )
        .expect("the embedder's width matches the index")
    });
    let state = Arc::new(AssistantState {
        app: harness.state.clone(),
        bundle,
        config,
        retrieval,
    });
    (router(state), store)
}

async fn ask(router: &axum::Router, question: &str, groups: Option<&[&str]>) -> Value {
    let mut request = http::Request::builder()
        .method("POST")
        .uri("/_liyasa/assistant")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(
            json!({ "question": question }).to_string(),
        ))
        .expect("a request");
    if let Some(groups) = groups {
        request.extensions_mut().insert(Principal {
            subject: "reader-1".to_owned(),
            groups: groups.iter().map(|g| (*g).to_owned()).collect(),
            ..Principal::default()
        });
    }
    let response = tower::ServiceExt::oneshot(router.clone(), request)
        .await
        .expect("the router answers");
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("a body");
    serde_json::from_slice(&bytes).expect("the answer is JSON")
}

#[tokio::test]
async fn a_question_returns_a_passage_from_this_sites_index() {
    // The assertion the three `is_array()` tests should have made.
    let (router, _store) = serving("ast10-passage", true).await;
    let answer = ask(&router, "what does it cost", None).await;

    let passages = answer["passages"].as_array().expect("passages");
    assert!(
        !passages.is_empty(),
        "no passage came back, which is what every search in this workspace did \
         before `ModelEmbed` existed: {answer}"
    );
    let first = &passages[0];
    assert_eq!(first["route"], "/pricing", "{answer}");
    assert_eq!(first["title"], "Pricing", "{answer}");
    assert!(
        first["score"].as_f64().is_some_and(|s| s > 0.0),
        "a passage came back with no positive score, so nothing was actually \
         compared: {answer}"
    );
    assert_eq!(answer["retrieval"]["searched"], true, "{answer}");
}

#[tokio::test]
async fn the_restricted_chunk_is_withheld_and_the_entitled_reader_still_gets_it() {
    // Both directions, so "withheld" is the filter working rather than the
    // chunk being absent from the index.
    let (router, _store) = serving("ast10-filtered", true).await;

    let anonymous = ask(&router, "what are the margins", None).await;
    let routes: Vec<&str> = anonymous["passages"]
        .as_array()
        .expect("passages")
        .iter()
        .filter_map(|p| p["route"].as_str())
        .collect();
    assert!(
        !routes.contains(&"/internal/margins"),
        "a staff-only chunk reached an anonymous reader: {anonymous}"
    );
    assert!(
        routes.contains(&"/pricing"),
        "the public chunk is missing too, so the assertion above proves nothing: {anonymous}"
    );

    let staff = ask(&router, "what are the margins", Some(&["staff"])).await;
    let staff_routes: Vec<&str> = staff["passages"]
        .as_array()
        .expect("passages")
        .iter()
        .filter_map(|p| p["route"].as_str())
        .collect();
    assert!(
        staff_routes.contains(&"/internal/margins"),
        "the entitled reader did not receive the chunk, so the withholding above \
         may be an unindexed chunk rather than the filter: {staff}"
    );
}

#[tokio::test]
async fn a_question_with_no_index_says_so_rather_than_answering_nothing() {
    // The distinction an empty array cannot make: nothing matched this
    // question, or nothing can match any question. A served instance is in the
    // second state today and the payload has to say which.
    let (router, _store) = serving("ast10-noindex", false).await;
    let answer = ask(&router, "what does it cost", None).await;

    assert_eq!(answer["retrieval"]["searched"], false, "{answer}");
    assert!(
        answer["retrieval"]["why"]
            .as_str()
            .is_some_and(|why| why.contains("no vector index")),
        "an empty retrieval did not say why it was empty: {answer}"
    );
    assert_eq!(
        answer["passages"].as_array().map(Vec::len),
        Some(0),
        "{answer}"
    );
}

#[tokio::test]
async fn an_embedder_narrower_than_the_index_is_refused_at_construction() {
    // Cosine similarity over mismatched widths is not an error below this
    // point: it compares the overlapping prefix and scores the rest zero, so
    // the query returns plausible rows in a meaningless order. Caught where
    // both numbers are in hand.
    let (store, dims) = indexed_store().await;
    let narrow = ModelEmbed::new(Arc::new(ScriptedModel {
        dims: DIMS - 1,
        vectors: 1,
    }));
    let error = Retrieval::new(store as Arc<dyn VectorStore>, narrow, dims)
        .map(|_| ())
        .expect_err("a narrower embedder is refused");
    assert!(error.contains("re-index"), "{error}");
}

#[tokio::test]
async fn a_provider_answering_the_wrong_number_of_vectors_is_an_error_not_a_zero_vector() {
    // One query in, so one vector out. None would otherwise reach the store as
    // an empty vector, whose similarity against everything is zero — scoring
    // every chunk equally and returning `k` arbitrary ones, which reads as a
    // working search over an irrelevant answer.
    let embed = ModelEmbed::new(Arc::new(ScriptedModel {
        dims: DIMS,
        vectors: 0,
    }));
    let error = embed
        .embed("anything")
        .await
        .map(|_| ())
        .expect_err("zero vectors is refused");
    assert!(error.contains("0 vectors"), "{error}");

    let good = ModelEmbed::new(Arc::new(ScriptedModel::good()));
    assert_eq!(
        good.embed("anything").await.expect("one vector").len(),
        DIMS,
        "the happy path stopped working, so the guard above may be refusing everything"
    );
}
