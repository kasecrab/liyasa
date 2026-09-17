//! Emulators of the listed static hosts (HOST-01).
//!
//! Each host is modelled on what its documentation says it does with an
//! uploaded directory and nothing else: which of the build's host files it
//! reads, which headers it sends on its own, and how it resolves a directory.
//! The model is what the host matrix is generated from, so a row in
//! `MATRIX.md` is a claim about the model here; a change to either is a
//! change to the other. Whatever a host needs beyond the upload (a bucket
//! policy, a server block) is a recipe in the docs (HOST-04, HOST-22), and the
//! matrix says "manual" for it.
//!
//! A host also decides which of the uploaded files it publishes at all. That
//! is modelled here too: a file a host silently declines to publish answers
//! 404 like any other missing path, which is a failure the build cannot see
//! and the matrix could not report until the filter was written down.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use super::{NOJEKYLL_FILE, fallback, headers};
use crate::agents::spec::HostHeaders;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Host {
    GitHubPages,
    CloudflarePages,
    Netlify,
    Vercel,
    /// A bucket with website hosting on, fronted by CloudFront.
    S3CloudFront,
    /// nginx or Apache serving the directory as it is.
    WebServer,
}

impl Host {
    pub const ALL: [Host; 6] = [
        Host::GitHubPages,
        Host::CloudflarePages,
        Host::Netlify,
        Host::Vercel,
        Host::S3CloudFront,
        Host::WebServer,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Host::GitHubPages => "GitHub Pages",
            Host::CloudflarePages => "Cloudflare Pages",
            Host::Netlify => "Netlify",
            Host::Vercel => "Vercel",
            Host::S3CloudFront => "S3 + CloudFront",
            Host::WebServer => "Web server",
        }
    }

    pub fn reads_headers_file(self) -> bool {
        matches!(self, Host::CloudflarePages | Host::Netlify)
    }

    pub fn reads_redirects_file(self) -> bool {
        matches!(self, Host::CloudflarePages | Host::Netlify)
    }

    pub fn reads_vercel_json(self) -> bool {
        self == Host::Vercel
    }

    /// Whether the host sends `Last-Modified` for a static file. The three
    /// CDN-first hosts validate with `ETag` alone.
    pub fn sends_last_modified(self) -> bool {
        matches!(
            self,
            Host::GitHubPages | Host::S3CloudFront | Host::WebServer
        )
    }

    /// The `Cache-Control` the host sends when nothing configures one.
    pub fn own_cache_control(self) -> Option<&'static str> {
        match self {
            Host::GitHubPages => Some("max-age=600"),
            Host::CloudflarePages | Host::Netlify | Host::Vercel => {
                Some("public, max-age=0, must-revalidate")
            }
            Host::S3CloudFront | Host::WebServer => None,
        }
    }

    /// What a `.md` file is served as. A bucket takes the type from the
    /// upload and a stock web server's MIME table has no entry for it.
    pub fn markdown_content_type(self) -> &'static str {
        match self {
            Host::S3CloudFront | Host::WebServer => "application/octet-stream",
            _ => headers::MARKDOWN_TYPE,
        }
    }

    /// The status of `/dir` when `/dir/index.html` exists.
    fn trailing_slash_status(self) -> u16 {
        match self {
            Host::GitHubPages | Host::Netlify | Host::WebServer => 301,
            Host::CloudflarePages | Host::Vercel => 308,
            Host::S3CloudFront => 302,
        }
    }

    /// The status a `permanent` rule gets.
    fn permanent_status(self) -> u16 {
        match self {
            Host::Vercel => 308,
            _ => 301,
        }
    }

    /// Files this host reads as deploy-time configuration and never serves as
    /// content. They are excluded from the publishing model rather than
    /// dropped by it: the host consumes them before the site exists.
    fn config_files(self) -> &'static [&'static str] {
        match self {
            Host::CloudflarePages | Host::Netlify => &[super::HEADERS_FILE, super::REDIRECTS_FILE],
            Host::Vercel => &[super::VERCEL_FILE],
            // A bucket and a web server serve every uploaded byte, including
            // the host files meant for someone else.
            Host::GitHubPages | Host::S3CloudFront | Host::WebServer => &[],
        }
    }

    /// Whether the host publishes an uploaded path as a servable file.
    pub fn publishes(self, dist: &Dist, relative: &str) -> bool {
        let relative = relative.trim_start_matches('/');
        if self.config_files().contains(&relative) {
            return false;
        }
        match self {
            // GitHub Pages runs Jekyll over the upload unless `.nojekyll` sits
            // at the site root, and Jekyll publishes no entry whose name
            // begins with `_` or `.`. That covers `_liyasa/`, `_image/`, and
            // `.well-known/` — the theme, the images, and the agent surfaces.
            Host::GitHubPages => {
                dist.contains(NOJEKYLL_FILE) || !relative.split('/').any(is_jekyll_special)
            }
            // Cloudflare Pages does not upload a file whose name begins with a
            // dot; `.well-known` is excepted by name, which is what keeps the
            // agent surfaces reachable there.
            Host::CloudflarePages => !relative
                .split('/')
                .any(|segment| segment.starts_with('.') && segment != WELL_KNOWN),
            Host::Netlify | Host::Vercel | Host::S3CloudFront | Host::WebServer => true,
        }
    }

    /// The bytes of an uploaded file, when this host publishes it.
    fn fetch<'a>(self, dist: &'a Dist, relative: &str) -> Option<&'a [u8]> {
        self.publishes(dist, relative)
            .then(|| dist.get(relative))
            .flatten()
    }

    /// Every uploaded path this host drops, host files aside: the files the
    /// build wrote and a reader will never receive.
    pub fn dropped(self, dist: &Dist) -> Vec<&str> {
        dist.paths()
            .filter(|path| !is_host_file(path))
            .filter(|path| !self.publishes(dist, path))
            .collect()
    }

    /// Whether an unknown route gets `404.html` as its body.
    fn serves_not_found_page(self) -> bool {
        // The bucket's error document and nginx's `error_page` are
        // configuration, not the upload.
        !matches!(self, Host::S3CloudFront | Host::WebServer)
    }
}

