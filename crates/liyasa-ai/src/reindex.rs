//! Re-index economics (AST-05) and the incremental half of AST-01.
//!
//! Two properties do the work here and both are about not surprising an
//! operator: a re-index that would cost real money is priced and confirmed
//! before it runs, and the old index answers every query until the new one is
//! complete and swapped in one transaction.
//!
//! The sleeps are injected. A backoff test that waits for a real delay measures
//! the machine rather than the backoff, and this fleet has twice mistaken a
//! wall clock for a result.

use std::collections::BTreeMap;
use std::time::Duration;

use liyasa_core::ai::{AiError, EmbeddingModel};
use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::ids::{ChunkId, IndexId, Route};
use liyasa_core::net::BoxFut;

use crate::chunk::estimate_tokens;
use crate::config::{ModelRef, ReindexConfig};
use crate::index::{ChunkRecord, IndexChange, IndexError, VectorStore};

/// Embeddings per request. Every provider in §6.7 accepts a batch; one chunk
/// per request would cost a round trip each for a corpus of tens of thousands.
pub const BATCH: usize = 64;

/// AST-05: checkpointed every 1,000 chunks so a restart resumes.
pub const CHECKPOINT_EVERY: usize = 1000;

/// Hundredths of a currency unit, the unit `ai.reindex.autoApproveCents` uses.
pub type Cents = u64;

/// Cents per million input tokens, by `provider:model`.
///
/// Supplied by the caller rather than compiled in. A price table in the binary
/// would be wrong within a quarter and an operator comparing a wrong estimate
/// with a real invoice has no way to tell which is which (RFC 1805).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PriceList(pub BTreeMap<String, Cents>);

impl PriceList {
    pub fn get(&self, model: &ModelRef) -> Option<Cents> {
        self.0.get(&model.to_string()).copied()
    }
}

/// What a re-index would cost, before it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Estimate {
    pub chunks: usize,
    pub tokens: usize,
    /// `None` when no price is known for the model. Not zero: an unknown price
    /// must not read as a free run.
    pub cents: Option<Cents>,
    pub model: ModelRef,
    pub reason: Option<IndexChange>,
}

