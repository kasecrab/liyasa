//! The output scrubber (PRD §30.2.4).
//!
//! Runner output, HTTP verification excerpts, agent tool results, analytics
//! free text, and log lines all pass through here before they reach a cache, a
//! snapshot, a report, or a log. A miss is a security bug, so the rules are
//! deliberately eager: a false redaction costs a reader one confusing excerpt,
//! a false pass leaks a credential.

use std::sync::LazyLock;

use regex::{Captures, Regex};

use liyasa_core::verify::SecretSource;

/// What replaces anything the scrubber matches.
pub const REDACTED: &str = "[redacted]";

/// The cap §30.2.4 puts on a stored excerpt.
pub const EXCERPT_LIMIT: usize = 512;

/// Shorter than this and a "secret" redacts ordinary prose: a two-character
/// value would blank every occurrence of those letters in every excerpt.
const MIN_SECRET_LEN: usize = 8;

const ELLIPSIS: &str = "…";

#[derive(Debug, Clone, Default)]
pub struct Scrubber {
    /// Longest first, so a secret that contains another is redacted whole.
    literals: Vec<String>,
}

impl Scrubber {
    /// Pattern rules only: for output from a check that was handed no secrets.
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_secrets<S: Into<String>>(values: impl IntoIterator<Item = S>) -> Self {
        let mut scrubber = Self::default();
        for value in values {
            scrubber.add_secret(&value.into());
        }
        scrubber
            .literals
            .sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        scrubber.literals.dedup();
        scrubber
    }

    /// The secrets a check declared in `CheckSpec::needs_secrets`, resolved
    /// through the store so the scrubber knows the values the runner saw.
    pub fn from_secret_source(source: &dyn SecretSource, names: &[String]) -> Self {
        let values = names
            .iter()
            .filter_map(|name| source.get(name).map(|v| v.as_str().to_owned()));
        Self::with_secrets(values)
    }

    fn add_secret(&mut self, value: &str) {
        if value.len() < MIN_SECRET_LEN {
            return;
        }
        self.literals.push(value.to_owned());
        for alphabet in [STANDARD, URL_SAFE] {
            for pad in [true, false] {
                let encoded = base64(value.as_bytes(), alphabet, pad);
                if encoded.len() >= MIN_SECRET_LEN {
                    self.literals.push(encoded);
                }
            }
        }
    }

    pub fn scrub(&self, text: &str) -> String {
        let mut out = text.to_owned();
        for secret in &self.literals {
            if out.contains(secret.as_str()) {
                out = out.replace(secret.as_str(), REDACTED);
            }
        }
        patterns().apply(&out)
    }

    /// Scrubbed and capped at [`EXCERPT_LIMIT`]. Scrubbing happens first: a
    /// secret cut in half is still most of a secret.
    pub fn excerpt(&self, text: &str) -> String {
        truncate(&self.scrub(text), EXCERPT_LIMIT)
    }

    /// The same, windowed around a byte offset — what VER-10 stores around a
    /// failing assertion. The offset is a hint: scrubbing may have moved it.
    pub fn excerpt_around(&self, text: &str, at: usize) -> String {
        let scrubbed = self.scrub(text);
        if scrubbed.len() <= EXCERPT_LIMIT {
            return scrubbed;
        }
        let at = at.min(scrubbed.len());
        let mut start = at.saturating_sub(EXCERPT_LIMIT / 2);
        while start < scrubbed.len() && !scrubbed.is_char_boundary(start) {
            start += 1;
        }
        let lead = if start > 0 { ELLIPSIS } else { "" };
        format!(
            "{lead}{}",
            truncate(&scrubbed[start..], EXCERPT_LIMIT - lead.len())
        )
    }
}

fn truncate(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_owned();
    }
    let mut end = limit.saturating_sub(ELLIPSIS.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{ELLIPSIS}", &text[..end])
}

// ---- pattern rules ----

/// One rule and how it rewrites what it matched.
enum Rule {
    /// The whole match is replaced.
    Whole(Regex),
    /// Capture 1 is kept, the rest of the match is replaced.
    KeepPrefix(Regex),
    /// The whole match is replaced only when its digits satisfy Luhn.
    Card(Regex),
}

struct Patterns(Vec<Rule>);

impl Patterns {
    fn apply(&self, text: &str) -> String {
        let mut out = std::borrow::Cow::Borrowed(text);
        for rule in &self.0 {
            let next = match rule {
                Rule::Whole(re) => re.replace_all(&out, REDACTED).into_owned(),
                Rule::KeepPrefix(re) => re
                    .replace_all(&out, |caps: &Captures<'_>| {
                        format!("{}{REDACTED}", &caps[1])
                    })
                    .into_owned(),
                Rule::Card(re) => re
                    .replace_all(&out, |caps: &Captures<'_>| {
                        let whole = &caps[0];
                        if luhn(whole) {
                            REDACTED.to_owned()
                        } else {
                            whole.to_owned()
                        }
                    })
                    .into_owned(),
            };
            out = std::borrow::Cow::Owned(next);
        }
        out.into_owned()
    }
}

