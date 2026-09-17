//! Provider adapters (§6.7, AST-21).
//!
//! Nothing above this module names a vendor. A feature declares a [`Role`] and
//! an operator routes the role to `provider:model`; [`chat_model`] and
//! [`embedding_model`] turn that string into an implementation of the frozen
//! contract.
//!
//! Every adapter goes out through [`HttpClient`] under
//! `Purpose::ModelProvider`, so the address policy of §30.2.3 applies to a
//! model provider exactly as it applies to a spec fetch. That has one
//! consequence an operator meets immediately: a local Ollama or vLLM endpoint
//! is on a private address, and a private address is denied unless
//! `network.allowPrivate` says otherwise.

pub mod anthropic;
pub mod google;
pub mod openai;
pub mod stream;

use std::sync::Arc;

use liyasa_core::ai::{ChatModel, EmbeddingModel};
use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::net::{HostSet, HttpClient, HttpPolicy, Purpose};
use zeroize::Zeroizing;

use crate::config::{ModelRef, ProviderConfig, Role};
use crate::error::AiFailure;

/// The providers §6.7 supports at 1.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Provider {
    OpenAi,
    Anthropic,
    Google,
    Ollama,
    Bedrock,
    /// Azure OpenAI, OpenRouter, Groq, Together, vLLM, and anything else that
    /// answers the OpenAI shape. Named by the operator; the name only picks the
    /// `ai.providers` entry that carries its `baseUrl`.
    OpenAiCompatible,
}

impl Provider {
    pub fn of(name: &str) -> Self {
        match name.to_ascii_lowercase().as_str() {
            "openai" => Self::OpenAi,
            "anthropic" | "claude" => Self::Anthropic,
            "google" | "gemini" => Self::Google,
            "ollama" => Self::Ollama,
            "bedrock" => Self::Bedrock,
            _ => Self::OpenAiCompatible,
        }
    }
}

/// One row of the compatibility table AST-21 asks the docs to carry.
///
/// Held here rather than written in Markdown so the table cannot claim support
/// this crate does not have: every row names the adapter behind it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Compatibility {
    /// What an operator writes before the colon in `provider:model`.
    pub name: &'static str,
    pub chat: bool,
    pub embeddings: bool,
    pub rerank: bool,
    /// The default endpoint when `ai.providers.<name>.baseUrl` is unset.
    pub default_base_url: Option<&'static str>,
    pub notes: &'static str,
}

/// Every provider this build can reach, and with which roles.
pub const COMPATIBILITY: &[Compatibility] = &[
    Compatibility {
        name: "anthropic",
        chat: true,
        embeddings: false,
        rerank: false,
        default_base_url: Some("https://api.anthropic.com"),
        notes: "Recommended for the assistant and agent roles. No embedding \
                endpoint: route `ai.models.embeddings` elsewhere.",
    },
    Compatibility {
        name: "openai",
        chat: true,
        embeddings: true,
        rerank: false,
        default_base_url: Some("https://api.openai.com/v1"),
        notes: "Recommended default for embeddings.",
    },
    Compatibility {
        name: "google",
        chat: true,
        embeddings: true,
        rerank: false,
        default_base_url: Some("https://generativelanguage.googleapis.com/v1beta"),
        notes: "Gemini. The key travels as `x-goog-api-key`, never in the URL.",
    },
    Compatibility {
        name: "ollama",
        chat: true,
        embeddings: true,
        rerank: false,
        default_base_url: Some("http://localhost:11434/v1"),
        notes: "Through Ollama's OpenAI-compatible endpoint. Local, so the \
                address is private and `network.allowPrivate` must list it.",
    },
    Compatibility {
        name: "<any OpenAI-compatible>",
        chat: true,
        embeddings: true,
        rerank: false,
        default_base_url: None,
        notes: "Azure OpenAI, OpenRouter, Groq, Together, vLLM. Set \
                `ai.providers.<name>.baseUrl`; there is no default to guess.",
    },
    Compatibility {
        name: "bedrock",
        chat: false,
        embeddings: false,
        rerank: false,
        default_base_url: None,
        notes: "Not reachable from this build: Bedrock signs every request with \
                SigV4 and no signing crate is in the dependency table (§6.2.1). \
                Configuring it raises E0910 rather than failing at the first \
                call.",
    },
];

