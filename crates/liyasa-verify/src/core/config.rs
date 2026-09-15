//! The complete `verify` object (VER-76).
//!
//! `schemas/liyasa.schema.json` types `verify` as an open object, so this is
//! where its keys get their meaning. Reading is lenient by key: a value
//! Liyasa cannot read is `E0635` on that key alone and the rest of the object
//! still applies, because one bad duration should not silently reset an
//! operator's whole verification setup to the defaults.

use std::collections::BTreeMap;

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::net::{HostPattern, HostSet};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::duration::DurationSetting;
use super::policy::Policy;

/// Which fenced blocks are verified (VER-01).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum VerifyDefault {
    /// Only blocks carrying `verify`.
    #[default]
    Tagged,
    /// Every block a runner claims; a block opts out with `verify=skip`.
    All,
}

/// Where code-executing runners run (VER-03).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum SandboxKind {
    #[default]
    Container,
    Remote,
    /// Rejected by `liyasa serve` with `E0620`.
    Local,
}

/// What `http` blocks run against (VER-10).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum HttpTarget {
    #[default]
    Mock,
    Staging,
    Both,
}

/// Drift's own four-level scale, which is not `diagnostics::Severity`
/// (RFC 1302).
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum DriftSeverity {
    Low,
    #[default]
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct RunnersConfig {
    pub sandbox: SandboxKind,
    /// A private image registry for offline mode (HOST-08).
    pub registry: Option<String>,
    /// Language to pinned image, `name@sha256:…`.
    pub images: BTreeMap<String, String>,
    pub custom: Vec<CustomRunner>,
}

/// A runner declared in config (VER-02.17). VER-02.17 names only "a command
/// template and image"; the rest of the field set is RFC 1302's.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct CustomRunner {
    pub id: String,
    pub languages: Vec<String>,
    pub image: Option<String>,
    pub command: Vec<String>,
    pub timeout: Option<DurationSetting>,
    pub network: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct HttpConfig {
    pub target: HttpTarget,
    pub staging: Option<StagingTarget>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct StagingTarget {
    pub base_url: String,
    /// `secret:<name>`; the value never leaves the secret store (§30.2.4).
    pub auth: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct LinksConfig {
    /// How often the scheduled sweep runs (VER-51).
    pub schedule: DurationSetting,
    /// How long a failing link may fail before it becomes drift.
    pub grace: DurationSetting,
    /// Hosts that block bots and are trusted without a check.
    pub allow_hosts: HostSet,
    pub deny_hosts: HostSet,
    /// Whether external links are checked at build time at all.
    pub external: bool,
}

impl Default for LinksConfig {
    fn default() -> Self {
        Self {
            schedule: DurationSetting::hours(6),
            grace: DurationSetting::hours(72),
            allow_hosts: HostSet::default(),
            deny_hosts: HostSet::default(),
            external: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct SourcesConfig {
    pub commands: CommandsConfig,
    pub trusted_branches: Vec<String>,
    pub refresh: DurationSetting,
}

impl Default for SourcesConfig {
    fn default() -> Self {
        Self {
            commands: CommandsConfig::default(),
            trusted_branches: vec!["main".to_owned()],
            refresh: DurationSetting::hours(24),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct CommandsConfig {
    pub allow: Vec<AllowedCommand>,
}

/// A `command` source the server will run, by path and content hash (VER-25).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct AllowedCommand {
    pub path: String,
    pub sha256: String,
}

impl AllowedCommand {
    pub fn hash_is_well_formed(&self) -> bool {
        self.sha256.len() == 64 && self.sha256.bytes().all(|b| b.is_ascii_hexdigit())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct BudgetConfig {
    /// Wall clock for a deploy build; `W0622` queues the remainder (VER-72).
    pub deploy: DurationSetting,
    /// Wall clock for the full run. The schema calls this key `total`; both
    /// spellings are read (RFC 1302).
    #[serde(alias = "total")]
    pub full: DurationSetting,
    pub per_check: Option<DurationSetting>,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            deploy: DurationSetting::seconds(60),
            full: DurationSetting::hours(2),
            per_check: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct BadgesConfig {
    pub enabled: bool,
    /// Whether readers who are not operators see "Needs review" (VER-74).
    pub public: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct ScreenshotsConfig {
    pub tolerance: f32,
}

impl Default for ScreenshotsConfig {
    fn default() -> Self {
        Self { tolerance: 0.02 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct DriftConfig {
    /// Above this many affected pages a drift is split into proposals
    /// (VER-73).
    pub batch_size: u32,
    pub severity_threshold: DriftSeverity,
    pub auto_resolve: bool,
}

impl Default for DriftConfig {
    fn default() -> Self {
        Self {
            batch_size: 25,
            severity_threshold: DriftSeverity::Medium,
            auto_resolve: false,
        }
    }
}

/// The whole `verify` object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct VerifyConfig {
    pub enabled: bool,
    pub default: VerifyDefault,
    /// The doctest-style prefix hidden lines carry (VER-04). VER-04 spells the
    /// key `hide_prefix` and VER-76 `hidePrefix`; both are read (RFC 1302).
    #[serde(alias = "hide_prefix")]
    pub hide_prefix: String,
    pub policy: Policy,
    pub runners: RunnersConfig,
    pub http: HttpConfig,
    pub links: LinksConfig,
    pub sources: SourcesConfig,
    pub budget: BudgetConfig,
    pub block_deploy: bool,
    /// A cron expression; the scheduler parses it, not this crate.
    pub schedule: String,
    pub badges: BadgesConfig,
    pub screenshots: ScreenshotsConfig,
    pub drift: DriftConfig,
}

impl Default for VerifyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default: VerifyDefault::default(),
            hide_prefix: "# ".to_owned(),
            policy: Policy::new(),
            runners: RunnersConfig::default(),
            http: HttpConfig::default(),
            links: LinksConfig::default(),
            sources: SourcesConfig::default(),
            budget: BudgetConfig::default(),
            block_deploy: false,
            schedule: "0 2 * * *".to_owned(),
            badges: BadgesConfig::default(),
            screenshots: ScreenshotsConfig::default(),
            drift: DriftConfig::default(),
        }
    }
}

impl VerifyConfig {
    /// Reads the `verify` object one key at a time, so a value Liyasa cannot
    /// read costs only that key.
    pub fn from_value(value: &Value) -> (Self, Vec<Diagnostic>) {
        let mut out = Self::default();
        let mut problems = Vec::new();
        let Some(object) = value.as_object() else {
            if !value.is_null() {
                problems.push(bad("verify", "an object", value));
            }
            return (out, problems);
        };

        read(object, &["enabled"], &mut out.enabled, &mut problems);
        read(object, &["default"], &mut out.default, &mut problems);
        read(
            object,
            &["hidePrefix", "hide_prefix"],
            &mut out.hide_prefix,
            &mut problems,
        );
        read(object, &["runners"], &mut out.runners, &mut problems);
        read(object, &["http"], &mut out.http, &mut problems);
        read(object, &["sources"], &mut out.sources, &mut problems);
        read(object, &["budget"], &mut out.budget, &mut problems);
        read(
            object,
            &["blockDeploy"],
            &mut out.block_deploy,
            &mut problems,
        );
        read(object, &["schedule"], &mut out.schedule, &mut problems);
        read(object, &["badges"], &mut out.badges, &mut problems);
        read(
            object,
            &["screenshots"],
            &mut out.screenshots,
            &mut problems,
        );
        read(object, &["drift"], &mut out.drift, &mut problems);

        if let Some(raw) = object.get("policy") {
            let (policy, mut found) = Policy::from_value(raw);
            out.policy = policy;
            problems.append(&mut found);
        }
        if let Some(raw) = object.get("links") {
            let (links, mut found) = read_links(raw);
            out.links = links;
            problems.append(&mut found);
        }

        problems.extend(out.lint());
        (out, problems)
    }

    /// Values that are well-formed JSON and still unusable.
    fn lint(&self) -> Vec<Diagnostic> {
        let mut problems = Vec::new();
        if !(0.0..=1.0).contains(&self.screenshots.tolerance) {
            problems.push(Diagnostic::new(
                code::E0635,
                format!(
                    "`verify.screenshots.tolerance` is {}, and a tolerance is a fraction between 0 and 1",
                    self.screenshots.tolerance
                ),
            ));
        }
        let fields = self.schedule.split_whitespace().count();
        if !(5..=6).contains(&fields) {
            problems.push(
                Diagnostic::new(
                    code::E0635,
                    format!(
                        "`verify.schedule` has {fields} fields, not a cron expression's five or six"
                    ),
                )
                .help("the default is `0 2 * * *`, which is nightly at 02:00"),
            );
        }
        for command in &self.sources.commands.allow {
            if !command.hash_is_well_formed() {
                problems.push(Diagnostic::new(
                    code::E0635,
                    format!(
                        "`verify.sources.commands.allow` entry for `{}` has no sha-256 digest",
                        command.path
                    ),
                ));
            }
        }
        problems
    }
}

/// `allowHosts` and `denyHosts` are written as plain strings in config while
/// `HostSet` is a tagged enum on the wire; both are read (RFC 1302).
fn read_links(value: &Value) -> (LinksConfig, Vec<Diagnostic>) {
    let mut out = LinksConfig::default();
    let mut problems = Vec::new();
    let Some(object) = value.as_object() else {
        problems.push(bad("verify.links", "an object", value));
        return (out, problems);
    };
    read(object, &["schedule"], &mut out.schedule, &mut problems);
    read(object, &["grace"], &mut out.grace, &mut problems);
    read(object, &["external"], &mut out.external, &mut problems);
    for (key, target) in [
        ("allowHosts", &mut out.allow_hosts),
        ("denyHosts", &mut out.deny_hosts),
    ] {
        if let Some(raw) = object.get(key) {
            match host_set(raw) {
                Ok(set) => *target = set,
                Err(()) => {
                    problems.push(bad(&format!("verify.links.{key}"), "a list of hosts", raw));
                }
            }
        }
    }
    (out, problems)
}

fn host_set(value: &Value) -> Result<HostSet, ()> {
    let items = value.as_array().ok_or(())?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        if let Some(text) = item.as_str() {
            out.push(host_pattern(text));
        } else if let Ok(pattern) = serde_json::from_value::<HostPattern>(item.clone()) {
            out.push(pattern);
        } else {
            return Err(());
        }
    }
    Ok(HostSet(out))
}

fn host_pattern(text: &str) -> HostPattern {
    match text.trim() {
        "*" => HostPattern::Any,
        rest => match rest.strip_prefix("*.").or_else(|| rest.strip_prefix('.')) {
            Some(suffix) => HostPattern::Suffix(suffix.to_owned()),
            None => HostPattern::Exact(rest.to_owned()),
        },
    }
}

fn read<T: DeserializeOwned>(
    object: &Map<String, Value>,
    keys: &[&str],
    out: &mut T,
    problems: &mut Vec<Diagnostic>,
) {
    let Some((key, raw)) = keys.iter().find_map(|k| object.get(*k).map(|v| (*k, v))) else {
        return;
    };
    match serde_json::from_value::<T>(raw.clone()) {
        Ok(value) => *out = value,
        Err(error) => problems.push(
            Diagnostic::new(
                code::E0635,
                format!("`verify.{key}` is not a value Liyasa can read: {error}"),
            )
            .help("the key keeps its default"),
        ),
    }
}

fn bad(path: &str, wanted: &str, found: &Value) -> Diagnostic {
    Diagnostic::new(
        code::E0635,
        format!("`{path}` is {}, not {wanted}", kind_of(found)),
    )
}

fn kind_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

#[cfg(test)]
mod tests;
