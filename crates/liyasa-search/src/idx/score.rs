//! BM25 against global statistics, plus the three adjustments SRC-03 names:
//! config boosts, an exact-phrase bonus, and a recency tie-break.
//!
//! One implementation, called by the browser reader and by the server
//! (plan/rfcs/0703-server-ranks-with-idx.md), so "both implement the same
//! ranking rules" (RX-31) is true by construction rather than by two people
//! reading the same paragraph.

use super::field::{ByField, Field};

/// Term-frequency saturation. tantivy's default, and the value the BM25 paper
/// settles on for prose.
pub const K1: f32 = 1.2;
/// Length normalization. 0 ignores length, 1 fully normalizes.
pub const B: f32 = 0.75;
/// What an exact phrase adds, per field, scaled by that field's weight. Two
/// terms next to each other in the title is the strongest signal a keyword
/// index has that the reader meant this document.
pub const PHRASE_BONUS: f32 = 2.0;

/// Corpus-wide statistics. Never a shard's own: a term that is rare in the
/// German shard and common overall must score as common, or the same query
/// ranks differently depending on which page the reader opened the dialog on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stats {
    pub documents: u64,
    pub average_length: ByField<f32>,
    pub weights: ByField<f32>,
}

impl Default for Stats {
    fn default() -> Self {
        let mut weights = ByField([0.0; 6]);
        for field in Field::ALL {
            weights[field] = field.weight();
        }
        Self {
            documents: 0,
            average_length: ByField([0.0; 6]),
            weights,
        }
    }
}

/// `ln(1 + (N - df + 0.5) / (df + 0.5))`, the probabilistic form with the
/// `+1` that keeps it non-negative. A term in every document contributes
/// almost nothing rather than a negative score.
pub fn idf(documents: u64, document_frequency: u32) -> f32 {
    let n = documents.max(1) as f32;
    let df = (document_frequency.max(1) as f32).min(n);
    (1.0 + (n - df + 0.5) / (df + 0.5)).ln()
}

/// One term's contribution from one field, before the field weight.
pub fn field_score(term_frequency: u32, length: u32, average_length: f32, idf: f32) -> f32 {
    if term_frequency == 0 {
        return 0.0;
    }
    let tf = term_frequency as f32;
    // A corpus where a field is empty everywhere has no length to normalize
    // against; treat it as the neutral length rather than dividing by zero.
    let average = if average_length > 0.0 {
        average_length
    } else {
        1.0
    };
    let normalized = 1.0 - B + B * (length as f32 / average);
    idf * (tf * (K1 + 1.0)) / (tf + K1 * normalized)
}

/// One document's score for one term, summed over the fields it occurs in.
pub fn term_score(
    stats: &Stats,
    lengths: &ByField<u32>,
    idf: f32,
    frequencies: &ByField<u32>,
) -> f32 {
    Field::ALL
        .into_iter()
        .map(|field| {
            stats.weights[field]
                * field_score(
                    frequencies[field],
                    lengths[field],
                    stats.average_length[field],
                    idf,
                )
        })
        .sum()
}

/// What an exact phrase adds when its terms are adjacent in `field`.
pub fn phrase_bonus(stats: &Stats, field: Field, occurrences: u32) -> f32 {
    if occurrences == 0 {
        return 0.0;
    }
    // Saturating, like term frequency: the second occurrence of a phrase says
    // much less than the first.
    stats.weights[field] * PHRASE_BONUS * (1.0 + (occurrences as f32).ln())
}

/// Applies the document's configured boost (§8.6). A factor under 1
/// de-prioritizes, as CFG-51 says.
pub fn with_boost(score: f32, boost: f32) -> f32 {
    score * if boost > 0.0 { boost } else { 1.0 }
}

/// What [`rank`] needs of a hit.
pub trait Ranked {
    fn score(&self) -> f32;
    /// Milliseconds since the epoch; `None` sorts last.
    fn updated(&self) -> Option<u64>;
    /// The last resort, so two identical documents always sort the same way
    /// and a build is reproducible (§6.6.2).
    fn tiebreak_key(&self) -> &str;
}

/// Two scores this close are a tie, and recency decides. Float summation order
/// differs between the two implementations (§12.2's parity gate says so), so
/// the comparison is relative rather than exact.
pub fn is_tie(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1e-4 * a.abs().max(b.abs()).max(1.0)
}

