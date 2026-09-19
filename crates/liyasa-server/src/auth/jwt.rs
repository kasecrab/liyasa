//! Strict JWT verification (AUTH-03, AUTH-09).
//!
//! Every rule the requirement names is checked here and none is optional:
//!
//! * the token is at most 8 KB;
//! * `alg` is in the configured allow list, and `none` is rejected whatever
//!   the list says;
//! * `kid` resolves in the cached JWKS (see [`super::jwks`] for the refresh
//!   throttle and the negative cache);
//! * the signature verifies;
//! * `iss` and `aud` match the configured values;
//! * `exp` and `nbf` are enforced with 60 s of skew.
//!
//! The order matters. Everything cheap and unconditional runs before anything
//! that costs a signature verification or a network fetch, so an attacker
//! cannot spend this server's time with a token that was never going to be
//! accepted.
//!
//! Signatures are verified with `ring` rather than with the crate PRD §6.2.1
//! names; RFC 1500 says why.

use std::collections::BTreeSet;

use ring::hmac;
use ring::signature::{self, UnparsedPublicKey};
use serde::{Deserialize, Serialize};

use crate::auth::base64url;
use crate::auth::config::JwtConfig;
use crate::auth::jwks::{Jwk, Jwks, Resolution, Source};
use crate::auth::session::Principal;

