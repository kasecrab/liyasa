//! `ai` in `liyasa.json` (AST-20, AST-21, CFG-97).
//!
//! Keys arrive in camelCase from the config file; every one of them is
//! optional and every default is the PRD's. A key the schema allows but this
//! version does not read is kept in `extra` rather than rejected, because
//! `ai.assistant` is `additionalProperties: true` in
//! `schemas/liyasa.schema.json` and rejecting here would contradict the schema.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use liyasa_core::server::RateLimit;

/// Which model a feature asks for (§6.7). An operator routes each role
/// separately, so the assistant and the writing agent need not share a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Assistant,
    Agent,
    Embeddings,
    Rerank,
    Translate,
}

impl Role {
    pub const ALL: [Role; 5] = [
        Role::Assistant,
        Role::Agent,
        Role::Embeddings,
        Role::Rerank,
        Role::Translate,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Role::Assistant => "assistant",
            Role::Agent => "agent",
            Role::Embeddings => "embeddings",
            Role::Rerank => "rerank",
            Role::Translate => "translate",
        }
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A `provider:model` string, kept as two parts.
///
/// The model half may itself contain colons — `bedrock:anthropic.claude:1` is
/// one provider and one model — so the split is on the FIRST colon only.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModelRef {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ModelRefError {
    #[error("`{0}` is not `provider:model`")]
    NotTwoParts(String),
    #[error("`{0}` has an empty provider or model")]
    Empty(String),
}

impl std::str::FromStr for ModelRef {
    type Err = ModelRefError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let Some((provider, model)) = text.split_once(':') else {
            return Err(ModelRefError::NotTwoParts(text.to_owned()));
        };
        if provider.trim().is_empty() || model.trim().is_empty() {
            return Err(ModelRefError::Empty(text.to_owned()));
        }
        Ok(Self {
            provider: provider.trim().to_owned(),
            model: model.trim().to_owned(),
        })
    }
}

impl std::fmt::Display for ModelRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.provider, self.model)
    }
}

impl Serialize for ModelRef {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ModelRef {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Placement {
    Search,
    Panel,
    Page,
    Floating,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Availability {
    All,
    SignedIn,
    Groups,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BotProtection {
    None,
    Pow,
    Turnstile,
}

/// Where a reader is sent when the assistant cannot answer (AST-14).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Deflection {
    pub email: Option<String>,
    pub support_url: Option<String>,
    /// Hosts an out-of-scope question may be sent on to. A host that is not
    /// listed is never offered, so the fallback cannot become an open redirect.
    pub search_domains: Vec<String>,
}

impl Deflection {
    pub fn is_empty(&self) -> bool {
        self.email.is_none() && self.support_url.is_none() && self.search_domains.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AssistantConfig {
    pub enabled: bool,
    pub name: String,
    pub avatar: Option<String>,
    pub sample_questions: Vec<String>,
    pub placement: Placement,
    pub availability: Availability,
    /// Overrides `ai.models.assistant` for this site's assistant only.
    pub model: Option<ModelRef>,
    pub embedding_model: Option<ModelRef>,
    pub max_tokens: u32,
    pub temperature: f32,
    /// Operator text. It is the only reader-visible string that may reach the
    /// system prompt (§30.2.2).
    pub instructions: Option<String>,
    /// Paths to skill files, resolved against the project root by the caller.
    pub skills: Vec<String>,
    pub deflection: Deflection,
    pub bot_protection: BotProtection,
    pub rate_limits: Option<RateLimit>,
    /// Keys the schema allows and this version does not read.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl Default for AssistantConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            name: "Assistant".to_owned(),
            avatar: None,
            sample_questions: Vec::new(),
            placement: Placement::Search,
            availability: Availability::All,
            model: None,
            embedding_model: None,
            max_tokens: 4096,
            temperature: 0.0,
            instructions: None,
            skills: Vec::new(),
            deflection: Deflection::default(),
            bot_protection: BotProtection::None,
            rate_limits: None,
            extra: BTreeMap::new(),
        }
    }
}

/// `ai.reindex` (AST-05).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReindexConfig {
    pub auto_approve_cents: u32,
}

impl Default for ReindexConfig {
    fn default() -> Self {
        Self {
            auto_approve_cents: 500,
        }
    }
}

/// Non-secret per-provider settings. Keys never live here (§6.7).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProviderConfig {
    pub base_url: Option<String>,
    /// Bedrock only.
    pub region: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AiConfig {
    pub models: BTreeMap<Role, ModelRef>,
    pub providers: BTreeMap<String, ProviderConfig>,
    pub assistant: AssistantConfig,
    /// Operator text prepended to the assistant's and the agent's system
    /// prompts.
    pub instructions: Option<String>,
    pub reindex: ReindexConfig,
    pub include_personalized: IncludePersonalized,
    pub respect_noindex: RespectNoindex,
}

/// `ai.includePersonalized`: what the index stores for a page whose body
/// depends on the reader.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IncludePersonalized {
    /// Index the neutral rendering, with no reader in context.
    #[default]
    Neutral,
    Exclude,
}

/// `ai.respectNoindex`, default true — a newtype so `default` means the PRD's
/// value rather than `bool`'s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RespectNoindex(pub bool);

impl Default for RespectNoindex {
    fn default() -> Self {
        Self(true)
    }
}

impl AiConfig {
    /// Reads `ai` out of a parsed `liyasa.json`.
    ///
    /// The server holds the config as a `Value` rather than the typify model
    /// (`liyasa_config::SiteConfig`), because `ai.assistant` is
    /// `additionalProperties: true` and the generated type discards what it
    /// does not name. A site with no `ai` object takes every default.
    pub fn from_site(config: &serde_json::Value) -> Result<Self, serde_json::Error> {
        match config.get("ai") {
            Some(ai) => serde_json::from_value(ai.clone()),
            None => Ok(Self::default()),
        }
    }