/// Sorts best first: score, then recency, then key.
pub fn rank<T: Ranked>(hits: &mut [T]) {
    hits.sort_by(|a, b| {
        if !is_tie(a.score(), b.score()) {
            return b
                .score()
                .partial_cmp(&a.score())
                .unwrap_or(std::cmp::Ordering::Equal);
        }
        b.updated()
            .cmp(&a.updated())
            .then_with(|| a.tiebreak_key().cmp(b.tiebreak_key()))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Hit {
        score: f32,
        updated: Option<u64>,
        key: &'static str,
    }

    impl Ranked for Hit {
        fn score(&self) -> f32 {
            self.score
        }
        fn updated(&self) -> Option<u64> {
            self.updated
        }
        fn tiebreak_key(&self) -> &str {
            self.key
        }
    }

    fn stats() -> Stats {
        Stats {
            documents: 1000,
            average_length: ByField([4.0, 4.0, 4.0, 6.0, 120.0, 40.0]),
            ..Stats::default()
        }
    }

    #[test]
    fn a_rare_term_outweighs_a_common_one() {
        assert!(idf(1000, 2) > idf(1000, 500));
    }

    #[test]
    fn a_term_in_every_document_is_worth_almost_nothing_and_never_less() {
        let none = idf(1000, 1000);
        assert!(none >= 0.0, "{none}");
        assert!(none < 0.01, "{none}");
    }

    #[test]
    fn idf_survives_an_empty_corpus_and_a_frequency_past_the_count() {
        assert!(idf(0, 0).is_finite());
        assert!(idf(10, 99).is_finite());
    }

    #[test]
    fn term_frequency_saturates() {
        let at = |tf| field_score(tf, 100, 100.0, 1.0);
        assert!(at(2) > at(1));
        assert!(at(10) > at(2));
        assert!(
            at(10) - at(9) < at(2) - at(1),
            "the tenth occurrence must add less than the second"
        );
    }

    #[test]
    fn a_longer_field_scores_lower_for_the_same_frequency() {
        let short = field_score(2, 20, 100.0, 1.0);
        let long = field_score(2, 400, 100.0, 1.0);
        assert!(short > long);
    }

    #[test]
    fn an_empty_field_is_zero_not_a_division_by_zero() {
        assert_eq!(field_score(0, 0, 0.0, 1.0), 0.0);
        assert!(field_score(1, 0, 0.0, 1.0).is_finite());
    }

    #[test]
    fn the_title_outweighs_the_body_for_the_same_term() {
        let stats = stats();
        let lengths = ByField([4, 4, 0, 6, 120, 0]);
        let mut in_title = ByField([0u32; 6]);
        in_title[Field::Title] = 1;
        let mut in_body = ByField([0u32; 6]);
        in_body[Field::Body] = 1;
        let idf = idf(1000, 10);
        assert!(
            term_score(&stats, &lengths, idf, &in_title)
                > term_score(&stats, &lengths, idf, &in_body)
        );
    }

    #[test]
    fn code_is_the_lightest_field() {
        let stats = stats();
        let lengths = ByField([4, 4, 4, 6, 40, 40]);
        let idf = idf(1000, 10);
        let mut in_body = ByField([0u32; 6]);
        in_body[Field::Body] = 1;
        let mut in_code = ByField([0u32; 6]);
        in_code[Field::Code] = 1;
        assert!(
            term_score(&stats, &lengths, idf, &in_body)
                > term_score(&stats, &lengths, idf, &in_code)
        );
    }

    #[test]
    fn a_phrase_in_the_title_is_worth_more_than_one_in_the_body() {
        let stats = stats();
        assert!(phrase_bonus(&stats, Field::Title, 1) > phrase_bonus(&stats, Field::Body, 1));
        assert_eq!(phrase_bonus(&stats, Field::Title, 0), 0.0);
    }

    #[test]
    fn a_boost_under_one_deprioritizes() {
        assert!(with_boost(10.0, 0.5) < 10.0);
        assert!(with_boost(10.0, 2.0) > 10.0);
        assert_eq!(with_boost(10.0, 0.0), 10.0, "a zero boost is not a mute");
    }

    #[test]
    fn recency_breaks_a_tie() {
        let mut hits = [
            Hit {
                score: 1.0,
                updated: Some(1),
                key: "/old",
            },
            Hit {
                score: 1.0,
                updated: Some(9),
                key: "/new",
            },
        ];
        rank(&mut hits);
        assert_eq!(hits[0].key, "/new");
    }

    #[test]
    fn a_dated_page_outranks_an_undated_one_on_a_tie() {
        let mut hits = [
            Hit {
                score: 1.0,
                updated: None,
                key: "/a",
            },
            Hit {
                score: 1.0,
                updated: Some(1),
                key: "/z",
            },
        ];
        rank(&mut hits);
        assert_eq!(hits[0].key, "/z");
    }

    #[test]
    fn score_beats_recency_when_it_is_not_a_tie() {
        let mut hits = [
            Hit {
                score: 1.0,
                updated: Some(9),
                key: "/new",
            },
            Hit {
                score: 5.0,
                updated: Some(1),
                key: "/old",
            },
        ];
        rank(&mut hits);
        assert_eq!(hits[0].key, "/old");
    }

    #[test]
    fn a_complete_tie_sorts_by_key_so_a_build_is_reproducible() {
        let mut hits = [
            Hit {
                score: 1.0,
                updated: None,
                key: "/z",
            },
            Hit {
                score: 1.0,
                updated: None,
                key: "/a",
            },
        ];
        rank(&mut hits);
        assert_eq!([hits[0].key, hits[1].key], ["/a", "/z"]);
    }

    #[test]
    fn scores_a_float_apart_are_a_tie() {
        assert!(is_tie(3.141_592_5, 3.141_592_7));
        assert!(!is_tie(3.0, 3.1));
    }
}
