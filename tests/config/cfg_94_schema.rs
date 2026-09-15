//! CFG-94: `schemas/liyasa.schema.json` is the definition of the config.
//! `liyasa schema config` emits it verbatim, the Rust type is generated from
//! it, and every key the config surface names is in it.

use liyasa_config::{SiteConfig, schema};
use serde_json::Value;

const EXAMPLE: &str = include_str!("../../crates/liyasa-config/tests/fixtures/example.json");

/// Every config key the WP-01 packet names, `[]` where a path steps through an
/// array. `scripts/req-coverage.py` makes the same check against the whole PRD;
/// this is the part of it that runs in the workspace.
const KEYS: &[&str] = &[
    "agents",
    "agents.llms.custom",
    "agents.llms.full",
    "agents.llms.fullMaxBytes",
    "agents.llms.split",
    "agents.markdown.includeOpenApiSchema",
    "agents.mcp.enabled",
    "agents.mcp.external",
    "agents.skill.enabled",
    "agents.specVersion",
    "ai",
    "ai.agent.limits.maxFilesChanged",
    "ai.models.assistant",
    "ai.providers.openai.baseUrl",
    "ai.respectNoindex",
    "analytics",
    "analytics.botList",
    "analytics.collector",
    "analytics.enabled",
    "analytics.retention.rawDays",
    "api",
    "api.auth.method",
    "api.baseUrl",
    "asyncapi[].source",
    "auth",
    "auth.jwt.jwksUrl",
    "auth.mode",
    "auth.preview.protection",
    "auth.session.maxAge",
    "authors",
    "automations",
    "automations.definitions",
    "automations.untrustedRunsPerDay",
    "build",
    "build.basePath",
    "build.budget.total",
    "build.cacheSize",
    "build.downloads.pdf",
    "build.drafts",
    "build.env",
    "build.hashing",
    "build.images.eager",
    "build.maxVariantsPerPage",
    "build.minify",
    "build.output",
    "build.prefetch",
    "build.strictLinks",
    "build.strictVerification",
    "content",
    "content.codeblocks.lineNumbers",
    "content.customElements",
    "content.frontmatter.strict",
    "content.html",
    "content.images.breakpoints",
    "content.math",
    "content.related.auto",
    "content.reviewCadence",
    "content.templating.undefined",
    "content.wikilinks",
    "contextRepos",
    "contextRepos[].repo",
    "description",
    "favicon",
    "feeds",
    "feeds.changelog",
    "feeds.updates",
    "footer.branding",
    "footer.links",
    "footer.socials",
    "footer.text",
    "integrations",
    "integrations.cookieConsent",
    "integrations.telemetry",
    "locales[].code",
    "localization",
    "localization.fallback",
    "localization.routeVisitors",
    "logo",
    "logo.href",
    "name",
    "navbar.items",
    "navbar.links",
    "navbar.primary",
    "navbar.primary.type",
    "navigation.autofill",
    "navigation.breadcrumbs",
    "navigation.drilldown",
    "network",
    "network.allowHosts.specRefs",
    "network.allowPrivate",
    "network.denyHosts",
    "network.egressProxy",
    "openapi[].overlays[]",
    "openapi[].source",
    "playground",
    "playground.display",
    "playground.languages",
    "playground.proxy.enabled",
    "playground.requiredOnly",
    "public",
    "redirects.externalAllow",
    "redirects.rules[].source",
    "regions",
    "regions.availability",
    "regions.default",
    "regions.detection",
    "regions.enabled",
    "regions.header",
    "regions.list",
    "search.shortcut",
    "security",
    "security.csp.extraScriptSrc",
    "security.frameAncestors",
    "security.styleAttribute",
    "security.uploads.allowTypes",
    "seo.canonicalOrigin",
    "seo.crawlers",
    "seo.indexing",
    "seo.metatags",
    "seo.organization",
    "seo.robots",
    "seo.sitemap",
    "seo.trailingSlash",
    "server",
    "server.drainTimeout",
    "server.dynamic.concurrency",
    "server.rateLimits.pages",
    "server.trustedProxies",
    "skills",
    "skills.files",
    "skills.groups",
    "theme.appearance",
    "theme.appearance.background",
    "theme.appearance.default",
    "theme.appearance.strict",
    "theme.codeTheme",
    "theme.codeTheme.light",
    "theme.colors",
    "theme.colors.background.dark",
    "theme.colors.background.light",
    "theme.css",
    "theme.fonts",
    "theme.fonts.heading",
    "theme.fonts.subset",
    "theme.icons",
    "theme.icons.library",
    "theme.js",
    "theme.layout",
    "theme.layout.density",
    "theme.layout.sidebarWidth",
    "theme.overrides",
    "theme.preset",
    "verify",
    "versions[].name",
];

/// What RFC 0105 decided the schema is still missing. The file is WP-00's
/// owned path, so this records the gap rather than closing it; when the row
/// lands, the test below fails and the RFC closes.
const OPEN_GAPS: &[&str] = &["versions[].tag"];

struct Schema {
    root: Value,
}

