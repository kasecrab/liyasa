//! Environments (GIT-22) and the hosts a preview is served on (GIT-30).
//!
//! `production` and `preview` exist without being configured. Anything else is
//! a named environment with its own branch, domain and config overlay, and is
//! held to the same name rules as a DNS label, because it becomes one.

use serde::{Deserialize, Serialize};

pub const PRODUCTION: &str = "production";
pub const PREVIEW: &str = "preview";

/// The longest a DNS label may be (RFC 1035). A preview host is assembled from
/// a project slug and a pull-request number, so the slug is what gets cut.
pub const MAX_LABEL: usize = 63;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnvironmentKind {
    Production,
    Preview,
    Named,
}

/// How a preview is gated (GIT-32, `auth.preview.protection`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protection {
    Public,
    Password,
    #[default]
    Organization,
}

impl Protection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Password => "password",
            Self::Organization => "organization",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "public" => Some(Self::Public),
            "password" => Some(Self::Password),
            "organization" => Some(Self::Organization),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    pub name: String,
    pub kind: EnvironmentKind,
    /// The branch whose pushes deploy here. `preview` has none: every branch
    /// that is not another environment's builds a preview.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub base_path: String,
    /// Config keys overlaid on `liyasa.json` for a build of this environment.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub overlay: serde_json::Value,
    pub protection: Protection,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NameError {
    #[error("an environment name is required")]
    Empty,
    #[error("`{0}` is longer than {MAX_LABEL} characters")]
    TooLong(String),
    #[error("`{0}` is not lowercase letters, digits and hyphens")]
    NotALabel(String),
    #[error("`{0}` may not start or end with a hyphen")]
    EdgeHyphen(String),
}

/// An environment name has to survive being a DNS label and a URL path
/// segment, so it is held to the stricter of the two up front rather than
/// failing later at certificate issuance.
pub fn validate_name(name: &str) -> Result<(), NameError> {
    if name.is_empty() {
        return Err(NameError::Empty);
    }
    if name.len() > MAX_LABEL {
        return Err(NameError::TooLong(name.to_owned()));
    }
    if !name
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(NameError::NotALabel(name.to_owned()));
    }
    if name.starts_with('-') || name.ends_with('-') {
        return Err(NameError::EdgeHyphen(name.to_owned()));
    }
    Ok(())
}

impl Environment {
    pub fn production(branch: impl Into<String>) -> Self {
        Self {
            name: PRODUCTION.to_owned(),
            kind: EnvironmentKind::Production,
            branch: Some(branch.into()),
            domain: None,
            base_path: String::new(),
            overlay: serde_json::Value::Null,
            protection: Protection::Public,
        }
    }

    pub fn preview() -> Self {
        Self {
            name: PREVIEW.to_owned(),
            kind: EnvironmentKind::Preview,
            branch: None,
            domain: None,
            base_path: String::new(),
            overlay: serde_json::Value::Null,
            protection: Protection::default(),
        }
    }

    pub fn named(name: &str, branch: impl Into<String>) -> Result<Self, NameError> {
        validate_name(name)?;
        Ok(Self {
            name: name.to_owned(),
            kind: EnvironmentKind::Named,
            branch: Some(branch.into()),
            domain: None,
            base_path: String::new(),
            overlay: serde_json::Value::Null,
            protection: Protection::Public,
        })
    }

    pub fn with_domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }

    pub fn with_base_path(mut self, base_path: impl Into<String>) -> Self {
        self.base_path = base_path.into();
        self
    }

    pub fn with_overlay(mut self, overlay: serde_json::Value) -> Self {
        self.overlay = overlay;
        self
    }

    pub fn with_protection(mut self, protection: Protection) -> Self {
        self.protection = protection;
        self
    }

    /// Whether a response from this environment may be indexed. Only
    /// production may (GIT-30).
    pub fn indexable(&self) -> bool {
        self.kind == EnvironmentKind::Production
    }
}

