//! The Content Security Policy (RX-110).
//!
//! One policy for every cacheable response of a build. Scripts run only from
//! the origin or with the build nonce, which is the same value in the header
//! and in the markup so a `304 Not Modified` that re-sends the header still
//! matches the cached page's inline bootstrap. The critical CSS is matched by
//! its hash. Everything else is `'self'` plus what content, config, and enabled
//! integrations declare.

use std::collections::BTreeSet;

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use serde_json::Value;

use super::digest;
use super::integrations;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Policy {
    /// The build nonce, without the `'nonce-'` wrapping.
    pub nonce: String,
    /// `'sha256-…'` for each inline style block the theme emits.
    pub style_hashes: Vec<String>,
    pub script_src: BTreeSet<String>,
    pub img_src: BTreeSet<String>,
    pub media_src: BTreeSet<String>,
    pub frame_src: BTreeSet<String>,
    pub connect_src: BTreeSet<String>,
    pub font_src: BTreeSet<String>,
    /// `security.frameAncestors`: who may embed a `frame`-mode page. Every
    /// other page is `'none'` regardless.
    pub frame_ancestors: Vec<String>,
    pub report_uri: Option<String>,
}

impl Policy {
    /// The baseline: the build nonce and the hash of the theme's critical CSS.
    pub fn new(nonce: &str, critical_css: &str) -> Self {
        Self {
            nonce: nonce.to_owned(),
            style_hashes: vec![digest::csp_sha256(critical_css)],
            ..Self::default()
        }
    }

    /// `security.csp.*`, `security.frameAncestors`, `network.allowHosts.embeds`,
    /// and the sources of every enabled integration. A value that is not a
    /// source expression is refused with `E0722` rather than written into a
    /// header where a `;` would open a directive of its own.
    pub fn extend_from_config(&mut self, config: &Value) -> Diagnostics {
        let mut diagnostics = Diagnostics::new();
        let csp = config.get("security").and_then(|s| s.get("csp"));
        let mut take = |path: &str, list: Option<&Value>, into: &mut BTreeSet<String>| {
            for source in strings(list) {
                match valid_source(&source) {
                    true => {
                        into.insert(source);
                    }
                    false => diagnostics.push(refused(path, &source)),
                }
            }
        };
        take(
            "security.csp.extraScriptSrc",
            csp.and_then(|c| c.get("extraScriptSrc")),
            &mut self.script_src,
        );
        take(
            "security.csp.extraImgSrc",
            csp.and_then(|c| c.get("extraImgSrc")),
            &mut self.img_src,
        );
        take(
            "security.csp.extraFrameSrc",
            csp.and_then(|c| c.get("extraFrameSrc")),
            &mut self.frame_src,
        );
        take(
            "network.allowHosts.embeds",
            config
                .get("network")
                .and_then(|n| n.get("allowHosts"))
                .and_then(|a| a.get("embeds")),
            &mut self.frame_src,
        );

        for source in strings(config.get("security").and_then(|s| s.get("frameAncestors"))) {
            match valid_source(&source) {
                true if !self.frame_ancestors.contains(&source) => {
                    self.frame_ancestors.push(source);
                }
                true => {}
                false => diagnostics.push(refused("security.frameAncestors", &source)),
            }
        }
        if let Some(uri) = csp
            .and_then(|c| c.get("reportUri"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|uri| !uri.is_empty())
        {
            match valid_source(uri) {
                true => self.report_uri = Some(uri.to_owned()),
                false => diagnostics.push(refused("security.csp.reportUri", uri)),
            }
        }

        for integration in integrations::enabled(config) {
            self.add_integration(integration);
        }
        diagnostics
    }

    pub fn add_integration(&mut self, integration: &integrations::Integration) {
        let copy = |from: &[&str], into: &mut BTreeSet<String>| {
            into.extend(from.iter().map(|host| (*host).to_owned()));
        };
        copy(integration.script_src, &mut self.script_src);
        copy(integration.connect_src, &mut self.connect_src);
        copy(integration.img_src, &mut self.img_src);
        copy(integration.media_src, &mut self.media_src);
        copy(integration.frame_src, &mut self.frame_src);
        copy(integration.font_src, &mut self.font_src);
    }

    /// The remote hosts content uses (CM-35), so a remote image never renders
    /// broken under the site's own policy. A host that is not on
    /// `security.csp.imgHostsAllow` is still added and reported with `W0719`,
    /// which is how an operator learns that a page started pulling from
    /// somewhere new.
    pub fn add_content_hosts<'a>(
        &mut self,
        images: impl IntoIterator<Item = &'a str>,
        media: impl IntoIterator<Item = &'a str>,
        config: &Value,
    ) -> Diagnostics {
        let allowed: BTreeSet<String> = strings(
            config
                .get("security")
                .and_then(|s| s.get("csp"))
                .and_then(|c| c.get("imgHostsAllow")),
        )
        .into_iter()
        .collect();
        let mut diagnostics = Diagnostics::new();
        let mut new_hosts: BTreeSet<String> = BTreeSet::new();
        for (host, into, what) in images
            .into_iter()
            .map(|host| (host, "img-src", "image"))
            .chain(media.into_iter().map(|host| (host, "media-src", "media")))
        {
            let Some(host) = content_host(host) else {
                continue;
            };
            let set = match into {
                "img-src" => &mut self.img_src,
                _ => &mut self.media_src,
            };
            if set.insert(host.clone()) && !allowed.contains(&host) {
                new_hosts.insert(format!("{host} ({what})"));
            }
        }
        for host in new_hosts {
            diagnostics.push(
                Diagnostic::new(
                    code::W0719,
                    format!("content loads from `{host}`, which the CSP now allows"),
                )
                .help("list the host under `security.csp.imgHostsAllow` to accept it"),
            );
        }
        diagnostics
    }

