//! A re-index run: batching, backoff, checkpoints, and the swap.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use liyasa_ai::config::ModelRef;
use liyasa_ai::index::{ChunkRecord, MemoryStore, VectorStore};
use liyasa_ai::reindex::{
    BATCH, Backoff, CHECKPOINT_EVERY, Embedder, Progress, ProgressSink, ReindexError, Sleeper, swap,
};
use liyasa_core::ai::{AiError, EmbeddingModel};
use liyasa_core::ids::Route;
use liyasa_core::net::BoxFut;

struct FakeEmbeddings {
    dims: usize,
    /// How many of the first calls answer with a rate limit.
    rate_limit: Mutex<u32>,
    batches: Mutex<Vec<usize>>,
}

impl FakeEmbeddings {
    fn new(dims: usize, rate_limit: u32) -> Arc<Self> {
        Arc::new(Self {
            dims,
            rate_limit: Mutex::new(rate_limit),
            batches: Mutex::new(Vec::new()),
        })
    }

    fn batches(&self) -> Vec<usize> {
        self.batches.lock().expect("lock").clone()
    }
}

impl EmbeddingModel for FakeEmbeddings {
    fn id(&self) -> &str {
        "fake"
    }

    fn dims(&self) -> usize {
        self.dims
    }

    fn embed<'a>(&'a self, inputs: &'a [String]) -> BoxFut<'a, Result<Vec<Vec<f32>>, AiError>> {
        {
            let mut left = self.rate_limit.lock().expect("lock");
            if *left > 0 {
                *left -= 1;
                return Box::pin(std::future::ready(Err(AiError::RateLimited {
                    retry_after: None,
                })));
            }
        }
        self.batches.lock().expect("lock").push(inputs.len());
        let dims = self.dims;
        let vectors = inputs
            .iter()
            .enumerate()
            .map(|(n, _)| {
                let mut v = vec![0.0; dims];
                v[n % dims] = 1.0;
                v
            })
            .collect();
        Box::pin(std::future::ready(Ok(vectors)))
    }
}

/// Records what it was asked to wait for, and returns at once.
#[derive(Default)]
struct FakeSleeper(Mutex<Vec<Duration>>);

impl FakeSleeper {
    fn waited(&self) -> Vec<Duration> {
        self.0.lock().expect("lock").clone()
    }
}

impl Sleeper for FakeSleeper {
    fn sleep(&self, duration: Duration) -> BoxFut<'_, ()> {
        self.0.lock().expect("lock").push(duration);
        Box::pin(std::future::ready(()))
    }
}

#[derive(Default)]
struct Reports {
    progress: Mutex<Vec<Progress>>,
    checkpoints: Mutex<Vec<usize>>,
}

impl ProgressSink for Reports {
    fn report(&self, progress: &Progress) {
        self.progress.lock().expect("lock").push(*progress);
    }

    fn checkpoint(&self, at: usize) {
        self.checkpoints.lock().expect("lock").push(at);
    }
}

fn model() -> ModelRef {
    "openai:text-embedding-3-small".parse().expect("model")
}

fn records(n: usize) -> Vec<ChunkRecord> {
    (0..n)
        .map(|i| {
            let route = Route::new(format!("/p{}", i / 4));
            let mut record = ChunkRecord::bare(ChunkRecord::id_for(&route, "", (i % 4) as u32));
            record.route = route;
            record.text = format!("chunk {i}");
            record.content_hash = format!("blake3:{i}");
            record
        })
        .collect()
}

#[tokio::test]
async fn embedding_goes_in_batches_not_one_call_per_chunk() {
    let store = MemoryStore::new();
    let index = store.create(&model(), 4).await.expect("create");
    let embeddings = FakeEmbeddings::new(4, 0);
    let sleeper = FakeSleeper::default();
    let reports = Reports::default();
    let records = records(BATCH + 5);

    let done = Embedder {
        store: &store,
        index: &index.id,
        model: embeddings.as_ref(),
        backoff: Backoff::default(),
        sleeper: &sleeper,
        progress: &reports,
    }
    .run(&records, 0)
    .await
    .expect("the run finishes");

    assert_eq!(done, records.len());
    assert_eq!(embeddings.batches(), [BATCH, 5]);
    assert_eq!(store.len(&index.id), records.len());
}

#[tokio::test]
async fn a_rate_limit_backs_off_and_the_run_still_finishes() {
    let store = MemoryStore::new();
    let index = store.create(&model(), 4).await.expect("create");
    let embeddings = FakeEmbeddings::new(4, 3);
    let sleeper = FakeSleeper::default();
    let reports = Reports::default();
    let records = records(4);

    let done = Embedder {
        store: &store,
        index: &index.id,
        model: embeddings.as_ref(),
        backoff: Backoff {
            base: Duration::from_millis(10),
            max: Duration::from_secs(1),
            attempts: 6,
        },
        sleeper: &sleeper,
        progress: &reports,
    }
    .run(&records, 0)
    .await
    .expect("the run finishes after backing off");

    assert_eq!(done, 4);
    assert_eq!(
        sleeper.waited(),
        [
            Duration::from_millis(10),
            Duration::from_millis(20),
            Duration::from_millis(40)
        ],
        "the delay must double"
    );
}

