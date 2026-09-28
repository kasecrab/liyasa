//! "Did you mean" — SRC-04's typo-tolerant suggestions.
//!
//! Separate from the fuzzy expansion in [`super::search`], which widens a query
//! silently and so has to stay conservative: a search for one word that quietly
//! answers with another is worse than a search that finds nothing. A suggestion
//! is offered rather than applied, so it can reach further, and it is the only
//! thing that helps a reader whose typo matched nothing at all.
//!
//! Ranking is by document frequency: the commonest neighbour is the likeliest
//! intent, for the same reason a search engine suggests "address" over "adduce"
//! for "addres". The frequency comes from each term's postings block, which
//! carries the corpus-wide `global_df`.
//!
//! Not from `manifest.idf`, which looks like the obvious source and is not one:
//! `writer::cross_shard_idf` inserts a term only when it appears in more than
//! one shard, because the table exists to keep scores comparable across shards
//! rather than to be a frequency index. A single-shard site has an empty table,
//! so ranking by it would silently degrade to "shortest, then alphabetical" —
//! which is what the first version of this file did, plausibly and wrongly,
//! until a test asked which of two real candidates won.

use super::query::Query;
use super::reader::ShardReader;

/// One term the reader probably meant.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    /// The query term this corrects, as the parsed query carries it — so the
    /// stem, not the reader's raw word. A caller rewrites the parsed query by
    /// replacing this term; it cannot substitute into the raw string, because
    /// `limis` reaches here as `limi`.
    pub term: String,
    /// An indexed term, so a stemmed one: the suggestion for `limis` is
    /// `limit`, not `limits`. The index holds stems and there is nothing here
    /// to unstem them with, and a reader who sees `limit` has what they need.
    pub suggested: String,
}

/// How far a suggestion may reach. One edit is the fuzzy rule; a second is
/// allowed only for a long term, where two edits still leave a word recognizable
/// and the neighbourhood is small enough that a match means something.
const TWO_EDIT_MIN_LENGTH: usize = 8;

/// Suggests a correction for every query term this shard does not hold.
///
/// A term the index holds is never corrected, however rare it is: the reader
/// typed a word that exists, and second-guessing that is how a search engine
/// starts answering questions nobody asked.
pub fn for_query(reader: &ShardReader<'_>, query: &Query) -> Vec<Suggestion> {
    let mut out: Vec<Suggestion> = Vec::new();

    for term in &query.terms {
        let typed = term.text.as_str();
        // `fuzzy` is `typed_length.max(stem_length) >= FUZZY_MIN_LENGTH`,
        // already computed by the parser. Gating on the stem's own length
        // instead would drop the common case: `limis` stems to `limi`, four
        // characters, and is exactly the typo worth correcting.
        if !term.fuzzy || reader.contains(typed) || out.iter().any(|s| s.term == typed) {
            continue;
        }
        if let Some(suggested) = best(reader, typed) {
            out.push(Suggestion {
                term: typed.to_owned(),
                suggested,
            });
        }
    }
    out
}

/// The commonest term within reach of `typed`, or nothing.
fn best(reader: &ShardReader<'_>, typed: &str) -> Option<String> {
    // The cap is the same order as the fuzzy expansion cap: a suggestion that
    // took longer than the search it is attached to would be worse than no
    // suggestion (SRC-05's 50 ms budget).
    let mut candidates = reader.terms_within(typed, 1, 32);
    if candidates.is_empty() && typed.chars().count() >= TWO_EDIT_MIN_LENGTH {
        candidates = reader.terms_within(typed, 2, 32);
    }

    candidates
        .into_iter()
        .filter(|candidate| candidate != typed)
        // A block that will not decode is a corrupt shard, and a suggestion is
        // not worth failing a query over: drop the candidate and rank the rest.
        .filter_map(|candidate| {
            let df = reader.block(&candidate).ok().flatten()?.global_df;
            Some((candidate, df))
        })
        .max_by(|a, b| {
            a.1.cmp(&b.1)
                // Ties go to the shorter term, then reverse-alphabetically, so
                // that `max_by` — which keeps the last of equal elements —
                // yields the same answer on every machine.
                .then_with(|| b.0.chars().count().cmp(&a.0.chars().count()))
                .then_with(|| b.0.cmp(&a.0))
        })
        .map(|(candidate, _)| candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_edit_threshold_is_above_the_one_edit_one() {
        assert!(
            TWO_EDIT_MIN_LENGTH > crate::idx::query::FUZZY_MIN_LENGTH,
            "a second edit is for long words only"
        );
    }
}