    /// The header of every page that is not `frame` mode.
    pub fn header(&self) -> String {
        self.render(false)
    }

    /// The header of a `frame`-mode page: `frame-ancestors` lists the hosts
    /// of `security.frameAncestors` (RX-112).
    pub fn frame_header(&self) -> String {
        self.render(true)
    }

    fn render(&self, frame_page: bool) -> String {
        let mut out: Vec<String> = Vec::with_capacity(12);
        out.push("default-src 'self'".to_owned());
        out.push(directive("script-src", self.script_sources()));
        out.push(directive("style-src", self.style_hashes.to_vec()));
        out.push(directive(
            "img-src",
            std::iter::once("data:".to_owned())
                .chain(self.img_src.iter().cloned())
                .collect(),
        ));
        out.push(directive(
            "media-src",
            self.media_src.iter().cloned().collect(),
        ));
        out.push(directive(
            "frame-src",
            self.frame_src.iter().cloned().collect(),
        ));
        out.push(directive(
            "connect-src",
            self.connect_src.iter().cloned().collect(),
        ));
        if !self.font_src.is_empty() {
            out.push(directive(
                "font-src",
                self.font_src.iter().cloned().collect(),
            ));
        }
        out.push("object-src 'none'".to_owned());
        out.push("base-uri 'self'".to_owned());
        out.push("form-action 'self'".to_owned());
        out.push(match (frame_page, self.frame_ancestors.is_empty()) {
            (true, false) => format!("frame-ancestors {}", self.frame_ancestors.join(" ")),
            _ => "frame-ancestors 'none'".to_owned(),
        });
        if let Some(uri) = &self.report_uri {
            out.push(format!("report-uri {uri}"));
        }
        out.join("; ")
    }

    fn script_sources(&self) -> Vec<String> {
        let mut sources = Vec::with_capacity(self.script_src.len() + 1);
        if !self.nonce.is_empty() {
            sources.push(format!("'nonce-{}'", self.nonce));
        }
        sources.extend(self.script_src.iter().cloned());
        sources
    }
}

