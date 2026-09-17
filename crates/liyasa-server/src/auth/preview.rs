//! Preview protection (AUTH-40).
//!
//! A preview is a build of a branch that is not production. It is linked from
//! a pull request, which means the link reaches CI logs, notification emails
//! and anyone with read access to the repository — so the default is that a
//! preview needs the organization's login, and serving one publicly is
//! something an operator has to write down.

use crate::auth::config::{AuthConfig, PreviewProtection};
use crate::auth::groups::{Decision, SiteDefault};
use crate::auth::session::Principal;

/// The environment name production is served under. Everything else is a
/// preview as far as AUTH-40 is concerned.
pub const PRODUCTION: &str = "production";

pub fn is_preview(env: &str) -> bool {
    env != PRODUCTION
}

/// What protects this environment, given the site's auth mode.
///
/// `auth.preview.protection` decides, but `org` on a site with no login
/// configured has nothing to fall back on except a password, and if there is
/// no password either the preview cannot be protected at all. That last case
/// is reported rather than silently served.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protection {
    /// Needs a session from the configured flow.
    Organization,
    /// Needs the environment's shared password.
    Password,
    /// Explicitly public.
    Public,
    /// `org` was asked for and the site has no login flow configured.
    Unprotectable,
}

pub fn protection(config: &AuthConfig, password_set: bool) -> Protection {
    match config.preview.protection {
        PreviewProtection::Public => Protection::Public,
        PreviewProtection::Password => match password_set {
            true => Protection::Password,
            false => Protection::Unprotectable,
        },
        PreviewProtection::Org => match config.mode {
            crate::auth::config::Mode::Public => match password_set {
                // A public site still gets a protected preview, because the
                // preview is not the public site.
                true => Protection::Password,
                false => Protection::Unprotectable,
            },
            _ => Protection::Organization,
        },
    }
}

/// The access decision for a request against an environment, before the
/// page's own groups are considered.
pub fn decide(
    env: &str,
    config: &AuthConfig,
    password_set: bool,
    reader: Option<&Principal>,
) -> Decision {
    if !is_preview(env) {
        return Decision::Allow;
    }
    match protection(config, password_set) {
        Protection::Public => Decision::Allow,
        Protection::Organization | Protection::Password => match reader {
            Some(_) => Decision::Allow,
            None => Decision::SignIn,
        },
        // Nothing can satisfy it, so nothing does. Answering `SignIn` would
        // send the reader to a flow that does not exist.
        Protection::Unprotectable => Decision::Deny,
    }
}

/// The site default a preview is served under: a preview is private unless it
/// was made public on purpose, whatever the production site is.
pub fn site_default(env: &str, config: &AuthConfig, password_set: bool) -> SiteDefault {
    match is_preview(env) && protection(config, password_set) != Protection::Public {
        true => SiteDefault::Private,
        false => match config.mode.is_public() {
            true => SiteDefault::Public,
            false => SiteDefault::Private,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::config::Mode;

    fn config(mode: Mode, protection: PreviewProtection) -> AuthConfig {
        let mut config = AuthConfig {
            mode,
            ..AuthConfig::default()
        };
        config.preview.protection = protection;
        config
    }

    fn reader() -> Principal {
        Principal::new("reader-1")
    }

    #[test]
    fn production_is_not_a_preview() {
        assert!(!is_preview("production"));
        assert!(is_preview("preview"));
        assert!(is_preview("pr-42"));
        assert!(is_preview("staging"));
    }

    #[test]
    fn the_default_is_organization_login() {
        assert_eq!(
            AuthConfig::default().preview.protection,
            PreviewProtection::Org
        );
        let config = config(Mode::Oidc, PreviewProtection::Org);
        assert_eq!(protection(&config, false), Protection::Organization);
        assert_eq!(decide("pr-42", &config, false, None), Decision::SignIn);
        assert_eq!(
            decide("pr-42", &config, false, Some(&reader())),
            Decision::Allow
        );
    }

    #[test]
    fn a_public_production_site_still_gets_a_protected_preview() {
        let config = config(Mode::Public, PreviewProtection::Org);
        assert_eq!(protection(&config, true), Protection::Password);
        assert_eq!(decide("pr-42", &config, true, None), Decision::SignIn);
        assert_eq!(
            decide("production", &config, true, None),
            Decision::Allow,
            "production is unaffected"
        );
    }

    #[test]
    fn a_public_preview_is_an_explicit_setting() {
        let config = config(Mode::Oidc, PreviewProtection::Public);
        assert_eq!(protection(&config, false), Protection::Public);
        assert_eq!(decide("pr-42", &config, false, None), Decision::Allow);
    }

    #[test]
    fn a_preview_that_cannot_be_protected_is_not_served_instead() {
        // `org` with no login flow and no password: there is nothing to sign
        // in to, so serving it would be serving it publicly.
        let config = config(Mode::Public, PreviewProtection::Org);
        assert_eq!(protection(&config, false), Protection::Unprotectable);
        assert_eq!(decide("pr-42", &config, false, None), Decision::Deny);
        assert_eq!(
            decide("pr-42", &config, false, Some(&reader())),
            Decision::Deny,
            "a session cannot satisfy a protection that does not exist"
        );
    }

    #[test]
    fn password_protection_needs_a_password_to_be_set() {
        let config = config(Mode::Public, PreviewProtection::Password);
        assert_eq!(protection(&config, true), Protection::Password);
        assert_eq!(protection(&config, false), Protection::Unprotectable);
    }

    #[test]
    fn a_preview_is_private_by_default_whatever_production_is() {
        let public = config(Mode::Public, PreviewProtection::Org);
        assert_eq!(site_default("pr-42", &public, true), SiteDefault::Private);
        assert_eq!(
            site_default("production", &public, true),
            SiteDefault::Public
        );

        let explicitly_public = config(Mode::Public, PreviewProtection::Public);
        assert_eq!(
            site_default("pr-42", &explicitly_public, true),
            SiteDefault::Public
        );
    }
}