/// The environment a push to `branch` deploys to, or `None` when the branch
/// belongs to no environment and therefore builds a preview.
///
/// A named environment wins over production when both claim the same branch,
/// because the named one was configured deliberately and production's branch
/// is usually a default nobody revisited.
pub fn for_branch<'a>(environments: &'a [Environment], branch: &str) -> Option<&'a Environment> {
    let matches = |kind: EnvironmentKind| {
        environments
            .iter()
            .find(|env| env.kind == kind && env.branch.as_deref() == Some(branch))
    };
    matches(EnvironmentKind::Named).or_else(|| matches(EnvironmentKind::Production))
}

pub fn by_name<'a>(environments: &'a [Environment], name: &str) -> Option<&'a Environment> {
    environments.iter().find(|env| env.name == name)
}

/// One DNS label out of arbitrary text: lowercase, non-alphanumeric collapsed
/// to a single hyphen, trimmed, and cut to `budget` characters.
pub fn label(text: &str, budget: usize) -> String {
    let mut out = String::with_capacity(text.len().min(budget));
    let mut pending_hyphen = false;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_hyphen && !out.is_empty() && out.len() < budget {
                out.push('-');
            }
            pending_hyphen = false;
            if out.len() < budget {
                out.push(ch.to_ascii_lowercase());
            }
        } else {
            pending_hyphen = true;
        }
    }
    out.trim_matches('-').to_owned()
}

/// `<project>-pr-<n>.<preview-domain>` (GIT-30). The project slug is cut so
/// that the whole label fits in 63 characters however long the number is.
pub fn pull_request_host(project_slug: &str, number: u64, preview_domain: &str) -> String {
    let suffix = format!("-pr-{number}");
    let budget = MAX_LABEL.saturating_sub(suffix.len());
    let slug = label(project_slug, budget);
    let slug = if slug.is_empty() { "site" } else { &slug };
    format!("{slug}{suffix}.{}", preview_domain.trim_matches('.'))
}

