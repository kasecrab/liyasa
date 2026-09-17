//! Turning a provider failure into a registered diagnostic.
//!
//! [`AiError`] is the frozen contract and carries no code, because
//! `liyasa-core` may not decide how a failure is presented. The mapping lives
//! here, so every crate that surfaces an assistant failure to a reader or an
//! operator shows the same code for the same cause.

use liyasa_core::ai::AiError;
use liyasa_core::diagnostics::{Diagnostic, code};

/// A failure with the diagnostic already chosen.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{}", .0.message)]
pub struct AiFailure(pub Box<Diagnostic>);

impl AiFailure {
    pub fn diagnostic(&self) -> &Diagnostic {
        &self.0
    }

    pub fn into_diagnostic(self) -> Diagnostic {
        *self.0
    }
}

impl From<Diagnostic> for AiFailure {
    fn from(d: Diagnostic) -> Self {
        Self(Box::new(d))
    }
}

impl From<AiError> for AiFailure {
    fn from(error: AiError) -> Self {
        diagnose(&error).into()
    }
}

/// The code each [`AiError`] variant is shown under.
pub fn diagnose(error: &AiError) -> Diagnostic {
    match error {
        AiError::Budget => Diagnostic::new(code::E0902, error.to_string()).help(
            "raise `ai.assistant.maxTokens`, or ask a narrower question so the plan needs fewer \
             retrieval steps",
        ),
        AiError::Policy(_) => Diagnostic::new(code::E0903, error.to_string()),
        AiError::RateLimited { retry_after } => {
            let d = Diagnostic::new(code::E0901, error.to_string());
            match retry_after {
                Some(after) => d.help(format!(
                    "the provider asked for {}s; the re-index job backs off on its own, a reader \
                     request does not",
                    after.as_secs()
                )),
                None => d.help("the provider gave no retry hint; retry with backoff"),
            }
        }
        AiError::Provider { status, .. } if *status == 401 || *status == 403 => {
            Diagnostic::new(code::E0901, error.to_string()).help(
                "check the provider key in the server's secret store; `ai.providers` holds only \
                 non-secret settings",
            )
        }
        AiError::Provider { .. } => Diagnostic::new(code::E0901, error.to_string()),
        AiError::Net(net) => Diagnostic::new(code::E0901, format!("model provider: {net}")).help(
            "a model provider is reached under `Purpose::ModelProvider`; a self-hosted endpoint \
             on a private address needs `network.allowPrivate`",
        ),
        // `AiError` is `#[non_exhaustive]`; a variant added later must not
        // reach a reader without a code, so it takes the generic one.
        _ => Diagnostic::new(code::E0901, error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn every_variant_has_a_registered_code() {
        let errors = [
            AiError::Budget,
            AiError::Policy("no write tools".to_owned()),
            AiError::RateLimited {
                retry_after: Some(Duration::from_secs(3)),
            },
            AiError::RateLimited { retry_after: None },
            AiError::Provider {
                status: 401,
                message: "unauthorized".to_owned(),
            },
            AiError::Provider {
                status: 500,
                message: "upstream".to_owned(),
            },
            AiError::Net(liyasa_core::net::NetError::Timeout),
        ];
        for error in errors {
            let d = diagnose(&error);
            assert_eq!(d.code.info().krate, "liyasa-ai");
            assert!(!d.message.is_empty());
        }
    }

    #[test]
    fn an_exhausted_budget_is_not_a_provider_error() {
        assert_eq!(diagnose(&AiError::Budget).code, code::E0902);
        assert_eq!(diagnose(&AiError::Policy("x".to_owned())).code, code::E0903);
    }
}
