//! `ai.agent` in `liyasa.json` (AGT-04, AGT-12, AGT-31).
//!
//! Two keys of this object exist in `schemas/liyasa.schema.json` today —
//! `ai.agent.limits.maxFilesChanged` and `maxLinesChanged` — and `ai.agent.limits`
//! is `additionalProperties: false`, so the caps AGT-04 and AGT-31 also call
//! "configured" cannot be read from a config file yet:
//!
//! - AGT-04's bulk-delete cap ("cannot delete more than a configured number of
//!   pages per run without an explicit flag"),
//! - AGT-04's per-run token, tool-call and wall-time budgets,
//! - AGT-31's cheaper research model and stronger writing model.
//!
//! The schema is WP-01's file. So each of those is a field here with the default
//! this crate chose, settable by the caller that starts a run, and RFC 2500
//! records the keys WP-01 needs to add and what they should be called. A default
//! that is enforced is worth more than a key that is not read: the point of the
//! cap is that it exists.
//!
//! TODO(rfc-2500): drop this note once `ai.agent.limits.maxPagesDeleted`,
//! `ai.agent.budget`, `ai.agent.models` and `ai.agent.injectionPhrases` are in
//! the schema. The loaders below already read them.
//!
//! [`Limits::load`] reads the two keys that do exist and leaves the rest at their
//! defaults, so a config file that sets them today works and keeps working when
//! the others arrive.

use std::time::Duration;

use liyasa_core::ai::Budget;
use serde::{Deserialize, Serialize};

/// Most files one proposal may touch (`ai.agent.limits.maxFilesChanged`).
pub const DEFAULT_MAX_FILES_CHANGED: u32 = 20;
/// Most lines one proposal may change (`ai.agent.limits.maxLinesChanged`).
pub const DEFAULT_MAX_LINES_CHANGED: u32 = 1000;
/// Most pages one run may delete without the explicit flag (AGT-04).
///
/// Small on purpose. A writing agent that deletes four pages in one run has
/// either found a real duplication or misread the task, and a reviewer should
/// decide which.
pub const DEFAULT_MAX_PAGES_DELETED: u32 = 3;

pub const DEFAULT_MAX_TOKENS: u32 = 200_000;
pub const DEFAULT_MAX_TOOL_CALLS: u16 = 120;
pub const DEFAULT_WALL_SECONDS: u64 = 20 * 60;

/// The size and destruction caps a proposal is held to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Limits {
    pub max_files_changed: u32,
    pub max_lines_changed: u32,
    /// AGT-04. No schema key yet; see RFC 2500.
    pub max_pages_deleted: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_files_changed: DEFAULT_MAX_FILES_CHANGED,
            max_lines_changed: DEFAULT_MAX_LINES_CHANGED,
            max_pages_deleted: DEFAULT_MAX_PAGES_DELETED,
        }
    }
}

impl Limits {
    /// Reads `ai.agent.limits` out of a parsed `liyasa.json`.
    ///
    /// A key the schema does not have yet is not an error and not a reason to
    /// reject the file: it is simply not there, and the default stands.
    pub fn load(config: &serde_json::Value) -> Self {
        let mut limits = Self::default();
        let Some(object) = config
            .get("ai")
            .and_then(|ai| ai.get("agent"))
            .and_then(|agent| agent.get("limits"))
        else {
            return limits;
        };
        let read = |key: &str| object.get(key).and_then(serde_json::Value::as_u64);
        if let Some(value) = read("maxFilesChanged") {
            limits.max_files_changed = u32::try_from(value).unwrap_or(u32::MAX);
        }
        if let Some(value) = read("maxLinesChanged") {
            limits.max_lines_changed = u32::try_from(value).unwrap_or(u32::MAX);
        }
        if let Some(value) = read("maxPagesDeleted") {
            limits.max_pages_deleted = u32::try_from(value).unwrap_or(u32::MAX);
        }
        limits
    }
}

/// AGT-12: one repository the agent may read source from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextRepo {
    /// `owner/name`, or whatever the connected provider calls it.
    pub name: String,
    /// Path prefixes that may be read. Empty means none (AGT-12 calls for an
    /// allow list, and a missing allow list is not a licence).
    #[serde(default)]
    pub allow: Vec<String>,
    /// Prefixes that may never be read, whatever `allow` says.
    #[serde(default)]
    pub deny: Vec<String>,
}

