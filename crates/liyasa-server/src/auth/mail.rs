//! The `mail` section of `liyasa.json`, and resolving the password it points at
//! (AUTH-05, AUTH-09; WP-01's key).
//!
//! Reading the key, turning `smtp.password` from a *reference* into a secret,
//! and the SMTP transport that sends the link.
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

use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
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
                        code::E0816,
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

/// The SMTP sender (AUTH-05, AUTH-09).
///
/// **`send_link` does not await the send, and that is a security property
/// rather than a performance one.** AUTH-09 requires
/// `POST /_liyasa/auth/magic` to answer identically whether or not the address
/// is known. A link is only minted for an address that can sign in, so
/// awaiting the send would make the response slower exactly when the address
/// exists — a timing oracle for the thing the identical response exists to
/// hide. The send is spawned and the handler returns at the same speed either
/// way.
///
/// The same reasoning forbids letting the outcome out by another door: the
/// failure path logs and does nothing else. A metric labelled by success or
/// failure, or a retry queue whose depth an attacker could probe, would
/// reintroduce the oracle one layer out.
pub struct SmtpMail {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
    reply_to: Option<Mailbox>,
    /// Where the link points. The reader has to arrive back at this site.
    origin: String,
}

impl std::fmt::Debug for SmtpMail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SmtpMail")
            .field("from", &self.from.to_string())
            .field("origin", &self.origin)
            .finish_non_exhaustive()
    }
}

impl SmtpMail {
    /// Builds a transport from the configuration, with the password already
    /// resolved by [`MailConfig::password`].
    pub fn new(
        config: &MailConfig,
        password: Option<Zeroizing<String>>,
        origin: &str,
    ) -> Result<Self, Unusable> {
        let from: Mailbox = config
            .from
            .parse()
            .map_err(|_| Unusable::Address("mail.from", config.from.clone()))?;
        let reply_to = config
            .reply_to
            .as_deref()
            .map(|address| {
                address
                    .parse::<Mailbox>()
                    .map_err(|_| Unusable::Address("mail.replyTo", address.to_owned()))
            })
            .transpose()?;

        let host = config.smtp.host.as_str();
        let builder = match config.smtp.security {
            Security::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(host)
                .map_err(|error| Unusable::Transport(error.to_string()))?,
            Security::Starttls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)
                .map_err(|error| Unusable::Transport(error.to_string()))?,
            // The schema's own description limits this to a loopback or
            // private-network relay. Nothing here can enforce that, so it is
            // said where an operator chooses it rather than checked here.
            Security::None => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host),
        };

        let builder = builder.port(config.smtp.port());
        let builder = match (&config.smtp.username, password) {
            (Some(username), Some(password)) => {
                builder.credentials(Credentials::new(username.clone(), password.to_string()))
            }
            // A username with no password, or a password with no username, is
            // half a credential: sending it would authenticate as nobody and
            // the failure would look like a server problem.
            (Some(_), None) | (None, Some(_)) => return Err(Unusable::HalfACredential),
            (None, None) => builder,
        };

        Ok(Self {
            transport: builder.build(),
            from,
            reply_to,
            origin: origin.trim_end_matches('/').to_owned(),
        })
    }

    /// The message. Separate from sending so a test can read what would go out
    /// without a server to send it to.
    pub fn message(&self, address: &str, token: &str) -> Result<Message, Unusable> {
        let link = self.link(token);
        self.compose(
            address,
            "Your sign-in link",
            &format!(
                "Open this link to sign in:\n\n{link}\n\n\
                 It is good for 15 minutes and only in the browser that asked \
                 for it — if your mail app opens links somewhere else, the page \
                 will say so and offer you a new one.\n\n\
                 If you did not ask to sign in, nothing has happened and you \
                 can ignore this.\n"
            ),
        )
    }

    /// One envelope for every message this sender produces, so `from`,
    /// `replyTo` and address validation cannot differ between a sign-in link
    /// and a notification.
    pub fn compose(&self, address: &str, subject: &str, body: &str) -> Result<Message, Unusable> {
        let to: Mailbox = address
            .parse()
            .map_err(|_| Unusable::Address("the recipient's address", address.to_owned()))?;
        let mut builder = Message::builder()
            .from(self.from.clone())
            .to(to)
            .subject(subject);
        if let Some(reply_to) = &self.reply_to {
            builder = builder.reply_to(reply_to.clone());
        }
        builder
            .body(body.to_owned())
            .map_err(|error| Unusable::Message(error.to_string()))
    }

    fn link(&self, token: &str) -> String {
        format!("{}/_liyasa/auth/magic/{token}", self.origin)
    }
}

