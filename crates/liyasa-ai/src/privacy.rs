//! What is kept of a conversation, and for how long (AST-22).
//!
//! Scrubbing goes through `liyasa_verify::core::scrub`, the §30.2.4 output
//! scrubber every other surface uses, rather than a second set of patterns
//! here. A second scrubber is a second chance to miss something, and the one
//! that already exists covers exactly what AST-22 names: addresses, phone
//! numbers, and key shapes.

use liyasa_verify::core::scrub::Scrubber;
use serde::{Deserialize, Serialize};

use crate::assistant::Answer;

/// AST-22's default.
pub const DEFAULT_RETENTION_DAYS: u32 = 90;

const MS_PER_DAY: u64 = 24 * 60 * 60 * 1000;

/// `ai.assistant.privacy` (RFC 1806).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PrivacyConfig {
    /// Off means nothing reaches the server at all: the thread lives in the
    /// browser and the dashboard shows counts only.
    pub store: bool,
    pub retention_days: u32,
    /// Whether a SIGNED-IN reader's identity is stored beside the exchange.
    /// A reader who is not signed in is never identified, whatever this says.
    pub store_identity: bool,
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            store: true,
            retention_days: DEFAULT_RETENTION_DAYS,
            store_identity: false,
        }
    }
}

/// Who asked (AST-40).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Caller {
    Human,
    /// An agent through MCP (AST-32).
    Agent,
    /// A Slack or Discord integration.
    Bot,
}

impl Caller {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::Agent => "agent",
            Self::Bot => "bot",
        }
    }
}

/// How a reader rated an answer (AST-40).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rating {
    Helpful,
    Unhelpful,
}

/// One exchange as it is stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredExchange {
    pub thread: String,
    /// Milliseconds since the epoch.
    pub at: u64,
    pub caller: Caller,
    /// Scrubbed.
    pub question: String,
    /// Scrubbed.
    pub answer: String,
    pub confidence: f32,
    /// `route#anchor` for each citation.
    pub citations: Vec<String>,
    pub rating: Option<Rating>,
    /// Present only for a signed-in reader whose operator turned identity on.
    pub reader: Option<String>,
    /// The route the reader was on, for the gap list.
    pub route: Option<String>,
}

/// One exchange, before the privacy rules are applied.
#[derive(Debug, Clone, PartialEq)]
pub struct Exchange<'a> {
    pub thread: &'a str,
    pub at: u64,
    pub caller: Caller,
    pub question: &'a str,
    pub answer: &'a Answer,
    pub route: Option<&'a str>,
    /// The signed-in reader, when there is one.
    pub reader: Option<&'a str>,
}

/// The row to store, or `None` when the operator turned storage off.
///
/// Identity is dropped unless the reader is signed in AND the operator asked
/// for it. The two conditions are separate on purpose: `storeIdentity` is an
/// operator's choice about people who chose to identify themselves, and it can
/// never reach back and identify someone who did not.
pub fn store(exchange: &Exchange<'_>, config: &PrivacyConfig) -> Option<StoredExchange> {
    if !config.store {
        return None;
    }
    let scrubber = Scrubber::new();
    Some(StoredExchange {
        thread: exchange.thread.to_owned(),
        at: exchange.at,
        caller: exchange.caller,
        question: scrubber.scrub(exchange.question),
        answer: scrubber.scrub(&exchange.answer.text),
        confidence: exchange.answer.confidence,
        citations: exchange
            .answer
            .citations
            .iter()
            .map(|c| c.href.clone())
            .collect(),
        rating: None,
        reader: exchange
            .reader
            .filter(|_| config.store_identity)
            .map(str::to_owned),
        route: exchange.route.map(str::to_owned),
    })
}

/// Whether a stored row is past its retention window.
///
/// `retention_days == 0` means "keep nothing", which is how an operator who
/// wants counts without transcripts configures it.
pub fn expired(row: &StoredExchange, now_ms: u64, config: &PrivacyConfig) -> bool {
    let window = u64::from(config.retention_days).saturating_mul(MS_PER_DAY);
    now_ms.saturating_sub(row.at) >= window
}