impl Estimate {
    pub fn of(
        records: &[ChunkRecord],
        model: &ModelRef,
        prices: &PriceList,
        reason: Option<IndexChange>,
    ) -> Self {
        let tokens: usize = records
            .iter()
            .map(|record| {
                if record.tokens > 0 {
                    record.tokens as usize
                } else {
                    estimate_tokens(&record.text)
                }
            })
            .sum();
        let cents = prices
            .get(model)
            .map(|per_million| (tokens as u64).saturating_mul(per_million) / 1_000_000);
        Self {
            chunks: records.len(),
            tokens,
            cents,
            model: model.clone(),
            reason,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Approval {
    /// Below `ai.reindex.autoApproveCents`.
    Auto,
    /// Shown for confirmation, with the diagnostic an operator sees.
    NeedsConfirmation(Box<Diagnostic>),
}

/// AST-05's threshold.
///
/// An estimate with no price NEEDS confirmation. Treating an unknown price as
/// zero would auto-approve exactly the runs nobody has priced, which is the
/// opposite of what the threshold is for.
pub fn approval(estimate: &Estimate, config: &ReindexConfig) -> Approval {
    let threshold = Cents::from(config.auto_approve_cents);
    match estimate.cents {
        Some(cents) if cents < threshold => Approval::Auto,
        Some(cents) => Approval::NeedsConfirmation(Box::new(
            Diagnostic::new(
                code::W0911,
                format!(
                    "re-indexing {} chunks with `{}` is estimated at {cents} cents, above the \
                     {threshold}-cent auto-approval threshold",
                    estimate.chunks, estimate.model
                ),
            )
            .help(
                "confirm the run on the dashboard, or raise `ai.reindex.autoApproveCents`. The \
                 figure is an estimate: token counts are vendor-neutral (RFC 1800)",
            ),
        )),
        None => Approval::NeedsConfirmation(Box::new(
            Diagnostic::new(
                code::W0911,
                format!(
                    "no list price is known for `{}`, so re-indexing {} chunks cannot be \
                     estimated",
                    estimate.model, estimate.chunks
                ),
            )
            .help("confirm the run on the dashboard; an unpriced model is never auto-approved"),
        )),
    }
}

/// The diagnostic AST-04 raises when the configured model no longer matches the
/// active index.
pub fn model_changed(change: &IndexChange) -> Diagnostic {
    let message = match change {
        IndexChange::Model { from, to } => {
            format!("the embedding model changed from `{from}` to `{to}`")
        }
        IndexChange::Dimension { from, to } => format!(
            "the embedding dimension changed from {from} to {to}; a new table is created rather \
             than migrating rows"
        ),
    };
    Diagnostic::new(code::W0906, message)
        .help("the old index answers every query until the new one is complete")
}

/// What an incremental pass must do for one route (AST-01).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Delta {
    /// New or changed: these are embedded.
    pub embed: Vec<ChunkRecord>,
    /// Rows for chunks the page no longer has.
    pub delete: Vec<ChunkId>,
    /// Unchanged, and so not re-embedded. Counted for the progress report.
    pub unchanged: usize,
}

/// Compares what the index holds for a route with what the page now produces.
pub fn delta(existing: &[(ChunkId, String)], produced: Vec<ChunkRecord>) -> Delta {
    let held: BTreeMap<&ChunkId, &String> = existing.iter().map(|(id, h)| (id, h)).collect();
    let mut out = Delta::default();
    let mut kept: Vec<ChunkId> = Vec::new();
    for record in produced {
        match held.get(&record.id) {
            Some(hash) if **hash == record.content_hash => {
                out.unchanged += 1;
                kept.push(record.id);
            }
            _ => {
                kept.push(record.id.clone());
                out.embed.push(record);
            }
        }
    }
    out.delete = existing
        .iter()
        .map(|(id, _)| id)
        .filter(|id| !kept.contains(id))
        .cloned()
        .collect();
    out
}

/// Where the run has got to (AST-05: visible on the dashboard).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub embedded: usize,
    pub total: usize,
    /// The last chunk count a restart would resume from.
    pub checkpoint: usize,
    pub retries: u32,
}

pub trait ProgressSink: Send + Sync {
    fn report(&self, progress: &Progress);
    /// Called every [`CHECKPOINT_EVERY`] chunks. Storing it is the caller's
    /// job; a restart resumes from what was stored.
    fn checkpoint(&self, at: usize);
}

/// Injected so a backoff test does not wait (AST-05's exponential backoff).
pub trait Sleeper: Send + Sync {
    fn sleep(&self, duration: Duration) -> BoxFut<'_, ()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backoff {
    pub base: Duration,
    pub max: Duration,
    pub attempts: u32,
}

impl Default for Backoff {
    fn default() -> Self {
        Self {
            base: Duration::from_millis(500),
            max: Duration::from_secs(60),
            attempts: 6,
        }
    }
}

impl Backoff {
    /// `base * 2^attempt`, capped. No jitter: the delay is per provider and one
    /// job holds the at-most-once lease, so there is no herd to disperse.
    pub fn delay(&self, attempt: u32, hinted: Option<Duration>) -> Duration {
        // A provider that named a retry time is obeyed rather than doubled.
        if let Some(hint) = hinted {
            return hint.min(self.max);
        }
        let factor = 1u32.checked_shl(attempt).unwrap_or(u32::MAX);
        self.base.saturating_mul(factor).min(self.max)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ReindexError {
    #[error("{0}")]
    Embedding(#[from] AiError),
    #[error("{0}")]
    Index(#[from] IndexError),
    #[error("the provider rate-limited {attempts} attempts in a row")]
    RateLimited { attempts: u32 },
}

/// Embeds `records` into `index`, in batches, with backoff on 429.
///
/// Writes into `index`, which is NOT the active one during a full re-index: the
/// old index answers every query until [`swap`] runs.
pub async fn embed_into(
    store: &dyn VectorStore,
    index: &IndexId,
    model: &dyn EmbeddingModel,
    records: &[ChunkRecord],
    from: usize,
    backoff: Backoff,
    sleeper: &dyn Sleeper,
    progress: &dyn ProgressSink,
) -> Result<usize, ReindexError> {
    let mut embedded = from;
    let mut retries = 0;
    let mut last_checkpoint = from;

    for batch in records[from.min(records.len())..].chunks(BATCH) {
        let inputs: Vec<String> = batch.iter().map(|r| r.text.clone()).collect();
        let vectors = loop {
            match model.embed(&inputs).await {
                Ok(vectors) => break vectors,
                Err(AiError::RateLimited { retry_after }) => {
                    if retries >= backoff.attempts {
                        return Err(ReindexError::RateLimited { attempts: retries });
                    }
                    sleeper.sleep(backoff.delay(retries, retry_after)).await;
                    retries += 1;
                }
                Err(other) => return Err(other.into()),
            }
        };

        let rows: Vec<(ChunkRecord, Vec<f32>)> = batch.iter().cloned().zip(vectors).collect();
        store.upsert(index, &rows).await?;
        embedded += rows.len();

        progress.report(&Progress {
            embedded,
            total: records.len(),
            checkpoint: last_checkpoint,
            retries,
        });
        // On crossing a multiple of the interval, not on having advanced by
        // it: a batch size that does not divide the interval otherwise drifts,
        // and the last checkpoint of a run can be missed entirely. Measured at
        // 2,010 chunks in batches of 64, where the drifting rule checkpointed
        // once instead of twice and a restart would have re-embedded 986.
        if embedded / CHECKPOINT_EVERY > last_checkpoint / CHECKPOINT_EVERY {
            last_checkpoint = embedded;
            progress.checkpoint(embedded);
        }
    }
    Ok(embedded)
}

/// One transaction: point at `index`, drop what it replaced (AST-05).
pub async fn swap(store: &dyn VectorStore, index: &IndexId) -> Result<(), ReindexError> {
    store.swap_active(index).await?;
    Ok(())
}

/// The routes an incremental pass touches, in a stable order.
pub fn routes_of(records: &[ChunkRecord]) -> Vec<Route> {
    let mut out: Vec<Route> = records.iter().map(|r| r.route.clone()).collect();
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> ModelRef {
        "openai:text-embedding-3-small".parse().expect("model")
    }

    fn prices() -> PriceList {
        // Two cents per million tokens: a number this test chose, not a claim
        // about any provider's price list.
        PriceList(BTreeMap::from([(model().to_string(), 2)]))
    }

    fn record(n: usize, hash: &str) -> ChunkRecord {
        let route = Route::new("/p");
        let mut record = ChunkRecord::bare(ChunkRecord::id_for(&route, "", n as u32));
        record.route = route;
        record.content_hash = hash.to_owned();
        record.tokens = 500_000;
        record
    }

    #[test]
    fn an_unpriced_model_is_never_auto_approved() {
        let records = vec![record(0, "a")];
        let estimate = Estimate::of(&records, &model(), &PriceList::default(), None);
        assert_eq!(estimate.cents, None);
        assert!(matches!(
            approval(&estimate, &ReindexConfig::default()),
            Approval::NeedsConfirmation(_)
        ));
    }

    #[test]
    fn a_cheap_run_is_auto_approved_and_an_expensive_one_is_not() {
        let cheap = Estimate::of(&[record(0, "a")], &model(), &prices(), None);
        assert_eq!(cheap.cents, Some(1));
        assert_eq!(approval(&cheap, &ReindexConfig::default()), Approval::Auto);

        let many: Vec<ChunkRecord> = (0..600).map(|n| record(n, "a")).collect();
        let expensive = Estimate::of(&many, &model(), &prices(), None);
        assert_eq!(expensive.cents, Some(600));
        assert!(matches!(
            approval(&expensive, &ReindexConfig::default()),
            Approval::NeedsConfirmation(_)
        ));
    }

    #[test]
    fn the_threshold_is_the_configured_one() {
        let estimate = Estimate::of(
            &(0..600).map(|n| record(n, "a")).collect::<Vec<_>>(),
            &model(),
            &prices(),
            None,
        );
        let config = ReindexConfig {
            auto_approve_cents: 1000,
        };
        assert_eq!(approval(&estimate, &config), Approval::Auto);
    }

    #[test]
    fn only_a_changed_chunk_is_embedded() {
        let held = vec![
            (
                ChunkRecord::id_for(&Route::new("/p"), "", 0),
                "a".to_owned(),
            ),
            (
                ChunkRecord::id_for(&Route::new("/p"), "", 1),
                "b".to_owned(),
            ),
            (
                ChunkRecord::id_for(&Route::new("/p"), "", 2),
                "c".to_owned(),
            ),
        ];
        let produced = vec![record(0, "a"), record(1, "b-edited")];
        let delta = delta(&held, produced);

        assert_eq!(delta.unchanged, 1);
        assert_eq!(delta.embed.len(), 1);
        assert_eq!(delta.embed[0].content_hash, "b-edited");
        assert_eq!(delta.delete.len(), 1, "the removed chunk's row must go");
        assert_eq!(
            delta.delete[0],
            ChunkRecord::id_for(&Route::new("/p"), "", 2)
        );
    }

    #[test]
    fn a_page_with_no_changes_embeds_nothing() {
        let held = vec![(
            ChunkRecord::id_for(&Route::new("/p"), "", 0),
            "a".to_owned(),
        )];
        let delta = delta(&held, vec![record(0, "a")]);
        assert!(delta.embed.is_empty());
        assert!(delta.delete.is_empty());
        assert_eq!(delta.unchanged, 1);
    }

    #[test]
    fn the_backoff_doubles_and_stops_at_the_cap() {
        let backoff = Backoff {
            base: Duration::from_millis(100),
            max: Duration::from_secs(1),
            attempts: 6,
        };
        assert_eq!(backoff.delay(0, None), Duration::from_millis(100));
        assert_eq!(backoff.delay(1, None), Duration::from_millis(200));
        assert_eq!(backoff.delay(3, None), Duration::from_millis(800));
        assert_eq!(backoff.delay(10, None), Duration::from_secs(1));
    }

    #[test]
    fn a_providers_own_retry_hint_wins_over_the_doubling() {
        let backoff = Backoff::default();
        assert_eq!(
            backoff.delay(5, Some(Duration::from_secs(2))),
            Duration::from_secs(2)
        );
        // But never above the cap.
        assert_eq!(
            backoff.delay(0, Some(Duration::from_secs(600))),
            backoff.max
        );
    }

    #[test]
    fn a_dimension_change_says_it_creates_a_table() {
        let d = model_changed(&IndexChange::Dimension {
            from: 1536,
            to: 3072,
        });
        assert_eq!(d.code, code::W0906);
        assert!(d.message.contains("new table"), "{}", d.message);
    }
}