impl Schema {
    fn parse() -> Self {
        Self {
            root: serde_json::from_str(schema::CONFIG_SCHEMA).expect("the schema is valid JSON"),
        }
    }

    fn deref<'a>(&'a self, mut node: &'a Value) -> &'a Value {
        for _ in 0..16 {
            let Some(reference) = node.get("$ref").and_then(Value::as_str) else {
                return node;
            };
            let Some(name) = reference.rsplit('/').next() else {
                return node;
            };
            match self.root.pointer(&format!("/$defs/{name}")) {
                Some(target) => node = target,
                None => return node,
            }
        }
        node
    }

    /// A node and every branch of a `oneOf`, `anyOf`, or `allOf` under it.
    fn branches<'a>(&'a self, node: &'a Value, out: &mut Vec<&'a Value>) {
        let node = self.deref(node);
        out.push(node);
        for key in ["oneOf", "anyOf", "allOf"] {
            if let Some(Value::Array(branches)) = node.get(key) {
                for branch in branches {
                    self.branches(branch, out);
                }
            }
        }
    }

    /// One path segment: a named property, a property of an array's items, or
    /// the value shape of a map.
    fn step<'a>(&'a self, nodes: &[&'a Value], key: &str) -> Vec<&'a Value> {
        let mut next = Vec::new();
        let mut branches = Vec::new();
        for node in nodes {
            branches.clear();
            self.branches(node, &mut branches);
            for branch in &branches {
                if let Some(property) = branch.pointer("/properties").and_then(|p| p.get(key)) {
                    next.push(property);
                }
                if let Some(items) = branch.get("items") {
                    let mut item_branches = Vec::new();
                    self.branches(items, &mut item_branches);
                    for item in item_branches {
                        if let Some(property) = item.pointer("/properties").and_then(|p| p.get(key))
                        {
                            next.push(property);
                        }
                    }
                }
                if let Some(value) = branch.get("additionalProperties").filter(|v| v.is_object()) {
                    next.push(value);
                }
            }
        }
        next
    }

    fn has(&self, path: &str) -> bool {
        let mut nodes = vec![&self.root];
        for segment in path.split('.') {
            nodes = self.step(&nodes, segment.trim_end_matches("[]"));
            if nodes.is_empty() {
                return false;
            }
        }
        true
    }
}

#[test]
fn liyasa_schema_config_emits_the_file_byte_for_byte() {
    let on_disk = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/liyasa.schema.json"
    ))
    .expect("the schema file is readable");
    let emitted = schema::named("config").expect("`liyasa schema config` names it");
    assert_eq!(emitted.json, on_disk);
}

#[test]
fn every_key_the_packet_names_is_in_the_schema() {
    let schema = Schema::parse();
    let missing: Vec<&str> = KEYS
        .iter()
        .copied()
        .filter(|key| !schema.has(key))
        .collect();
    assert_eq!(
        missing,
        Vec::<&str>::new(),
        "CFG-94: the schema is the definition of the config, so a key the requirements name has to exist in it"
    );
}

#[test]
fn the_schema_annotates_each_property_with_its_requirement() {
    let schema = Schema::parse();
    let properties = schema
        .root
        .pointer("/properties")
        .and_then(Value::as_object)
        .expect("the root has properties");
    let unannotated: Vec<&str> = properties
        .iter()
        .filter(|(name, value)| {
            name.as_str() != "$schema" && value.get("x-liyasa-requirement").is_none()
        })
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(unannotated, Vec::<&str>::new());
}

#[test]
fn the_generated_type_round_trips_the_example() {
    let config: SiteConfig = serde_json::from_str(EXAMPLE).expect("typify reads the example");
    let text = serde_json::to_string(&config).expect("a config serializes");
    let again: SiteConfig = serde_json::from_str(&text).expect("its own output deserializes");
    assert_eq!(config, again);
}

#[test]
fn the_row_the_version_badges_wait_on_is_still_missing() {
    let schema = Schema::parse();
    let present: Vec<&str> = OPEN_GAPS
        .iter()
        .copied()
        .filter(|key| schema.has(key))
        .collect();
    assert_eq!(
        present,
        Vec::<&str>::new(),
        "RFC 0105 and RFC 0601 can close: the schema now carries this, so move \
         it into KEYS and delete OPEN_GAPS"
    );
}

#[test]
fn turning_asset_hashing_off_is_still_unsayable() {
    let hashing = Schema::parse()
        .root
        .pointer("/properties/build/properties/hashing/enum")
        .and_then(Value::as_array)
        .cloned()
        .expect("`build.hashing` is an enum");
    assert!(
        !hashing.iter().any(|value| value == "none"),
        "RFC 0105 can close: `build.hashing` now allows \"none\""
    );
}

/// RFC 0105: CM-82 writes `permanent`, the schema writes `status`, and one
/// spelling is the point of CFG-94. `status` is the one that stays.
#[test]
fn a_redirect_says_permanent_by_its_status() {
    let schema = Schema::parse();
    assert!(schema.has("redirects.rules[].status"));
    assert!(!schema.has("redirects.rules[].permanent"));
}