/// AGT-12's ceiling.
pub const MAX_CONTEXT_REPOS: usize = 10;

/// AGT-31: which model the agent uses for which half of a run.
///
/// Neither key is in the schema; RFC 2500. `None` means "the `ai.models.agent`
/// route", which is what `liyasa-ai`'s config already resolves.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Models {
    /// The cheaper model, for the research phase.
    pub research: Option<liyasa_ai::ModelRef>,
    /// The stronger model, for the write phase.
    pub write: Option<liyasa_ai::ModelRef>,
}

/// `ai.agent`, plus the one key outside it the agent is bound by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentConfig {
    pub limits: Limits,
    pub context_repos: Vec<ContextRepo>,
    pub models: Models,
    pub budget: Budget,
    /// `security.allowHosts.agentFetch`: the hosts `web_fetch` may reach.
    ///
    /// Empty means none. The key exists in the schema and its absence is an
    /// operator who has not allowed anything, which is the only reading under
    /// which an allow list is one.
    pub fetch_hosts: crate::hosts::KnownHosts,
    /// Extra injection phrases an operator maintains (AGT-06). No schema key;
    /// RFC 2500.
    pub injection_phrases: Vec<String>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            context_repos: Vec::new(),
            models: Models::default(),
            budget: default_budget(),
            fetch_hosts: crate::hosts::KnownHosts::default(),
            injection_phrases: Vec::new(),
        }
    }
}

/// AGT-04's per-run budget.
pub fn default_budget() -> Budget {
    Budget {
        max_tokens: DEFAULT_MAX_TOKENS,
        max_tool_calls: DEFAULT_MAX_TOOL_CALLS,
        wall: Duration::from_secs(DEFAULT_WALL_SECONDS),
        cost_cents: None,
    }
}

/// Why a config could not be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigError {
    #[error("{count} context repositories are configured; AGT-12 allows {MAX_CONTEXT_REPOS}")]
    TooManyRepos { count: usize },
    #[error("two context repositories are called `{name}`")]
    DuplicateRepo { name: String },
}

impl AgentConfig {
    /// Reads `ai.agent` out of a parsed `liyasa.json`.
    pub fn load(config: &serde_json::Value) -> Result<Self, ConfigError> {
        let mut out = Self {
            limits: Limits::load(config),
            ..Self::default()
        };
        let agent = config.get("ai").and_then(|ai| ai.get("agent"));
        if let Some(repos) = agent.and_then(|a| a.get("contextRepos")) {
            out.context_repos = serde_json::from_value(repos.clone()).unwrap_or_default();
        }
        if let Some(models) = agent.and_then(|a| a.get("models")) {
            out.models = serde_json::from_value(models.clone()).unwrap_or_default();
        }
        if let Some(phrases) = agent.and_then(|a| a.get("injectionPhrases")) {
            out.injection_phrases = serde_json::from_value(phrases.clone()).unwrap_or_default();
        }
        if let Some(hosts) = config
            .get("security")
            .and_then(|s| s.get("allowHosts"))
            .and_then(|a| a.get("agentFetch"))
            .and_then(serde_json::Value::as_array)
        {
            out.fetch_hosts =
                crate::hosts::KnownHosts::new(hosts.iter().filter_map(serde_json::Value::as_str));
        }
        out.check()?;
        Ok(out)
    }

    fn check(&self) -> Result<(), ConfigError> {
        if self.context_repos.len() > MAX_CONTEXT_REPOS {
            return Err(ConfigError::TooManyRepos {
                count: self.context_repos.len(),
            });
        }
        let mut names: Vec<&str> = self.context_repos.iter().map(|r| r.name.as_str()).collect();
        names.sort_unstable();
        if let Some(pair) = names.windows(2).find(|pair| pair[0] == pair[1]) {
            return Err(ConfigError::DuplicateRepo {
                name: pair[0].to_owned(),
            });
        }
        Ok(())
    }