/// The host a branch preview is served on, for a branch that is not a pull
/// request (GIT-33).
pub fn branch_host(project_slug: &str, branch: &str, preview_domain: &str) -> String {
    let project = label(project_slug, MAX_LABEL);
    let budget = MAX_LABEL.saturating_sub(project.len() + 1);
    let branch = label(branch, budget);
    let left = match branch.is_empty() {
        true => project,
        false => format!("{project}-{branch}"),
    };
    format!("{left}.{}", preview_domain.trim_matches('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_and_preview_exist_without_being_configured() {
        let production = Environment::production("main");
        assert_eq!(production.name, PRODUCTION);
        assert!(production.indexable());
        assert_eq!(production.protection, Protection::Public);

        let preview = Environment::preview();
        assert!(!preview.indexable(), "a preview is never indexable");
        assert_eq!(
            preview.protection,
            Protection::Organization,
            "a preview defaults to the organization, not to the open web"
        );
        assert_eq!(preview.branch, None);
    }

    #[test]
    fn a_named_environment_is_held_to_the_rules_of_a_dns_label() {
        assert!(Environment::named("staging", "release").is_ok());
        assert_eq!(validate_name(""), Err(NameError::Empty));
        assert_eq!(
            validate_name("Staging"),
            Err(NameError::NotALabel("Staging".to_owned()))
        );
        assert_eq!(
            validate_name("stag_ing"),
            Err(NameError::NotALabel("stag_ing".to_owned()))
        );
        assert_eq!(
            validate_name("-staging"),
            Err(NameError::EdgeHyphen("-staging".to_owned()))
        );
        assert_eq!(
            validate_name("staging-"),
            Err(NameError::EdgeHyphen("staging-".to_owned()))
        );
        let long = "s".repeat(MAX_LABEL + 1);
        assert_eq!(validate_name(&long), Err(NameError::TooLong(long)));
        assert!(validate_name(&"s".repeat(MAX_LABEL)).is_ok());
    }

    #[test]
    fn a_named_environment_carries_its_own_branch_domain_and_overlay() {
        let staging = Environment::named("staging", "release")
            .expect("a valid name")
            .with_domain("staging.example.com")
            .with_base_path("/docs")
            .with_overlay(serde_json::json!({ "site": { "name": "Staging" } }));
        assert_eq!(staging.kind, EnvironmentKind::Named);
        assert_eq!(staging.domain.as_deref(), Some("staging.example.com"));
        assert_eq!(staging.base_path, "/docs");
        assert_eq!(staging.overlay["site"]["name"], "Staging");
        assert!(!staging.indexable());
    }

    #[test]
    fn a_branch_resolves_to_the_environment_that_claims_it() {
        let environments = [
            Environment::production("main"),
            Environment::named("staging", "release").expect("a valid name"),
            Environment::preview(),
        ];
        assert_eq!(
            for_branch(&environments, "main").map(|e| e.name.as_str()),
            Some(PRODUCTION)
        );
        assert_eq!(
            for_branch(&environments, "release").map(|e| e.name.as_str()),
            Some("staging")
        );
        assert_eq!(
            for_branch(&environments, "feat/x"),
            None,
            "a branch nobody claims builds a preview"
        );
    }

    #[test]
    fn a_named_environment_wins_a_branch_it_shares_with_production() {
        let environments = [
            Environment::production("main"),
            Environment::named("staging", "main").expect("a valid name"),
        ];
        assert_eq!(
            for_branch(&environments, "main").map(|e| e.name.as_str()),
            Some("staging")
        );
    }

    #[test]
    fn a_preview_host_is_the_project_the_number_and_the_preview_domain() {
        assert_eq!(
            pull_request_host("liyasa", 42, "preview.example.com"),
            "liyasa-pr-42.preview.example.com"
        );
        assert_eq!(
            pull_request_host("Acme Docs!", 7, "preview.example.com"),
            "acme-docs-pr-7.preview.example.com"
        );
    }

    #[test]
    fn a_long_project_slug_is_cut_so_the_label_still_fits() {
        let host = pull_request_host(&"a".repeat(100), 1234, "preview.example.com");
        let first = host.split('.').next().expect("a label");
        assert!(first.len() <= MAX_LABEL, "{} is {}", first, first.len());
        assert!(first.ends_with("-pr-1234"));
    }

    #[test]
    fn a_project_slug_with_nothing_usable_in_it_still_makes_a_host() {
        assert_eq!(
            pull_request_host("!!!", 1, "preview.example.com"),
            "site-pr-1.preview.example.com"
        );
    }

    #[test]
    fn a_branch_preview_host_folds_the_slash_in_a_branch_name() {
        assert_eq!(
            branch_host("liyasa", "feat/new-nav", "preview.example.com"),
            "liyasa-feat-new-nav.preview.example.com"
        );
    }

    #[test]
    fn a_preview_domain_with_stray_dots_does_not_double_them() {
        assert_eq!(
            pull_request_host("liyasa", 1, ".preview.example.com."),
            "liyasa-pr-1.preview.example.com"
        );
    }

    #[test]
    fn a_label_never_runs_two_hyphens_together_or_ends_on_one() {
        assert_eq!(label("a -- b", MAX_LABEL), "a-b");
        assert_eq!(label("--lead and trail--", MAX_LABEL), "lead-and-trail");
        assert_eq!(label("", MAX_LABEL), "");
    }

    #[test]
    fn protection_round_trips_through_its_wire_name() {
        for protection in [
            Protection::Public,
            Protection::Password,
            Protection::Organization,
        ] {
            assert_eq!(Protection::parse(protection.as_str()), Some(protection));
        }
        assert_eq!(Protection::parse("sso"), None);
    }

    #[test]
    fn an_environment_round_trips_as_json_without_its_empty_fields() {
        let preview = Environment::preview();
        let text = serde_json::to_string(&preview).expect("an environment serializes");
        assert!(!text.contains("branch"), "{text}");
        assert!(!text.contains("overlay"), "{text}");
        let back: Environment = serde_json::from_str(&text).expect("it deserializes");
        assert_eq!(back, preview);
    }
}
