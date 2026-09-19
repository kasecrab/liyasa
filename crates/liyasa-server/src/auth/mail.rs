//! The `mail` section of `liyasa.json`, and resolving the password it points at
//! (AUTH-05, AUTH-09; WP-01's key).
//!
//! The transport itself is not here yet — `lettre` cannot be added until
//! `deny.toml` allows `0BSD` for `quoted_printable`, which is WP-00's file.
//! What is here is everything that does not depend on it: reading the key, and
//! turning `smtp.password` from a *reference* into a secret.
//!
//! **The password in the file is never the password.** The schema's pattern
//! admits only `secret:<name>` and `env:<VAR>`, because `liyasa.json` is
//! committed to the repository the docs live in. Accepting a literal here
//! would make that pattern decorative — the check would exist and not be
//! load-bearing, which is this codebase's own recurring defect applied to a
//! check added to prevent it.
//!
//! **A note that belongs with the sender rather than the config, recorded here
//! because this is where someone will look first.** `Mail::send_link` must not
//! be awaited in the request path. AUTH-09 requires `POST /_liyasa/auth/magic`
//! to answer identically whether or not the address is known, and awaiting an
//! SMTP send *only when a link was minted* makes response time a timing oracle
//! for exactly the thing the identical response exists to hide. The send is
//! spawned and its result is not observable from outside: a log line is fine,
//! but a metric labelled by outcome, or a retry that moves a queue depth an
//! attacker can probe, reintroduces the oracle one layer out.

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::verify::SecretSource;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Security {
    /// Upgrade a plain connection. The schema's default.
    #[default]
    Starttls,
    /// Encrypted from the first byte.
    Tls,
    /// Plaintext, which the schema's own description limits to a loopback or
    /// private-network relay.
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SmtpConfig {
    pub host: String,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub security: Security,
    #[serde(default)]
    pub username: Option<String>,
    /// A **reference**, never the password. See [`Reference`].
    #[serde(default)]
    pub password: Option<String>,
}

impl SmtpConfig {
    /// The port to connect on, from the key or from what `security` implies:
    /// 587 for STARTTLS, 465 for implicit TLS, 25 for a plaintext relay —
    /// which is the mapping the schema's own description gives.
    pub fn port(&self) -> u16 {
        self.port.unwrap_or(match self.security {
            Security::Starttls => 587,
            Security::Tls => 465,
            Security::None => 25,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MailConfig {
    pub from: String,
    #[serde(default)]
    pub reply_to: Option<String>,
    pub smtp: SmtpConfig,
}

impl MailConfig {
    /// Reads `mail` from a whole `liyasa.json`. `Ok(None)` when the block is
    /// absent, which the schema's description calls out as a state rather than
    /// a default: without it the product sends nothing **and says so**.
    pub fn from_site_config(config: &serde_json::Value) -> Result<Option<Self>, Diagnostic> {
        match config.get("mail") {
            None | Some(serde_json::Value::Null) => Ok(None),
            Some(section) => serde_json::from_value(section.clone())
                .map(Some)
                .map_err(|error| {
                    Diagnostic::new(
                        code::E0803,
                        format!("`mail` is not a valid mail configuration: {error}"),
                    )
                    .help("every key under `mail` is listed in `schemas/liyasa.schema.json`")
                }),
        }
    }

    /// The password, resolved. `Ok(None)` is an unauthenticated relay, which
    /// is legitimate on a private network; `Err` is a reference that names
    /// something the instance cannot read, which is a misconfiguration and
    /// must not be mistaken for the unauthenticated case.
    pub fn password(
        &self,
        secrets: Option<&dyn SecretSource>,
    ) -> Result<Option<Zeroizing<String>>, Unresolved> {
        self.password_with(secrets, &system_env)
    }

    /// [`MailConfig::password`] with the environment supplied.
    ///
    /// The seam exists because `unsafe_code` is forbidden workspace-wide and
    /// `std::env::set_var` is `unsafe` from the 2024 edition, so a test cannot
    /// set a variable to exercise the `env:` form. Injecting the lookup is the
    /// same shape as [`Clock`](crate::auth::clock::Clock) and
    /// [`Resolver`](crate::auth::dns::Resolver): what the process reads from
    /// outside is a parameter, so a test can be the outside.
    pub fn password_with(
        &self,
        secrets: Option<&dyn SecretSource>,
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Result<Option<Zeroizing<String>>, Unresolved> {
        let Some(reference) = self.smtp.password.as_deref() else {
            return Ok(None);
        };
        Reference::parse(reference)
            .ok_or_else(|| Unresolved::NotAReference(redact(reference)))?
            .resolve_with(secrets, env)
            .map(Some)
    }
}

/// Where a secret actually lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reference {
    /// `secret:<name>` — the encrypted secret store.
    Secret(String),
    /// `env:<VAR>` — the process environment.
    Env(String),
}

impl Reference {
    /// Parses the two forms the schema's pattern admits, and **nothing else**.
    /// A literal password is not a reference and is refused here as well as by
    /// the schema, so an instance whose config skipped validation still does
    /// not send a plaintext secret to an SMTP server.
    pub fn parse(text: &str) -> Option<Self> {
        let (kind, rest) = text.split_once(':')?;
        if rest.is_empty() {
            return None;
        }
        match kind {
            "secret" => Some(Reference::Secret(rest.to_owned())),
            "env" => Some(Reference::Env(rest.to_owned())),
            _ => None,
        }
    }

    pub fn resolve(
        &self,
        secrets: Option<&dyn SecretSource>,
    ) -> Result<Zeroizing<String>, Unresolved> {
        self.resolve_with(secrets, &system_env)
    }

    pub fn resolve_with(
        &self,
        secrets: Option<&dyn SecretSource>,
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Result<Zeroizing<String>, Unresolved> {
        match self {
            Reference::Secret(name) => secrets
                .ok_or_else(|| Unresolved::NoSecretStore(name.clone()))?
                .get(name)
                .ok_or_else(|| Unresolved::MissingSecret(name.clone())),
            Reference::Env(var) => env(var)
                .map(Zeroizing::new)
                .ok_or_else(|| Unresolved::MissingEnv(var.clone())),
        }
    }
}

/// Why a password reference did not produce a password. Every variant names
/// what to fix; none of them carries the value it was looking for.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Unresolved {
    #[error("`mail.smtp.password` is `{0}`, which is not `secret:<name>` or `env:<VAR>`")]
    NotAReference(String),
    #[error("`mail.smtp.password` names secret `{0}` and this instance has no secret store")]
    NoSecretStore(String),
    #[error("`mail.smtp.password` names secret `{0}`, which is not in the secret store")]
    MissingSecret(String),
    #[error("`mail.smtp.password` names environment variable `{0}`, which is not set")]
    MissingEnv(String),
}

/// The process environment, which is what production reads.
fn system_env(var: &str) -> Option<String> {
    std::env::var(var).ok()
}

/// Enough of a malformed reference to recognise it, and not enough to leak it.
/// A config that put a real password here is a mistake that must be reportable
/// without the report becoming the second place the password appears.
fn redact(text: &str) -> String {
    match text.split_once(':') {
        Some((kind, _)) if !kind.is_empty() && kind.len() <= 16 => format!("{kind}:…"),
        _ => "…".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[derive(Debug, Default)]
    struct Vault(BTreeMap<String, String>);

    impl Vault {
        fn with(name: &str, value: &str) -> Self {
            Self(BTreeMap::from([(name.to_owned(), value.to_owned())]))
        }
    }

    impl SecretSource for Vault {
        fn get(&self, name: &str) -> Option<Zeroizing<String>> {
            self.0.get(name).cloned().map(Zeroizing::new)
        }
    }

    fn config(mail: serde_json::Value) -> serde_json::Value {
        serde_json::json!({ "name": "docs", "mail": mail })
    }

    #[test]
    fn a_site_with_no_mail_block_sends_nothing_and_that_is_not_an_error() {
        assert_eq!(
            MailConfig::from_site_config(&serde_json::json!({ "name": "docs" })).expect("ok"),
            None
        );
    }

    #[test]
    fn the_block_reads_every_key_the_schema_declares() {
        let parsed = MailConfig::from_site_config(&config(serde_json::json!({
            "from": "Docs <docs@example.com>",
            "replyTo": "support@example.com",
            "smtp": {
                "host": "smtp.example.com",
                "port": 2525,
                "security": "tls",
                "username": "docs",
                "password": "secret:smtp-password"
            }
        })))
        .expect("a parse")
        .expect("a block");

        assert_eq!(parsed.from, "Docs <docs@example.com>");
        assert_eq!(parsed.reply_to.as_deref(), Some("support@example.com"));
        assert_eq!(parsed.smtp.host, "smtp.example.com");
        assert_eq!(parsed.smtp.port(), 2525);
        assert_eq!(parsed.smtp.security, Security::Tls);
        assert_eq!(parsed.smtp.username.as_deref(), Some("docs"));
    }

    #[test]
    fn a_typo_under_mail_is_a_diagnostic_rather_than_a_silent_default() {
        let error = MailConfig::from_site_config(&config(serde_json::json!({
            "from": "d@example.com",
            "smpt": { "host": "x" }
        })))
        .expect_err("a typo is refused");
        assert_eq!(error.code, code::E0803);
    }

    #[test]
    fn the_port_follows_the_security_the_schema_describes() {
        let port_for = |security: &str| {
            MailConfig::from_site_config(&config(serde_json::json!({
                "from": "d@example.com",
                "smtp": { "host": "h", "security": security }
            })))
            .expect("a parse")
            .expect("a block")
            .smtp
            .port()
        };
        assert_eq!(port_for("starttls"), 587);
        assert_eq!(port_for("tls"), 465);
        assert_eq!(port_for("none"), 25);

        // An explicit port always wins over the implication.
        let explicit = MailConfig::from_site_config(&config(serde_json::json!({
            "from": "d@example.com",
            "smtp": { "host": "h", "security": "tls", "port": 10025 }
        })))
        .expect("a parse")
        .expect("a block");
        assert_eq!(explicit.smtp.port(), 10025);
    }

    #[test]
    fn starttls_is_the_default_so_an_unstated_security_is_not_plaintext() {
        let parsed = MailConfig::from_site_config(&config(serde_json::json!({
            "from": "d@example.com",
            "smtp": { "host": "h" }
        })))
        .expect("a parse")
        .expect("a block");
        assert_eq!(parsed.smtp.security, Security::Starttls);
        assert_ne!(parsed.smtp.security, Security::None);
    }

    fn with_password(password: Option<&str>) -> MailConfig {
        let mut smtp = serde_json::json!({ "host": "h" });
        if let Some(password) = password {
            smtp["password"] = serde_json::json!(password);
        }
        MailConfig::from_site_config(&config(serde_json::json!({
            "from": "d@example.com",
            "smtp": smtp
        })))
        .expect("a parse")
        .expect("a block")
    }

    #[test]
    fn a_secret_reference_reads_the_secret_store() {
        let vault = Vault::with("smtp-password", "hunter2");
        let resolved = with_password(Some("secret:smtp-password"))
            .password(Some(&vault))
            .expect("resolved")
            .expect("a password");
        assert_eq!(resolved.as_str(), "hunter2");
    }

    #[test]
    fn an_env_reference_reads_the_environment() {
        let env = |var: &str| match var {
            "SMTP_PASSWORD" => Some("from-the-env".to_owned()),
            _ => None,
        };
        let resolved = with_password(Some("env:SMTP_PASSWORD"))
            .password_with(None, &env)
            .expect("resolved")
            .expect("a password");
        assert_eq!(resolved.as_str(), "from-the-env");
    }

    #[test]
    fn no_password_is_an_unauthenticated_relay_rather_than_a_failure() {
        assert!(
            with_password(None)
                .password(None)
                .expect("no password is fine")
                .is_none()
        );
    }

    /// The distinction that matters: "there is no password" and "the password
    /// reference does not resolve" are different states, and collapsing them
    /// would send unauthenticated to a server that requires authentication and
    /// report nothing.
    #[test]
    fn a_reference_that_does_not_resolve_is_an_error_and_not_an_absent_password() {
        let vault = Vault::default();
        assert_eq!(
            with_password(Some("secret:absent")).password(Some(&vault)),
            Err(Unresolved::MissingSecret("absent".to_owned()))
        );
        assert_eq!(
            with_password(Some("secret:anything")).password(None),
            Err(Unresolved::NoSecretStore("anything".to_owned()))
        );
        assert_eq!(
            with_password(Some("env:SMTP_PASSWORD")).password_with(None, &|_| None),
            Err(Unresolved::MissingEnv("SMTP_PASSWORD".to_owned()))
        );
    }

    /// The schema's pattern refuses a literal, and so does this — an instance
    /// whose config skipped validation still must not hand a plaintext secret
    /// to an SMTP server.
    #[test]
    fn a_literal_password_is_refused_here_as_well_as_by_the_schema() {
        for literal in ["hunter2", "", "secret:", "env:", "vault:name", "::"] {
            assert_eq!(Reference::parse(literal), None, "{literal}");
        }
        assert_eq!(
            Reference::parse("secret:a"),
            Some(Reference::Secret("a".to_owned()))
        );
        assert_eq!(
            Reference::parse("env:A"),
            Some(Reference::Env("A".to_owned()))
        );
    }

    /// Reporting a bad reference must not become the second place the password
    /// appears.
    #[test]
    fn refusing_a_literal_does_not_repeat_it() {
        let error = with_password(Some("hunter2"))
            .password(None)
            .expect_err("not a reference");
        let shown = error.to_string();
        assert!(!shown.contains("hunter2"), "{shown}");

        // A value shaped like a reference with an unknown scheme keeps its
        // scheme, which is the part that tells an operator what they got wrong,
        // and drops everything after the colon, which is the part that might be
        // the password they meant to reference.
        let error = with_password(Some("vault:s3cret-value"))
            .password(None)
            .expect_err("bad scheme");
        let shown = error.to_string();
        assert!(shown.contains("vault:…"), "{shown}");
        assert!(!shown.contains("s3cret-value"), "{shown}");
        // `<name>` in the message is the placeholder in `secret:<name>`, not
        // anything the operator wrote. Asserting on the absence of "name"
        // would have failed against correct output — which it did.
    }
}