/// The one dot-directory a static host is expected to publish (RFC 8615); the
/// agent card and the skills live under it.
pub const WELL_KNOWN: &str = ".well-known";

/// Jekyll's own entry rule. It applies to the entries of every directory it
/// walks, not only the site root, so the test is per path segment.
///
/// Markdown is the other thing Jekyll would touch, and it does not touch this
/// build's: a page's `.md` surface carries no YAML front matter, so Jekyll
/// copies it verbatim as a static file rather than converting it.
fn is_jekyll_special(segment: &str) -> bool {
    segment.starts_with('_') || segment.starts_with('.')
}

/// Whether a path is one host's configuration rather than site content. Such
/// a file being unserved is the host working correctly, on every host.
fn is_host_file(path: &str) -> bool {
    matches!(
        path.trim_start_matches('/'),
        super::HEADERS_FILE | super::REDIRECTS_FILE | super::VERCEL_FILE | NOJEKYLL_FILE
    )
}

/// The uploaded directory.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Dist {
    files: BTreeMap<String, Vec<u8>>,
}

impl Dist {
    pub fn read(dir: &Path) -> std::io::Result<Self> {
        let mut dist = Self::default();
        let mut pending = vec![dir.to_path_buf()];
        while let Some(current) = pending.pop() {
            for entry in std::fs::read_dir(&current)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    pending.push(path);
                    continue;
                }
                let relative = path
                    .strip_prefix(dir)
                    .map_err(|_| std::io::Error::other("a path outside the directory"))?
                    .to_string_lossy()
                    .replace('\\', "/");
                dist.files.insert(relative, std::fs::read(&path)?);
            }
        }
        Ok(dist)
    }

    pub fn insert(&mut self, path: &str, bytes: impl Into<Vec<u8>>) {
        self.files
            .insert(path.trim_start_matches('/').to_owned(), bytes.into());
    }

    pub fn get(&self, path: &str) -> Option<&[u8]> {
        self.files
            .get(path.trim_start_matches('/'))
            .map(Vec::as_slice)
    }

    /// Drops a file, for modelling an upload that a build did not produce —
    /// a hand-assembled `dist/`, or one from before a file was emitted.
    pub fn remove(&mut self, path: &str) {
        self.files.remove(path.trim_start_matches('/'));
    }

    pub fn contains(&self, path: &str) -> bool {
        self.files.contains_key(path.trim_start_matches('/'))
    }

    pub fn text(&self, path: &str) -> Option<String> {
        self.get(path)
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
    }

    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn location(&self) -> Option<&str> {
        self.header("Location")
    }

    fn set(&mut self, name: &str, value: &str) {
        match self
            .headers
            .iter_mut()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
        {
            Some(slot) => slot.1 = value.to_owned(),
            None => self.headers.push((name.to_owned(), value.to_owned())),
        }
    }
}

