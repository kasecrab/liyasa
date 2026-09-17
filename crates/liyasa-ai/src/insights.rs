//! What the dashboard shows (AST-40).
//!
//! Every figure is derived from the stored exchanges of [`crate::privacy`], so
//! an operator who turned storage off gets no transcripts and no clusters —
//! and the dashboard says so, rather than showing an empty chart that reads as
//! "nobody asked anything".

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::assistant::answer::LOW_CONFIDENCE;
use crate::index::cosine;
use crate::privacy::{Caller, Rating, StoredExchange};

const MS_PER_DAY: u64 = 24 * 60 * 60 * 1000;

/// How near two questions must be to land in one topic. Cosine similarity, and
/// deliberately high: two clusters an operator can tell apart are more useful
/// than one they cannot.
pub const CLUSTER_THRESHOLD: f32 = 0.82;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallerBreakdown {
    pub human: u64,
    pub agent: u64,
    pub bot: u64,
}

impl CallerBreakdown {
    pub fn total(&self) -> u64 {
        self.human + self.agent + self.bot
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyVolume {
    /// Whole days since the epoch, so a chart needs no calendar type.
    pub day: u64,
    pub questions: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ratings {
    pub helpful: u64,
    pub unhelpful: u64,
    pub unrated: u64,
}

/// A group of questions that mean roughly the same thing (AST-40).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Topic {
    /// The question nearest the cluster's centre, as its label.
    pub label: String,
    pub questions: Vec<String>,
    /// Mean confidence over the cluster.
    pub confidence: f32,
    /// How many of them cited nothing.
    pub uncited: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Insights {
    pub callers: CallerBreakdown,
    pub daily: Vec<DailyVolume>,
    pub ratings: Ratings,
    /// Questions answered below the confidence threshold, worst first.
    pub low_confidence: Vec<String>,
    /// Questions that cited no page at all: the gap list AST-40 sends to the
    /// writing agent.
    pub gaps: Vec<String>,
    pub topics: Vec<Topic>,
    /// False when the operator turned storage off, so a caller can say "not
    /// recorded" rather than show zeroes.
    pub from_transcripts: bool,
}

/// Everything AST-40 shows except the clusters, which need embeddings.
pub fn summarize(rows: &[StoredExchange]) -> Insights {
    let mut callers = CallerBreakdown::default();
    let mut ratings = Ratings::default();
    let mut per_day: BTreeMap<u64, u64> = BTreeMap::new();
    let mut low: Vec<(f32, String)> = Vec::new();
    let mut gaps = Vec::new();

    for row in rows {
        match row.caller {
            Caller::Human => callers.human += 1,
            Caller::Agent => callers.agent += 1,
            Caller::Bot => callers.bot += 1,
        }
        match row.rating {
            Some(Rating::Helpful) => ratings.helpful += 1,
            Some(Rating::Unhelpful) => ratings.unhelpful += 1,
            None => ratings.unrated += 1,
        }
        *per_day.entry(row.at / MS_PER_DAY).or_default() += 1;
        if row.confidence < LOW_CONFIDENCE {
            low.push((row.confidence, row.question.clone()));
        }
        if row.citations.is_empty() {
            gaps.push(row.question.clone());
        }
    }

    low.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    Insights {
        callers,
        daily: per_day
            .into_iter()
            .map(|(day, questions)| DailyVolume { day, questions })
            .collect(),
        ratings,
        low_confidence: low.into_iter().map(|(_, q)| q).collect(),
        gaps,
        topics: Vec::new(),
        from_transcripts: !rows.is_empty(),
    }
}

/// Clusters questions by their embeddings (AST-40).
///
/// Greedy single-pass agglomeration against cluster centroids: the corpus is
/// one site's questions over one retention window, and a k-means would need a
/// `k` nobody can choose. Deterministic given the input order, which matters
/// because a dashboard that reshuffles its topics between refreshes is one an
/// operator stops trusting.
pub fn cluster(questions: &[(StoredExchange, Vec<f32>)], threshold: f32) -> Vec<Topic> {
    let mut centroids: Vec<Vec<f32>> = Vec::new();
    let mut members: Vec<Vec<usize>> = Vec::new();

    for (at, (_, vector)) in questions.iter().enumerate() {
        let best = centroids
            .iter()
            .enumerate()
            .map(|(n, centroid)| (n, cosine(centroid, vector)))
            .filter(|(_, score)| *score >= threshold)
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        match best {
            Some((n, _)) => {
                members[n].push(at);
                let count = members[n].len() as f32;
                for (i, value) in centroids[n].iter_mut().enumerate() {
                    let sample = vector.get(i).copied().unwrap_or(0.0);
                    *value += (sample - *value) / count;
                }
            }
            None => {
                centroids.push(vector.clone());
                members.push(vec![at]);
            }
        }
    }

    let mut topics: Vec<Topic> = centroids
        .iter()
        .zip(&members)
        .map(|(centroid, group)| {
            let label = group
                .iter()
                .max_by(|a, b| {
                    cosine(centroid, &questions[**a].1)
                        .partial_cmp(&cosine(centroid, &questions[**b].1))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|at| questions[*at].0.question.clone())
                .unwrap_or_default();
            let confidence = group
                .iter()
                .map(|at| questions[*at].0.confidence)
                .sum::<f32>()
                / group.len() as f32;
            Topic {
                label,
                questions: group
                    .iter()
                    .map(|at| questions[*at].0.question.clone())
                    .collect(),
                confidence,
                uncited: group
                    .iter()
                    .filter(|at| questions[**at].0.citations.is_empty())
                    .count(),
            }
        })
        .collect();

    // Largest first, then by label, so the order is stable for equal sizes.
    topics.sort_by(|a, b| {
        b.questions
            .len()
            .cmp(&a.questions.len())
            .then_with(|| a.label.cmp(&b.label))
    });
    topics
}

/// A gap-list entry as the writing agent receives it (AST-40).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GapTask {
    pub title: String,
    /// Every question in the topic, so the agent writes for all of them.
    pub questions: Vec<String>,
    pub asked: usize,
}

/// The topics with nothing cited, as tasks. A topic where some questions were
/// answered is not a gap.
pub fn gap_tasks(topics: &[Topic]) -> Vec<GapTask> {
    topics
        .iter()
        .filter(|topic| topic.uncited == topic.questions.len())
        .map(|topic| GapTask {
            title: topic.label.clone(),
            questions: topic.questions.clone(),
            asked: topic.questions.len(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(
        question: &str,
        confidence: f32,
        caller: Caller,
        cited: bool,
        day: u64,
    ) -> StoredExchange {
        StoredExchange {
            thread: "th".to_owned(),
            at: day * MS_PER_DAY + 1,
            caller,
            question: question.to_owned(),
            answer: "an answer".to_owned(),
            confidence,
            citations: if cited {
                vec!["/guides/auth#bearer".to_owned()]
            } else {
                Vec::new()
            },
            rating: None,
            reader: None,
            route: None,
        }
    }

    #[test]
    fn the_caller_breakdown_counts_every_kind() {
        let rows = vec![
            row("a", 0.9, Caller::Human, true, 1),
            row("b", 0.9, Caller::Agent, true, 1),
            row("c", 0.9, Caller::Bot, true, 2),
        ];
        let insights = summarize(&rows);
        assert_eq!(insights.callers.human, 1);
        assert_eq!(insights.callers.agent, 1);
        assert_eq!(insights.callers.bot, 1);
        assert_eq!(insights.callers.total(), 3);
        assert_eq!(insights.daily.len(), 2);
        assert_eq!(insights.daily[0].questions, 2);
    }

    #[test]
    fn a_question_that_cited_nothing_is_a_gap_whatever_its_confidence() {
        let rows = vec![row("what is the sla?", 0.9, Caller::Human, false, 1)];
        let insights = summarize(&rows);
        assert_eq!(insights.gaps, ["what is the sla?"]);
        // High confidence with no citation is not low-confidence, and would be
        // invisible if the gap list were derived from the score.
        assert!(insights.low_confidence.is_empty());
    }

    #[test]
    fn low_confidence_questions_come_back_worst_first() {
        let rows = vec![
            row("b", 0.4, Caller::Human, true, 1),
            row("a", 0.1, Caller::Human, true, 1),
            row("fine", 0.9, Caller::Human, true, 1),
        ];
        assert_eq!(summarize(&rows).low_confidence, ["a", "b"]);
    }

    #[test]
    fn no_transcripts_says_so_rather_than_showing_zeroes() {
        let insights = summarize(&[]);
        assert!(!insights.from_transcripts);
        assert_eq!(insights.callers.total(), 0);
    }

    #[test]
    fn near_questions_cluster_and_far_ones_do_not() {
        let near = |n: f32| vec![n.cos(), n.sin()];
        let questions = vec![
            (
                row("how do i authenticate", 0.2, Caller::Human, false, 1),
                near(0.0),
            ),
            (
                row("how do i sign in", 0.2, Caller::Human, false, 1),
                near(0.05),
            ),
            (
                row("what is the sla", 0.2, Caller::Human, false, 1),
                near(1.4),
            ),
        ];
        let topics = cluster(&questions, CLUSTER_THRESHOLD);
        assert_eq!(topics.len(), 2, "{topics:?}");
        assert_eq!(topics[0].questions.len(), 2);
        assert_eq!(topics[1].questions.len(), 1);
    }

    #[test]
    fn a_topic_with_one_answered_question_is_not_a_gap() {
        let near = |n: f32| vec![n.cos(), n.sin()];
        let questions = vec![
            (row("a", 0.2, Caller::Human, false, 1), near(0.0)),
            (row("b", 0.2, Caller::Human, true, 1), near(0.02)),
            (row("c", 0.2, Caller::Human, false, 1), near(1.4)),
        ];
        let topics = cluster(&questions, CLUSTER_THRESHOLD);
        let tasks = gap_tasks(&topics);
        assert_eq!(tasks.len(), 1, "{topics:?}");
        assert_eq!(tasks[0].title, "c");
    }

    #[test]
    fn clustering_the_same_input_twice_gives_the_same_order() {
        let near = |n: f32| vec![n.cos(), n.sin()];
        let questions: Vec<(StoredExchange, Vec<f32>)> = (0..10)
            .map(|n| {
                (
                    row(&format!("q{n}"), 0.5, Caller::Human, false, 1),
                    near(n as f32 * 0.3),
                )
            })
            .collect();
        assert_eq!(
            cluster(&questions, CLUSTER_THRESHOLD),
            cluster(&questions, CLUSTER_THRESHOLD)
        );
    }
}