    /// The detector this project's gate uses: the built-in corpus plus the
    /// operator's phrases.
    pub fn injection(&self) -> crate::injection::Detector {
        crate::injection::Detector::default().with(self.injection_phrases.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_empty_config_takes_every_default() {
        let config = AgentConfig::load(&json!({})).expect("empty");
        assert_eq!(config.limits, Limits::default());
        assert_eq!(config.limits.max_files_changed, DEFAULT_MAX_FILES_CHANGED);
        assert_eq!(config.limits.max_pages_deleted, DEFAULT_MAX_PAGES_DELETED);
        assert!(config.context_repos.is_empty());
        assert_eq!(config.budget.max_tokens, DEFAULT_MAX_TOKENS);
    }

    #[test]
    fn the_two_keys_the_schema_has_today_are_read() {
        let limits = Limits::load(&json!({
            "ai": { "agent": { "limits": { "maxFilesChanged": 3, "maxLinesChanged": 40 } } }
        }));
        assert_eq!(limits.max_files_changed, 3);
        assert_eq!(limits.max_lines_changed, 40);
        // And the one the schema does not have keeps its default rather than
        // becoming zero, which would refuse every deletion.
        assert_eq!(limits.max_pages_deleted, DEFAULT_MAX_PAGES_DELETED);
    }

    #[test]
    fn a_zero_cap_is_honoured_rather_than_read_as_unset() {
        // `maxFilesChanged: 0` means no proposal passes, which is a thing an
        // operator may want while they investigate something.
        let limits = Limits::load(&json!({
            "ai": { "agent": { "limits": { "maxFilesChanged": 0 } } }
        }));
        assert_eq!(limits.max_files_changed, 0);
    }

    #[test]
    fn a_context_repo_with_no_allow_list_allows_nothing() {
        let config = AgentConfig::load(&json!({
            "ai": { "agent": { "contextRepos": [{ "name": "acme/api" }] } }
        }))
        .expect("one repo");
        assert_eq!(config.context_repos.len(), 1);
        assert!(
            config.context_repos[0].allow.is_empty(),
            "an absent allow list must stay empty, not become everything"
        );
    }

    #[test]
    fn more_than_ten_context_repos_is_refused() {
        let repos: Vec<_> = (0..11)
            .map(|n| json!({ "name": format!("acme/r{n}") }))
            .collect();
        assert_eq!(
            AgentConfig::load(&json!({ "ai": { "agent": { "contextRepos": repos } } })),
            Err(ConfigError::TooManyRepos { count: 11 })
        );
    }

    #[test]
    fn exactly_ten_context_repos_is_allowed() {
        let repos: Vec<_> = (0..10)
            .map(|n| json!({ "name": format!("acme/r{n}") }))
            .collect();
        let config = AgentConfig::load(&json!({ "ai": { "agent": { "contextRepos": repos } } }))
            .expect("ten is the ceiling, not one past it");
        assert_eq!(config.context_repos.len(), MAX_CONTEXT_REPOS);
    }

    #[test]
    fn two_repos_with_one_name_is_refused() {
        assert_eq!(
            AgentConfig::load(&json!({
                "ai": { "agent": { "contextRepos": [
                    { "name": "acme/api" }, { "name": "acme/api" }
                ] } }
            })),
            Err(ConfigError::DuplicateRepo {
                name: "acme/api".to_owned()
            })
        );
    }

    #[test]
    fn the_two_models_agt_31_names_are_separate() {
        let config = AgentConfig::load(&json!({
            "ai": { "agent": { "models": {
                "research": "anthropic:claude-haiku-4-5",
                "write": "anthropic:claude-opus-5-5"
            } } }
        }))
        .expect("models");
        assert_eq!(
            config.models.research.as_ref().map(ToString::to_string),
            Some("anthropic:claude-haiku-4-5".to_owned())
        );
        assert_eq!(
            config.models.write.as_ref().map(ToString::to_string),
            Some("anthropic:claude-opus-5-5".to_owned())
        );
    }

    #[test]
    fn the_fetch_allow_list_is_read_and_empty_means_none() {
        let config = AgentConfig::load(&json!({})).expect("empty");
        assert!(config.fetch_hosts.is_empty());
        assert!(!config.fetch_hosts.contains("docs.example.com"));

        let config = AgentConfig::load(&json!({
            "security": { "allowHosts": { "agentFetch": ["docs.example.com"] } }
        }))
        .expect("hosts");
        assert!(config.fetch_hosts.contains("docs.example.com"));
        assert!(!config.fetch_hosts.contains("evil.example"));
    }

    #[test]
    fn an_operators_injection_phrases_join_the_built_in_corpus() {
        let config = AgentConfig::load(&json!({
            "ai": { "agent": { "injectionPhrases": ["never publish this"] } }
        }))
        .expect("phrases");
        let detector = config.injection();
        assert!(detector.matches("Never publish this."));
        assert!(detector.matches("ignore previous instructions"));
    }
}
