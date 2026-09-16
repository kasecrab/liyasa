//! The response headers of a static build (RX-13, RX-112) and the `_headers`
//! file that Cloudflare Pages and Netlify read.
//!
//! Rules are cumulative on both hosts: every rule whose path matches applies,
//! and a later rule wins for a header both set. The general rule therefore
//! comes first and the specific ones after it.

use liyasa_core::ids::Route;

use super::csp::Policy;

pub const HSTS: &str = "max-age=63072000; includeSubDomains";
pub const HSTS_PRELOAD: &str = "max-age=63072000; includeSubDomains; preload";
pub const PERMISSIONS_POLICY: &str = "camera=(), microphone=(), geolocation=(), payment=()";
pub const REFERRER_POLICY: &str = "strict-origin-when-cross-origin";
pub const OPENER_POLICY: &str = "same-origin";
/// HTML and Markdown: fresh for five minutes, then revalidated against the
/// host's `ETag` (RX-13).
pub const CACHE_HTML: &str = "public, max-age=300, must-revalidate";
/// Hashed assets never change under their URL.
pub const CACHE_IMMUTABLE: &str = "public, max-age=31536000, immutable";
/// What an authenticated or on-demand response carries; never in a static
/// build, listed so the server and the tests share one spelling.
pub const CACHE_PRIVATE: &str = "private, no-store";
pub const MARKDOWN_TYPE: &str = "text/markdown; charset=utf-8";

/// Directories the build fills with hashed files only.
pub const IMMUTABLE_DIRS: &[&str] = &[crate::images::VARIANT_PREFIX, "_liyasa"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// A `_headers` path: exact, or `/prefix/*`, or `/*.ext`.
    pub path: String,
    pub headers: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rules(pub Vec<Rule>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options<'a> {
    pub policy: &'a Policy,
    /// `build.basePath`, empty or `/docs`.
    pub base_path: &'a str,
    /// `security.hstsPreload`; the header without it is the default because
    /// preload is a one-way submission to browser lists.
    pub hsts_preload: bool,
    /// `build.hashing: "filename"`: every copied asset carries its digest.
    pub hashed_assets: bool,
    /// `mode: frame` pages, whose `frame-ancestors` is the configured list.
    pub frame_routes: &'a [Route],
}

/// The headers every response carries (RX-112) plus the page cache policy
/// (RX-13). `csp` is the policy for the pages the rule covers.
fn security(csp: String, hsts_preload: bool, frame_page: bool) -> Vec<(String, String)> {
    let hsts = if hsts_preload { HSTS_PRELOAD } else { HSTS };
    // CSP Level 2 §7.9: a `frame-ancestors` directive makes the browser ignore
    // `X-Frame-Options`, so on a frame page the value only matters to a browser
    // without CSP, where the same-origin fallback is the safe one.
    let frame_options = if frame_page { "SAMEORIGIN" } else { "DENY" };
    vec![
        ("Content-Security-Policy".to_owned(), csp),
        ("Strict-Transport-Security".to_owned(), hsts.to_owned()),
        ("X-Content-Type-Options".to_owned(), "nosniff".to_owned()),
        ("Referrer-Policy".to_owned(), REFERRER_POLICY.to_owned()),
        ("X-Frame-Options".to_owned(), frame_options.to_owned()),
        (
            "Permissions-Policy".to_owned(),
            PERMISSIONS_POLICY.to_owned(),
        ),
        (
            "Cross-Origin-Opener-Policy".to_owned(),
            OPENER_POLICY.to_owned(),
        ),
        ("Cache-Control".to_owned(), CACHE_HTML.to_owned()),
    ]
}

pub fn rules(options: &Options<'_>) -> Rules {
    let base = options.base_path.trim_end_matches('/');
    let mut rules = vec![Rule {
        path: format!("{base}/*"),
        headers: security(options.policy.header(), options.hsts_preload, false),
    }];
    for dir in IMMUTABLE_DIRS {
        rules.push(Rule {
            path: format!("{base}/{dir}/*"),
            headers: vec![("Cache-Control".to_owned(), CACHE_IMMUTABLE.to_owned())],
        });
    }
    if options.hashed_assets {
        rules.push(Rule {
            path: format!("{base}/assets/*"),
            headers: vec![("Cache-Control".to_owned(), CACHE_IMMUTABLE.to_owned())],
        });
    }
    rules.push(Rule {
        path: format!("{base}/*.md"),
        headers: vec![("Content-Type".to_owned(), MARKDOWN_TYPE.to_owned())],
    });
    for route in options.frame_routes {
        let trimmed = route.as_str().trim_matches('/');
        let path = match trimmed.is_empty() {
            true => base.to_owned(),
            false => format!("{base}/{trimmed}"),
        };
        let headers = security(options.policy.frame_header(), options.hsts_preload, true);
        rules.push(Rule {
            path: format!("{path}/*"),
            headers: headers.clone(),
        });
        rules.push(Rule {
            path: if path.is_empty() {
                "/".to_owned()
            } else {
                path
            },
            headers,
        });
    }
    Rules(rules)
}

impl Rules {
    /// The `_headers` file: a path line, then one indented header per line.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for rule in &self.0 {
            out.push_str(&rule.path);
            out.push('\n');
            for (name, value) in &rule.headers {
                out.push_str("  ");
                out.push_str(name);
                out.push_str(": ");
                out.push_str(value);
                out.push('\n');
            }
        }
        out
    }

    /// The headers a request path receives under the cumulative rule.
    pub fn resolve(&self, path: &str) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = Vec::new();
        for rule in self.0.iter().filter(|rule| matches(&rule.path, path)) {
            for (name, value) in &rule.headers {
                match out.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
                    Some(slot) => slot.1 = value.clone(),
                    None => out.push((name.clone(), value.clone())),
                }
            }
        }
        out
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Rule> {
        self.0.iter()
    }
}