/// The rows to delete at `now_ms`.
pub fn sweep<'a>(
    rows: &'a [StoredExchange],
    now_ms: u64,
    config: &PrivacyConfig,
) -> Vec<&'a StoredExchange> {
    rows.iter()
        .filter(|row| expired(row, now_ms, config))
        .collect()
}

#[cfg(test)]
mod tests {
    use liyasa_core::ai::TrustLevel;

    use super::*;
    use crate::assistant::Citation;

    fn answer(text: &str) -> Answer {
        Answer {
            text: text.to_owned(),
            citations: vec![Citation {
                href: "/guides/auth#bearer".to_owned(),
                title: "Bearer tokens".to_owned(),
                breadcrumb: Vec::new(),
            }],
            confidence: 0.8,
            follow_ups: Vec::new(),
            deflection: Vec::new(),
            trust: TrustLevel::Anonymous,
        }
    }

    fn exchange<'a>(question: &'a str, answer: &'a Answer) -> Exchange<'a> {
        Exchange {
            thread: "th_1",
            at: 1_000,
            caller: Caller::Human,
            question,
            answer,
            route: Some("/guides/auth"),
            reader: Some("user_42"),
        }
    }

    #[test]
    fn an_address_and_a_key_are_scrubbed_out_of_what_is_stored() {
        let a = answer("Write to support@example.com with sk-abcdefghijklmnop0123.");
        let row = store(
            &exchange("my key sk-abcdefghijklmnop0123 fails", &a),
            &PrivacyConfig::default(),
        )
        .expect("storage is on");
        assert!(
            !row.question.contains("sk-abcdefghijklmnop0123"),
            "{}",
            row.question
        );
        assert!(
            !row.answer.contains("support@example.com"),
            "{}",
            row.answer
        );
        assert!(
            !row.answer.contains("sk-abcdefghijklmnop0123"),
            "{}",
            row.answer
        );
    }

    #[test]
    fn a_reader_is_not_identified_unless_the_operator_asked() {
        let a = answer("x");
        let row = store(&exchange("q", &a), &PrivacyConfig::default()).expect("on");
        assert_eq!(row.reader, None);

        let config = PrivacyConfig {
            store_identity: true,
            ..PrivacyConfig::default()
        };
        let row = store(&exchange("q", &a), &config).expect("on");
        assert_eq!(row.reader.as_deref(), Some("user_42"));
    }

    #[test]
    fn an_anonymous_reader_is_never_identified_however_it_is_configured() {
        let a = answer("x");
        let mut e = exchange("q", &a);
        e.reader = None;
        let config = PrivacyConfig {
            store_identity: true,
            ..PrivacyConfig::default()
        };
        assert_eq!(store(&e, &config).expect("on").reader, None);
    }

    #[test]
    fn storage_can_be_turned_off_entirely() {
        let a = answer("x");
        let config = PrivacyConfig {
            store: false,
            ..PrivacyConfig::default()
        };
        assert_eq!(store(&exchange("q", &a), &config), None);
    }

    #[test]
    fn the_retention_default_is_ninety_days() {
        let config = PrivacyConfig::default();
        assert_eq!(config.retention_days, 90);
        let a = answer("x");
        let row = store(&exchange("q", &a), &config).expect("on");

        let eighty_nine = row.at + 89 * MS_PER_DAY;
        assert!(!expired(&row, eighty_nine, &config));
        let ninety = row.at + 90 * MS_PER_DAY;
        assert!(expired(&row, ninety, &config));
    }

    #[test]
    fn zero_days_keeps_nothing() {
        let a = answer("x");
        let config = PrivacyConfig {
            retention_days: 0,
            ..PrivacyConfig::default()
        };
        let row = store(&exchange("q", &a), &config).expect("on");
        assert!(expired(&row, row.at, &config));
    }
}