    /// The model for a role: the assistant's own override first, then
    /// `ai.models`.
    pub fn model_for(&self, role: Role) -> Option<&ModelRef> {
        match role {
            Role::Assistant => self.assistant.model.as_ref(),
            Role::Embeddings => self.assistant.embedding_model.as_ref(),
            _ => None,
        }
        .or_else(|| self.models.get(&role))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_model_ref_splits_on_the_first_colon_only() {
        let r: ModelRef = "bedrock:anthropic.claude-sonnet-4:0"
            .parse()
            .expect("parses");
        assert_eq!(r.provider, "bedrock");
        assert_eq!(r.model, "anthropic.claude-sonnet-4:0");
        assert_eq!(r.to_string(), "bedrock:anthropic.claude-sonnet-4:0");
    }

    #[test]
    fn a_model_ref_without_a_provider_is_rejected() {
        assert!("gpt-4o".parse::<ModelRef>().is_err());
        assert!(":gpt-4o".parse::<ModelRef>().is_err());
        assert!("openai:".parse::<ModelRef>().is_err());
    }

    #[test]
    fn an_empty_ai_object_takes_every_prd_default() {
        let c: AiConfig = serde_json::from_str("{}").expect("empty object");
        assert_eq!(c.reindex.auto_approve_cents, 500);
        assert!(c.respect_noindex.0);
        assert_eq!(c.include_personalized, IncludePersonalized::Neutral);
        assert!(!c.assistant.enabled);
        assert_eq!(c.assistant.placement, Placement::Search);
        assert_eq!(c.assistant.availability, Availability::All);
    }

    #[test]
    fn the_prd_example_config_parses() {
        // PRD §33's example site, verbatim.
        let c: AiConfig = serde_json::from_value(serde_json::json!({
            "assistant": {
                "enabled": true,
                "name": "Acme Assistant",
                "sampleQuestions": ["How do I authenticate?"]
            }
        }))
        .expect("example");
        assert!(c.assistant.enabled);
        assert_eq!(c.assistant.name, "Acme Assistant");
        assert_eq!(c.assistant.sample_questions.len(), 1);
    }

    #[test]
    fn an_unread_assistant_key_is_kept_rather_than_rejected() {
        let c: AiConfig = serde_json::from_value(serde_json::json!({
            "assistant": { "enabled": true, "widgetTheme": "dark" }
        }))
        .expect("additionalProperties is true on ai.assistant");
        assert_eq!(
            c.assistant
                .extra
                .get("widgetTheme")
                .and_then(|v| v.as_str()),
            Some("dark")
        );
    }

    #[test]
    fn the_assistant_override_beats_ai_models() {
        let c: AiConfig = serde_json::from_value(serde_json::json!({
            "models": { "assistant": "openai:gpt-4o", "agent": "anthropic:claude-opus-4" },
            "assistant": { "model": "anthropic:claude-sonnet-4" }
        }))
        .expect("config");
        assert_eq!(
            c.model_for(Role::Assistant).map(ToString::to_string),
            Some("anthropic:claude-sonnet-4".to_owned())
        );
        assert_eq!(
            c.model_for(Role::Agent).map(ToString::to_string),
            Some("anthropic:claude-opus-4".to_owned())
        );
        assert_eq!(c.model_for(Role::Rerank), None);
    }

    #[test]
    fn availability_spells_signed_in_the_way_the_prd_does() {
        let a: Availability = serde_json::from_str("\"signed-in\"").expect("enum");
        assert_eq!(a, Availability::SignedIn);
    }
}