/// `_headers` path matching: `*` matches the rest of the path (including
/// nothing), `/*.md` matches by suffix, anything else is exact.
pub fn matches(pattern: &str, path: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('*') {
        return path.starts_with(prefix);
    }
    if let Some((prefix, suffix)) = pattern.split_once('*') {
        return path.starts_with(prefix)
            && path.ends_with(suffix)
            && path.len() >= prefix.len() + suffix.len();
    }
    pattern == path
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> Policy {
        let mut policy = Policy::new("n0nce", ":root{}");
        policy.frame_ancestors = vec!["https://app.acme.com".to_owned()];
        policy
    }

    fn options<'a>(policy: &'a Policy, frame_routes: &'a [Route]) -> Options<'a> {
        Options {
            policy,
            base_path: "",
            hsts_preload: false,
            hashed_assets: false,
            frame_routes,
        }
    }

    #[test]
    fn every_page_gets_the_rx_112_set_and_the_rx_13_cache_policy() {
        let policy = policy();
        let rules = super::rules(&options(&policy, &[]));
        let page = rules.resolve("/guides/install/");
        let get = |name: &str| {
            page.iter()
                .find(|(n, _)| n == name)
                .map(|(_, v)| v.as_str())
                .unwrap_or_else(|| panic!("{name} missing from {page:?}"))
        };
        assert_eq!(get("Strict-Transport-Security"), HSTS);
        assert_eq!(get("X-Content-Type-Options"), "nosniff");
        assert_eq!(get("Referrer-Policy"), "strict-origin-when-cross-origin");
        assert_eq!(get("X-Frame-Options"), "DENY");
        assert_eq!(get("Permissions-Policy"), PERMISSIONS_POLICY);
        assert_eq!(get("Cross-Origin-Opener-Policy"), "same-origin");
        assert_eq!(get("Cache-Control"), CACHE_HTML);
        assert!(get("Content-Security-Policy").ends_with("frame-ancestors 'none'"));
        assert!(
            page.iter()
                .all(|(n, _)| n != "Cross-Origin-Embedder-Policy")
        );
    }

    #[test]
    fn hashed_files_are_immutable_and_pages_are_not() {
        let policy = policy();
        let rules = super::rules(&options(&policy, &[]));
        let cache = |path: &str| {
            rules
                .resolve(path)
                .into_iter()
                .find(|(n, _)| n == "Cache-Control")
                .map(|(_, v)| v)
                .expect("a cache policy")
        };
        assert_eq!(
            cache("/_liyasa/theme.0123456789abcdef.css"),
            CACHE_IMMUTABLE
        );
        assert_eq!(cache("/_image/0123456789abcdef/640.webp"), CACHE_IMMUTABLE);
        assert_eq!(cache("/assets/manual.pdf"), CACHE_HTML);
        assert_eq!(cache("/guides/install.md"), CACHE_HTML);
        assert_eq!(cache("/"), CACHE_HTML);

        let hashed = super::rules(&Options {
            hashed_assets: true,
            ..options(&policy, &[])
        });
        let asset = hashed.resolve("/assets/manual.0123456789abcdef.pdf");
        assert!(asset.contains(&("Cache-Control".to_owned(), CACHE_IMMUTABLE.to_owned())));
    }

    #[test]
    fn markdown_routes_are_typed() {
        let policy = policy();
        let rules = super::rules(&options(&policy, &[]));
        let md = rules.resolve("/guides/install.md");
        assert!(md.contains(&("Content-Type".to_owned(), MARKDOWN_TYPE.to_owned())));
        assert!(
            !rules
                .resolve("/guides/install/")
                .iter()
                .any(|(n, _)| n == "Content-Type")
        );
    }

    #[test]
    fn a_frame_page_relaxes_only_its_own_route() {
        let policy = policy();
        let frame = [Route::new("/embed/widget")];
        let rules = super::rules(&options(&policy, &frame));
        let csp = |path: &str| {
            rules
                .resolve(path)
                .into_iter()
                .find(|(n, _)| n == "Content-Security-Policy")
                .map(|(_, v)| v)
                .expect("a policy")
        };
        assert!(csp("/embed/widget/").ends_with("frame-ancestors https://app.acme.com"));
        assert!(csp("/embed/widget").ends_with("frame-ancestors https://app.acme.com"));
        assert!(csp("/embed/").ends_with("frame-ancestors 'none'"));
        assert!(csp("/").ends_with("frame-ancestors 'none'"));
        let xfo = rules
            .resolve("/embed/widget/")
            .into_iter()
            .find(|(n, _)| n == "X-Frame-Options")
            .map(|(_, v)| v);
        assert_eq!(xfo.as_deref(), Some("SAMEORIGIN"));
    }

    #[test]
    fn the_base_path_prefixes_every_rule() {
        let policy = policy();
        let rules = super::rules(&Options {
            base_path: "/docs/",
            ..options(&policy, &[Route::new("/embed")])
        });
        let paths: Vec<&str> = rules.iter().map(|rule| rule.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "/docs/*",
                "/docs/_image/*",
                "/docs/_liyasa/*",
                "/docs/*.md",
                "/docs/embed/*",
                "/docs/embed"
            ]
        );
        assert!(rules.resolve("/other/").is_empty());
    }

    #[test]
    fn the_file_is_one_path_line_and_indented_headers() {
        let policy = policy();
        let text = super::rules(&options(&policy, &[])).render();
        let mut lines = text.lines();
        assert_eq!(lines.next(), Some("/*"));
        assert!(
            lines
                .next()
                .is_some_and(|l| l.starts_with("  Content-Security-Policy: default-src 'self'; "))
        );
        assert!(
            text.contains("\n/_liyasa/*\n  Cache-Control: public, max-age=31536000, immutable\n")
        );
        assert!(text.contains("\n/*.md\n  Content-Type: text/markdown; charset=utf-8\n"));
    }

    #[test]
    fn preload_is_opt_in() {
        let policy = policy();
        let rules = super::rules(&Options {
            hsts_preload: true,
            ..options(&policy, &[])
        });
        assert!(rules.resolve("/").contains(&(
            "Strict-Transport-Security".to_owned(),
            HSTS_PRELOAD.to_owned()
        )));
    }
}
