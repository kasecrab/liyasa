//! How far a source's word goes, and what its transport must satisfy (VER-26).
//!
//! Two separate claims live here. [`trust_of`] is about the *content*: a value
//! an operator wrote is not a value a vendor's API returned, and the difference
//! decides whether the value is Markdown-escaped on the way into a page (CM-20)
//! and how the Truth dashboard badges it. [`TransportPolicy`] is about getting
//! it: a fact fetched over plain HTTP is a fact anyone on the path can choose,
//! whoever wrote the URL.

use liyasa_core::ai::TrustLevel;
use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::net::{HostPattern, HostSet, Url};
use liyasa_core::verify::SourceKind;

use super::spec::{SourceSpec, kind_name};

/// VER-26's table. `openapi` is the one kind whose trust depends on where the
/// document came from rather than on the kind alone: fetched over the network
/// it is external, and read from the repository it is a repository file.
pub fn trust_of(spec: &SourceSpec) -> TrustLevel {
    match spec.kind {
        SourceKind::File | SourceKind::Manual => TrustLevel::Operator,
        SourceKind::Repo => TrustLevel::Member,
        SourceKind::OpenApi if spec.url.is_none() => TrustLevel::Member,
        _ => TrustLevel::External,
    }
}

/// A value below `operator` is escaped when it is interpolated and is listed as
/// an untrusted input at the "content to renderer" boundary (§30.2.1).
pub fn needs_escaping(trust: TrustLevel) -> bool {
    trust > TrustLevel::Operator
}

/// Which transports a source may use (VER-26).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportPolicy {
    /// `network.allowInsecureHosts`. The default is `localhost` alone.
    pub allow_insecure_hosts: HostSet,
}

impl Default for TransportPolicy {
    fn default() -> Self {
        Self {
            allow_insecure_hosts: HostSet(vec![HostPattern::Exact("localhost".to_owned())]),
        }
    }
}

impl TransportPolicy {
    pub fn with_insecure_hosts(hosts: HostSet) -> Self {
        Self {
            allow_insecure_hosts: hosts,
        }
    }

    /// The URL this source may be fetched from, or why it may not be.
    ///
    /// Certificate validation is never switched off anywhere in this crate, so
    /// there is nothing to check for that: the guarantee is that no code asks
    /// for it.
    pub fn check(&self, spec: &SourceSpec) -> Result<Url, Diagnostic> {
        let kind = kind_name(spec.kind);
        let id = &spec.id;
        let Some(raw) = spec.url.as_deref() else {
            return Err(Diagnostic::new(
                code::E0806,
                format!("`verify.sources.{id}` is a `{kind}` source with no `url`"),
            ));
        };
        let url = Url::parse(raw).map_err(|why| {
            Diagnostic::new(
                code::E0806,
                format!("`verify.sources.{id}.url` is not a URL: {why}"),
            )
        })?;

        // TODO(rfc-2033): drop this once `HttpPolicy` can carry a pin.
        if spec.pin.is_some() {
            return Err(Diagnostic::new(
                code::E0806,
                format!(
                    "certificate pinning is configured for `verify.sources.{id}` and this client cannot verify a pin"
                ),
            )
            .help("remove `pin` to fetch this source without it"));
        }

        match url.scheme() {
            "https" => Ok(url),
            "http" => {
                let host = url.host_str().unwrap_or_default();
                if self.allow_insecure_hosts.matches(host) {
                    Ok(url)
                } else {
                    Err(Diagnostic::new(
                        code::E0806,
                        format!(
                            "`verify.sources.{id}` is fetched over plain HTTP from `{host}`, which is not in `network.allowInsecureHosts`"
                        ),
                    )
                    .help("use `https`, or add the host to `network.allowInsecureHosts`"))
                }
            }
            other => Err(Diagnostic::new(
                code::E0806,
                format!(
                    "`verify.sources.{id}` uses the `{other}` scheme, and a fact source is fetched over HTTP"
                ),
            )),
        }
    }
}

#[cfg(test)]
mod tests;
