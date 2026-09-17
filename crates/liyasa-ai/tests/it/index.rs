//! Retrieval: the two stages, the over-fetch, and the entitlement rule.

use liyasa_ai::config::ModelRef;
use liyasa_ai::index::{
    ChunkKind, ChunkQuery, ChunkRecord, IndexChange, MemoryStore, OVERFETCH, VectorStore, sql,
};
use liyasa_core::ids::{Locale, Route, Version};

fn model() -> ModelRef {
    "openai:text-embedding-3-small".parse().expect("model ref")
}

fn record(n: usize) -> ChunkRecord {
    let route = Route::new(format!("/page-{n}"));
    ChunkRecord {
        id: ChunkRecord::id_for(&route, "", 0),
        route,
        anchor: String::new(),
        title: format!("Page {n}"),
        breadcrumb: vec!["Docs".to_owned()],
        version: None,
        locale: Locale::new("en"),
        groups: Vec::new(),
        regions: Vec::new(),
        product: None,
        last_verified: None,
        kind: ChunkKind::Prose,
        ordinal: 0,
        tokens: 10,
        content_hash: format!("blake3:{n}"),
        text: format!("body {n}"),
    }
}

/// A vector whose similarity to `[1, 0]` falls as `n` rises, so rank is
/// predictable without depending on an embedding model.
fn vector(n: usize) -> Vec<f32> {
    let angle = n as f32 * 0.01;
    vec![angle.cos(), angle.sin()]
}

async fn store_with(records: Vec<ChunkRecord>) -> MemoryStore {
    let store = MemoryStore::new();
    let index = store.create(&model(), 2).await.expect("create");
    let rows: Vec<(ChunkRecord, Vec<f32>)> = records
        .into_iter()
        .enumerate()
        .map(|(n, r)| (r, vector(n)))
        .collect();
    store.upsert(&index.id, &rows).await.expect("upsert");
    store.swap_active(&index.id).await.expect("swap");
    store
}

#[tokio::test]
async fn nearest_first() {
    let store = store_with((0..10).map(record).collect()).await;
    let hits = store
        .query(&[1.0, 0.0], 3, &ChunkQuery::default())
        .await
        .expect("query");
    let routes: Vec<&str> = hits.iter().map(|h| h.record.route.as_str()).collect();
    assert_eq!(routes, ["/page-0", "/page-1", "/page-2"]);
    assert!(hits[0].score > hits[1].score);
}

#[tokio::test]
async fn a_reader_never_receives_a_chunk_they_could_not_browse() {
    let mut records: Vec<ChunkRecord> = (0..4).map(record).collect();
    records[0].groups = vec!["staff".to_owned()];
    records[1].groups = vec!["staff".to_owned(), "beta".to_owned()];
    let store = store_with(records).await;

    let anonymous = store
        .query(&[1.0, 0.0], 4, &ChunkQuery::default())
        .await
        .expect("query");
    let routes: Vec<&str> = anonymous.iter().map(|h| h.record.route.as_str()).collect();
    assert_eq!(routes, ["/page-2", "/page-3"], "entitled chunks leaked");

    let beta = store
        .query(
            &[1.0, 0.0],
            4,
            &ChunkQuery {
                groups: vec!["beta".to_owned()],
                ..Default::default()
            },
        )
        .await
        .expect("query");
    let routes: Vec<&str> = beta.iter().map(|h| h.record.route.as_str()).collect();
    assert_eq!(routes, ["/page-1", "/page-2", "/page-3"]);
}

#[tokio::test]
async fn a_region_gated_chunk_needs_the_readers_region() {
    let mut records: Vec<ChunkRecord> = (0..2).map(record).collect();
    records[0].regions = vec!["eu".to_owned()];
    let store = store_with(records).await;

    let without = store
        .query(&[1.0, 0.0], 2, &ChunkQuery::default())
        .await
        .expect("query");
    assert_eq!(without.len(), 1);

    let with = store
        .query(
            &[1.0, 0.0],
            2,
            &ChunkQuery {
                region: Some("eu".to_owned()),
                ..Default::default()
            },
        )
        .await
        .expect("query");
    assert_eq!(with.len(), 2);
}

