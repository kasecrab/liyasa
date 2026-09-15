//! Diagnostics and the code registry (PRD §34.5, §34.9).
//!
//! Every user-facing failure in Liyasa is a [`Diagnostic`] carrying a [`Code`]
//! that exists in `codes.toml`; there is no path from user input to a panic.

use serde::{Deserialize, Serialize};

use crate::span::Span;

include!(concat!(env!("OUT_DIR"), "/codes.rs"));

/// Base for the generated help URL of a code with no explicit `url` row.
pub const HELP_URL_BASE: &str = "https://kasecrab.github.io/liyasa/docs/errors/";

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

/// A registered diagnostic code such as `E0210`.
///
/// The inner string is private and the only constructors consult the registry,
/// so an unregistered code cannot be built. Named constants live in
/// [`code`], generated from `codes.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct Code(&'static str);

/// One row of `codes.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodeInfo {
    pub code: Code,
    pub severity: Severity,
    /// The crate that owns the range this code falls in.
    pub krate: &'static str,
    pub title: &'static str,
    /// Set only when the row overrides the generated [`HELP_URL_BASE`] URL.
    pub url: Option<&'static str>,
}

/// One row of `[ranges]`: which crate may claim which numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub first: u16,
    pub last: u16,
    pub area: &'static str,
    pub krate: &'static str,
}

impl Code {
    /// Looks a code up by its text. Returns `None` when it is not registered,
    /// which is how deserialized and operator-supplied codes are validated.
    pub fn new(text: &str) -> Option<Self> {
        Self::info_of(text).map(|i| i.code)
    }

    pub const fn as_str(&self) -> &'static str {
        self.0
    }

    /// The numeric part, shared across the `E` and `W` prefixes.
    pub fn number(&self) -> u16 {
        self.0[1..].parse().unwrap_or_default()
    }

    pub fn info(&self) -> &'static CodeInfo {
        // A `Code` can only exist if it came from the registry.
        Self::info_of(self.0).unwrap_or_else(|| unreachable!("unregistered code {}", self.0))
    }

    pub fn severity(&self) -> Severity {
        self.info().severity
    }

    pub fn title(&self) -> &'static str {
        self.info().title
    }

    pub fn url(&self) -> String {
        match self.info().url {
            Some(url) => url.to_owned(),
            None => format!("{HELP_URL_BASE}{}", self.0),
        }
    }

    fn info_of(text: &str) -> Option<&'static CodeInfo> {
        REGISTRY
            .binary_search_by(|i| i.code.0.cmp(text))
            .ok()
            .and_then(|at| REGISTRY.get(at))
    }
}

impl std::fmt::Display for Code {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl<'de> Deserialize<'de> for Code {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'_, str>>::deserialize(d)?;
        Self::new(&text).ok_or_else(|| serde::de::Error::custom(format!("unknown code `{text}`")))
    }
}

impl schemars::JsonSchema for Code {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Code".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": "^[EW][0-9]{4}$",
            "description": "A code registered in liyasa-core's codes.toml.",
        })
    }
}

/// Every registered code, sorted by text.
pub fn registry() -> &'static [CodeInfo] {
    REGISTRY
}

/// Every claimable range and its owning crate.
pub fn ranges() -> &'static [Range] {
    RANGES
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Diagnostic {
    pub code: Code,
    pub severity: Severity,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<(Span, String)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    /// Generated from `code`; carried in the payload so consumers that do not
    /// link `liyasa-core` still have it.
    pub url: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related: Vec<Diagnostic>,
}

impl Diagnostic {
    /// A diagnostic at the code's default severity.
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self {
            code,
            severity: code.severity(),
            message: message.into(),
            span: None,
            labels: Vec::new(),
            help: None,
            url: code.url(),
            related: Vec::new(),
        }
    }

    #[must_use]
    pub fn at(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    #[must_use]
    pub fn label(mut self, span: Span, text: impl Into<String>) -> Self {
        self.labels.push((span, text.into()));
        self
    }

    #[must_use]
    pub fn help(mut self, text: impl Into<String>) -> Self {
        self.help = Some(text.into());
        self
    }

    #[must_use]
    pub fn related(mut self, other: Diagnostic) -> Self {
        self.related.push(other);
        self
    }

    /// Promotes or demotes within what the registry row allows: a row whose
    /// default is a warning may be raised to an error and back, but a code may
    /// never be rendered under the prefix it does not carry.
    #[must_use]
    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

/// A diagnostic list kept sorted by `(source, start)` (§34.9).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct Diagnostics(Vec<Diagnostic>);

impl Diagnostics {
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    pub fn push(&mut self, diagnostic: Diagnostic) {
        let at = self
            .0
            .partition_point(|d| Self::key(d) <= Self::key(&diagnostic));
        self.0.insert(at, diagnostic);
    }

    pub fn extend(&mut self, other: impl IntoIterator<Item = Diagnostic>) {
        for d in other {
            self.push(d);
        }
    }

    pub fn has_errors(&self) -> bool {
        self.0.iter().any(Diagnostic::is_error)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Diagnostic> {
        self.0.iter()
    }

    pub fn as_slice(&self) -> &[Diagnostic] {
        &self.0
    }

    pub fn into_vec(self) -> Vec<Diagnostic> {
        self.0
    }

    /// Spanless diagnostics sort before every located one, in insertion order.
    fn key(d: &Diagnostic) -> (u32, u32) {
        d.span
            .map_or((0, 0), |s| (s.source.0.saturating_add(1), s.start))
    }
}

impl FromIterator<Diagnostic> for Diagnostics {
    fn from_iter<I: IntoIterator<Item = Diagnostic>>(iter: I) -> Self {
        let mut out = Self::new();
        out.extend(iter);
        out
    }
}

impl IntoIterator for Diagnostics {
    type Item = Diagnostic;
    type IntoIter = std::vec::IntoIter<Diagnostic>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Diagnostics {
    type Item = &'a Diagnostic;
    type IntoIter = std::slice::Iter<'a, Diagnostic>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
