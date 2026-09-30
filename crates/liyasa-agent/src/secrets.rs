//! The secret patterns, used twice (AGT-05, AGT-06).
//!
//! AGT-06 rejects a diff containing a string that matches one. AGT-05 stores
//! every tool call and model exchange "with secrets redacted". Those are the same
//! corpus, and they are here together on purpose: a pattern added for the gate
//! and not for the scrubber would reject a diff carrying a key and then write the
//! key into the run record.
//!
//! A reason shown to a reviewer never carries the match. [`Match::excerpt`] is
//! the line with the secret already replaced, because the proposal header AGT-06
//! puts a flag in is read by more people than the diff is, and a rejection that
//! quotes the key it found has published it.
//!
//! Placeholders are excluded, and that exclusion is the risky half of this file:
//! every marker in [`PLACEHOLDERS`] is a case the scan stops catching. They are
//! substrings a real credential does not contain and a documentation example
//! almost always does, and a test asserts that removing the markers from a real
//! key brings it back.

use std::sync::OnceLock;

use regex::Regex;

/// Substrings that mark a value as an example rather than a credential.
///
/// Compared case-insensitively against the matched text only, never against the
/// surrounding prose: a page *about* API keys says "example" in every paragraph,
/// and excluding on context would turn that page into a blind spot.
pub const PLACEHOLDERS: &[&str] = &[
    "your",
    "example",
    "changeme",
    "redacted",
    "xxxx",
    "placeholder",
    "<",
    "...",
    "abcdef0123",
    "1234567890",
];

/// One named pattern.
pub struct Pattern {
    pub name: &'static str,
    /// What the reviewer is told a match is.
    pub description: &'static str,
    pub regex: Regex,
}

/// Where a secret was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub pattern: &'static str,
    pub description: &'static str,
    /// 1-based.
    pub line: u32,
    /// The line, with the secret replaced. Never carries the match.
    pub excerpt: String,
}

/// What replaces a secret.
pub const REDACTION: &str = "[redacted]";

fn compile(name: &'static str, description: &'static str, pattern: &str) -> Pattern {
    Pattern {
        name,
        description,
        regex: Regex::new(pattern).unwrap_or_else(|e| panic!("`{name}` does not compile: {e}")),
    }
}

/// The corpus. Every entry is a shape that is a credential and not prose.
pub fn patterns() -> &'static [Pattern] {
    static COMPILED: OnceLock<Vec<Pattern>> = OnceLock::new();
    COMPILED.get_or_init(|| {
        vec![
            compile(
                "pem-private-key",
                "a PEM private key block",
                r"-----BEGIN (?:[A-Z]+ )*PRIVATE KEY-----",
            ),
            compile(
                "aws-access-key-id",
                "an AWS access key id",
                r"\b(?:AKIA|ASIA|ABIA|ACCA)[0-9A-Z]{16}\b",
            ),
            compile(
                "anthropic-api-key",
                "an Anthropic API key",
                r"\bsk-ant-[A-Za-z0-9_-]{20,}",
            ),
            compile(
                "openai-api-key",
                "an OpenAI API key",
                r"\bsk-(?:proj-)?[A-Za-z0-9_-]{20,}",
            ),
            compile(
                "github-token",
                "a GitHub token",
                r"\bgh[pousr]_[A-Za-z0-9]{36,}",
            ),
            compile(
                "google-api-key",
                "a Google API key",
                r"\bAIza[0-9A-Za-z_-]{35}\b",
            ),
            compile(
                "slack-token",
                "a Slack token",
                r"\bxox[abprs]-[0-9A-Za-z-]{10,}",
            ),
            compile(
                "stripe-key",
                "a Stripe key",
                r"\b[sr]k_(?:live|test)_[0-9A-Za-z]{16,}",
            ),
            compile(
                "jwt",
                "a signed JSON Web Token",
                r"\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}",
            ),
            // The catch-all. Narrow enough that prose does not trip it: a key
            // word, an assignment, and twenty or more characters of credential
            // alphabet with no space in them.
            compile(
                "assigned-credential",
                "a value assigned to a secret-looking name",
                r#"(?i)\b(?:password|passwd|secret|api[_-]?key|access[_-]?token|auth[_-]?token|client[_-]?secret|private[_-]?key)\b["']?\s*[:=]\s*["']?([A-Za-z0-9/+=_.-]{20,})"#,
            ),
        ]
    })
}