#[tokio::test]
async fn the_filter_runs_over_an_over_fetched_candidate_set() {
    // The first `k * OVERFETCH` by distance are all entitled, so a filter
    // applied inside the scan would return nothing. The whole point of the
    // over-fetch is that the stages are separate.
    let k = 2;
    let mut records: Vec<ChunkRecord> = (0..20).map(record).collect();
    for r in records.iter_mut().take(k * OVERFETCH - 1) {
        r.groups = vec!["staff".to_owned()];
    }
    let store = store_with(records).await;

    let hits = store
        .query(&[1.0, 0.0], k, &ChunkQuery::default())
        .await
        .expect("query");
    assert_eq!(
        hits.len(),
        1,
        "the candidate set is {} rows and all but one are entitled",
        k * OVERFETCH
    );
    assert_eq!(hits[0].record.route.as_str(), "/page-7");
}

#[tokio::test]
async fn a_vector_of_the_wrong_length_is_refused() {
    let store = MemoryStore::new();
    let index = store.create(&model(), 2).await.expect("create");
    let error = store
        .upsert(&index.id, &[(record(0), vec![1.0, 0.0, 0.0])])
        .await
        .expect_err("three dimensions into a two-dimensional index");
    assert!(error.to_string().contains('3'), "{error}");
}

#[tokio::test]
async fn the_old_index_answers_until_the_swap() {
    let store = MemoryStore::new();
    let old = store.create(&model(), 2).await.expect("create");
    store
        .upsert(&old.id, &[(record(0), vector(0))])
        .await
        .expect("upsert");
    store.swap_active(&old.id).await.expect("swap");

    let new = store.create(&model(), 2).await.expect("create");
    store
        .upsert(&new.id, &[(record(1), vector(1)), (record(2), vector(2))])
        .await
        .expect("upsert");

    let before = store
        .query(&[1.0, 0.0], 5, &ChunkQuery::default())
        .await
        .expect("query");
    assert_eq!(before.len(), 1, "the new index answered before the swap");

    store.swap_active(&new.id).await.expect("swap");
    let after = store
        .query(&[1.0, 0.0], 5, &ChunkQuery::default())
        .await
        .expect("query");
    assert_eq!(after.len(), 2);
    assert_eq!(
        store.indexes(),
        vec![new.id],
        "the old index was not dropped"
    );
}

#[tokio::test]
async fn hashes_answer_for_one_route_only() {
    let store = store_with((0..3).map(record).collect()).await;
    let index = store.active().await.expect("active").expect("some");
    let hashes = store
        .hashes(&index.id, &Route::new("/page-1"))
        .await
        .expect("hashes");
    assert_eq!(hashes.len(), 1);
    assert_eq!(hashes[0].1, "blake3:1");
}

#[tokio::test]
async fn deleting_a_chunk_removes_it_from_answers() {
    let store = store_with((0..2).map(record).collect()).await;
    let index = store.active().await.expect("active").expect("some");
    let id = ChunkRecord::id_for(&Route::new("/page-0"), "", 0);
    store.delete(&index.id, &[id]).await.expect("delete");
    let hits = store
        .query(&[1.0, 0.0], 5, &ChunkQuery::default())
        .await
        .expect("query");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].record.route.as_str(), "/page-1");
}

#[tokio::test]
async fn a_query_with_no_active_index_is_an_error_not_an_empty_answer() {
    let store = MemoryStore::new();
    let error = store
        .query(&[1.0, 0.0], 3, &ChunkQuery::default())
        .await
        .expect_err("no index");
    assert!(error.to_string().contains("no index"), "{error}");
}