pub fn compatibility(provider: Provider) -> &'static Compatibility {
    let name = match provider {
        Provider::OpenAi => "openai",
        Provider::Anthropic => "anthropic",
        Provider::Google => "google",
        Provider::Ollama => "ollama",
        Provider::Bedrock => "bedrock",
        Provider::OpenAiCompatible => "<any OpenAI-compatible>",
    };
    COMPATIBILITY
        .iter()
        .find(|row| row.name == name)
        .unwrap_or_else(|| unreachable!("every Provider has a compatibility row"))
}

/// What an adapter needs besides the model name.
pub struct Endpoint {
    pub base_url: String,
    pub key: Option<Zeroizing<String>>,
    pub http: Arc<dyn HttpClient>,
    pub policy: HttpPolicy,
}

/// The default policy for a model call. Hosts are left empty, which denies
/// everything: the caller fills `allow_hosts` from the endpoint it configured,
/// so a misconfigured base URL cannot reach an arbitrary host.
pub fn policy_for(base_url: &str, allow_private: bool) -> Result<HttpPolicy, AiFailure> {
    let url = url::Url::parse(base_url).map_err(|e| {
        AiFailure::from(
            Diagnostic::new(code::E0910, format!("`{base_url}` is not a URL: {e}"))
                .help("set `ai.providers.<name>.baseUrl` to an absolute `https://` URL"),
        )
    })?;
    let Some(host) = url.host_str() else {
        return Err(AiFailure::from(Diagnostic::new(
            code::E0910,
            format!("`{base_url}` has no host"),
        )));
    };
    Ok(HttpPolicy {
        allow_hosts: HostSet(vec![liyasa_core::net::HostPattern::Exact(host.to_owned())]),
        deny_hosts: HostSet::default(),
        allow_private,
        max_redirects: 0,
        max_bytes: 16 * 1024 * 1024,
        timeout: std::time::Duration::from_secs(120),
        purpose: Purpose::ModelProvider,
    })
}

/// The base URL for a provider: the operator's, else the documented default.
pub fn base_url(provider: Provider, config: &ProviderConfig) -> Result<String, AiFailure> {
    if let Some(base) = config.base_url.as_deref().filter(|b| !b.is_empty()) {
        return Ok(base.trim_end_matches('/').to_owned());
    }
    match compatibility(provider).default_base_url {
        Some(default) => Ok(default.to_owned()),
        None => Err(AiFailure::from(
            Diagnostic::new(
                code::E0910,
                "this provider has no default endpoint".to_owned(),
            )
            .help("set `ai.providers.<name>.baseUrl`"),
        )),
    }
}

fn unsupported(model: &ModelRef, role: Role) -> AiFailure {
    let row = compatibility(Provider::of(&model.provider));
    AiFailure::from(
        Diagnostic::new(
            code::E0910,
            format!("`{}` cannot serve the `{role}` role", model.provider),
        )
        .help(row.notes),
    )
}

/// The chat model for `model`, or a diagnostic naming why not.
pub fn chat_model(
    model: &ModelRef,
    endpoint: Endpoint,
    role: Role,
) -> Result<Box<dyn ChatModel>, AiFailure> {
    let provider = Provider::of(&model.provider);
    if !compatibility(provider).chat {
        return Err(unsupported(model, role));
    }
    Ok(match provider {
        Provider::Anthropic => Box::new(anthropic::Chat::new(model.model.clone(), endpoint)),
        Provider::Google => Box::new(google::Chat::new(model.model.clone(), endpoint)),
        _ => Box::new(openai::Chat::new(model.model.clone(), endpoint)),
    })
}

