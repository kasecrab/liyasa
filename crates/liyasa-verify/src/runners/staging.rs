//! The staging target's credential (VER-10).
//!
//! VER-10 says an `http` block run against `staging` uses "a real base URL
//! with credentials from the secret store, only through `liyasa-net`".
//! `verify.http.staging.auth` is `secret:<name>`, never the credential itself:
//! a token written into `liyasa.json` is a token in the repository.
//!
//! The value is resolved here and copied once, into the header of the request
//! that is about to go out. It never reaches a `CheckSpec`, a digest, an
//! excerpt, or a log — the scrubber registers it by name (`needs_secrets`) so
//! that if it does appear in a body it is redacted before the excerpt is cut.

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::net::HttpRequest;
use liyasa_core::verify::SecretSource;

use crate::core::config::StagingTarget;

/// How `verify.http.staging.auth` names a secret.
pub const SECRET_PREFIX: &str = "secret:";

/// The secret `auth` names, if it names one. `Ok(None)` is a staging target
/// with no credential, which is a target that needs none.
pub fn secret_name(staging: &StagingTarget) -> Result<Option<&str>, Diagnostic> {
    let Some(auth) = staging
        .auth
        .as_deref()
        .map(str::trim)
        .filter(|a| !a.is_empty())
    else {
        return Ok(None);
    };
    match auth.strip_prefix(SECRET_PREFIX).map(str::trim) {
        Some(name) if !name.is_empty() => Ok(Some(name)),
        _ => Err(literal()),
    }
}

/// Adds the credential to a request bound for the staging target.
///
/// Returns the secret's name so the caller can record it in
/// `CheckSpec::needs_secrets`; the value itself is not returned, because the
/// only place it belongs is the header this function just wrote.
pub fn authorize(
    request: &mut HttpRequest,
    staging: &StagingTarget,
    secrets: &dyn SecretSource,
) -> Result<Option<String>, Diagnostic> {
    let Some(name) = secret_name(staging)? else {
        return Ok(None);
    };
    let value = secrets.get(name).ok_or_else(|| missing(name))?;
    // A block that set its own Authorization meant that one: a sample showing
    // an expired token must go out with the expired token.
    if !request
        .headers
        .iter()
        .any(|(key, _)| key.eq_ignore_ascii_case("authorization"))
    {
        request
            .headers
            .push(("authorization".to_owned(), scheme(&value)));
    }
    Ok(Some(name.to_owned()))
}

/// A secret that already names its scheme is used verbatim, so a store holding
/// `Basic dXNlcjpwdw==` or `Token abc` is not turned into a bearer token it is
/// not.
fn scheme(value: &str) -> String {
    let leading = value.split_whitespace().next().unwrap_or_default();
    let named = [
        "bearer",
        "basic",
        "digest",
        "token",
        "negotiate",
        "hoba",
        "mutual",
    ]
    .iter()
    .any(|s| leading.eq_ignore_ascii_case(s));
    if named && value.split_whitespace().count() > 1 {
        value.to_owned()
    } else {
        format!("Bearer {value}")
    }
}

fn literal() -> Diagnostic {
    Diagnostic::new(
        code::E0635,
        "`verify.http.staging.auth` is not a secret reference",
    )
    .help("write `secret:<name>`; a credential in `liyasa.json` is a credential in the repository")
}

/// `E0635` is the config-reading code, and a well-formed `auth` naming a
/// secret the store does not have is not a config Liyasa could not read. WP-13
/// has claimed `E0637` for it and maps this at their call site until that row
/// is on main; the mapping belongs here once it is.
fn missing(name: &str) -> Diagnostic {
    Diagnostic::new(
        code::E0635,
        format!(
            "`verify.http.staging.auth` names the secret `{name}` and the store has no such secret"
        ),
    )
    .help("add it to the secret store, or clear `verify.http.staging.auth`")
}

#[cfg(test)]
mod tests;
