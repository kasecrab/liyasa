//! Liyasa's reading of the Vale rule format (VER-61).
//!
//! §6.2.1 is explicit that the `vale` crate on crates.io is an empty 2020
//! placeholder, so the format is implemented here for the seven rule types
//! that cover the bundled, Google, and Microsoft packages. A rule of any other
//! type parses into [`RuleKind::Unsupported`], which the caller hands to the
//! Vale binary in the companion runtime when there is one and reports as a
//! skip when there is not — never as a silent pass.

use std::collections::BTreeMap;

use liyasa_core::diagnostics::Severity;
use regex::{Regex, RegexBuilder};
use serde::Deserialize;

/// A rule package is operator-supplied content, so its patterns are compiled
/// under a ceiling (RFC 1300).
const SIZE_LIMIT: usize = 1 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Suggestion,
    #[default]
    Warning,
    Error,
}

impl Level {
    pub const fn severity(self) -> Severity {
        match self {
            Self::Suggestion => Severity::Hint,
            Self::Warning => Severity::Warning,
            Self::Error => Severity::Error,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RuleError {
    #[error("the rule file is not YAML: {0}")]
    Yaml(String),
    #[error("`extends` is missing")]
    NoKind,
    #[error("a `{kind}` rule needs {missing}")]
    Incomplete { kind: String, missing: String },
    #[error("pattern `{pattern}` does not compile: {error}")]
    Pattern { pattern: String, error: String },
    #[error("pattern `{pattern}` needs look-around, which Liyasa's regex engine does not have")]
    LookAround { pattern: String },
}

/// The fields every Vale rule shares, before `extends` decides the rest.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "lowercase")]
struct Common {
    extends: Option<String>,
    message: Option<String>,
    level: Level,
    link: Option<String>,
    scope: Option<serde_norway::Value>,
    ignorecase: bool,
    nonword: bool,
    tokens: Vec<serde_norway::Value>,
    raw: Vec<String>,
    exceptions: Vec<String>,
    swap: BTreeMap<String, String>,
    either: BTreeMap<String, String>,
    #[serde(rename = "match")]
    match_: Option<String>,
    style: Option<String>,
    threshold: Option<f32>,
    max: Option<usize>,
    min: Option<usize>,
    token: Option<String>,
    ignore: Vec<String>,
    dictionaries: Vec<String>,
    filters: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub name: String,
    pub level: Level,
    pub message: String,
    pub link: Option<String>,
    /// Vale's scope selectors. Empty means the default, `text`.
    pub scope: Vec<String>,
    pub kind: RuleKind,
    /// Matches that are never reported, whatever the rule says.
    pub exceptions: Option<Regex>,
}

#[derive(Debug, Clone)]
pub enum RuleKind {
    /// Any match is a finding.
    Existence { pattern: Regex },
    /// A match is a finding, and the replacement goes in the message.
    Substitution {
        pattern: Regex,
        /// Replacements in the same order as the pattern's alternatives.
        swap: Vec<(Regex, String)>,
    },
    /// More (or fewer) than this many matches in one scope.
    Occurrence {
        pattern: Regex,
        max: Option<usize>,
        min: Option<usize>,
    },
    Capitalization {
        style: CapStyle,
        exceptions: Vec<String>,
    },
    /// Delegates to the project dictionary (VER-62).
    Spelling {
        dictionaries: Vec<String>,
        ignore: Vec<String>,
        filters: Vec<Regex>,
    },
    /// Both spellings used in one document.
    Consistency {
        either: Vec<(Regex, Regex, String, String)>,
    },
    /// Consecutive tokens. Vale's part-of-speech `tag` is not read; a rule
    /// that uses one is `Unsupported` (RFC 1305).
    Sequence { patterns: Vec<Regex> },
    /// `extends` names a type Liyasa does not implement.
    Unsupported(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapStyle {
    Sentence,
    Title,
    Lower,
    Upper,
    Pattern(String),
}

impl Rule {
    pub fn parse(name: &str, yaml: &str) -> Result<Self, RuleError> {
        let common: Common =
            serde_norway::from_str(yaml).map_err(|e| RuleError::Yaml(e.to_string()))?;
        let extends = common.extends.clone().ok_or(RuleError::NoKind)?;
        let built = build(&extends, &common).and_then(|kind| {
            let exceptions = (!common.exceptions.is_empty())
                .then(|| compile(&alternation(&common.exceptions, false), true))
                .transpose()?;
            Ok((kind, exceptions))
        });
        // A pattern that needs look-around is the one failure that is not the
        // rule author's mistake, so the rule is delegated rather than dropped:
        // dropping it is the "ran and found nothing" that RFC 1305 forbids.
        let (kind, exceptions) = match built {
            Ok(pair) => pair,
            Err(RuleError::LookAround { .. }) => (
                RuleKind::Unsupported(format!("{extends} (look-around)")),
                None,
            ),
            Err(other) => return Err(other),
        };
        Ok(Self {
            name: name.to_owned(),
            level: common.level,
            message: common
                .message
                .clone()
                .unwrap_or_else(|| format!("`{name}` matched")),
            link: common.link.clone(),
            scope: scopes(common.scope.as_ref()),
            kind,
            exceptions,
        })
    }

    /// An existence rule over literal words, which is what Vale generates for
    /// a vocabulary's `reject.txt` and what [`super::package`] wants from one.
    ///
    /// The words are escaped, so `C++` searches for `C++`.
    pub fn from_words(
        name: &str,
        level: Level,
        message: &str,
        words: &[String],
    ) -> Result<Self, RuleError> {
        if words.is_empty() {
            return Err(RuleError::Incomplete {
                kind: "existence".to_owned(),
                missing: "`tokens` or `raw`".to_owned(),
            });
        }
        let body: Vec<String> = words.iter().map(|word| bounded(word)).collect();
        Ok(Self {
            name: name.to_owned(),
            level,
            message: message.to_owned(),
            link: None,
            scope: Vec::new(),
            kind: RuleKind::Existence {
                pattern: compile(&format!("(?:{})", body.join("|")), false)?,
            },
            exceptions: None,
        })
    }

    pub fn severity(&self) -> Severity {
        self.level.severity()
    }

    pub fn is_supported(&self) -> bool {
        !matches!(self.kind, RuleKind::Unsupported(_))
    }

    pub fn excepted(&self, text: &str) -> bool {
        self.exceptions
            .as_ref()
            .is_some_and(|pattern| pattern.is_match(text))
    }

    /// Vale fills `%s` in a rule's message with the match, then with the
    /// suggestion. A message with no `%s` is used as written.
    pub fn message_for(&self, found: &str, suggestion: Option<&str>) -> String {
        let mut out = String::with_capacity(self.message.len() + found.len());
        let mut rest = self.message.as_str();
        // Vale's order is (suggestion, found) for substitutions and (found)
        // for everything else.
        let mut fills = match suggestion {
            Some(suggestion) => vec![suggestion.to_owned(), found.to_owned()],
            None => vec![found.to_owned()],
        }
        .into_iter();
        while let Some(at) = rest.find("%s") {
            out.push_str(&rest[..at]);
            out.push_str(&fills.next().unwrap_or_else(|| found.to_owned()));
            rest = &rest[at + 2..];
        }
        out.push_str(rest);
        out
    }
}

fn build(extends: &str, common: &Common) -> Result<RuleKind, RuleError> {
    match extends {
        "existence" => Ok(RuleKind::Existence {
            pattern: word_pattern(common, &collect(common))?,
        }),
        "substitution" => {
            if common.swap.is_empty() {
                return Err(RuleError::Incomplete {
                    kind: extends.to_owned(),
                    missing: "`swap`".to_owned(),
                });
            }
            let froms: Vec<String> = common.swap.keys().cloned().collect();
            let swap = common
                .swap
                .iter()
                .map(|(from, to)| {
                    Ok((
                        word_pattern(common, std::slice::from_ref(from))?,
                        to.clone(),
                    ))
                })
                .collect::<Result<Vec<_>, RuleError>>()?;
            Ok(RuleKind::Substitution {
                pattern: word_pattern(common, &froms)?,
                swap,
            })
        }
        "occurrence" => {
            let token = common.token.clone().ok_or_else(|| RuleError::Incomplete {
                kind: extends.to_owned(),
                missing: "`token`".to_owned(),
            })?;
            if common.max.is_none() && common.min.is_none() {
                return Err(RuleError::Incomplete {
                    kind: extends.to_owned(),
                    missing: "`max` or `min`".to_owned(),
                });
            }
            Ok(RuleKind::Occurrence {
                pattern: compile(&token, common.ignorecase)?,
                max: common.max,
                min: common.min,
            })
        }
        "capitalization" => {
            let raw = common.match_.clone().ok_or_else(|| RuleError::Incomplete {
                kind: extends.to_owned(),
                missing: "`match`".to_owned(),
            })?;
            Ok(RuleKind::Capitalization {
                style: match raw.as_str() {
                    "$sentence" => CapStyle::Sentence,
                    "$title" => CapStyle::Title,
                    "$lower" => CapStyle::Lower,
                    "$upper" => CapStyle::Upper,
                    other => {
                        // Compiling now so a broken pattern is a parse error.
                        compile(other, common.ignorecase)?;
                        CapStyle::Pattern(other.to_owned())
                    }
                },
                exceptions: common.exceptions.clone(),
            })
        }
        "spelling" => Ok(RuleKind::Spelling {
            dictionaries: common.dictionaries.clone(),
            ignore: common.ignore.clone(),
            filters: common
                .filters
                .iter()
                .map(|f| compile(f, false))
                .collect::<Result<_, _>>()?,
        }),
        "consistency" => {
            if common.either.is_empty() {
                return Err(RuleError::Incomplete {
                    kind: extends.to_owned(),
                    missing: "`either`".to_owned(),
                });
            }
            Ok(RuleKind::Consistency {
                either: common
                    .either
                    .iter()
                    .map(|(a, b)| {
                        Ok((
                            word_pattern(common, std::slice::from_ref(a))?,
                            word_pattern(common, std::slice::from_ref(b))?,
                            a.clone(),
                            b.clone(),
                        ))
                    })
                    .collect::<Result<Vec<_>, RuleError>>()?,
            })
        }
        "sequence" => {
            let tokens = collect(common);
            if tokens.is_empty() {
                // A sequence rule with no plain patterns is one written
                // against part-of-speech tags.
                return Ok(RuleKind::Unsupported(extends.to_owned()));
            }
            Ok(RuleKind::Sequence {
                patterns: tokens
                    .iter()
                    .map(|t| compile(t, common.ignorecase))
                    .collect::<Result<_, _>>()?,
            })
        }
        // conditional, readability, metric, script, and anything a future Vale
        // adds.
        other => Ok(RuleKind::Unsupported(other.to_owned())),
    }
}

/// `tokens` and `raw` together: Vale treats `raw` as patterns spliced in
/// without word boundaries.
///
/// A `sequence` rule writes its tokens as maps (`- tag: MD`) when it is about
/// parts of speech. Those carry no pattern, so they are dropped here and the
/// rule ends up with none, which is what makes it `Unsupported`.
fn collect(common: &Common) -> Vec<String> {
    common
        .tokens
        .iter()
        .filter_map(|value| match value {
            serde_norway::Value::String(text) => Some(text.clone()),
            serde_norway::Value::Mapping(map) => map
                .get(serde_norway::Value::String("pattern".to_owned()))
                .and_then(|v| v.as_str())
                .map(str::to_owned),
            _ => None,
        })
        .chain(common.raw.iter().cloned())
        .collect()
}

fn word_pattern(common: &Common, tokens: &[String]) -> Result<Regex, RuleError> {
    if tokens.is_empty() {
        return Err(RuleError::Incomplete {
            kind: common.extends.clone().unwrap_or_default(),
            missing: "`tokens` or `raw`".to_owned(),
        });
    }
    compile(&alternation(tokens, !common.nonword), common.ignorecase)
}

fn alternation(tokens: &[String], word_bounded: bool) -> String {
    let body = tokens.join("|");
    if word_bounded {
        format!(r"\b(?:{body})\b")
    } else {
        format!("(?:{body})")
    }
}

/// The look-around constructs Vale 3 runs through a backtracking engine and
/// the `regex` crate does not have. Fourteen of the eighty-three rules Google
/// and Microsoft ship use one.
const LOOKAROUND: &[&str] = &["(?=", "(?!", "(?<=", "(?<!"];

fn needs_backtracking(pattern: &str) -> bool {
    LOOKAROUND.iter().any(|construct| {
        pattern
            .match_indices(construct)
            .any(|(at, _)| !is_escaped(pattern, at))
    })
}

fn is_escaped(pattern: &str, at: usize) -> bool {
    pattern[..at]
        .chars()
        .rev()
        .take_while(|c| *c == '\\')
        .count()
        % 2
        == 1
}

fn compile(pattern: &str, ignorecase: bool) -> Result<Regex, RuleError> {
    if needs_backtracking(pattern) {
        return Err(RuleError::LookAround {
            pattern: pattern.to_owned(),
        });
    }
    RegexBuilder::new(pattern)
        .case_insensitive(ignorecase)
        .size_limit(SIZE_LIMIT)
        .dfa_size_limit(SIZE_LIMIT)
        .build()
        .map_err(|error| RuleError::Pattern {
            pattern: pattern.to_owned(),
            error: error.to_string(),
        })
}

/// One literal word, escaped, with a word boundary only on the sides that can
/// carry one. `\bC\+\+\b` never matches, because `+` is not a word
/// character and the boundary after it needs one.
fn bounded(word: &str) -> String {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let left = word.chars().next().is_some_and(is_word);
    let right = word.chars().next_back().is_some_and(is_word);
    format!(
        "{}{}{}",
        if left { r"\b" } else { "" },
        regex::escape(word),
        if right { r"\b" } else { "" }
    )
}

/// `scope` is a string or a list of them.
fn scopes(value: Option<&serde_norway::Value>) -> Vec<String> {
    match value {
        Some(serde_norway::Value::String(one)) => vec![one.clone()],
        Some(serde_norway::Value::Sequence(many)) => many
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}