/// AUTH-03. A token larger than this is refused before it is parsed.
pub const MAX_TOKEN_BYTES: usize = 8 * 1024;
/// AUTH-03's clock skew allowance, applied to `exp` and `nbf` alike.
pub const SKEW_SECS: i64 = 60;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    #[serde(default)]
    pub alg: String,
    #[serde(default)]
    pub kid: Option<String>,
    #[serde(default)]
    pub typ: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invalid {
    TooLarge(usize),
    Malformed(&'static str),
    /// `alg: none` — never accepted, and reported separately from an
    /// algorithm that is merely not allowed, because it is not a
    /// configuration mistake.
    AlgNone,
    AlgNotAllowed(String),
    NoKid,
    UnknownKid(String),
    UnusableKey(&'static str),
    BadSignature,
    WrongIssuer,
    WrongAudience,
    Expired,
    NotYetValid,
    NoSubject,
}

impl Invalid {
    /// What the reader is told. Deliberately the same sentence for every
    /// variant: the detail is for the operator's log, not for whoever is
    /// holding the token.
    pub fn reader_message(&self) -> &'static str {
        "this token is not accepted; sign in again"
    }

    pub fn detail(&self) -> String {
        match self {
            Invalid::TooLarge(n) => {
                format!("the token is {n} bytes, over the {MAX_TOKEN_BYTES} limit")
            }
            Invalid::Malformed(what) => format!("the token is malformed: {what}"),
            Invalid::AlgNone => "the token is unsigned (`alg: none`)".to_owned(),
            Invalid::AlgNotAllowed(alg) => {
                format!("`{alg}` is not in `auth.jwt.algs`")
            }
            Invalid::NoKid => {
                "the token names no `kid` and the JWKS has more than one key".to_owned()
            }
            Invalid::UnknownKid(kid) => format!("`kid` `{kid}` is not in the JWKS"),
            Invalid::UnusableKey(why) => format!("the JWKS key cannot verify this token: {why}"),
            Invalid::BadSignature => "the signature does not verify".to_owned(),
            Invalid::WrongIssuer => "`iss` is not the configured issuer".to_owned(),
            Invalid::WrongAudience => "`aud` does not contain the configured audience".to_owned(),
            Invalid::Expired => "`exp` has passed".to_owned(),
            Invalid::NotYetValid => "`nbf` has not arrived".to_owned(),
            Invalid::NoSubject => "the token carries no `sub`".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verified {
    pub header: Header,
    pub claims: serde_json::Value,
}

impl Verified {
    /// The reader the token describes, as every group check sees them
    /// (AUTH-03: subject, groups, region, locale, and arbitrary user data).
    pub fn principal(&self, config: &JwtConfig) -> Principal {
        let string = |key: &str| {
            self.claims
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        };
        let groups = match self.claims.get(&config.groups_claim) {
            Some(serde_json::Value::Array(items)) => items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_owned)
                .collect(),
            // A provider that sends a space- or comma-separated string is
            // common enough that refusing it would be a support burden.
            Some(serde_json::Value::String(text)) => text
                .split([',', ' '])
                .map(str::trim)
                .filter(|group| !group.is_empty())
                .map(str::to_owned)
                .collect(),
            _ => BTreeSet::new(),
        };
        let reserved = [
            "iss",
            "aud",
            "exp",
            "nbf",
            "iat",
            "jti",
            "sub",
            config.groups_claim.as_str(),
        ];
        let data = self
            .claims
            .as_object()
            .map(|fields| {
                fields
                    .iter()
                    .filter(|(key, _)| !reserved.contains(&key.as_str()))
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect()
            })
            .unwrap_or_default();

        Principal {
            subject: string("sub").unwrap_or_default(),
            groups,
            region: config.region_claim.as_ref().and_then(|c| string(c)),
            locale: config.locale_claim.as_ref().and_then(|c| string(c)),
            data,
            grant: None,
            shared: false,
            role: crate::auth::roles::Role::Reader,
            via: "jwt".to_owned(),
        }
    }
}

/// Verifies a token end to end. `now` is seconds since the Unix epoch.
pub async fn verify(
    token: &str,
    config: &JwtConfig,
    jwks: &Jwks,
    source: &dyn Source,
    now: i64,
) -> Result<Verified, Invalid> {
    // Length first: nothing here should be parsed before it is bounded.
    if token.len() > MAX_TOKEN_BYTES {
        return Err(Invalid::TooLarge(token.len()));
    }
    let mut parts = token.split('.');
    let (Some(header_b64), Some(payload_b64), Some(signature_b64), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(Invalid::Malformed("a JWS has exactly three parts"));
    };

    let header: Header = decode_json(header_b64).ok_or(Invalid::Malformed("the header"))?;
    if header.alg.eq_ignore_ascii_case("none") {
        return Err(Invalid::AlgNone);
    }
    if !config
        .algs
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(&header.alg))
    {
        return Err(Invalid::AlgNotAllowed(header.alg.clone()));
    }

    // The claims are read before the signature so that a token that was never
    // for this site — wrong issuer, long expired — costs no verification. It
    // is decoded, not trusted: nothing here is acted on, and every one of
    // these checks is repeated against the same values after the signature
    // holds.
    let claims: serde_json::Value =
        decode_json(payload_b64).ok_or(Invalid::Malformed("the claims"))?;
    check_claims(&claims, config, now)?;

    let kid = header.kid.clone().unwrap_or_default();
    let key = match jwks.resolve(&kid, source).await {
        Resolution::Found(key) => *key,
        Resolution::Unknown => {
            return Err(match kid.is_empty() {
                true => Invalid::NoKid,
                false => Invalid::UnknownKid(kid),
            });
        }
    };

    let signature = base64url::decode(signature_b64).ok_or(Invalid::Malformed("the signature"))?;
    let signed = format!("{header_b64}.{payload_b64}");
    check_signature(&header.alg, &key, signed.as_bytes(), &signature)?;

    // Re-checked against the verified bytes. The earlier pass was a filter;
    // this one is the decision.
    check_claims(&claims, config, now)?;
    if claims
        .get("sub")
        .and_then(serde_json::Value::as_str)
        .is_none_or(str::is_empty)
    {
        return Err(Invalid::NoSubject);
    }
    Ok(Verified { header, claims })
}

fn decode_json<T: serde::de::DeserializeOwned>(part: &str) -> Option<T> {
    serde_json::from_slice(&base64url::decode(part)?).ok()
}

fn check_claims(claims: &serde_json::Value, config: &JwtConfig, now: i64) -> Result<(), Invalid> {
    if let Some(expected) = &config.iss {
        let found = claims.get("iss").and_then(serde_json::Value::as_str);
        if found != Some(expected.as_str()) {
            return Err(Invalid::WrongIssuer);
        }
    }
    if let Some(expected) = &config.aud {
        let matches = match claims.get("aud") {
            Some(serde_json::Value::String(one)) => one == expected,
            Some(serde_json::Value::Array(many)) => many
                .iter()
                .filter_map(serde_json::Value::as_str)
                .any(|aud| aud == expected),
            _ => false,
        };
        if !matches {
            return Err(Invalid::WrongAudience);
        }
    }
    if let Some(exp) = claims.get("exp").and_then(serde_json::Value::as_i64)
        && now - SKEW_SECS >= exp
    {
        return Err(Invalid::Expired);
    }
    if let Some(nbf) = claims.get("nbf").and_then(serde_json::Value::as_i64)
        && now + SKEW_SECS < nbf
    {
        return Err(Invalid::NotYetValid);
    }
    Ok(())
}

fn check_signature(alg: &str, key: &Jwk, signed: &[u8], signature: &[u8]) -> Result<(), Invalid> {
    let component = |value: &Option<String>, what: &'static str| {
        value
            .as_deref()
            .and_then(base64url::decode)
            .ok_or(Invalid::UnusableKey(what))
    };

    match alg.to_ascii_uppercase().as_str() {
        "HS256" | "HS384" | "HS512" => {
            let secret = component(&key.k, "the `k` member is missing or not base64url")?;
            let algorithm = match alg {
                "HS384" => hmac::HMAC_SHA384,
                "HS512" => hmac::HMAC_SHA512,
                _ => hmac::HMAC_SHA256,
            };
            hmac::verify(&hmac::Key::new(algorithm, &secret), signed, signature)
                .map_err(|_| Invalid::BadSignature)
        }
        "RS256" | "RS384" | "RS512" | "PS256" | "PS384" | "PS512" => {
            let n = component(&key.n, "the `n` member is missing or not base64url")?;
            let e = component(&key.e, "the `e` member is missing or not base64url")?;
            let public = signature::RsaPublicKeyComponents { n: &n, e: &e };
            let algorithm: &'static signature::RsaParameters = match alg {
                "RS384" => &signature::RSA_PKCS1_2048_8192_SHA384,
                "RS512" => &signature::RSA_PKCS1_2048_8192_SHA512,
                "PS256" => &signature::RSA_PSS_2048_8192_SHA256,
                "PS384" => &signature::RSA_PSS_2048_8192_SHA384,
                "PS512" => &signature::RSA_PSS_2048_8192_SHA512,
                _ => &signature::RSA_PKCS1_2048_8192_SHA256,
            };
            public
                .verify(algorithm, signed, signature)
                .map_err(|_| Invalid::BadSignature)
        }
        "ES256" | "ES384" => {
            let x = component(&key.x, "the `x` member is missing or not base64url")?;
            let y = component(&key.y, "the `y` member is missing or not base64url")?;
            // The uncompressed point, which is what a JWK's `x` and `y` are.
            let mut point = Vec::with_capacity(1 + x.len() + y.len());
            point.push(0x04);
            point.extend_from_slice(&x);
            point.extend_from_slice(&y);
            let algorithm: &'static signature::EcdsaVerificationAlgorithm = match alg {
                "ES384" => &signature::ECDSA_P384_SHA384_FIXED,
                _ => &signature::ECDSA_P256_SHA256_FIXED,
            };
            UnparsedPublicKey::new(algorithm, point)
                .verify(signed, signature)
                .map_err(|_| Invalid::BadSignature)
        }
        "EDDSA" => {
            let x = component(&key.x, "the `x` member is missing or not base64url")?;
            UnparsedPublicKey::new(&signature::ED25519, x)
                .verify(signed, signature)
                .map_err(|_| Invalid::BadSignature)
        }
        // Unreachable through `verify`, which checks the allow list first, and
        // the allow list is checked against `SUPPORTED_ALGS` at startup.
        _ => Err(Invalid::AlgNotAllowed(alg.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::clock::Clock;
    use crate::auth::jwks::{JwkSet, NoSource};

    const SECRET: &[u8] = b"a shared secret long enough for HMAC-SHA256";
    const NOW: i64 = 1_767_225_600;

    fn hs256_jwks() -> Jwks {
        let jwks = Jwks::new(Clock::manual());
        jwks.install(JwkSet {
            keys: vec![Jwk {
                kid: "k1".to_owned(),
                kty: "oct".to_owned(),
                alg: Some("HS256".to_owned()),
                k: Some(base64url::encode(SECRET)),
                ..Jwk::default()
            }],
        });
        jwks
    }

    fn config() -> JwtConfig {
        JwtConfig {
            algs: vec!["HS256".to_owned()],
            iss: Some("https://idp.example".to_owned()),
            aud: Some("https://docs.example.com".to_owned()),
            ..JwtConfig::default()
        }
    }

    /// Mints a token the way the operator's application would.
    fn token(header: serde_json::Value, claims: serde_json::Value) -> String {
        let header_b64 = base64url::encode(header.to_string().as_bytes());
        let claims_b64 = base64url::encode(claims.to_string().as_bytes());
        let signed = format!("{header_b64}.{claims_b64}");
        let tag = hmac::sign(
            &hmac::Key::new(hmac::HMAC_SHA256, SECRET),
            signed.as_bytes(),
        );
        format!("{signed}.{}", base64url::encode(tag.as_ref()))
    }

    fn good_claims() -> serde_json::Value {
        serde_json::json!({
            "iss": "https://idp.example",
            "aud": "https://docs.example.com",
            "sub": "reader-1",
            "exp": NOW + 3_600,
            "nbf": NOW - 10,
            "groups": ["partner", "staff"],
        })
    }

    fn head() -> serde_json::Value {
        serde_json::json!({ "alg": "HS256", "kid": "k1", "typ": "JWT" })
    }

    async fn check(token: &str) -> Result<Verified, Invalid> {
        verify(token, &config(), &hs256_jwks(), &NoSource, NOW).await
    }

    #[tokio::test]
    async fn a_well_formed_token_verifies_and_yields_the_reader() {
        let verified = check(&token(head(), good_claims())).await.expect("a token");
        let principal = verified.principal(&config());
        assert_eq!(principal.subject, "reader-1");
        assert_eq!(
            principal
                .groups
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["partner", "staff"]
        );
        assert_eq!(principal.via, "jwt");
    }

    #[tokio::test]
    async fn a_token_over_eight_kilobytes_is_refused_before_it_is_parsed() {
        let padded = serde_json::json!({
            "iss": "https://idp.example",
            "aud": "https://docs.example.com",
            "sub": "reader-1",
            "exp": NOW + 3_600,
            "filler": "x".repeat(9 * 1024),
        });
        let token = token(head(), padded);
        assert!(token.len() > MAX_TOKEN_BYTES);
        assert!(matches!(check(&token).await, Err(Invalid::TooLarge(_))));
    }

    #[tokio::test]
    async fn an_unsigned_token_is_refused_however_none_is_spelled() {
        for spelling in ["none", "None", "NONE"] {
            let unsigned = format!(
                "{}.{}.",
                base64url::encode(
                    serde_json::json!({ "alg": spelling, "kid": "k1" })
                        .to_string()
                        .as_bytes()
                ),
                base64url::encode(good_claims().to_string().as_bytes())
            );
            assert_eq!(check(&unsigned).await, Err(Invalid::AlgNone), "{spelling}");
        }
    }

    #[tokio::test]
    async fn alg_none_is_refused_even_when_the_configuration_lists_it() {
        // The allow list cannot re-enable it: this is the check that does not
        // depend on configuration being right.
        let mut permissive = config();
        permissive.algs.push("none".to_owned());
        let unsigned = format!(
            "{}.{}.",
            base64url::encode(
                serde_json::json!({ "alg": "none", "kid": "k1" })
                    .to_string()
                    .as_bytes()
            ),
            base64url::encode(good_claims().to_string().as_bytes())
        );
        assert_eq!(
            verify(&unsigned, &permissive, &hs256_jwks(), &NoSource, NOW).await,
            Err(Invalid::AlgNone)
        );
    }

    #[tokio::test]
    async fn an_algorithm_outside_the_allow_list_is_refused() {
        let token = token(
            serde_json::json!({ "alg": "HS512", "kid": "k1" }),
            good_claims(),
        );
        assert_eq!(
            check(&token).await,
            Err(Invalid::AlgNotAllowed("HS512".to_owned()))
        );
    }

    #[tokio::test]
    async fn a_tampered_payload_does_not_verify() {
        let mut claims = good_claims();
        claims["groups"] = serde_json::json!(["admin"]);
        let honest = token(head(), good_claims());
        let forged_payload = base64url::encode(claims.to_string().as_bytes());
        let mut parts = honest.split('.');
        let header = parts.next().unwrap_or_default();
        let _ = parts.next();
        let signature = parts.next().unwrap_or_default();
        let forged = format!("{header}.{forged_payload}.{signature}");
        assert_eq!(check(&forged).await, Err(Invalid::BadSignature));
    }

    #[tokio::test]
    async fn the_wrong_issuer_or_audience_is_refused() {
        let mut claims = good_claims();
        claims["iss"] = serde_json::json!("https://someone-else.example");
        assert_eq!(
            check(&token(head(), claims)).await,
            Err(Invalid::WrongIssuer)
        );

        let mut claims = good_claims();
        claims["aud"] = serde_json::json!("https://another-site.example");
        assert_eq!(
            check(&token(head(), claims)).await,
            Err(Invalid::WrongAudience)
        );

        // An array audience containing ours is fine; one that does not is not.
        let mut claims = good_claims();
        claims["aud"] = serde_json::json!(["https://x.example", "https://docs.example.com"]);
        assert!(check(&token(head(), claims)).await.is_ok());
    }

    #[tokio::test]
    async fn exp_and_nbf_are_enforced_with_sixty_seconds_of_skew() {
        let with = |key: &str, value: i64| {
            let mut claims = good_claims();
            claims[key] = serde_json::json!(value);
            token(head(), claims)
        };
        // Expired 59 seconds ago: still inside the skew.
        assert!(check(&with("exp", NOW - 59)).await.is_ok());
        assert_eq!(
            check(&with("exp", NOW - 60)).await,
            Err(Invalid::Expired),
            "the skew is 60 seconds and not a second more"
        );
        // Valid from 59 seconds in the future: inside the skew.
        assert!(check(&with("nbf", NOW + 59)).await.is_ok());
        assert_eq!(
            check(&with("nbf", NOW + 61)).await,
            Err(Invalid::NotYetValid)
        );
    }

    #[tokio::test]
    async fn a_kid_that_is_not_in_the_jwks_is_refused() {
        let token = token(
            serde_json::json!({ "alg": "HS256", "kid": "rotated-away" }),
            good_claims(),
        );
        assert_eq!(
            check(&token).await,
            Err(Invalid::UnknownKid("rotated-away".to_owned()))
        );
    }

    #[tokio::test]
    async fn a_token_with_no_subject_is_not_a_reader() {
        let mut claims = good_claims();
        claims.as_object_mut().expect("an object").remove("sub");
        assert_eq!(check(&token(head(), claims)).await, Err(Invalid::NoSubject));
    }

    #[tokio::test]
    async fn something_that_is_not_a_jws_is_refused_rather_than_panicking() {
        for junk in ["", ".", "a.b", "a.b.c.d", "....", "not a token"] {
            assert!(
                matches!(
                    check(junk).await,
                    Err(Invalid::Malformed(_)) | Err(Invalid::AlgNone)
                ),
                "{junk}"
            );
        }
    }

    #[tokio::test]
    async fn arbitrary_user_data_reaches_the_principal_and_reserved_claims_do_not() {
        let mut claims = good_claims();
        claims["plan"] = serde_json::json!("enterprise");
        claims["seat"] = serde_json::json!(42);
        let verified = check(&token(head(), claims)).await.expect("a token");
        let principal = verified.principal(&config());
        assert_eq!(
            principal.data.get("plan"),
            Some(&serde_json::json!("enterprise"))
        );
        assert_eq!(principal.data.get("seat"), Some(&serde_json::json!(42)));
        for reserved in ["iss", "aud", "exp", "nbf", "sub", "groups"] {
            assert!(
                !principal.data.contains_key(reserved),
                "`{reserved}` is not user data"
            );
        }
    }

    #[tokio::test]
    async fn a_groups_claim_sent_as_a_string_is_still_a_group_list() {
        let mut claims = good_claims();
        claims["groups"] = serde_json::json!("partner, staff");
        let verified = check(&token(head(), claims)).await.expect("a token");
        assert_eq!(
            verified
                .principal(&config())
                .groups
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["partner", "staff"]
        );
    }

    #[tokio::test]
    async fn the_region_and_locale_claims_are_the_configured_ones() {
        let mut config = config();
        config.region_claim = Some("https://acme.example/region".to_owned());
        config.locale_claim = Some("lang".to_owned());
        let mut claims = good_claims();
        claims["https://acme.example/region"] = serde_json::json!("EU");
        claims["lang"] = serde_json::json!("de");
        let verified = verify(
            &token(head(), claims),
            &config,
            &hs256_jwks(),
            &NoSource,
            NOW,
        )
        .await
        .expect("a token");
        let principal = verified.principal(&config);
        assert_eq!(principal.region.as_deref(), Some("EU"));
        assert_eq!(principal.locale.as_deref(), Some("de"));
    }

    #[tokio::test]
    async fn a_key_missing_the_member_its_algorithm_needs_is_not_a_signature_failure() {
        let jwks = Jwks::new(Clock::manual());
        jwks.install(JwkSet {
            keys: vec![Jwk {
                kid: "k1".to_owned(),
                kty: "oct".to_owned(),
                k: None,
                ..Jwk::default()
            }],
        });
        assert!(matches!(
            verify(
                &token(head(), good_claims()),
                &config(),
                &jwks,
                &NoSource,
                NOW
            )
            .await,
            Err(Invalid::UnusableKey(_))
        ));
    }

    #[test]
    fn every_refusal_tells_the_reader_the_same_thing() {
        let refusals = [
            Invalid::TooLarge(9_000),
            Invalid::AlgNone,
            Invalid::BadSignature,
            Invalid::Expired,
            Invalid::UnknownKid("k9".to_owned()),
        ];
        let messages: BTreeSet<&str> = refusals.iter().map(|r| r.reader_message()).collect();
        assert_eq!(messages.len(), 1, "a refusal must not be an oracle");
        // The operator's detail does distinguish them.
        let details: BTreeSet<String> = refusals.iter().map(Invalid::detail).collect();
        assert_eq!(details.len(), refusals.len());
    }
}