/// Whether a matched string is an example rather than a credential.
pub fn is_placeholder(matched: &str) -> bool {
    let lower = matched.to_ascii_lowercase();
    PLACEHOLDERS.iter().any(|marker| lower.contains(marker))
}

/// The byte ranges of every secret in `text`, merged and in order.
fn spans(text: &str) -> Vec<(usize, usize, &'static Pattern)> {
    let mut found: Vec<(usize, usize, &'static Pattern)> = Vec::new();
    for pattern in patterns() {
        for m in pattern.regex.find_iter(text) {
            if is_placeholder(m.as_str()) {
                continue;
            }
            found.push((m.start(), m.end(), pattern));
        }
    }
    found.sort_by_key(|(start, end, _)| (*start, std::cmp::Reverse(*end)));
    let mut merged: Vec<(usize, usize, &'static Pattern)> = Vec::new();
    for span in found {
        match merged.last_mut() {
            Some(last) if span.0 < last.1 => last.1 = last.1.max(span.1),
            _ => merged.push(span),
        }
    }
    merged
}

/// Every secret in `text`, each reported with a redacted excerpt of its line.
pub fn scan(text: &str) -> Vec<Match> {
    spans(text)
        .into_iter()
        .map(|(start, end, pattern)| {
            let line = text[..start].matches('\n').count() as u32 + 1;
            let line_start = text[..start].rfind('\n').map_or(0, |at| at + 1);
            let line_end = text[end..].find('\n').map_or(text.len(), |at| end + at);
            Match {
                pattern: pattern.name,
                description: pattern.description,
                line,
                excerpt: format!(
                    "{}{REDACTION}{}",
                    &text[line_start..start],
                    &text[end..line_end]
                ),
            }
        })
        .collect()
}

/// `text` with every secret replaced. AGT-05's scrubber.
pub fn redact(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0usize;
    for (start, end, _) in spans(text) {
        out.push_str(&text[at..start]);
        out.push_str(REDACTION);
        at = end;
    }
    out.push_str(&text[at..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One synthetic credential per pattern. Not real keys: each is built to the
    /// published shape with filler that carries no placeholder marker.
    const CORPUS: &[(&str, &str)] = &[
        (
            "pem-private-key",
            "-----BEGIN RSA PRIVATE KEY-----\nMIIB\n-----END RSA PRIVATE KEY-----",
        ),
        ("aws-access-key-id", "AKIAQWERTYUIOPASDFGH"),
        (
            "anthropic-api-key",
            "sk-ant-api03-QWERtyuiOPasdfGHjklZXCVbnmQWERtyui",
        ),
        ("openai-api-key", "sk-QWERtyuiOPasdfGHjklZXCVbnm"),
        ("github-token", "ghp_QWERtyuiOPasdfGHjklZXCVbnmQWERtyuiOP"),
        ("google-api-key", "AIzaQWERtyuiOPasdfGHjklZXCVbnmQWERtyuiO"),
        ("slack-token", "xoxb-QWERtyuiOP-asdfGHjklZXC"),
        ("stripe-key", "sk_live_QWERtyuiOPasdfGHjkl"),
        (
            "jwt",
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJhZ2VudCJ9.QWERtyuiOPasdfGHjkl",
        ),
        (
            "assigned-credential",
            "client_secret = QWERtyuiOPasdfGHjklZXCVbnm",
        ),
    ];

    #[test]
    fn every_pattern_catches_the_shape_it_is_for() {
        for (name, sample) in CORPUS {
            let found = scan(sample);
            assert!(
                found.iter().any(|m| m.pattern == *name),
                "`{name}` did not match its own sample; got {:?}",
                found.iter().map(|m| m.pattern).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn a_reason_never_carries_the_secret_it_found() {
        // The excerpt goes in a proposal header, which is read by more people
        // than the diff is.
        for (_, sample) in CORPUS {
            let line = sample.lines().next().expect("a line");
            for m in scan(sample) {
                assert!(
                    !m.excerpt.contains(line) || line.len() < 8,
                    "the excerpt quoted the match: {}",
                    m.excerpt
                );
                assert!(m.excerpt.contains(REDACTION), "{}", m.excerpt);
            }
        }
    }

    #[test]
    fn ordinary_documentation_prose_does_not_trip_the_scan() {
        const PROSE: &str = "\
Set your API key in the environment before you start. The password policy is \
described in the security guide, and the access token is issued by the login \
endpoint. Never commit a private key. See `AKIA` prefixes in the AWS docs.
The header is `Authorization: Bearer <token>`.
";
        assert_eq!(scan(PROSE), Vec::new(), "a false positive in plain prose");
    }

    #[test]
    fn a_documented_placeholder_is_not_a_secret() {
        for sample in [
            "api_key = YOUR_API_KEY_GOES_HERE_HERE",
            "sk-ant-api03-example-key-value-here-xx",
            "password: <your-password-here-goes>",
            "secret = xxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        ] {
            assert_eq!(scan(sample), Vec::new(), "`{sample}` was called a secret");
        }
    }

    #[test]
    fn taking_the_placeholder_marker_out_brings_the_match_back() {
        // The exclusion above is the only thing that can silently narrow this
        // scan, so it is checked from the other side as well: the same shape
        // without a marker must still match.
        assert_eq!(scan("api_key = YOUR_API_KEY_GOES_HERE_HERE"), Vec::new());
        assert_eq!(
            scan("api_key = QWERtyuiOPasdfGHjklZXCVbnm")
                .into_iter()
                .map(|m| m.pattern)
                .collect::<Vec<_>>(),
            vec!["assigned-credential"]
        );
    }

    #[test]
    fn the_scrubber_and_the_gate_read_the_same_corpus() {
        // AGT-05 and AGT-06 must not disagree: whatever the gate rejects, the
        // run record must not store.
        for (_, sample) in CORPUS {
            let scrubbed = redact(sample);
            assert!(scrubbed.contains(REDACTION), "{sample} was not scrubbed");
            assert_eq!(
                scan(&scrubbed),
                Vec::new(),
                "a scrubbed value still matches: {scrubbed}"
            );
        }
    }

    #[test]
    fn the_scrubber_keeps_the_text_around_a_secret() {
        let scrubbed = redact("before AKIAQWERTYUIOPASDFGH after");
        assert_eq!(scrubbed, format!("before {REDACTION} after"));
    }

    #[test]
    fn two_patterns_matching_one_string_redact_it_once() {
        // `sk_live_...` matches the Stripe pattern, and an assignment around it
        // matches the catch-all. Overlapping matches merge, or the scrubber
        // splices its own replacement into the middle of a span.
        let scrubbed = redact("client_secret = sk_live_QWERtyuiOPasdfGHjkl");
        assert_eq!(scrubbed.matches(REDACTION).count(), 1, "{scrubbed}");
        assert_eq!(scan(&scrubbed), Vec::new(), "{scrubbed}");
    }

    #[test]
    fn a_line_number_is_one_based_and_counts_from_the_text() {
        let found = scan("one\ntwo\nAKIAQWERTYUIOPASDFGH\n");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 3);
    }

    #[test]
    fn every_pattern_has_a_distinct_name() {
        let mut names: Vec<&str> = patterns().iter().map(|p| p.name).collect();
        let before = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), before, "two patterns share a name");
    }

    #[test]
    fn the_corpus_covers_every_pattern() {
        // A pattern with no sample is a pattern nothing proves fires.
        for pattern in patterns() {
            assert!(
                CORPUS.iter().any(|(name, _)| *name == pattern.name),
                "`{}` has no sample in the corpus",
                pattern.name
            );
        }
    }
}
