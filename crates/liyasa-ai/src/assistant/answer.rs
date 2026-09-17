//! What an answer carries (AST-12) and when it declines to give one (AST-14).

use liyasa_core::ai::TrustLevel;
use serde::{Deserialize, Serialize};

use crate::config::Deflection;
use crate::index::Hit;

/// Below this, the assistant says it is not sure and deflects (RFC 1804).
pub const LOW_CONFIDENCE: f32 = 0.45;

/// A deep link into the docs (AST-12).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Citation {
    /// `route#anchor`.
    pub href: String,
    pub title: String,
    pub breadcrumb: Vec<String>,
}

impl Citation {
    pub fn of(hit: &Hit) -> Self {
        Self {
            href: hit.record.citation(),
            title: hit.record.title.clone(),
            breadcrumb: hit.record.breadcrumb.clone(),
        }
    }
}

/// Where a reader is sent when the assistant cannot answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeflectionTarget {
    pub kind: DeflectionKind,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeflectionKind {
    Email,
    Support,
    Search,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Answer {
    pub text: String,
    pub citations: Vec<Citation>,
    /// `0.0..=1.0` (RFC 1804).
    pub confidence: f32,
    pub follow_ups: Vec<String>,
    /// Non-empty when the assistant declined or hedged (AST-14).
    pub deflection: Vec<DeflectionTarget>,
    /// Which trust level the exchange ran at, recorded so an incident can be
    /// traced (§30.2.2 item 6).
    pub trust: TrustLevel,
}

impl Answer {
    pub fn is_low_confidence(&self) -> bool {
        self.confidence < LOW_CONFIDENCE
    }
}

/// What the model is asked to return, so citations and confidence are the
/// model's own rather than inferred from its prose.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ModelAnswer {
    pub answer: String,
    /// The model's own assessment, `0.0..=1.0`.
    pub confidence: f32,
    /// `route#anchor` strings the model says it used.
    pub citations: Vec<String>,
    pub follow_ups: Vec<String>,
    /// The model says the question is not about this documentation.
    pub out_of_scope: bool,
}

/// The JSON schema sent as `output_schema`.
pub fn answer_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "answer": { "type": "string" },
            "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
            "citations": { "type": "array", "items": { "type": "string" } },
            "followUps": { "type": "array", "items": { "type": "string" } },
            "outOfScope": { "type": "boolean" }
        },
        "required": ["answer", "confidence", "citations", "followUps", "outOfScope"],
        "additionalProperties": false
    })
}

/// Retrieval and the model's self-assessment, combined (RFC 1804).
///
/// The geometric mean, so a low value on either side pulls the result down: an
/// answer the model is sure of from passages that barely matched is exactly the
/// case AST-14 exists for, and so is a confident-sounding answer over nothing
/// retrieved at all — which scores zero rather than something small.
pub fn confidence(retrieval: f32, model: f32) -> f32 {
    let retrieval = retrieval.clamp(0.0, 1.0);
    let model = model.clamp(0.0, 1.0);
    (retrieval * model).sqrt()
}

/// The retrieval side of the score: the best passage, softened by how many
/// others agreed with it.
///
/// One strong hit and nothing else is weaker evidence than three strong hits,
/// and the `k`-th hit says nothing, so only the first three count.
pub fn retrieval_score(hits: &[Hit]) -> f32 {
    if hits.is_empty() {
        return 0.0;
    }
    let top = hits[0].score.clamp(0.0, 1.0);
    let agreeing = hits.iter().take(3).filter(|h| h.score >= top * 0.8).count();
    let support = match agreeing {
        0 | 1 => 0.8,
        2 => 0.9,
        _ => 1.0,
    };
    top * support
}

/// Only citations that name something actually retrieved.
///
/// A model that invents `/guides/oauth#refresh-tokens` produces a link that
/// 404s, and a reader has no way to tell that from a real one. AST-14's "never
/// invents API fields" is enforced here for the one thing that can be checked
/// mechanically: whether the passage exists.
pub fn resolve_citations(claimed: &[String], hits: &[Hit]) -> (Vec<Citation>, Vec<String>) {
    let mut kept = Vec::new();
    let mut dropped = Vec::new();
    for claim in claimed {
        let normalized = claim.trim();
        match hits.iter().find(|hit| hit.record.citation() == normalized) {
            Some(hit) => {
                let citation = Citation::of(hit);
                if !kept.iter().any(|c: &Citation| c.href == citation.href) {
                    kept.push(citation);
                }
            }
            None => dropped.push(normalized.to_owned()),
        }
    }
    (kept, dropped)
}

/// The deflection targets an operator configured, in the order AST-14 lists
/// them.
pub fn deflection_targets(deflection: &Deflection, thread: &str) -> Vec<DeflectionTarget> {
    let mut out = Vec::new();
    if let Some(email) = deflection.email.as_ref().filter(|e| !e.is_empty()) {
        out.push(DeflectionTarget {
            kind: DeflectionKind::Email,
            value: email.clone(),
        });
    }
    if let Some(url) = deflection.support_url.as_ref().filter(|u| !u.is_empty()) {
        // The thread id travels so support can read the conversation that led
        // here (AST-14).
        let joiner = if url.contains('?') { '&' } else { '?' };
        out.push(DeflectionTarget {
            kind: DeflectionKind::Support,
            value: format!("{url}{joiner}thread={thread}"),
        });
    }
    for domain in &deflection.search_domains {
        if !domain.is_empty() {
            out.push(DeflectionTarget {
                kind: DeflectionKind::Search,
                value: domain.clone(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_retrieved_scores_zero_however_sure_the_model_is() {
        assert_eq!(retrieval_score(&[]), 0.0);
        assert_eq!(confidence(0.0, 1.0), 0.0);
    }

    #[test]
    fn a_sure_model_over_weak_passages_lands_below_the_threshold() {
        // 0.2 retrieval, 0.95 self-assessed.
        assert!(confidence(0.2, 0.95) < LOW_CONFIDENCE);
    }

    #[test]
    fn an_unsure_model_over_strong_passages_also_lands_low() {
        assert!(confidence(0.95, 0.2) < LOW_CONFIDENCE);
    }

    #[test]
    fn agreement_between_passages_raises_the_retrieval_side() {
        use liyasa_core::ids::Route;

        let hit = |score: f32, n: usize| Hit {
            record: crate::index::ChunkRecord::bare(crate::index::ChunkRecord::id_for(
                &Route::new(format!("/p{n}")),
                "",
                0,
            )),
            score,
        };
        let alone = retrieval_score(&[hit(0.9, 0)]);
        let three = retrieval_score(&[hit(0.9, 0), hit(0.88, 1), hit(0.85, 2)]);
        assert!(three > alone, "{three} is not above {alone}");
        assert!(three <= 0.9);
    }

    #[test]
    fn the_support_link_carries_the_thread_and_keeps_an_existing_query() {
        let deflection = Deflection {
            email: Some("support@example.com".to_owned()),
            support_url: Some("https://example.com/help?product=widgets".to_owned()),
            search_domains: vec!["docs.example.com".to_owned()],
        };
        let targets = deflection_targets(&deflection, "th_1");
        assert_eq!(targets.len(), 3);
        assert_eq!(
            targets[1].value,
            "https://example.com/help?product=widgets&thread=th_1"
        );
    }

    #[test]
    fn an_empty_deflection_offers_nothing_rather_than_an_empty_link() {
        assert!(deflection_targets(&Deflection::default(), "th_1").is_empty());
    }
}