impl crate::auth::state::Mail for SmtpMail {
    fn send_link(&self, address: &str, token: &str) {
        let message = match self.message(address, token) {
            Ok(message) => message,
            Err(error) => {
                tracing::warn!(target: "liyasa_server", %error, "a sign-in link could not be built");
                return;
            }
        };
        // Spawned rather than awaited: see the type's documentation. The
        // handler must return at the same speed whether or not there was
        // anything to send.
        //
        // `tokio::spawn` panics with no runtime, and this is on a path that
        // reaches user input, so ask rather than assume. In the server there
        // is always one; a caller outside a runtime gets a log line instead of
        // a panicking sign-in endpoint.
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            tracing::warn!(
                target: "liyasa_server",
                "a sign-in link could not be sent: no runtime to send it on"
            );
            return;
        };
        let transport = self.transport.clone();
        runtime.spawn(async move {
            if let Err(error) = transport.send(message).await {
                // Logged and nothing else. The reader is told the same thing
                // either way, by design.
                tracing::warn!(target: "liyasa_server", %error, "a sign-in link could not be sent");
            }
        });
    }

    fn send<'a>(
        &'a self,
        address: &'a str,
        subject: &'a str,
        body: &'a str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), crate::auth::state::Unsent>> + Send + 'a>,
    > {
        Box::pin(async move {
            let message = self.compose(address, subject, body)?;
            self.transport
                .send(message)
                .await
                .map(|_| ())
                .map_err(|error| crate::auth::state::Unsent::Transport(error.to_string()))
        })
    }
}

impl From<Unusable> for crate::auth::state::Unsent {
    fn from(error: Unusable) -> Self {
        use crate::auth::state::Unsent;
        match error {
            Unusable::Address(_, address) => Unsent::Address(address),
            Unusable::Message(detail) => Unsent::Message(detail),
            Unusable::Transport(detail) => Unsent::Transport(detail),
            Unusable::HalfACredential => Unsent::Transport(Unusable::HalfACredential.to_string()),
        }
    }
}

