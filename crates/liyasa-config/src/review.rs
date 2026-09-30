//! `content.reviewCadence`, in the shape its consumer needs (VER-77).
//!
//! The key is either one duration for the whole site or an object with a
//! default and per-directory overrides. Reading it is here rather than in
//! `liyasa-verify` so the two forms are unfolded once, by the package that owns
//! the schema; the durations stay strings, because parsing them is the
//! consumer's `DurationSetting` and this crate has no business owning a second
//! parser for the same syntax.

use std::collections::BTreeMap;

use serde_json::Value;

/// What `content.reviewCadence` says, with both forms flattened into one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReviewCadence {
    /// The site-wide cadence, as written. `None` leaves the default to the
    /// consumer, which is VER-77's 180 days.
    pub default: Option<String>,
    /// Per-directory cadences, keyed by the directory route as written. The
    /// consumer decides precedence; VER-77 says the longest matching prefix
    /// wins.
    pub overrides: BTreeMap<String, String>,
}

impl ReviewCadence {
    pub fn is_empty(&self) -> bool {
        self.default.is_none() && self.overrides.is_empty()
    }
}

/// Reads the key out of a whole `liyasa.json` value. A shape the schema would
/// reject reads as absent rather than panicking: validation reports it, and a
/// consumer that ran anyway should see nothing rather than half of something.
pub fn review_cadence(config: &Value) -> ReviewCadence {
    let Some(node) = config.pointer("/content/reviewCadence") else {
        return ReviewCadence::default();
    };
    match node {
        Value::String(cadence) => ReviewCadence {
            default: Some(cadence.clone()),
            overrides: BTreeMap::new(),
        },
        Value::Object(_) => ReviewCadence {
            default: node
                .get("default")
                .and_then(Value::as_str)
                .map(str::to_owned),
            overrides: node
                .get("overrides")
                .and_then(Value::as_object)
                .map(|entries| {
                    entries
                        .iter()
                        .filter_map(|(directory, cadence)| {
                            Some((directory.clone(), cadence.as_str()?.to_owned()))
                        })
                        .collect()
                })
                .unwrap_or_default(),
        },
        _ => ReviewCadence::default(),
    }
}

/// A directory route as the cadence compares it: `docs/api/`, `/docs/api` and
/// `docs/api` are one directory. The consumer normalizes the same way, so a
/// warning about an override that matches nothing is about the same set of
/// routes the consumer will later fail to match.
pub fn normalize(directory: &str) -> String {
    let trimmed = directory.trim_matches('/');
    match trimmed.is_empty() {
        true => String::from("/"),
        false => format!("/{trimmed}"),
    }
}