struct Rule {
    source: String,
    destination: String,
    status: u16,
}

impl Host {
    /// One request, as the host would answer it.
    pub fn serve(self, dist: &Dist, path: &str) -> Response {
        let path = path.split(['?', '#']).next().unwrap_or(path);
        let path = match path.starts_with('/') {
            true => path.to_owned(),
            false => format!("/{path}"),
        };

        let mut response = match self.redirect_for(dist, &path) {
            Some((status, location)) => Response {
                status,
                headers: vec![("Location".to_owned(), location)],
                body: Vec::new(),
            },
            None => self.file_for(dist, &path),
        };

        self.add_own_headers(&mut response);
        self.add_configured_headers(dist, &path, &mut response);
        response
    }

    fn redirect_for(self, dist: &Dist, path: &str) -> Option<(u16, String)> {
        let rules = self.rules(dist);
        let trimmed = normalize(path);
        for rule in &rules {
            if let Some(bindings) = match_source(&rule.source, &trimmed) {
                return Some((rule.status, interpolate(&rule.destination, &bindings)));
            }
        }
        None
    }

    fn rules(self, dist: &Dist) -> Vec<Rule> {
        if self.reads_redirects_file() {
            return dist
                .text(super::REDIRECTS_FILE)
                .map(|text| {
                    text.lines()
                        .filter_map(|line| {
                            let mut parts = line.split_whitespace();
                            let source = parts.next()?;
                            let destination = parts.next()?;
                            let status = parts.next().and_then(|s| s.parse().ok()).unwrap_or(301);
                            Some(Rule {
                                source: source.to_owned(),
                                destination: destination.to_owned(),
                                status,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
        }
        if self.reads_vercel_json() {
            let Some(value) = dist
                .text(super::VERCEL_FILE)
                .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            else {
                return Vec::new();
            };
            return value["redirects"]
                .as_array()
                .map(|rules| {
                    rules
                        .iter()
                        .filter_map(|rule| {
                            Some(Rule {
                                source: rule["source"].as_str()?.replace("/:splat*", "/*"),
                                destination: rule["destination"].as_str()?.to_owned(),
                                status: match rule["permanent"].as_bool().unwrap_or(true) {
                                    true => self.permanent_status(),
                                    false => 307,
                                },
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
        }
        Vec::new()
    }

    fn file_for(self, dist: &Dist, path: &str) -> Response {
        let relative = path.trim_start_matches('/');
        if !relative.is_empty() && !relative.ends_with('/') {
            if let Some(bytes) = self.fetch(dist, relative) {
                return self.ok(relative, bytes);
            }
            if self
                .fetch(dist, &format!("{relative}/index.html"))
                .is_some()
            {
                return Response {
                    status: self.trailing_slash_status(),
                    headers: vec![("Location".to_owned(), format!("{path}/"))],
                    body: Vec::new(),
                };
            }
        }
        let index = format!("{relative}index.html");
        if let Some(bytes) = self.fetch(dist, &index) {
            return self.ok(&index, bytes);
        }
        let body = match self.serves_not_found_page() {
            true => self
                .fetch(dist, "404.html")
                .map(<[u8]>::to_vec)
                .unwrap_or_default(),
            false => b"<h1>404 Not Found</h1>".to_vec(),
        };
        Response {
            status: 404,
            headers: vec![(
                "Content-Type".to_owned(),
                "text/html; charset=utf-8".to_owned(),
            )],
            body,
        }
    }

    fn ok(self, relative: &str, bytes: &[u8]) -> Response {
        let extension = relative.rsplit('.').next().unwrap_or_default();
        let content_type = match extension {
            "html" => "text/html; charset=utf-8".to_owned(),
            "md" => self.markdown_content_type().to_owned(),
            other => crate::assets::content_type(other).to_owned(),
        };
        Response {
            status: 200,
            headers: vec![("Content-Type".to_owned(), content_type)],
            body: bytes.to_vec(),
        }
    }

    fn add_own_headers(self, response: &mut Response) {
        if response.status == 200 {
            let etag = liyasa_core::ids::Fingerprint::of(&response.body).to_hex();
            response.set("ETag", &format!("\"{}\"", &etag[..16]));
            if self.sends_last_modified() {
                response.set("Last-Modified", "Thu, 01 Jan 2026 00:00:00 GMT");
            }
        }
        if let Some(cache) = self.own_cache_control() {
            response.set("Cache-Control", cache);
        }
    }

    fn add_configured_headers(self, dist: &Dist, path: &str, response: &mut Response) {
        if self.reads_headers_file()
            && let Some(text) = dist.text(super::HEADERS_FILE)
        {
            for (name, value) in parse_headers_file(&text).resolve(path) {
                response.set(&name, &value);
            }
        }
        if self.reads_vercel_json()
            && let Some(value) = dist
                .text(super::VERCEL_FILE)
                .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        {
            for rule in value["headers"].as_array().into_iter().flatten() {
                let Some(source) = rule["source"].as_str() else {
                    continue;
                };
                let pattern = source.replace("(.*)", "*").replace('\\', "");
                if !headers::matches(&pattern, path) {
                    continue;
                }
                for header in rule["headers"].as_array().into_iter().flatten() {
                    if let (Some(key), Some(value)) =
                        (header["key"].as_str(), header["value"].as_str())
                    {
                        response.set(key, value);
                    }
                }
            }
        }
    }

    /// The spec runner's view of this host over this upload
    /// ([`crate::agents::spec::run`]).
    pub fn host_headers(self, dist: &Dist) -> HostHeaders {
        let probe = Probe::from(dist);
        let page = self.serve(dist, &probe.page);
        let markdown = probe.markdown.as_deref().map(|path| self.serve(dist, path));
        let missing = self.serve(dist, "/liyasa-no-such-route/");
        // With the slash, so a host that reads no rule file serves the
        // fallback page rather than redirecting to it.
        let redirect = probe
            .redirect_source
            .as_deref()
            .map(|source| self.serve(dist, &format!("{}/", source.trim_end_matches('/'))));
        HostHeaders {
            markdown_content_type: markdown
                .as_ref()
                .and_then(|r| r.header("Content-Type"))
                .map(str::to_owned),
            honours_accept: false,
            vary_accept: false,
            cache_control: page.header("Cache-Control").map(str::to_owned),
            etag: page.header("ETag").is_some(),
            last_modified: page.header("Last-Modified").is_some(),
            not_found_status: missing.status,
            redirect_status: redirect
                .as_ref()
                .filter(|r| (300..400).contains(&r.status))
                .map(|r| r.status),
            javascript_redirects: redirect.as_ref().is_some_and(|r| {
                r.status == 200 && fallback::is_refresh_page(&String::from_utf8_lossy(&r.body))
            }),
            serves_challenge: false,
        }
    }
}

/// The requests the matrix and the spec view are built from, found in the
/// upload rather than assumed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    /// A page route with a trailing slash.
    pub page: String,
    /// The same route without it.
    pub page_bare: Option<String>,
    pub markdown: Option<String>,
    pub redirect_source: Option<String>,
    pub hashed_asset: Option<String>,
}

impl Probe {
    pub fn from(dist: &Dist) -> Self {
        let mut page = "/".to_owned();
        let mut page_bare = None;
        let mut markdown = None;
        let mut hashed_asset = None;
        for path in dist.paths() {
            if let Some(dir) = path.strip_suffix("/index.html")
                && page_bare.is_none()
                && !dir.contains('.')
            {
                page = format!("/{dir}/");
                page_bare = Some(format!("/{dir}"));
            }
            if path.ends_with(".md") && markdown.is_none() && path != "404.md" {
                markdown = Some(format!("/{path}"));
            }
            if hashed_asset.is_none()
                && headers::IMMUTABLE_DIRS
                    .iter()
                    .any(|dir| path.starts_with(&format!("{dir}/")))
            {
                hashed_asset = Some(format!("/{path}"));
            }
        }
        let redirect_source = dist
            .text(super::REDIRECTS_FILE)
            .and_then(|text| {
                text.lines()
                    .filter_map(|line| line.split_whitespace().next().map(str::to_owned))
                    .find(|source| fallback::is_literal(source))
            })
            .or_else(|| {
                dist.text(super::VERCEL_FILE)
                    .and_then(|text| serde_json::from_str::<Value>(&text).ok())
                    .and_then(|value| {
                        value["redirects"]
                            .as_array()?
                            .iter()
                            .filter_map(|rule| rule["source"].as_str())
                            .find(|source| fallback::is_literal(source))
                            .map(str::to_owned)
                    })
            });
        Self {
            page,
            page_bare,
            markdown,
            redirect_source,
            hashed_asset,
        }
    }
}

pub fn parse_headers_file(text: &str) -> headers::Rules {
    let mut rules: Vec<headers::Rule> = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Some(header) = line.strip_prefix("  ") {
            if let (Some(rule), Some((name, value))) = (rules.last_mut(), header.split_once(':')) {
                rule.headers
                    .push((name.trim().to_owned(), value.trim().to_owned()));
            }
            continue;
        }
        rules.push(headers::Rule {
            path: line.trim().to_owned(),
            headers: Vec::new(),
        });
    }
    headers::Rules(rules)
}

fn normalize(path: &str) -> String {
    match path.len() > 1 {
        true => path.trim_end_matches('/').to_owned(),
        false => path.to_owned(),
    }
}

fn match_source(source: &str, path: &str) -> Option<BTreeMap<String, String>> {
    let pattern: Vec<&str> = source.trim_matches('/').split('/').collect();
    let segments: Vec<&str> = path.trim_matches('/').split('/').collect();
    let mut bindings = BTreeMap::new();
    let mut i = 0;
    for (n, part) in pattern.iter().enumerate() {
        match *part {
            "*" => {
                let rest = segments.get(i..).unwrap_or_default().join("/");
                bindings.insert(crate::redirects::SPLAT.to_owned(), rest);
                return Some(bindings);
            }
            "" if n == 0 && pattern.len() == 1 => {
                return segments.iter().all(|s| s.is_empty()).then_some(bindings);
            }
            _ => {
                let segment = segments.get(i)?;
                if let Some(name) = part.strip_prefix(':') {
                    bindings.insert(name.to_owned(), (*segment).to_owned());
                } else if part != segment {
                    return None;
                }
                i += 1;
            }
        }
    }
    (i == segments.len()).then_some(bindings)
}

fn interpolate(destination: &str, bindings: &BTreeMap<String, String>) -> String {
    let mut out = destination.to_owned();
    for (name, value) in bindings {
        out = out.replace(&format!(":{name}"), value);
    }
    out
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::hosting::{self, Inputs, fixture};

    fn upload() -> Dist {
        let config = json!({ "security": { "frameAncestors": ["https://app.acme.com"] } });
        let manifest = fixture::manifest("");
        let output = hosting::generate(&Inputs {
            config: &config,
            manifest: &manifest,
            critical_css: ":root{}",
            frame_routes: &[liyasa_core::ids::Route::new("/embed/widget")],
            image_hosts: &[],
            media_hosts: &[],
            hsts_preload: false,
        });
        let mut dist = Dist::default();
        for file in &output.files {
            dist.insert(&file.path, file.contents.as_bytes());
        }
        dist.insert("index.html", "<!doctype html><p>home</p>");
        dist.insert("index.md", "# Home");
        dist.insert("guides/install/index.html", "<!doctype html><p>install</p>");
        dist.insert("guides/install.md", "# Install");
        dist.insert("embed/widget/index.html", "<!doctype html><p>widget</p>");
        dist.insert("404.html", "<!doctype html><p>not found</p>");
        dist.insert("_liyasa/theme.0123456789abcdef.css", "body{}");
        dist.insert("_image/0123456789abcdef/640.webp", "RIFF");
        dist.insert(".well-known/agent-card.json", "{}");
        dist
    }

    #[test]
    fn every_host_resolves_a_directory_and_answers_404_with_a_status() {
        let dist = upload();
        for host in Host::ALL {
            let page = host.serve(&dist, "/guides/install/");
            assert_eq!(page.status, 200, "{}", host.name());
            assert_eq!(page.body, b"<!doctype html><p>install</p>");
            assert!(page.header("ETag").is_some(), "{}", host.name());
            let bare = host.serve(&dist, "/guides/install");
            assert!(
                (300..400).contains(&bare.status),
                "{}: {}",
                host.name(),
                bare.status
            );
            assert_eq!(bare.location(), Some("/guides/install/"));
            let missing = host.serve(&dist, "/nowhere/");
            assert_eq!(missing.status, 404, "{}", host.name());
        }
        assert_eq!(
            Host::Netlify.serve(&dist, "/nowhere/").body,
            b"<!doctype html><p>not found</p>"
        );
        assert_ne!(
            Host::WebServer.serve(&dist, "/nowhere/").body,
            b"<!doctype html><p>not found</p>"
        );
    }

    #[test]
    fn the_hosts_that_read_headers_send_the_policy_and_the_others_do_not() {
        let dist = upload();
        for host in [Host::CloudflarePages, Host::Netlify, Host::Vercel] {
            let page = host.serve(&dist, "/guides/install/");
            assert!(
                page.header("Content-Security-Policy").is_some(),
                "{}",
                host.name()
            );
            assert_eq!(page.header("X-Frame-Options"), Some("DENY"));
            assert_eq!(page.header("Cache-Control"), Some(headers::CACHE_HTML));
            let widget = host.serve(&dist, "/embed/widget/");
            assert!(
                widget
                    .header("Content-Security-Policy")
                    .is_some_and(|csp| csp.ends_with("frame-ancestors https://app.acme.com")),
                "{}",
                host.name()
            );
            let asset = host.serve(&dist, "/_liyasa/theme.0123456789abcdef.css");
            assert_eq!(
                asset.header("Cache-Control"),
                Some(headers::CACHE_IMMUTABLE)
            );
        }
        for host in [Host::GitHubPages, Host::S3CloudFront, Host::WebServer] {
            let page = host.serve(&dist, "/guides/install/");
            assert!(
                page.header("Content-Security-Policy").is_none(),
                "{}",
                host.name()
            );
        }
        assert_eq!(
            Host::GitHubPages.serve(&dist, "/").header("Cache-Control"),
            Some("max-age=600")
        );
    }

    #[test]
    fn redirect_rules_are_honoured_where_the_file_is_read() {
        let dist = upload();
        assert_eq!(Host::Netlify.serve(&dist, "/old").status, 301);
        assert_eq!(
            Host::Netlify.serve(&dist, "/old").location(),
            Some("/guides/install")
        );
        assert_eq!(Host::CloudflarePages.serve(&dist, "/old/").status, 301);
        let vercel = Host::Vercel.serve(&dist, "/old");
        assert_eq!(vercel.status, 308);
        assert_eq!(vercel.location(), Some("/guides/install"));
        assert_eq!(Host::GitHubPages.serve(&dist, "/old").status, 404);

        let mut with_pages = dist.clone();
        for file in fallback::pages(
            &[hosting::Redirect {
                source: "/old".to_owned(),
                destination: "/guides/install".to_owned(),
                status: 301,
            }],
            |path| dist.contains(path),
        ) {
            with_pages.insert(&file.path, file.contents.as_bytes());
        }
        let github = Host::GitHubPages.serve(&with_pages, "/old/");
        assert_eq!(github.status, 200);
        assert!(
            Host::GitHubPages
                .host_headers(&with_pages)
                .javascript_redirects
        );
        assert!(!Host::Netlify.host_headers(&with_pages).javascript_redirects);
    }

    #[test]
    fn wildcards_and_parameters_match_like_the_hosts_do() {
        let mut dist = Dist::default();
        dist.insert(
            "_redirects",
            "/v1/* /v2/:splat 301\n/docs/:slug /guides/:slug 302\n/exact /there 301\n",
        );
        let host = Host::CloudflarePages;
        assert_eq!(host.serve(&dist, "/v1/a/b").location(), Some("/v2/a/b"));
        assert_eq!(host.serve(&dist, "/v1").location(), Some("/v2/"));
        let docs = host.serve(&dist, "/docs/install");
        assert_eq!(docs.status, 302);
        assert_eq!(docs.location(), Some("/guides/install"));
        assert_eq!(host.serve(&dist, "/docs/install/deeper").status, 404);
        assert_eq!(host.serve(&dist, "/exact/").location(), Some("/there"));
    }

    #[test]
    fn the_spec_view_matches_what_each_host_sends() {
        let dist = upload();
        let netlify = Host::Netlify.host_headers(&dist);
        assert_eq!(
            netlify.markdown_content_type.as_deref(),
            Some(headers::MARKDOWN_TYPE)
        );
        assert_eq!(netlify.cache_control.as_deref(), Some(headers::CACHE_HTML));
        assert!(netlify.etag);
        assert!(!netlify.last_modified);
        assert_eq!(netlify.not_found_status, 404);
        assert_eq!(netlify.redirect_status, Some(301));

        let github = Host::GitHubPages.host_headers(&dist);
        assert_eq!(github.cache_control.as_deref(), Some("max-age=600"));
        assert!(github.last_modified);
        assert_eq!(github.redirect_status, None);

        let server = Host::WebServer.host_headers(&dist);
        assert_eq!(
            server.markdown_content_type.as_deref(),
            Some("application/octet-stream")
        );
        assert_eq!(server.cache_control, None);
    }

    #[test]
    fn github_pages_drops_underscore_and_dot_paths_until_nojekyll_is_there() {
        const EXCLUDED: [&str; 3] = [
            "/_liyasa/theme.0123456789abcdef.css",
            "/_image/0123456789abcdef/640.webp",
            "/.well-known/agent-card.json",
        ];
        let host = Host::GitHubPages;

        // An upload without the marker: the theme, the images, and the agent
        // surfaces are all gone, and the pages that reference them are not, so
        // the build looks successful and the site renders unstyled.
        let mut bare = upload();
        bare.remove(NOJEKYLL_FILE);
        for path in EXCLUDED {
            assert_eq!(host.serve(&bare, path).status, 404, "{path}");
        }
        assert_eq!(host.serve(&bare, "/guides/install/").status, 200);
        assert_eq!(host.serve(&bare, "/guides/install.md").status, 200);
        assert_eq!(host.serve(&bare, "/404.html").status, 200);
        assert_eq!(host.dropped(&bare).len(), EXCLUDED.len());

        // What `hosting::generate` actually writes, which is the fix.
        let built = upload();
        assert!(
            built.contains(NOJEKYLL_FILE),
            "every build emits the marker (RFC 1202)"
        );
        for path in EXCLUDED {
            assert_eq!(host.serve(&built, path).status, 200, "{path}");
        }
        assert!(host.dropped(&built).is_empty());
    }

    #[test]
    fn the_other_hosts_publish_what_they_are_given() {
        let dist = upload();
        for host in Host::ALL {
            if host == Host::GitHubPages {
                continue;
            }
            for path in [
                "/_liyasa/theme.0123456789abcdef.css",
                "/_image/0123456789abcdef/640.webp",
                "/.well-known/agent-card.json",
            ] {
                assert_eq!(
                    host.serve(&dist, path).status,
                    200,
                    "{}: {path}",
                    host.name()
                );
            }
            assert!(host.dropped(&dist).is_empty(), "{}", host.name());
        }
    }

    #[test]
    fn a_host_never_serves_the_configuration_it_reads() {
        let dist = upload();
        // Cloudflare Pages and Netlify consume `_headers` and `_redirects`;
        // Vercel consumes `vercel.json`. A bucket has no such notion and
        // serves them as the files they are.
        assert_eq!(Host::Netlify.serve(&dist, "/_headers").status, 404);
        assert_eq!(
            Host::CloudflarePages.serve(&dist, "/_redirects").status,
            404
        );
        assert_eq!(Host::Vercel.serve(&dist, "/vercel.json").status, 404);
        assert_eq!(Host::Netlify.serve(&dist, "/vercel.json").status, 200);
        assert_eq!(Host::S3CloudFront.serve(&dist, "/_headers").status, 200);
        // None of that counts as dropping the site's content.
        assert!(Host::Netlify.dropped(&dist).is_empty());
    }

    #[test]
    fn cloudflare_pages_drops_dotfiles_but_not_well_known() {
        let mut dist = upload();
        dist.insert(NOJEKYLL_FILE, "");
        let host = Host::CloudflarePages;
        assert_eq!(
            host.serve(&dist, "/.well-known/agent-card.json").status,
            200
        );
        // The marker is not uploaded there, and nothing asks for it.
        assert!(!host.publishes(&dist, NOJEKYLL_FILE));
        assert!(host.dropped(&dist).is_empty());
    }

    #[test]
    fn the_probe_finds_its_requests_in_the_upload() {
        let probe = Probe::from(&upload());
        assert_eq!(probe.page, "/embed/widget/");
        assert_eq!(probe.markdown.as_deref(), Some("/guides/install.md"));
        assert_eq!(probe.redirect_source.as_deref(), Some("/old"));
        // Either immutable directory will do; the upload holds one of each and
        // the probe takes whichever it meets first.
        assert_eq!(
            probe.hashed_asset.as_deref(),
            Some("/_image/0123456789abcdef/640.webp")
        );
    }
}