/// Why a configured mail block cannot produce a sender.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Unusable {
    #[error("`{0}` is `{1}`, which is not an email address")]
    Address(&'static str, String),
    #[error("the SMTP transport could not be built: {0}")]
    Transport(String),
    #[error("`mail.smtp` has a username without a password or a password without a username")]
    HalfACredential,
    #[error("the message could not be built: {0}")]
    Message(String),
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
        assert_eq!(error.code, code::E0816);
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

#[cfg(test)]
mod sender_tests {
    use super::*;
    use crate::auth::state::Mail as _;

    fn config(security: Security, username: Option<&str>) -> MailConfig {
        MailConfig {
            from: "Docs <docs@example.com>".to_owned(),
            reply_to: None,
            smtp: SmtpConfig {
                host: "smtp.example.com".to_owned(),
                port: None,
                security,
                username: username.map(str::to_owned),
                password: None,
            },
        }
    }

    fn sender(config: &MailConfig) -> SmtpMail {
        SmtpMail::new(
            config,
            config
                .smtp
                .username
                .as_ref()
                .map(|_| Zeroizing::new("pw".to_owned())),
            "https://docs.example.com",
        )
        .expect("a well-formed configuration builds a sender")
    }

    fn body(message: &Message) -> String {
        String::from_utf8(message.formatted()).expect("the message is UTF-8")
    }

    #[test]
    fn the_link_points_at_this_site_and_carries_the_token() {
        let config = config(Security::Starttls, None);
        let sent = body(
            &sender(&config)
                .message("reader@example.com", "abc123")
                .expect("a valid address builds a message"),
        );
        assert!(
            sent.contains("https://docs.example.com/_liyasa/auth/magic/abc123"),
            "the reader has to be able to click back to this site: {sent}"
        );
    }

    /// The origin is configured, so a trailing slash is one typo away from
    /// `https://docs.example.com//_liyasa/...`, which some proxies redirect
    /// and some reject.
    #[test]
    fn a_trailing_slash_on_the_origin_does_not_double_in_the_link() {
        let config = config(Security::Starttls, None);
        let with_slash = SmtpMail::new(&config, None, "https://docs.example.com/").expect("builds");
        assert!(
            body(
                &with_slash
                    .message("reader@example.com", "t")
                    .expect("builds")
            )
            .contains("https://docs.example.com/_liyasa/auth/magic/t")
        );
    }

    #[test]
    fn a_reply_to_reaches_the_message_when_one_is_configured() {
        let mut config = config(Security::Starttls, None);
        config.reply_to = Some("support@example.com".to_owned());
        let sent = body(
            &sender(&config)
                .message("reader@example.com", "t")
                .expect("builds"),
        );
        assert!(sent.contains("Reply-To: support@example.com"), "{sent}");
    }

    /// The address comes from a request body. A malformed one has to be an
    /// error value rather than a panic: `unwrap` on user input is not
    /// acceptable, and this is user input at its most direct.
    #[test]
    fn a_malformed_reader_address_is_an_error_rather_than_a_panic() {
        let config = config(Security::Starttls, None);
        let outcome = sender(&config).message("not an address", "t");
        assert!(
            matches!(outcome, Err(Unusable::Address(_, _))),
            "{outcome:?}"
        );
    }

    #[test]
    fn a_malformed_from_is_refused_when_the_sender_is_built_rather_than_per_message() {
        let mut config = config(Security::Starttls, None);
        config.from = "docs at example.com".to_owned();
        assert!(matches!(
            SmtpMail::new(&config, None, "https://docs.example.com"),
            Err(Unusable::Address("mail.from", _))
        ));
    }

    /// Half a credential authenticates as nobody, and the relay's refusal
    /// arrives at the far end of a spawned send where nobody sees it.
    #[test]
    fn a_username_without_a_password_is_refused_rather_than_sent_as_half_a_credential() {
        let config = config(Security::Starttls, Some("docs"));
        assert_eq!(
            SmtpMail::new(&config, None, "https://docs.example.com")
                .expect_err("half a credential"),
            Unusable::HalfACredential
        );
    }

    #[test]
    fn a_password_without_a_username_is_refused_the_same_way() {
        let config = config(Security::Starttls, None);
        assert_eq!(
            SmtpMail::new(
                &config,
                Some(Zeroizing::new("pw".to_owned())),
                "https://docs.example.com"
            )
            .expect_err("half a credential"),
            Unusable::HalfACredential
        );
    }

    /// `Debug` on the sender is reachable from `AuthState`'s derived one, and
    /// `AuthState` is printed on a configuration failure.
    #[test]
    fn debug_does_not_print_the_password() {
        let config = config(Security::Starttls, Some("docs"));
        let shown = format!(
            "{:?}",
            SmtpMail::new(
                &config,
                Some(Zeroizing::new("hunter2".to_owned())),
                "https://docs.example.com"
            )
            .expect("builds")
        );
        assert!(!shown.contains("hunter2"), "{shown}");
    }

    #[tokio::test]
    async fn a_notification_carries_the_subject_and_body_it_was_given() {
        let config = config(Security::Starttls, None);
        let sent = body(
            &sender(&config)
                .compose("ops@example.com", "Deployment finished", "docs v3 is live")
                .expect("a valid address composes"),
        );
        assert!(sent.contains("Subject: Deployment finished"), "{sent}");
        assert!(sent.contains("docs v3 is live"), "{sent}");
    }

    /// The envelope is shared with the sign-in link, so `from` and `replyTo`
    /// cannot drift between the two kinds of message.
    #[tokio::test]
    async fn a_notification_and_a_sign_in_link_come_from_the_same_address() {
        let mut config = config(Security::Starttls, None);
        config.reply_to = Some("support@example.com".to_owned());
        let sender = sender(&config);
        let link = body(&sender.message("reader@example.com", "t").expect("builds"));
        let note = body(&sender.compose("ops@example.com", "s", "b").expect("builds"));
        for header in [
            "From: Docs <docs@example.com>",
            "Reply-To: support@example.com",
        ] {
            assert!(link.contains(header), "{link}");
            assert!(note.contains(header), "{note}");
        }
    }

    /// `send` is awaited and reports, which is the whole difference from
    /// `send_link`. A host that cannot resolve proves it waited for an answer
    /// rather than spawning and returning.
    #[tokio::test]
    async fn send_waits_for_the_relay_and_reports_what_happened() {
        use crate::auth::state::Mail as _;
        let mut config = config(Security::Starttls, None);
        config.smtp.host = "smtp.invalid".to_owned();
        let outcome = sender(&config)
            .send("ops@example.com", "Deployment finished", "docs v3 is live")
            .await;
        assert!(
            matches!(outcome, Err(crate::auth::state::Unsent::Transport(_))),
            "{outcome:?}"
        );
    }

    #[tokio::test]
    async fn send_reports_a_malformed_address_rather_than_panicking() {
        use crate::auth::state::Mail as _;
        let config = config(Security::Starttls, None);
        let outcome = sender(&config).send("not an address", "s", "b").await;
        assert!(
            matches!(outcome, Err(crate::auth::state::Unsent::Address(_))),
            "{outcome:?}"
        );
    }

    /// A no-op that reported success is how a caller comes to believe a
    /// notification was delivered.
    #[tokio::test]
    async fn an_unconfigured_sender_reports_failure_rather_than_success() {
        use crate::auth::state::{Mail as _, NoMail, Unsent};
        assert_eq!(
            NoMail.send("ops@example.com", "s", "b").await,
            Err(Unsent::NotConfigured)
        );
    }

    /// AUTH-09: the endpoint answers identically whether or not the address is
    /// known, so the send cannot be awaited in the handler. This asserts the
    /// shape that makes that true — `send_link` returns without a runtime to
    /// spawn onto being required to have *finished* anything — by calling it
    /// against a host that does not resolve and observing that it returns.
    #[tokio::test]
    async fn send_link_returns_without_waiting_for_the_relay() {
        let mut config = config(Security::Starttls, None);
        config.smtp.host = "smtp.invalid".to_owned();
        let sender = sender(&config);
        let started = std::time::Instant::now();
        sender.send_link("reader@example.com", "t");
        assert!(
            started.elapsed() < std::time::Duration::from_millis(100),
            "send_link waited for the relay: {:?}",
            started.elapsed()
        );
    }

    /// The same, for the branch that cannot even build a message. An early
    /// return there is just as much a timing signal as an awaited send: it is
    /// the *fast* path, and the address it is fast for is a malformed one.
    #[tokio::test]
    async fn send_link_swallows_a_malformed_address_rather_than_panicking() {
        let config = config(Security::Starttls, None);
        sender(&config).send_link("not an address", "t");
    }
}
