//! The reference site both indexes are ranked against, and the query corpus
//! §12.2 asks for (`corpus.toml`).

use liyasa_core::ids::{Locale, Route, Version};
use liyasa_search::doc::{DocKind, EndpointFacts, PageMeta, SectionDocument};
use liyasa_search::idx::field::ByField;

/// One section, spelled out rather than extracted, so a ranking test is not
/// also a test of the extractor.
fn section(route: &str, anchor: &str, title: &str, heading: &str, body: &str) -> SectionDocument {
    SectionDocument {
        route: Route::new(route),
        anchor: anchor.to_owned(),
        title: title.to_owned(),
        section: heading.to_owned(),
        breadcrumb: vec!["Guides".to_owned()],
        body: body.to_owned(),
        code: String::new(),
        keywords: Vec::new(),
        tab: Some("docs".to_owned()),
        version: None,
        locale: Locale::new("en"),
        kind: DocKind::Page,
        boost: 1.0,
        groups: Vec::new(),
        regions: Vec::new(),
        updated: Some(1_700_000_000_000),
    }
}

fn endpoint(route: &str, method: &str, path: &str, summary: &str) -> SectionDocument {
    let mut meta = PageMeta::new(
        Route::new(route),
        format!("{method} {path}"),
        Locale::new("en"),
    );
    meta.kind = DocKind::Endpoint;
    meta.tab = Some("api".to_owned());
    meta.endpoint = Some(EndpointFacts {
        method: method.to_owned(),
        path: path.to_owned(),
        summary: summary.to_owned(),
        parameters: vec!["id — the user's identifier".to_owned()],
        responses: vec!["200".to_owned(), "404".to_owned()],
    });
    let mut document = section(
        route,
        "",
        &format!("{method} {path}"),
        &format!("{method} {path}"),
        summary,
    );
    document.kind = DocKind::Endpoint;
    document.tab = Some("api".to_owned());
    document.keywords = vec![method.to_owned(), "200".to_owned(), "404".to_owned()];
    document.code = format!("{method} {path}");
    document.body = format!("{summary} id — the user's identifier");
    document
}

/// Twelve sections that between them exercise every ranking rule SRC-03 names
/// and every query feature SRC-04 names.
pub fn documents() -> Vec<SectionDocument> {
    let mut out = Vec::new();

    out.push(section(
        "/guides/limits",
        "",
        "Rate limits",
        "Rate limits",
        "Every API key has a rate limit.",
    ));
    out.push(section(
        "/guides/limits",
        "burst",
        "Rate limits",
        "Burst limits",
        "Burst traffic is allowed up to twice the sustained rate limit for ten seconds.",
    ));
    out.push(section(
        "/guides/auth",
        "",
        "Authentication",
        "Authentication",
        "Sign requests with an API key.",
    ));

    let mut keys = section(
        "/guides/auth",
        "api-keys",
        "Authentication",
        "API keys",
        "Create a key in the dashboard and store it in your secrets manager.",
    );
    keys.code = "curl -H 'Authorization: Bearer $KEY' https://api.example.com".to_owned();
    out.push(keys);

    out.push(section(
        "/guides/auth",
        "rotating",
        "Authentication",
        "Rotating a key",
        "Rotate keys every ninety days.",
    ));

    out.push(endpoint(
        "/api-reference/users/get",
        "GET",
        "/users/{id}",
        "Fetch one user by id.",
    ));
    out.push(endpoint(
        "/api-reference/users/create",
        "POST",
        "/users",
        "Create a user.",
    ));

    let mut changelog = section(
        "/changelog/2026-09",
        "",
        "September 2026",
        "September 2026",
        "Rate limits are now per key rather than per organization.",
    );
    changelog.kind = DocKind::Changelog;
    changelog.updated = Some(1_760_000_000_000);
    out.push(changelog);

    let mut german = section(
        "/de/anleitungen/limits",
        "",
        "Ratenbegrenzungen",
        "Ratenbegrenzungen",
        "Jeder API-Schlüssel hat eine Ratenbegrenzung.",
    );
    german.locale = Locale::new("de");
    out.push(german);

    let mut old = section(
        "/v1/guides/limits",
        "",
        "Rate limits",
        "Rate limits",
        "Every API key has a rate limit.",
    );
    old.version = Some(Version::new("v1"));
    old.updated = Some(1_600_000_000_000);
    out.push(old);

    let mut current = section(
        "/v2/guides/limits",
        "",
        "Rate limits",
        "Rate limits",
        "Every API key has a rate limit, and bursts are smoothed.",
    );
    current.version = Some(Version::new("v2"));
    out.push(current);

    let mut internal = section(
        "/internal/runbook",
        "",
        "Raising a limit",
        "Raising a limit",
        "Rate limits can be raised from the admin console.",
    );
    internal.groups = vec!["staff".to_owned()];
    out.push(internal);

    let mut sdk = section(
        "/guides/sdk",
        "getting-a-user",
        "SDK",
        "Getting a user",
        "Call the client helper.",
    );
    sdk.code = "const user = await client.getUserById(id);".to_owned();
    out.push(sdk);

    out
}

/// One row of `corpus.toml`.
#[derive(Debug, Default, Clone)]
pub struct Case {
    pub query: String,
    pub locale: String,
    /// The result that must come first.
    pub top: Option<String>,
    /// URLs that must appear somewhere in the top five.
    pub contains: Vec<String>,
    /// URLs that must not appear at all.
    pub absent: Vec<String>,
    pub expect_empty: bool,
}