#[test]
fn a_dimension_change_needs_a_new_table_and_a_model_change_does_not() {
    let descriptor = liyasa_ai::index::IndexDescriptor {
        id: liyasa_core::ids::IndexId::new("i1"),
        model: model(),
        dims: 1536,
        backend: liyasa_ai::index::Backend::SqliteVec,
        extension_version: Some("0.1.6".to_owned()),
        created_at: 0,
    };
    assert_eq!(descriptor.change_from(&model(), 1536), None);

    let other: ModelRef = "openai:text-embedding-3-large".parse().expect("model");
    let change = descriptor.change_from(&other, 1536).expect("model change");
    assert!(matches!(change, IndexChange::Model { .. }));
    assert!(!change.needs_new_table());

    let change = descriptor.change_from(&model(), 3072).expect("dimension");
    assert!(matches!(change, IndexChange::Dimension { .. }));
    assert!(change.needs_new_table());
}

#[test]
fn a_group_name_with_a_like_wildcard_cannot_match_another_group() {
    let pattern = sql::membership_pattern("a_b");
    assert_eq!(pattern, "%|a\\_b|%");
    assert_eq!(sql::encode_list(&["a_b".to_owned()]), "|a_b|");
    assert_eq!(sql::encode_list(&[]), "");
    assert_eq!(
        sql::decode_list("|admin|staff|"),
        vec!["admin".to_owned(), "staff".to_owned()]
    );
}

#[test]
fn a_prefix_never_matches_a_longer_group() {
    // `admin` against a chunk entitled to `superadmin` only.
    let stored = sql::encode_list(&["superadmin".to_owned()]);
    let pattern = sql::membership_pattern("admin");
    let needle = pattern.trim_start_matches('%').trim_end_matches('%');
    assert!(
        !stored.contains(needle),
        "`{stored}` matched the pattern for `admin`"
    );
}

#[test]
fn the_search_statement_binds_what_it_declares() {
    let index = liyasa_core::ids::IndexId::new("i1");
    let filter = ChunkQuery {
        groups: vec!["staff".to_owned(), "beta".to_owned()],
        region: Some("eu".to_owned()),
        version: Some(Version::new("v2")),
        locale: Some(Locale::new("en")),
        kind: Some(ChunkKind::Prose),
        routes: Vec::new(),
    };
    for backend in [
        liyasa_ai::index::Backend::SqliteVec,
        liyasa_ai::index::Backend::PgVector,
    ] {
        let search = sql::search(backend, &index, &filter);
        let placeholders = match backend {
            liyasa_ai::index::Backend::SqliteVec => search.sql.matches('?').count(),
            liyasa_ai::index::Backend::PgVector => (1..=search.bindings.len())
                .filter(|n| search.sql.contains(&format!("${n}")))
                .count(),
        };
        assert_eq!(
            placeholders,
            search.bindings.len(),
            "{backend:?}: {} placeholders, {} bindings\n{}",
            placeholders,
            search.bindings.len(),
            search.sql
        );
    }
}

#[test]
fn a_query_with_no_filter_still_excludes_entitled_chunks() {
    let index = liyasa_core::ids::IndexId::new("i1");
    let search = sql::search(
        liyasa_ai::index::Backend::SqliteVec,
        &index,
        &ChunkQuery::default(),
    );
    assert!(
        search.sql.contains("c.groups = ''"),
        "an anonymous reader's query must exclude entitled chunks:\n{}",
        search.sql
    );
    assert!(search.sql.contains("c.regions = ''"), "{}", search.sql);
}

#[test]
fn the_candidate_limit_is_over_fetched_and_the_result_limit_is_not() {
    assert_eq!(sql::overfetch(10), 40);
    let index = liyasa_core::ids::IndexId::new("i1");
    let search = sql::search(
        liyasa_ai::index::Backend::SqliteVec,
        &index,
        &ChunkQuery::default(),
    );
    assert_eq!(search.bindings[0], sql::Binding::Embedding);
    assert_eq!(search.bindings[1], sql::Binding::Overfetch);
    assert_eq!(
        search.bindings.last(),
        Some(&sql::Binding::Limit),
        "the outer LIMIT is bound last"
    );
}

#[test]
fn a_table_name_cannot_carry_an_index_id_verbatim() {
    let hostile = liyasa_core::ids::IndexId::new("a\"; DROP TABLE chunks; --");
    let name = sql::chunks_table(&hostile);
    assert!(
        name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "{name}"
    );
}
