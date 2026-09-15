//! Verification policy: what a failing check costs (VER-71).
//!
//! Three layers, narrowest last: the site's `verify.policy`, a page's `verify`
//! front matter, and a block's `verify=skip`. A layer that says nothing about
//! a class leaves the layer above it alone, so a page that promotes links to
//! errors does not silently reset the other five classes to their defaults.

use std::collections::BTreeMap;

use liyasa_core::diagnostics::{Code, Diagnostic, Severity, code};
use liyasa_core::document::FenceAttrs;
use liyasa_core::ids::Route;
use serde::{Deserialize, Serialize};

/// The six classes `verify.policy` names.
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
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum CheckClass {
    Code,
    Facts,
    Links,
    Screenshots,
    Prose,
    /// VER-71 spells this one with an underscore while every neighbouring key
    /// in `verify` is camelCase; both spellings are read (RFC 1301).
    #[serde(rename = "expired_attestations", alias = "expiredAttestations")]
    ExpiredAttestations,
}

impl CheckClass {
    pub const ALL: [Self; 6] = [
        Self::Code,
        Self::Facts,
        Self::Links,
        Self::Screenshots,
        Self::Prose,
        Self::ExpiredAttestations,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Code => "code",
            Self::Facts => "facts",
            Self::Links => "links",
            Self::Screenshots => "screenshots",
            Self::Prose => "prose",
            Self::ExpiredAttestations => "expired_attestations",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|class| {
            class.as_str() == key
                || (*class == Self::ExpiredAttestations && key == "expiredAttestations")
        })
    }

    /// VER-71's table.
    const fn default_level(self) -> PolicyLevel {
        match self {
            Self::Code | Self::Facts | Self::ExpiredAttestations => PolicyLevel::Error,
            Self::Links | Self::Screenshots | Self::Prose => PolicyLevel::Warn,
        }
    }
}

impl std::fmt::Display for CheckClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

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
#[non_exhaustive]
pub enum PolicyLevel {
    Error,
    #[serde(alias = "warning")]
    Warn,
    /// Not in VER-71's table: a page or a block needs a way to turn a class
    /// off without turning verification off wholesale (RFC 1301).
    #[serde(alias = "ignore")]
    Off,
}

impl PolicyLevel {
    /// The severity a finding in this class is reported at, or `None` when the
    /// class is off and the finding is not reported at all.
    pub const fn severity(self) -> Option<Severity> {
        match self {
            Self::Error => Some(Severity::Error),
            Self::Warn => Some(Severity::Warning),
            Self::Off => None,
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "error" => Some(Self::Error),
            "warn" | "warning" => Some(Self::Warn),
            "off" | "ignore" => Some(Self::Off),
            _ => None,
        }
    }
}

/// A policy layer. An absent class falls through to the layer above, and at
/// the bottom to [`CheckClass::default_level`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct Policy(BTreeMap<CheckClass, PolicyLevel>);

impl Policy {
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with(mut self, class: CheckClass, level: PolicyLevel) -> Self {
        self.0.insert(class, level);
        self
    }

    pub fn set(&mut self, class: CheckClass, level: PolicyLevel) {
        self.0.insert(class, level);
    }

    pub fn declared(&self, class: CheckClass) -> Option<PolicyLevel> {
        self.0.get(&class).copied()
    }

    pub fn level(&self, class: CheckClass) -> PolicyLevel {
        self.declared(class)
            .unwrap_or_else(|| class.default_level())
    }