/// The embedding model for `model`, or a diagnostic naming why not.
///
/// `dims` is the dimension the operator recorded for this model; a provider
/// reports its own on the first call and the two are compared there, because
/// nothing in a model name says how wide its vectors are.
pub fn embedding_model(
    model: &ModelRef,
    endpoint: Endpoint,
    dims: usize,
) -> Result<Box<dyn EmbeddingModel>, AiFailure> {
    let provider = Provider::of(&model.provider);
    if !compatibility(provider).embeddings {
        return Err(unsupported(model, Role::Embeddings));
    }
    Ok(match provider {
        Provider::Google => Box::new(google::Embeddings::new(model.model.clone(), dims, endpoint)),
        _ => Box::new(openai::Embeddings::new(model.model.clone(), dims, endpoint)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_name_is_treated_as_openai_compatible() {
        assert_eq!(Provider::of("groq"), Provider::OpenAiCompatible);
        assert_eq!(Provider::of("OpenAI"), Provider::OpenAi);
        assert_eq!(Provider::of("claude"), Provider::Anthropic);
    }

    #[test]
    fn every_provider_has_exactly_one_compatibility_row() {
        for provider in [
            Provider::OpenAi,
            Provider::Anthropic,
            Provider::Google,
            Provider::Ollama,
            Provider::Bedrock,
            Provider::OpenAiCompatible,
        ] {
            let name = compatibility(provider).name;
            assert_eq!(
                COMPATIBILITY.iter().filter(|r| r.name == name).count(),
                1,
                "{provider:?}"
            );
        }
    }

    #[test]
    fn a_row_that_claims_a_role_has_an_adapter_behind_it() {
        // The table is the documentation (AST-21); a row claiming chat must
        // produce a chat model rather than a diagnostic.
        for row in COMPATIBILITY {
            let provider = Provider::of(row.name);
            if row.name.starts_with('<') {
                continue;
            }
            let model = ModelRef {
                provider: row.name.to_owned(),
                model: "m".to_owned(),
            };
            let made = chat_model(
                &model,
                Endpoint {
                    base_url: "https://example.invalid".to_owned(),
                    key: None,
                    http: Arc::new(NoClient),
                    policy: policy_for("https://example.invalid", false).expect("policy"),
                },
                Role::Assistant,
            );
            assert_eq!(
                made.is_ok(),
                row.chat,
                "`{}` claims chat={} and the builder disagrees",
                row.name,
                row.chat
            );
            let _ = provider;
        }
    }

    #[test]
    fn bedrock_is_refused_where_it_is_configured_not_at_the_first_call() {
        let model = ModelRef {
            provider: "bedrock".to_owned(),
            model: "anthropic.claude-sonnet-4:0".to_owned(),
        };
        let error = chat_model(
            &model,
            Endpoint {
                base_url: "https://example.invalid".to_owned(),
                key: None,
                http: Arc::new(NoClient),
                policy: policy_for("https://example.invalid", false).expect("policy"),
            },
            Role::Assistant,
        )
        .map(|_| ())
        .expect_err("bedrock has no adapter");
        assert_eq!(error.diagnostic().code, code::E0910);
        assert!(
            error
                .diagnostic()
                .help
                .as_deref()
                .unwrap_or("")
                .contains("SigV4")
        );
    }

    #[test]
    fn a_base_url_without_a_default_must_be_configured() {
        let mut config = ProviderConfig::default();
        assert!(base_url(Provider::OpenAiCompatible, &config).is_err());
        config.base_url = Some("https://llm.example.com/v1/".to_owned());
        assert_eq!(
            base_url(Provider::OpenAiCompatible, &config).expect("configured"),
            "https://llm.example.com/v1"
        );
    }

    #[test]
    fn the_policy_allows_only_the_configured_host() {
        let policy = policy_for("https://api.openai.com/v1", false).expect("policy");
        assert!(policy.allow_hosts.matches("api.openai.com"));
        assert!(!policy.allow_hosts.matches("evil.example"));
        assert_eq!(policy.purpose, Purpose::ModelProvider);
        assert_eq!(policy.max_redirects, 0);
    }

    struct NoClient;

    impl HttpClient for NoClient {
        fn fetch<'a>(
            &'a self,
            _req: liyasa_core::net::HttpRequest,
            _policy: &'a HttpPolicy,
        ) -> liyasa_core::net::BoxFut<
            'a,
            Result<liyasa_core::net::HttpResponse, liyasa_core::net::NetError>,
        > {
            Box::pin(std::future::ready(Err(liyasa_core::net::NetError::Io(
                "no client in this test".to_owned(),
            ))))
        }
    }
}