/// Reads the corpus. Line-oriented, like `codes.toml`, so the workspace gains
/// no TOML dependency for a file the tests alone read.
pub fn cases() -> Vec<Case> {
    let text = include_str!("../../corpus.toml");
    let mut out: Vec<Case> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[[query]]" {
            out.push(Case {
                locale: "en".to_owned(),
                ..Case::default()
            });
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            panic!("corpus.toml: `{line}` is neither a header nor `key = value`");
        };
        let case = out
            .last_mut()
            .unwrap_or_else(|| panic!("corpus.toml: `{line}` appears before any [[query]]"));
        let key = key.trim();
        let value = value.trim();
        match key {
            "q" => case.query = unquote(value),
            "locale" => case.locale = unquote(value),
            "top" => case.top = Some(unquote(value)),
            "contains" => case.contains = list(value),
            "absent" => case.absent = list(value),
            "empty" => case.expect_empty = value == "true",
            other => panic!("corpus.toml: unknown key `{other}`"),
        }
    }
    out
}

/// A quoted string, with `\"` as the only escape — which is all the corpus
/// needs, because a phrase query is written `q = "\"rate limit\""`.
///
/// Exactly one quote comes off each end, so the escaped ones inside survive.
fn unquote(value: &str) -> String {
    let value = value.trim();
    let inner = value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(value);
    inner.replace("\\\"", "\"")
}

fn list(value: &str) -> Vec<String> {
    value
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(str::trim)
        .filter(|piece| !piece.is_empty())
        .map(unquote)
        .collect()
}

/// A field-length vector, for the tests that assert weights directly.
pub fn lengths(values: [u32; 6]) -> ByField<u32> {
    ByField(values)
}

/// A deterministic pseudo-random source, so the reference site is the same
/// site on every machine and a budget number means something.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // The multiplier is Knuth's for a 64-bit linear congruential
        // generator; the quality needed here is "not a pattern", not
        // cryptographic.
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        self.0 >> 33
    }

    fn pick<'a>(&mut self, from: &[&'a str]) -> &'a str {
        from[self.next() as usize % from.len()]
    }
}

const VOCABULARY: &[&str] = &[
    "account",
    "allow",
    "api",
    "authentication",
    "batch",
    "budget",
    "cache",
    "callback",
    "client",
    "cluster",
    "configuration",
    "connection",
    "credential",
    "dashboard",
    "deploy",
    "endpoint",
    "environment",
    "error",
    "event",
    "export",
    "field",
    "filter",
    "gateway",
    "header",
    "identity",
    "import",
    "instance",
    "integration",
    "key",
    "latency",
    "limit",
    "locale",
    "message",
    "metadata",
    "migration",
    "namespace",
    "node",
    "organization",
    "parameter",
    "payload",
    "permission",
    "pipeline",
    "policy",
    "project",
    "query",
    "queue",
    "quota",
    "rate",
    "region",
    "request",
    "resource",
    "response",
    "retry",
    "role",
    "schema",
    "scope",
    "secret",
    "session",
    "snapshot",
    "status",
    "storage",
    "stream",
    "subscription",
    "tenant",
    "threshold",
    "timeout",
    "token",
    "trace",
    "transaction",
    "usage",
    "user",
    "version",
    "webhook",
    "workspace",
];

const HEADINGS: &[&str] = &[
    "Overview",
    "Getting started",
    "Configuration",
    "Limits",
    "Errors",
    "Examples",
    "Reference",
    "Troubleshooting",
];

/// The 1,000-page reference site SRC-05's budgets are measured against: three
/// sections per page, prose drawn from a documentation vocabulary so term
/// frequencies look like a real corpus rather than like random bytes.
pub fn reference_site(pages: usize) -> Vec<SectionDocument> {
    let mut rng = Rng(0x5eed);
    let mut out = Vec::with_capacity(pages * 3);

    for page in 0..pages {
        let area = VOCABULARY[page % VOCABULARY.len()];
        let route = format!("/docs/{area}/page-{page}");
        let title = format!("{} {page}", capitalize(area));

        for at in 0..3 {
            let heading = if at == 0 {
                title.clone()
            } else {
                HEADINGS[(page + at) % HEADINGS.len()].to_owned()
            };
            let anchor = if at == 0 {
                String::new()
            } else {
                format!("s{at}")
            };
            let mut body = String::with_capacity(700);
            for word in 0..110 {
                if word > 0 {
                    body.push(if word % 18 == 0 { '.' } else { ' ' });
                    if word % 18 == 0 {
                        body.push(' ');
                    }
                }
                body.push_str(rng.pick(VOCABULARY));
            }
            let mut document = section(&route, &anchor, &title, &heading, &body);
            document.breadcrumb = vec!["Docs".to_owned(), capitalize(area)];
            if at == 2 {
                document.code = format!(
                    "const {area} = await client.get{}ById(id);",
                    capitalize(area)
                );
            }
            out.push(document);
        }
    }
    out
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// The same site spread over `locales` locales and `versions` versions, for
/// the sharding rules of §12.2.
pub fn multi_context(pages: usize, locales: &[&str], versions: &[&str]) -> Vec<SectionDocument> {
    let mut out = Vec::new();
    for locale in locales {
        for version in versions {
            for mut document in reference_site(pages) {
                document.route =
                    Route::new(format!("/{locale}/{version}{}", document.route.as_str()));
                document.locale = Locale::new(*locale);
                document.version = Some(Version::new(*version));
                out.push(document);
            }
        }
    }
    out
}
