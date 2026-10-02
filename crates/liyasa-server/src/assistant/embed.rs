//! The production [`Embed`]: one query, one vector, from a real provider.
//!
//! `ServerTools::search` returns early unless it holds BOTH an index and an
//! `Embed`:
//!
//! ```text
//! let (Some(index), Some(embed)) = (&self.index, &self.embed) else {
//!     return Ok(Vec::new());
//! };
//! ```
//!
//! Until this file, the workspace's only `impl Embed` was a four-dimensional
//! fixed vector in WP-25's test, so `with_index` was unconstructable from
//! production and **every `ServerTools::search` anywhere returned an empty
//! vector** — the assistant's and the agent research job's alike.
//!
//! What hid it is worth keeping: the comment above that early return is
//! correct. An unindexed site genuinely retrieves nothing, the assistant
//! scores that at 0.0 and deflects (RFC 1804), and reporting `Unavailable`
//! would make an unindexed site look broken. Every word true, and its
//! rightness is the camouflage — a reader checking whether the empty answer is
//! justified finds a justified empty answer and never asks whether it is the
//! only answer reachable. Three tests asserted `passages` was an array and all
//! three passed on the shape of nothing.
//!
//! `EmbeddingModel` is the frozen contract (§34.9) and batches: `&[String]` in,
//! `Vec<Vec<f32>>` out. `Embed` is one query. This is that narrowing, and the
//! narrowing is where a provider that answers with the wrong number of vectors
//! has to be caught rather than indexed into.

use std::sync::Arc;

use liyasa_core::ai::EmbeddingModel;
use liyasa_core::net::BoxFut;

use crate::routes::tools::Embed;

/// An [`EmbeddingModel`] as the single-query [`Embed`] the tools want.
pub struct ModelEmbed {
    model: Arc<dyn EmbeddingModel>,
}

impl ModelEmbed {
    pub fn new(model: Arc<dyn EmbeddingModel>) -> Self {
        Self { model }
    }

    /// The width this embedder produces, for checking it against the index's
    /// own `dims` before a query rather than after a meaningless score.
    pub fn dims(&self) -> usize {
        self.model.dims()
    }
}

impl std::fmt::Debug for ModelEmbed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelEmbed")
            .field("model", &self.model.id())
            .field("dims", &self.model.dims())
            .finish()
    }
}

impl Embed for ModelEmbed {
    fn embed<'a>(&'a self, query: &'a str) -> BoxFut<'a, Result<Vec<f32>, String>> {
        Box::pin(async move {
            let inputs = [query.to_owned()];
            let mut vectors = self
                .model
                .embed(&inputs)
                .await
                .map_err(|e| format!("the embedding provider refused the query: {e}"))?;
            // One input, so one vector. A provider that returns none would
            // otherwise reach the store as an empty vector, whose cosine
            // similarity against everything is zero — scoring every chunk
            // equally and returning `k` arbitrary ones, which reads as a
            // working search over an irrelevant answer.
            if vectors.len() != 1 {
                return Err(format!(
                    "the embedding provider answered {} vectors for one query; refusing to \
                     search on that rather than scoring every chunk the same",
                    vectors.len()
                ));
            }
            let vector = vectors.remove(0);
            if vector.is_empty() {
                return Err("the embedding provider answered an empty vector".to_owned());
            }
            Ok(vector)
        })
    }
}
