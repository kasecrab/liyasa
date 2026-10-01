//! The instance's own mail sender (VER-77, and whatever else needs to tell a
//! person something).
//!
//! `auth::mount` already builds one of these for sign-in links, and this is
//! deliberately not that one. A public site returns from `contribute` before
//! the mailer is built — `auth.mode` public means there is nothing to sign in
//! to — so a docs site with no login and a `mail` block configured would have
//! no sender at all, and review reminders have nothing to do with signing in.
//!
//! The two are built from the same `mail` block and must stay that way. If
//! this one starts reading a key the auth one does not, a site's reminders and
//! its sign-in links would come from different addresses with no error.

use std::sync::Arc;

use crate::auth::mail::{MailConfig, SmtpMail};
use crate::auth::state::Mail;

/// A sender for this instance, or `None` when the site configures none.
///
/// An absent `mail` block is not an error: most sites have one, and a site
/// without one is told per attempt rather than at startup.
pub fn open(
    site_config: &serde_json::Value,
    store: Option<&liyasa_store::SqliteStore>,
) -> Option<Arc<dyn Mail>> {
    let config = match MailConfig::from_site_config(site_config) {
        Ok(Some(config)) => config,
        Ok(None) => return None,
        Err(diagnostic) => {
            tracing::warn!(
                target: "liyasa_server",
                code = diagnostic.code.as_str(),
                "{}",
                diagnostic.message
            );
            return None;
        }
    };

    // A reminder names pages, and a link in an email cannot be relative: there
    // is no page for the mail client to resolve it against. `auth::mount`
    // raises E0816 for the same condition when auth is configured; this does
    // not, because two diagnostics for one missing key reads as two problems.
    let Some(origin) = canonical_origin(site_config) else {
        tracing::warn!(
            target: "liyasa_server",
            "`mail` is configured and `seo.canonicalOrigin` is not, so a message \
             would have nowhere to point; nothing will be sent"
        );
        return None;
    };

    let secrets =
        store.map(|store| store.secrets_typed() as &dyn liyasa_core::verify::SecretSource);
    let password = match config.password(secrets) {
        Ok(password) => password,
        Err(error) => {
            tracing::warn!(target: "liyasa_server", %error, "the mail password did not resolve");
            return None;
        }
    };
    match SmtpMail::new(&config, password, &origin) {
        Ok(mail) => Some(Arc::new(mail)),
        Err(error) => {
            tracing::warn!(target: "liyasa_server", %error, "the mail transport is unusable");
            None
        }
    }
}

fn canonical_origin(site_config: &serde_json::Value) -> Option<String> {
    site_config
        .get("seo")
        .and_then(|seo| seo.get("canonicalOrigin"))
        .and_then(serde_json::Value::as_str)
        .map(|origin| origin.trim_end_matches('/').to_owned())
}