fn patterns() -> &'static Patterns {
    static PATTERNS: LazyLock<Patterns> = LazyLock::new(build_patterns);
    &PATTERNS
}

fn build_patterns() -> Patterns {
    let whole = [
        // Private keys, whole PEM block.
        r"(?s)-----BEGIN [A-Z ]*PRIVATE KEY-----.*?-----END [A-Z ]*PRIVATE KEY-----",
        // Connection strings: the host goes with the credential, because a
        // host reached with a password is itself part of the secret.
        r"[a-zA-Z][a-zA-Z0-9+.-]*://[^\s/@:]+:[^\s/@]*@\S+",
        // JWTs.
        r"\beyJ[A-Za-z0-9_-]{6,}\.[A-Za-z0-9_-]{4,}\.[A-Za-z0-9_-]{4,}",
        // Vendor key shapes.
        r"\bgithub_pat_[A-Za-z0-9_]{20,}\b",
        r"\bgh[pousr]_[A-Za-z0-9]{20,}\b",
        r"\b(?:sk|pk|rk)_(?:live|test)_[A-Za-z0-9]{8,}\b",
        r"\bsk-[A-Za-z0-9_-]{16,}\b",
        r"\bxox[baprs]-[A-Za-z0-9-]{10,}\b",
        r"\bxapp-[0-9]-[A-Za-z0-9-]{10,}\b",
        r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b",
        r"\bAIza[0-9A-Za-z_-]{35,}\b",
        r"\bglpat-[0-9A-Za-z_-]{20,}\b",
        r"\bnpm_[A-Za-z0-9]{30,}\b",
        r"\bhf_[A-Za-z0-9]{30,}\b",
    ];
    let keep_prefix = [
        // An auth scheme is worth keeping: it says which credential failed.
        r"(?i)\b(bearer\s+|basic\s+)[A-Za-z0-9._~+/=-]{12,}",
        // `key: value` and `key=value` in any of the usual spellings.
        concat!(
            r"(?i)\b(",
            r"(?:api[_-]?key|access[_-]?key|private[_-]?key|client[_-]?secret",
            r"|auth[_-]?token|secret|token|password|passwd|pwd)",
            r"\s*[:=]\s*)",
            r#"("[^"]*"|'[^']*'|[^\s,;}]+)"#,
        ),
    ];
    // 13 to 19 digits, optionally grouped; the Luhn check keeps order numbers
    // and identifiers of the same shape out of the redaction.
    let card = r"\b(?:\d[ -]?){12,18}\d\b";
    let phone = [
        r"\+\d[\d .()-]{7,17}\d",
        r"\(\d{3}\)\s?\d{3}[ -]\d{4}\b",
        r"\b\d{3}-\d{3}-\d{4}\b",
    ];
    let email = r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b";

    let mut rules = Vec::new();
    for source in whole {
        rules.push(Rule::Whole(compile(source)));
    }
    for source in keep_prefix {
        rules.push(Rule::KeepPrefix(compile(source)));
    }
    rules.push(Rule::Card(compile(card)));
    for source in phone {
        rules.push(Rule::Whole(compile(source)));
    }
    rules.push(Rule::Whole(compile(email)));
    Patterns(rules)
}

/// Every pattern here is a literal in this file, so a compile failure is a bug
/// in this file and not something a user can reach.
fn compile(source: &str) -> Regex {
    Regex::new(source).unwrap_or_else(|error| unreachable!("scrubber pattern: {error}"))
}

fn luhn(text: &str) -> bool {
    let digits: Vec<u32> = text.chars().filter_map(|c| c.to_digit(10)).collect();
    if digits.len() < 13 || digits.len() > 19 {
        return false;
    }
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &d)| {
            if i % 2 == 0 {
                d
            } else if d > 4 {
                d * 2 - 9
            } else {
                d * 2
            }
        })
        .sum();
    sum.is_multiple_of(10)
}

// ---- base64, for the "same secret, different encoding" case ----

const STANDARD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL_SAFE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn base64(bytes: &[u8], alphabet: &[u8; 64], pad: bool) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let packed = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        let indices = [
            (packed >> 18) & 0x3f,
            (packed >> 12) & 0x3f,
            (packed >> 6) & 0x3f,
            packed & 0x3f,
        ];
        for (n, index) in indices.iter().enumerate() {
            if n <= chunk.len() {
                out.push(alphabet[*index as usize] as char);
            } else if pad {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests;