    pub fn severity(&self, class: CheckClass) -> Option<Severity> {
        self.level(class).severity()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// `layer` wins for the classes it names; the rest of `self` is kept.
    #[must_use]
    pub fn overlay(&self, layer: &Self) -> Self {
        let mut out = self.clone();
        out.0.extend(layer.0.iter().map(|(k, v)| (*k, *v)));
        out
    }

    /// Reads `verify.policy` leniently: an unreadable class or level is a
    /// diagnostic and the rest of the object is still applied, because a typo
    /// in one key should not silently drop the other five.
    pub fn from_value(value: &serde_json::Value) -> (Self, Vec<Diagnostic>) {
        let mut policy = Self::new();
        let mut problems = Vec::new();
        let Some(object) = value.as_object() else {
            problems.push(bad_value(
                code::E0635,
                format!("`verify.policy` is {}, not an object", kind_of(value)),
            ));
            return (policy, problems);
        };
        for (key, raw) in object {
            let Some(class) = CheckClass::parse(key) else {
                problems.push(
                    bad_value(
                        code::E0634,
                        format!("`verify.policy.{key}` is not a check class"),
                    )
                    .help(format!("the classes are {}", class_list())),
                );
                continue;
            };
            let Some(level) = raw.as_str().and_then(PolicyLevel::parse) else {
                problems.push(
                    bad_value(
                        code::E0635,
                        format!("`verify.policy.{key}` is {}, not a level", kind_of(raw)),
                    )
                    .help("a level is `error`, `warn`, or `off`"),
                );
                continue;
            };
            policy.set(class, level);
        }
        (policy, problems)
    }
}

/// What a page's `verify` front matter says (VER-71).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum PageVerify {
    /// No `verify` key, or `verify: true`.
    #[default]
    Inherit,
    /// `verify: false`: nothing on this page is verified.
    Disabled,
    /// `verify: { links: "error" }`, or the same under a `policy` key.
    Overlay(Policy),
}

impl PageVerify {
    /// Reads `FrontmatterFields::verify`.
    pub fn from_value(value: Option<&serde_json::Value>) -> (Self, Vec<Diagnostic>) {
        match value {
            None => (Self::Inherit, Vec::new()),
            Some(serde_json::Value::Bool(true)) => (Self::Inherit, Vec::new()),
            Some(serde_json::Value::Bool(false)) => (Self::Disabled, Vec::new()),
            Some(serde_json::Value::Object(object)) => {
                // Both `verify: { policy: {…} }` and the class map written
                // directly are accepted; `policy` wins when both are present.
                let inner = object.get("policy").unwrap_or(value.unwrap_or(&NULL));
                let (policy, problems) = Policy::from_value(inner);
                (Self::Overlay(policy), problems)
            }
            Some(other) => (
                Self::Inherit,
                vec![bad_value(
                    code::E0635,
                    format!(
                        "page `verify` is {}, not an object or a boolean",
                        kind_of(other)
                    ),
                )],
            ),
        }
    }
}

static NULL: serde_json::Value = serde_json::Value::Null;

/// A site policy and the page overrides that sit on top of it.
#[derive(Debug, Clone, Default)]
pub struct PolicySet {
    site: Policy,
    pages: BTreeMap<Route, PageVerify>,
}

impl PolicySet {
    pub fn new(site: Policy) -> Self {
        Self {
            site,
            pages: BTreeMap::new(),
        }
    }

    pub fn set_page(&mut self, page: Route, verify: PageVerify) {
        self.pages.insert(page, verify);
    }

    pub fn site(&self) -> &Policy {
        &self.site
    }

    pub fn level(&self, page: &Route, class: CheckClass) -> PolicyLevel {
        match self.pages.get(page) {
            None | Some(PageVerify::Inherit) => self.site.level(class),
            Some(PageVerify::Disabled) => PolicyLevel::Off,
            Some(PageVerify::Overlay(layer)) => self.site.overlay(layer).level(class),
        }
    }

    pub fn severity(&self, page: &Route, class: CheckClass) -> Option<Severity> {
        self.level(page, class).severity()
    }
}

/// A block's `verify=skip reason="…"` (VER-71).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skip {
    pub reason: Option<String>,
}

impl Skip {
    /// What `CheckOutcome::Skip` records. A block that gives no reason still
    /// skips: VER-01 calls `verify=skip` a documented exclusion, so the
    /// missing reason is the author's omission, not a failure.
    pub fn reason(&self) -> String {
        self.reason
            .clone()
            .unwrap_or_else(|| "verify=skip, no reason given".to_owned())
    }
}

pub fn block_skip(attrs: &FenceAttrs) -> Option<Skip> {
    (attrs.kv.get("verify").map(String::as_str) == Some("skip")).then(|| Skip {
        reason: attrs.kv.get("reason").cloned().filter(|r| !r.is_empty()),
    })
}

fn bad_value(code: Code, message: String) -> Diagnostic {
    Diagnostic::new(code, message)
}

fn class_list() -> String {
    CheckClass::ALL
        .iter()
        .map(|c| format!("`{c}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn kind_of(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "a boolean",
        serde_json::Value::Number(_) => "a number",
        serde_json::Value::String(_) => "a string",
        serde_json::Value::Array(_) => "an array",
        serde_json::Value::Object(_) => "an object",
    }
}

#[cfg(test)]
mod tests;
