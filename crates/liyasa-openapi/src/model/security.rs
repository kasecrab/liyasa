//! Security schemes and the requirements that select them (API-43).

use serde::Serialize;

use super::map::OrderedMap;
use super::{Extensions, ParameterIn};

/// One alternative from an operation's `security` list: every scheme named in
/// it must be satisfied, and satisfying any one alternative is enough.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(transparent)]
pub struct SecurityRequirement(pub OrderedMap<Vec<String>>);

impl SecurityRequirement {
    pub fn is_anonymous(&self) -> bool {
        self.0.is_empty()
    }

    pub fn schemes(&self) -> impl Iterator<Item = (&str, &[String])> {
        self.0
            .iter()
            .map(|(name, scopes)| (name, scopes.as_slice()))
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum SecuritySchemeKind {
    /// `http` with `scheme: basic`, `bearer`, or anything else the API uses.
    Http {
        scheme: String,
        bearer_format: Option<String>,
    },
    ApiKey {
        name: String,
        #[serde(rename = "in")]
        location: ParameterIn,
    },
    #[serde(rename = "oauth2")]
    OAuth2 {
        flows: Box<OAuthFlows>,
    },
    OpenIdConnect {
        url: String,
    },
    /// 3.1's `mutualTLS`, which the playground cannot drive from a browser.
    MutualTls,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityScheme {
    pub kind: SecuritySchemeKind,
    pub description: Option<String>,
    pub extensions: Extensions,
}

impl SecurityScheme {
    /// What the playground has to ask the reader for (API-43).
    pub fn prompt(&self) -> &'static str {
        match &self.kind {
            SecuritySchemeKind::Http { scheme, .. } if scheme.eq_ignore_ascii_case("basic") => {
                "username and password"
            }
            SecuritySchemeKind::Http { .. } => "token",
            SecuritySchemeKind::ApiKey { .. } => "API key",
            SecuritySchemeKind::OAuth2 { .. } | SecuritySchemeKind::OpenIdConnect { .. } => {
                "authorization"
            }
            SecuritySchemeKind::MutualTls => "client certificate",
        }
    }

    /// Whether a browser can drive this scheme at all (API-45).
    pub fn drivable_in_a_browser(&self) -> bool {
        !matches!(self.kind, SecuritySchemeKind::MutualTls)
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthFlows {
    pub implicit: Option<OAuthFlow>,
    pub password: Option<OAuthFlow>,
    pub client_credentials: Option<OAuthFlow>,
    pub authorization_code: Option<OAuthFlow>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthFlow {
    pub authorization_url: Option<String>,
    pub token_url: Option<String>,
    pub refresh_url: Option<String>,
    /// Scope to its description, in document order.
    pub scopes: OrderedMap<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_requirement_is_the_anonymous_alternative() {
        assert!(SecurityRequirement::default().is_anonymous());
    }

    #[test]
    fn basic_asks_for_a_password_and_bearer_for_a_token() {
        let basic = SecurityScheme {
            kind: SecuritySchemeKind::Http {
                scheme: "Basic".to_owned(),
                bearer_format: None,
            },
            description: None,
            extensions: Extensions::default(),
        };
        assert_eq!(basic.prompt(), "username and password");

        let bearer = SecurityScheme {
            kind: SecuritySchemeKind::Http {
                scheme: "bearer".to_owned(),
                bearer_format: Some("JWT".to_owned()),
            },
            description: None,
            extensions: Extensions::default(),
        };
        assert_eq!(bearer.prompt(), "token");
        assert!(bearer.drivable_in_a_browser());
    }

    #[test]
    fn mutual_tls_cannot_be_driven_from_a_browser() {
        let scheme = SecurityScheme {
            kind: SecuritySchemeKind::MutualTls,
            description: None,
            extensions: Extensions::default(),
        };
        assert!(!scheme.drivable_in_a_browser());
    }
}
