//! The `auth` section of `liyasa.json` (CFG-96, §19).
//!
//! Deserialized from the raw configuration value rather than from
//! `liyasa_config::SiteConfig`: the server is handed whatever the build wrote
//! and the generated model is regenerated whenever the schema moves, so the
//! narrow view the auth code actually needs is declared here.
//!
//! Every key here exists in `schemas/liyasa.schema.json`. Nothing reads a key
//! that is not in the schema, and `deny_unknown_fields` makes a typo a
//! diagnostic rather than a silently ignored setting.

use std::time::Duration;

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use serde::{Deserialize, Serialize};

/// OWASP's floor, which `auth.password.argon2` may raise and never lower
/// (NFR-12).
pub const ARGON2_MIN_MEMORY_KIB: u32 = 65_536;
pub const ARGON2_MIN_ITERATIONS: u32 = 3;
pub const ARGON2_MIN_PARALLELISM: u32 = 1;
/// NFR-12 fixes both; they are not configurable.
pub const ARGON2_SALT_BYTES: usize = 16;
pub const ARGON2_TAG_BYTES: usize = 32;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// AUTH-01: no auth, and a static export is possible.
    #[default]
    Public,
    Password,
    Jwt,
    Oidc,
    Managed,
}

impl Mode {
    /// Whether a reader needs a session at all. The one case where the server
    /// has no auth code path on a request (AUTH-01).
    pub fn is_public(self) -> bool {
        self == Mode::Public
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Public => "public",
            Mode::Password => "password",
            Mode::Jwt => "jwt",
            Mode::Oidc => "oidc",
            Mode::Managed => "managed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Argon2Params {
    #[serde(default = "default_memory")]
    pub memory_kib: u32,
    #[serde(default = "default_iterations")]
    pub iterations: u32,
    #[serde(default = "default_parallelism")]
    pub parallelism: u32,
}

fn default_memory() -> u32 {
    ARGON2_MIN_MEMORY_KIB
}
fn default_iterations() -> u32 {
    ARGON2_MIN_ITERATIONS
}
fn default_parallelism() -> u32 {
    ARGON2_MIN_PARALLELISM
}

impl Default for Argon2Params {
    fn default() -> Self {
        Self {
            memory_kib: ARGON2_MIN_MEMORY_KIB,
            iterations: ARGON2_MIN_ITERATIONS,
            parallelism: ARGON2_MIN_PARALLELISM,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PasswordConfig {
    #[serde(default)]
    pub argon2: Argon2Params,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JwtConfig {
    /// The `alg` allow list. `none` is rejected whatever this says.
    #[serde(default = "default_algs")]
    pub algs: Vec<String>,
    #[serde(default)]
    pub iss: Option<String>,
    #[serde(default)]
    pub aud: Option<String>,
    #[serde(default)]
    pub jwks_url: Option<String>,
    #[serde(default)]
    pub login_url: Option<String>,
    #[serde(default = "default_groups_claim")]
    pub groups_claim: String,
    #[serde(default)]
    pub region_claim: Option<String>,
    #[serde(default)]
    pub locale_claim: Option<String>,
}

fn default_algs() -> Vec<String> {
    vec!["RS256".to_owned()]
}
fn default_groups_claim() -> String {
    "groups".to_owned()
}

impl Default for JwtConfig {
    fn default() -> Self {
        Self {
            algs: default_algs(),
            iss: None,
            aud: None,
            jwks_url: None,
            login_url: None,
            groups_claim: default_groups_claim(),
            region_claim: None,
            locale_claim: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OidcConfig {
    #[serde(default)]
    pub issuer: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default = "default_scopes")]
    pub scopes: Vec<String>,
    #[serde(default = "default_groups_claim")]
    pub groups_claim: String,
}

fn default_scopes() -> Vec<String> {
    vec![
        "openid".to_owned(),
        "profile".to_owned(),
        "email".to_owned(),
    ]
}

impl Default for OidcConfig {
    fn default() -> Self {
        Self {
            issuer: None,
            client_id: None,
            scopes: default_scopes(),
            groups_claim: default_groups_claim(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedConfig {
    /// `@acme.com` entries: any address in the domain may sign in (AUTH-05).
    #[serde(default)]
    pub allow_domains: Vec<String>,
    #[serde(default = "default_magic_ttl")]
    pub magic_link_ttl: String,
}

fn default_magic_ttl() -> String {
    "15m".to_owned()
}

impl Default for ManagedConfig {
    fn default() -> Self {
        Self {
            allow_domains: Vec::new(),
            magic_link_ttl: default_magic_ttl(),
        }
    }
}

impl ManagedConfig {
    pub fn ttl(&self) -> Duration {
        parse_duration(&self.magic_link_ttl).unwrap_or(Duration::from_secs(900))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionConfig {
    #[serde(default = "default_max_age")]
    pub max_age: String,
    #[serde(default = "default_idle")]
    pub idle_timeout: String,
    #[serde(default = "default_cookie_name")]
    pub cookie_name: String,
}

fn default_max_age() -> String {
    "30d".to_owned()
}
fn default_idle() -> String {
    "7d".to_owned()
}
fn default_cookie_name() -> String {
    "liyasa_session".to_owned()
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            max_age: default_max_age(),
            idle_timeout: default_idle(),
            cookie_name: default_cookie_name(),
        }
    }
}

impl SessionConfig {
    pub fn max_age(&self) -> Duration {
        parse_duration(&self.max_age).unwrap_or(Duration::from_secs(30 * 86_400))
    }

    pub fn idle_timeout(&self) -> Duration {
        parse_duration(&self.idle_timeout).unwrap_or(Duration::from_secs(7 * 86_400))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LogoutConfig {
    /// Where a reader lands after `POST /_liyasa/auth/logout` (AUTH-22).
    #[serde(default)]
    pub redirect: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PreviewProtection {
    /// AUTH-40: organization login unless the operator says otherwise.
    #[default]
    Org,
    Password,
    Public,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewConfig {
    #[serde(default)]
    pub protection: PreviewProtection,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthConfig {
    #[serde(default)]
    pub mode: Mode,
    #[serde(default)]
    pub password: PasswordConfig,
    #[serde(default)]
    pub jwt: JwtConfig,
    #[serde(default)]
    pub oidc: OidcConfig,
    #[serde(default)]
    pub managed: ManagedConfig,
    #[serde(default)]
    pub session: SessionConfig,
    #[serde(default)]
    pub logout: LogoutConfig,
    #[serde(default)]
    pub preview: PreviewConfig,
}

impl AuthConfig {
    /// Reads the `auth` object out of a whole `liyasa.json` value. A config
    /// with no `auth` section is public, which is the documented default.
    pub fn from_site_config(config: &serde_json::Value) -> Result<Self, Diagnostic> {
        match config.get("auth") {
            None | Some(serde_json::Value::Null) => Ok(Self::default()),
            Some(section) => Self::from_value(section),
        }
    }

    pub fn from_value(section: &serde_json::Value) -> Result<Self, Diagnostic> {
        serde_json::from_value(section.clone()).map_err(|error| {
            Diagnostic::new(
                code::E0803,
                format!("`auth` is not a valid authentication configuration: {error}"),
            )
            .help("every key under `auth` is listed in `schemas/liyasa.schema.json`")
        })
    }

    /// Everything that makes a configuration unusable, as diagnostics rather
    /// than as a refusal to start: an operator sees all of them at once.
    pub fn check(&self) -> Diagnostics {
        let mut diagnostics = Diagnostics::default();
        let argon2 = &self.password.argon2;
        let mut floor = |name: &str, given: u32, least: u32| {
            if given < least {
                diagnostics.push(
                    Diagnostic::new(
                        code::E0803,
                        format!(
                            "`auth.password.argon2.{name}` is {given}, below the {least} \
                             floor NFR-12 sets"
                        ),
                    )
                    .help(format!("raise it to {least} or more, or remove the key")),
                );
            }
        };
        floor("memoryKiB", argon2.memory_kib, ARGON2_MIN_MEMORY_KIB);
        floor("iterations", argon2.iterations, ARGON2_MIN_ITERATIONS);
        floor("parallelism", argon2.parallelism, ARGON2_MIN_PARALLELISM);

        for alg in &self.jwt.algs {
            if alg.eq_ignore_ascii_case("none") {
                diagnostics.push(
                    Diagnostic::new(
                        code::E0803,
                        "`auth.jwt.algs` lists `none`, which is never an acceptable signature",
                    )
                    .help("name the algorithms your identity provider signs with, such as `RS256`"),
                );
            } else if !SUPPORTED_ALGS.contains(&alg.as_str()) {
                diagnostics.push(
                    Diagnostic::new(
                        code::E0803,
                        format!("`auth.jwt.algs` names `{alg}`, which this server cannot verify"),
                    )
                    .help(format!("supported: {}", SUPPORTED_ALGS.join(", "))),
                );
            }
        }

        for (key, value) in [
            ("auth.session.maxAge", &self.session.max_age),
            ("auth.session.idleTimeout", &self.session.idle_timeout),
            ("auth.managed.magicLinkTtl", &self.managed.magic_link_ttl),
        ] {
            if parse_duration(value).is_none() {
                diagnostics.push(
                    Diagnostic::new(
                        code::E0803,
                        format!("`{key}` is `{value}`, which is not a duration"),
                    )
                    .help("a duration is digits and one of `ms`, `s`, `m`, `h`, `d`: `15m`, `30d`"),
                );
            }
        }

        if self.mode == Mode::Jwt && self.jwt.jwks_url.is_none() {
            diagnostics.push(
                Diagnostic::new(
                    code::E0803,
                    "`auth.mode` is `jwt` and `auth.jwt.jwksUrl` is not set",
                )
                .help("JWT mode verifies signatures against a JWKS; name the URL to fetch it from"),
            );
        }
        if self.mode == Mode::Oidc {
            for (key, value) in [
                ("auth.oidc.issuer", &self.oidc.issuer),
                ("auth.oidc.clientId", &self.oidc.client_id),
            ] {
                if value.is_none() {
                    diagnostics.push(
                        Diagnostic::new(
                            code::E0803,
                            format!("`auth.mode` is `oidc` and `{key}` is not set"),
                        )
                        .help(
                            "OIDC discovery needs the issuer and the client this site registers as",
                        ),
                    );
                }
            }
        }
        diagnostics
    }

    /// The effective Argon2id parameters: never below the NFR-12 floor, so a
    /// running server is safe even where the diagnostic above was demoted.
    pub fn argon2(&self) -> Argon2Params {
        let given = self.password.argon2;
        Argon2Params {
            memory_kib: given.memory_kib.max(ARGON2_MIN_MEMORY_KIB),
            iterations: given.iterations.max(ARGON2_MIN_ITERATIONS),
            parallelism: given.parallelism.max(ARGON2_MIN_PARALLELISM),
        }
    }
}

/// What `jsonwebtoken` can verify and AUTH-03 names. `none` is absent by
/// construction, not by a check that could be forgotten.
pub const SUPPORTED_ALGS: &[&str] = &[
    "HS256", "HS384", "HS512", "RS256", "RS384", "RS512", "ES256", "ES384", "PS256", "PS384",
    "PS512", "EdDSA",
];

/// The schema's duration: digits then one of `ms`, `s`, `m`, `h`, `d`.
pub fn parse_duration(text: &str) -> Option<Duration> {
    let digits = text.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    let unit = &text[digits.len()..];
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: u64 = digits.parse().ok()?;
    let millis = match unit {
        "ms" => n,
        "s" => n.checked_mul(1_000)?,
        "m" => n.checked_mul(60_000)?,
        "h" => n.checked_mul(3_600_000)?,
        "d" => n.checked_mul(86_400_000)?,
        _ => return None,
    };
    Some(Duration::from_millis(millis))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_config_with_no_auth_section_is_public() {
        let config = AuthConfig::from_site_config(&serde_json::json!({ "name": "docs" }))
            .expect("a default");
        assert_eq!(config.mode, Mode::Public);
        assert!(config.mode.is_public());
        assert!(config.check().is_empty());
    }

    #[test]
    fn the_documented_defaults_are_the_ones_cfg_96_lists() {
        let config = AuthConfig::default();
        assert_eq!(config.password.argon2.memory_kib, 65_536);
        assert_eq!(config.password.argon2.iterations, 3);
        assert_eq!(config.password.argon2.parallelism, 1);
        assert_eq!(config.jwt.algs, ["RS256"]);
        assert_eq!(config.managed.magic_link_ttl, "15m");
        assert_eq!(config.session.max_age, "30d");
        assert_eq!(config.session.idle_timeout, "7d");
        assert_eq!(config.preview.protection, PreviewProtection::Org);
    }

    #[test]
    fn an_unknown_key_under_auth_is_a_diagnostic_rather_than_a_silent_default() {
        let error = AuthConfig::from_site_config(&serde_json::json!({
            "auth": { "mode": "password", "passwrod": {} }
        }))
        .expect_err("a typo is refused");
        assert_eq!(error.code, code::E0803);
    }

    #[test]
    fn argon2_parameters_below_the_owasp_floor_are_refused_and_then_raised() {
        let config = AuthConfig::from_site_config(&serde_json::json!({
            "auth": { "mode": "password", "password": { "argon2": { "memoryKiB": 8, "iterations": 1 } } }
        }))
        .expect("a parse");
        let diagnostics = config.check();
        assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
        assert!(diagnostics.iter().all(|d| d.code == code::E0803));
        // Whatever policy does with the diagnostic, the running server never
        // hashes below the floor.
        assert_eq!(config.argon2().memory_kib, ARGON2_MIN_MEMORY_KIB);
        assert_eq!(config.argon2().iterations, ARGON2_MIN_ITERATIONS);
    }

    #[test]
    fn an_operator_may_raise_the_argon2_parameters() {
        let config = AuthConfig::from_site_config(&serde_json::json!({
            "auth": { "password": { "argon2": { "memoryKiB": 131072, "iterations": 4, "parallelism": 2 } } }
        }))
        .expect("a parse");
        assert!(config.check().is_empty());
        assert_eq!(config.argon2().memory_kib, 131_072);
        assert_eq!(config.argon2().iterations, 4);
        assert_eq!(config.argon2().parallelism, 2);
    }

    #[test]
    fn the_alg_none_is_refused_wherever_it_is_written() {
        for spelling in ["none", "None", "NONE"] {
            let config = AuthConfig::from_site_config(&serde_json::json!({
                "auth": { "mode": "jwt", "jwt": { "algs": [spelling], "jwksUrl": "https://idp.example/jwks" } }
            }))
            .expect("a parse");
            let diagnostics = config.check();
            assert_eq!(diagnostics.len(), 1, "{spelling}: {diagnostics:?}");
            assert!(
                diagnostics.as_slice()[0]
                    .message
                    .contains("never an acceptable")
            );
        }
    }

    #[test]
    fn jwt_mode_without_a_jwks_url_cannot_verify_anything() {
        let config = AuthConfig::from_site_config(&serde_json::json!({
            "auth": { "mode": "jwt" }
        }))
        .expect("a parse");
        assert_eq!(config.check().len(), 1);
    }

    #[test]
    fn a_duration_is_digits_and_one_of_the_five_units() {
        assert_eq!(parse_duration("500ms"), Some(Duration::from_millis(500)));
        assert_eq!(parse_duration("30s"), Some(Duration::from_secs(30)));
        assert_eq!(parse_duration("15m"), Some(Duration::from_secs(900)));
        assert_eq!(parse_duration("2h"), Some(Duration::from_secs(7_200)));
        assert_eq!(
            parse_duration("180d"),
            Some(Duration::from_secs(15_552_000))
        );
        for bad in ["", "d", "15", "15w", "-1s", "1.5h", "fifteen m", "15 m"] {
            assert_eq!(parse_duration(bad), None, "{bad}");
        }
    }

    #[test]
    fn an_unparseable_session_duration_is_a_diagnostic_and_then_the_default() {
        let config = AuthConfig::from_site_config(&serde_json::json!({
            "auth": { "session": { "maxAge": "forever" } }
        }))
        .expect("a parse");
        assert_eq!(config.check().len(), 1);
        assert_eq!(config.session.max_age(), Duration::from_secs(30 * 86_400));
    }
}