fn directive(name: &str, extra: Vec<String>) -> String {
    let mut out = format!("{name} 'self'");
    for source in extra {
        out.push(' ');
        out.push_str(&source);
    }
    out
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// A CSP source expression is one token: no whitespace, no `;` or `,` that
/// would start another directive or value, and no quote unless it is one of
/// the keyword sources.
pub fn valid_source(source: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "'self'",
        "'none'",
        "'unsafe-inline'",
        "'unsafe-eval'",
        "'strict-dynamic'",
        "'wasm-unsafe-eval'",
    ];
    if source.is_empty() || source.len() > 512 {
        return false;
    }
    if KEYWORDS.contains(&source) {
        return true;
    }
    if source.starts_with("'sha256-")
        || source.starts_with("'sha384-")
        || source.starts_with("'sha512-")
        || source.starts_with("'nonce-")
    {
        return source.ends_with('\'') && source[1..source.len() - 1].chars().all(hash_char);
    }
    source
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "-._~:/?#[]@!$&()*+,%=".contains(c) && c != ',')
}

fn hash_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || "+/-_=".contains(c)
}

/// The host a remote `src` contributes, or `None` when it is not remote.
/// The scheme is kept only when it is not `https`, so `http://cdn.example`
/// stays distinguishable from `cdn.example`.
pub fn content_host(src: &str) -> Option<String> {
    let src = src.trim();
    let (scheme, rest) = if let Some(rest) = src.strip_prefix("//") {
        ("https", rest)
    } else {
        let (scheme, rest) = src.split_once("://")?;
        (scheme, rest)
    };
    if !matches!(scheme, "http" | "https") {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next()?;
    if host.is_empty() || !valid_source(host) {
        return None;
    }
    Some(match scheme {
        "https" => host.to_ascii_lowercase(),
        other => format!("{other}://{}", host.to_ascii_lowercase()),
    })
}

fn refused(path: &str, source: &str) -> Diagnostic {
    Diagnostic::new(
        code::E0722,
        format!("`{path}` contains `{source}`, which is not a CSP source expression"),
    )
    .help(
        "a source is one host, scheme, or keyword such as `cdn.example.com`, `https:`, or `'self'`",
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const CRITICAL: &str = ":root{--x:1}";

    fn hash() -> String {
        digest::csp_sha256(CRITICAL)
    }

    #[test]
    fn the_baseline_is_the_policy_rx_110_lists_in_its_order() {
        let policy = Policy::new("abc123", CRITICAL);
        assert_eq!(
            policy.header(),
            format!(
                "default-src 'self'; script-src 'self' 'nonce-abc123'; style-src 'self' {}; \
                 img-src 'self' data:; media-src 'self'; frame-src 'self'; connect-src 'self'; \
                 object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'",
                hash()
            )
        );
    }

    #[test]
    fn config_extends_each_directive_and_only_that_directive() {
        let mut policy = Policy::new("n", CRITICAL);
        let diagnostics = policy.extend_from_config(&json!({
            "security": {
                "csp": {
                    "extraScriptSrc": ["cdn.example.com"],
                    "extraImgSrc": ["images.example.com"],
                    "extraFrameSrc": ["player.example.com"],
                    "reportUri": "https://csp.example.com/report"
                },
                "frameAncestors": ["https://app.acme.com"]
            },
            "network": { "allowHosts": { "embeds": ["www.youtube-nocookie.com"] } }
        }));
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let header = policy.header();
        assert!(header.contains("script-src 'self' 'nonce-n' cdn.example.com;"));
        assert!(header.contains("img-src 'self' data: images.example.com;"));
        assert!(header.contains("frame-src 'self' player.example.com www.youtube-nocookie.com;"));
        assert!(header.contains("connect-src 'self';"));
        assert!(
            header.ends_with("frame-ancestors 'none'; report-uri https://csp.example.com/report")
        );
        assert!(!header.contains("app.acme.com"));
        assert!(
            policy
                .frame_header()
                .contains("frame-ancestors https://app.acme.com;")
        );
    }

    #[test]
    fn a_source_that_would_open_a_directive_is_refused() {
        let mut policy = Policy::new("n", CRITICAL);
        let diagnostics = policy.extend_from_config(&json!({
            "security": { "csp": { "extraScriptSrc": ["cdn.example.com; script-src *", "'unsafe-inline'", "ok.example"] } }
        }));
        let codes: Vec<String> = diagnostics.iter().map(|d| d.code.to_string()).collect();
        assert_eq!(codes, ["E0722"]);
        let header = policy.header();
        assert!(header.contains("script-src 'self' 'nonce-n' 'unsafe-inline' ok.example;"));
        assert!(!header.contains("script-src *"));
    }

    #[test]
    fn a_frame_page_relaxes_frame_ancestors_and_nothing_else() {
        let mut policy = Policy::new("n", CRITICAL);
        policy.frame_ancestors = vec!["https://app.acme.com".to_owned()];
        let strict = policy.header();
        let relaxed = policy.frame_header();
        assert!(strict.ends_with("frame-ancestors 'none'"));
        assert!(relaxed.ends_with("frame-ancestors https://app.acme.com"));
        assert_eq!(
            strict.trim_end_matches("'none'"),
            relaxed.trim_end_matches("https://app.acme.com")
        );
        // Without a list a frame page is still 'none'.
        assert!(
            Policy::new("n", CRITICAL)
                .frame_header()
                .ends_with("'none'")
        );
    }

    #[test]
    fn an_integration_adds_exactly_its_declared_sources() {
        let mut bare = Policy::new("n", CRITICAL);
        bare.extend_from_config(&json!({}));
        let mut with = Policy::new("n", CRITICAL);
        with.extend_from_config(&json!({ "integrations": { "plausible": { "id": "docs" } } }));
        let plausible = integrations::by_key("plausible").expect("a row");
        let mut expected = bare.clone();
        expected.add_integration(plausible);
        assert_eq!(with, expected);
        assert!(
            with.header()
                .contains("script-src 'self' 'nonce-n' plausible.io;")
        );
        assert!(with.header().contains("connect-src 'self' plausible.io;"));
        assert!(!with.header().contains("font-src"));
        let mut intercom = Policy::new("n", CRITICAL);
        intercom.extend_from_config(&json!({ "integrations": { "intercom": { "id": "x" } } }));
        assert!(
            intercom
                .header()
                .contains("; font-src 'self' js.intercomcdn.com; object-src")
        );
    }

    #[test]
    fn content_hosts_join_the_policy_and_new_ones_are_reported() {
        let mut policy = Policy::new("n", CRITICAL);
        let config = json!({ "security": { "csp": { "imgHostsAllow": ["cdn.acme.com"] } } });
        let diagnostics = policy.add_content_hosts(
            [
                "https://cdn.acme.com/a.png",
                "https://CDN.acme.com/b.png",
                "//images.unsplash.com/photo",
                "http://legacy.example/x.gif",
                "./local.png",
                "/assets/site.png",
            ],
            ["https://media.acme.com/clip.mp4"],
            &config,
        );
        let header = policy.header();
        assert!(header.contains(
            "img-src 'self' data: cdn.acme.com http://legacy.example images.unsplash.com;"
        ));
        assert!(header.contains("media-src 'self' media.acme.com;"));
        let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(messages.len(), 3, "{messages:?}");
        assert!(messages.iter().all(|m| !m.contains("cdn.acme.com")));
        assert!(
            messages
                .iter()
                .any(|m| m.contains("images.unsplash.com (image)"))
        );
        assert!(
            messages
                .iter()
                .any(|m| m.contains("media.acme.com (media)"))
        );
        assert!(diagnostics.iter().all(|d| d.code.to_string() == "W0719"));
    }

    #[test]
    fn a_missing_nonce_leaves_script_src_at_self() {
        let policy = Policy::new("", CRITICAL);
        assert!(policy.header().contains("script-src 'self'; style-src"));
    }

    #[test]
    fn content_host_reads_the_authority_only() {
        assert_eq!(content_host("https://a.b/c?d#e").as_deref(), Some("a.b"));
        assert_eq!(
            content_host("https://user@a.b:8443/c").as_deref(),
            Some("a.b:8443")
        );
        assert_eq!(content_host("data:image/png;base64,AAAA"), None);
        assert_eq!(content_host("mailto:x@y"), None);
        assert_eq!(content_host("https://bad host/x"), None);
    }
}