#[tokio::test]
async fn a_provider_that_never_lets_up_fails_rather_than_looping() {
    let store = MemoryStore::new();
    let index = store.create(&model(), 4).await.expect("create");
    let embeddings = FakeEmbeddings::new(4, 1000);
    let sleeper = FakeSleeper::default();
    let reports = Reports::default();

    let error = Embedder {
        store: &store,
        index: &index.id,
        model: embeddings.as_ref(),
        backoff: Backoff {
            base: Duration::from_millis(1),
            max: Duration::from_millis(10),
            attempts: 3,
        },
        sleeper: &sleeper,
        progress: &reports,
    }
    .run(&records(4), 0)
    .await
    .expect_err("it gives up");

    assert!(matches!(error, ReindexError::RateLimited { attempts: 3 }));
    assert_eq!(sleeper.waited().len(), 3);
}

#[tokio::test]
async fn a_restart_resumes_from_the_checkpoint_rather_than_re_embedding() {
    let store = MemoryStore::new();
    let index = store.create(&model(), 4).await.expect("create");
    let embeddings = FakeEmbeddings::new(4, 0);
    let sleeper = FakeSleeper::default();
    let reports = Reports::default();
    let records = records(BATCH * 3);

    let done = Embedder {
        store: &store,
        index: &index.id,
        model: embeddings.as_ref(),
        backoff: Backoff::default(),
        sleeper: &sleeper,
        progress: &reports,
    }
    .run(&records, BATCH)
    .await
    .expect("the run finishes");

    assert_eq!(done, records.len());
    let embedded: usize = embeddings.batches().iter().sum();
    assert_eq!(
        embedded,
        records.len() - BATCH,
        "the first batch was embedded again"
    );
}

#[tokio::test]
async fn progress_is_reported_and_checkpointed_every_thousand() {
    let store = MemoryStore::new();
    let index = store.create(&model(), 4).await.expect("create");
    let embeddings = FakeEmbeddings::new(4, 0);
    let sleeper = FakeSleeper::default();
    let reports = Reports::default();
    let records = records(CHECKPOINT_EVERY * 2 + 10);

    Embedder {
        store: &store,
        index: &index.id,
        model: embeddings.as_ref(),
        backoff: Backoff::default(),
        sleeper: &sleeper,
        progress: &reports,
    }
    .run(&records, 0)
    .await
    .expect("the run finishes");

    let progress = reports.progress.lock().expect("lock").clone();
    assert!(!progress.is_empty());
    assert_eq!(progress.last().expect("last").embedded, records.len());
    assert_eq!(progress.last().expect("last").total, records.len());

    let checkpoints = reports.checkpoints.lock().expect("lock").clone();
    assert_eq!(checkpoints.len(), 2, "{checkpoints:?}");
    for at in checkpoints {
        assert!(at >= CHECKPOINT_EVERY, "a checkpoint before the interval");
    }
}

#[tokio::test]
async fn the_old_index_answers_every_query_until_the_swap() {
    let store = MemoryStore::new();
    let old = store.create(&model(), 4).await.expect("create");
    let embeddings = FakeEmbeddings::new(4, 0);
    let sleeper = FakeSleeper::default();
    let reports = Reports::default();

    Embedder {
        store: &store,
        index: &old.id,
        model: embeddings.as_ref(),
        backoff: Backoff::default(),
        sleeper: &sleeper,
        progress: &reports,
    }
    .run(&records(4), 0)
    .await
    .expect("run");
    swap(&store, &old.id).await.expect("swap");

    let new = store.create(&model(), 4).await.expect("create");
    Embedder {
        store: &store,
        index: &new.id,
        model: embeddings.as_ref(),
        backoff: Backoff::default(),
        sleeper: &sleeper,
        progress: &reports,
    }
    .run(&records(12), 0)
    .await
    .expect("run");

    let before = store
        .query(&[1.0, 0.0, 0.0, 0.0], 50, &Default::default())
        .await
        .expect("query");
    assert_eq!(before.len(), 4, "the half-built index answered");

    swap(&store, &new.id).await.expect("swap");
    let after = store
        .query(&[1.0, 0.0, 0.0, 0.0], 50, &Default::default())
        .await
        .expect("query");
    assert_eq!(after.len(), 12);
    assert_eq!(
        store.indexes(),
        vec![new.id],
        "the old index was not dropped"
    );
}

#[tokio::test]
async fn a_failed_run_leaves_the_live_index_untouched() {
    let store = MemoryStore::new();
    let old = store.create(&model(), 4).await.expect("create");
    let good = FakeEmbeddings::new(4, 0);
    let sleeper = FakeSleeper::default();
    let reports = Reports::default();
    Embedder {
        store: &store,
        index: &old.id,
        model: good.as_ref(),
        backoff: Backoff::default(),
        sleeper: &sleeper,
        progress: &reports,
    }
    .run(&records(4), 0)
    .await
    .expect("run");
    swap(&store, &old.id).await.expect("swap");

    let new = store.create(&model(), 4).await.expect("create");
    let broken = FakeEmbeddings::new(4, 1000);
    let _ = Embedder {
        store: &store,
        index: &new.id,
        model: broken.as_ref(),
        backoff: Backoff {
            base: Duration::from_millis(1),
            max: Duration::from_millis(2),
            attempts: 1,
        },
        sleeper: &sleeper,
        progress: &reports,
    }
    .run(&records(12), 0)
    .await
    .expect_err("the run fails");

    let hits = store
        .query(&[1.0, 0.0, 0.0, 0.0], 50, &Default::default())
        .await
        .expect("query");
    assert_eq!(hits.len(), 4, "the live index changed under a failed run");
}
