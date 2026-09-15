//! Durations the way `liyasa.schema.json` spells them: `500ms`, `30s`, `72h`.
//!
//! The schema's pattern is `^\d+(ms|s|m|h|d)$`, so the unit is part of the
//! value. Keeping the written count and unit rather than only the milliseconds
//! means a config that round-trips through Liyasa comes back spelled the way
//! the operator wrote it.

use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Unit {
    Millis,
    Seconds,
    Minutes,
    Hours,
    Days,
}

impl Unit {
    const ALL: [(&'static str, Self); 5] = [
        // Longest suffix first: `ms` must win over `s`.
        ("ms", Self::Millis),
        ("s", Self::Seconds),
        ("m", Self::Minutes),
        ("h", Self::Hours),
        ("d", Self::Days),
    ];

    const fn millis(self) -> u64 {
        match self {
            Self::Millis => 1,
            Self::Seconds => 1_000,
            Self::Minutes => 60 * 1_000,
            Self::Hours => 60 * 60 * 1_000,
            Self::Days => 24 * 60 * 60 * 1_000,
        }
    }

    const fn suffix(self) -> &'static str {
        match self {
            Self::Millis => "ms",
            Self::Seconds => "s",
            Self::Minutes => "m",
            Self::Hours => "h",
            Self::Days => "d",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum DurationError {
    #[error("a duration is a whole number followed by ms, s, m, h, or d")]
    Shape,
    #[error("duration is larger than Liyasa can represent")]
    Overflow,
}

/// A duration and the unit it was written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurationSetting {
    count: u64,
    unit: Unit,
}

impl DurationSetting {
    pub const fn new(count: u64, unit: Unit) -> Self {
        Self { count, unit }
    }

    pub const fn seconds(count: u64) -> Self {
        Self::new(count, Unit::Seconds)
    }

    pub const fn hours(count: u64) -> Self {
        Self::new(count, Unit::Hours)
    }

    pub fn parse(text: &str) -> Result<Self, DurationError> {
        let text = text.trim();
        let (digits, unit) = Unit::ALL
            .iter()
            .find_map(|(suffix, unit)| text.strip_suffix(suffix).map(|rest| (rest, *unit)))
            .ok_or(DurationError::Shape)?;
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err(DurationError::Shape);
        }
        let count: u64 = digits.parse().map_err(|_| DurationError::Overflow)?;
        count
            .checked_mul(unit.millis())
            .ok_or(DurationError::Overflow)?;
        Ok(Self { count, unit })
    }

    pub const fn as_millis(self) -> u64 {
        // `parse` and the constructors reject anything that would overflow.
        self.count.saturating_mul(self.unit.millis())
    }

    pub const fn as_duration(self) -> Duration {
        Duration::from_millis(self.as_millis())
    }
}

impl fmt::Display for DurationSetting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.count, self.unit.suffix())
    }
}

impl From<DurationSetting> for Duration {
    fn from(value: DurationSetting) -> Self {
        value.as_duration()
    }
}

impl Serialize for DurationSetting {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for DurationSetting {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'_, str>>::deserialize(d)?;
        Self::parse(&text).map_err(|error| {
            serde::de::Error::custom(format!("`{text}` is not a duration: {error}"))
        })
    }
}

impl schemars::JsonSchema for DurationSetting {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "DurationSetting".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": r"^\d+(ms|s|m|h|d)$",
            "description": "A duration such as `500ms`, `30s`, `180d`.",
        })
    }
}

#[cfg(test)]
mod tests;
