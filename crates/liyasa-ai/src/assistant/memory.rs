//! Conversation memory within a thread (AST-16).
//!
//! A thread is held by the caller: the browser keeps one in local storage, and
//! the server keeps one only when the reader consented or is signed in
//! (AST-22). This module is the shape and the trimming rule; where it is stored
//! is the server's decision, not this crate's.

use liyasa_core::ai::TrustLevel;
use serde::{Deserialize, Serialize};

use super::answer::Answer;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Turn {
    pub question: String,
    pub answer: Option<Answer>,
    /// Milliseconds since the epoch.
    pub at: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub id: String,
    pub turns: Vec<Turn>,
    /// Whether the reader agreed to the server keeping this (AST-22). A thread
    /// with `false` here lives in the browser only.
    pub stored: bool,
}

impl Thread {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            turns: Vec::new(),
            stored: false,
        }
    }

    /// The turns that fit in `budget` tokens, most recent last.
    ///
    /// Trimmed from the FRONT: the question being answered is the last one, and
    /// dropping it to keep an older turn would answer the wrong question.
    pub fn recent(&self, budget: usize) -> &[Turn] {
        let mut total = 0;
        let mut first = self.turns.len();
        for (at, turn) in self.turns.iter().enumerate().rev() {
            let cost = crate::chunk::estimate_tokens(&turn.question)
                + turn
                    .answer
                    .as_ref()
                    .map_or(0, |a| crate::chunk::estimate_tokens(&a.text));
            if total + cost > budget && at + 1 < self.turns.len() {
                break;
            }
            total += cost;
            first = at;
        }
        &self.turns[first..]
    }

    /// The trust the whole thread runs at: the least trusted thing in it.
    pub fn trust(&self) -> TrustLevel {
        self.turns
            .iter()
            .filter_map(|t| t.answer.as_ref().map(|a| a.trust))
            .chain(std::iter::once(TrustLevel::Anonymous))
            .max()
            .unwrap_or(TrustLevel::Anonymous)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(question: &str) -> Turn {
        Turn {
            question: question.to_owned(),
            answer: None,
            at: 0,
        }
    }

    #[test]
    fn a_small_budget_keeps_the_question_being_answered() {
        let mut thread = Thread::new("th_1");
        thread.turns.push(turn(&"old ".repeat(200)));
        thread.turns.push(turn("the newest question"));
        let recent = thread.recent(10);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].question, "the newest question");
    }

    #[test]
    fn a_generous_budget_keeps_the_whole_thread() {
        let mut thread = Thread::new("th_1");
        for n in 0..5 {
            thread.turns.push(turn(&format!("question {n}")));
        }
        assert_eq!(thread.recent(10_000).len(), 5);
    }

    #[test]
    fn an_empty_thread_has_nothing_recent() {
        assert!(Thread::new("th_1").recent(100).is_empty());
    }

    #[test]
    fn a_thread_is_not_stored_until_the_reader_says_so() {
        assert!(!Thread::new("th_1").stored);
    }
}
