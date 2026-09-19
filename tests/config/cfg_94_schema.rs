//! CFG-94: `schemas/liyasa.schema.json` is the definition of the config.
//! `liyasa schema config` emits it verbatim, the Rust type is generated from
//! it, and every key the config surface names is in it.

use liyasa_config::json::SpanIndex;
use liyasa_config::{SiteConfig, schema};
use liyasa_core::span::SourceId;
use serde_json::{Map, Value};

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
    "auth.operators",
    "auth.operators[].role",
    "auth.operators[].subject",
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
    "mail.from",
    "mail.smtp.host",
    "mail.smtp.password",
    "mail.smtp.port",
    "mail.smtp.security",
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
    "versions[].tag",
];

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

/// RFC 0105 item 3: CM-93 renders version badges from a tag, and §34.2's own
/// example config writes one. The version object is `additionalProperties:
/// false`, so without this row the example does not validate.
#[test]
fn the_row_the_version_badges_wait_on_has_landed() {
    let schema = Schema::parse();
    assert!(
        schema.has("versions[].tag"),
        "CM-93 has no badge to render without it"
    );
}

/// RFC 0105 item 2. The default is pinned alongside the new member because
/// RFC 0601 reads an absent `build.hashing` as `"none"`: CFG-83 says a config
/// that stays silent gets `filename`, and only the schema can settle which.
#[test]
fn turning_asset_hashing_off_is_sayable() {
    let hashing = Schema::parse()
        .root
        .pointer("/properties/build/properties/hashing")
        .cloned()
        .expect("`build.hashing` is in the schema");
    let values = hashing["enum"]
        .as_array()
        .expect("`build.hashing` is an enum");
    assert!(
        values.iter().any(|value| value == "none"),
        "an author has to be able to turn asset hashing off"
    );
    assert_eq!(
        hashing["default"], "filename",
        "CFG-83: saying nothing still means hashed file names"
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

impl Schema {
    /// Every string enum the schema offers at an object path, as
    /// `(["content", "html"], ["allow", "sanitize", "off"])`. Paths that step
    /// through an array are skipped: the value needs a whole entry around it,
    /// and the entries differ per list.
    fn enums(&self) -> Vec<(Vec<String>, Vec<String>)> {
        let mut found = Vec::new();
        self.collect(&self.root, &mut Vec::new(), &mut Vec::new(), &mut found);
        found
    }

    fn collect(
        &self,
        node: &Value,
        path: &mut Vec<String>,
        visiting: &mut Vec<String>,
        found: &mut Vec<(Vec<String>, Vec<String>)>,
    ) {
        if let Some(reference) = node.get("$ref").and_then(Value::as_str) {
            let name = reference.rsplit('/').next().unwrap_or_default().to_owned();
            if visiting.contains(&name) {
                return;
            }
            let Some(target) = self.root.pointer(&format!("/$defs/{name}")) else {
                return;
            };
            visiting.push(name);
            self.collect(target, path, visiting, found);
            visiting.pop();
            return;
        }
        if node.get("type").and_then(Value::as_str) == Some("string")
            && let Some(Value::Array(values)) = node.get("enum")
        {
            let values: Vec<String> = values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
            if !values.is_empty() && !path.is_empty() {
                found.push((path.clone(), values));
            }
        }
        for key in ["oneOf", "anyOf", "allOf"] {
            if let Some(Value::Array(branches)) = node.get(key) {
                for branch in branches {
                    self.collect(branch, path, visiting, found);
                }
            }
        }
        if let Some(properties) = node.pointer("/properties").and_then(Value::as_object) {
            for (name, property) in properties {
                path.push(name.clone());
                self.collect(property, path, visiting, found);
                path.pop();
            }
        }
    }
}

/// `{"name": "Acme", <path>: value}`, plus whatever the schema requires
/// alongside each step of the path. A block with required siblings — `mail`
/// wants a `from` and an `smtp.host`, because a mail block that could not send
/// is refused — would otherwise fail for the sibling rather than for the enum
/// value under test.
fn config_with(schema: &Schema, path: &[String], value: &str) -> Value {
    fn stub(schema: &Schema, node: &Value) -> Value {
        let node = schema.deref(node);
        if let Some(Value::Array(values)) = node.get("enum")
            && let Some(first) = values.first()
        {
            return first.clone();
        }
        match node.get("type").and_then(Value::as_str) {
            Some("object") => {
                let mut object = Map::new();
                fill(schema, node, &mut object);
                Value::Object(object)
            }
            Some("array") => Value::Array(Vec::new()),
            Some("integer") | Some("number") => Value::from(1),
            Some("boolean") => Value::Bool(true),
            _ => Value::String("x".to_owned()),
        }
    }

    /// Every key `node` requires, except the ones already written.
    fn fill(schema: &Schema, node: &Value, into: &mut Map<String, Value>) {
        let Some(Value::Array(required)) = node.get("required") else {
            return;
        };
        for key in required.iter().filter_map(Value::as_str) {
            if into.contains_key(key) {
                continue;
            }
            let Some(property) = node.pointer("/properties").and_then(|p| p.get(key)) else {
                continue;
            };
            into.insert(key.to_owned(), stub(schema, property));
        }
    }

    // Walk down from the root, collecting the node at each step, then build
    // back up so each level can be filled from its own schema.
    let mut nodes = vec![&schema.root];
    for segment in path {
        let next = nodes
            .last()
            .and_then(|node| schema.deref(node).pointer("/properties"))
            .and_then(|properties| properties.get(segment));
        match next {
            Some(node) => nodes.push(node),
            None => break,
        }
    }

    let mut leaf = Value::String(value.to_owned());
    for (depth, segment) in path.iter().enumerate().rev() {
        let mut object = Map::new();
        object.insert(segment.clone(), leaf);
        if let Some(parent) = nodes.get(depth) {
            fill(schema, schema.deref(parent), &mut object);
        }
        leaf = Value::Object(object);
    }
    let Value::Object(mut object) = leaf else {
        unreachable!("a path always nests at least one object")
    };
    object.insert("name".to_owned(), Value::String("Acme".to_owned()));
    Value::Object(object)
}

/// CFG-94: the schema and the generated type cannot diverge, so every value the
/// schema offers has to be one `SiteConfig` reads.
#[test]
fn every_enum_value_the_schema_offers_is_one_the_type_accepts() {
    let schema = Schema::parse();
    let enums = schema.enums();
    assert!(enums.len() >= 28, "found only {} enums", enums.len());

    for (path, values) in enums {
        if path.iter().any(|segment| segment == "items") {
            continue;
        }
        for value in values {
            let config = config_with(&schema, &path, &value);
            let text = serde_json::to_string(&config).expect("serializes");
            let report = schema::check(&config, &SpanIndex::scan(SourceId(0), &text));
            assert!(
                report.diagnostics.is_empty() && report.unknown.is_empty(),
                "{}: `{value}` does not validate: {:?}",
                path.join("."),
                report.diagnostics
            );
            serde_json::from_value::<SiteConfig>(config).unwrap_or_else(|e| {
                panic!("{}: `{value}` is not in the type: {e}", path.join("."))
            });
        }
    }
}

/// Validates and deserializes, the way `liyasa validate` and the generated
/// type both have to accept a config before an operator can use it.
fn accepts(config: Value) -> Result<(), String> {
    let text = serde_json::to_string(&config).map_err(|e| e.to_string())?;
    let report = schema::check(&config, &SpanIndex::scan(SourceId(0), &text));
    if !report.diagnostics.is_empty() || !report.unknown.is_empty() {
        return Err(format!("{:?} {:?}", report.diagnostics, report.unknown));
    }
    serde_json::from_value::<SiteConfig>(config)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// RFC 1201: RX-112's preload is an opt-in, and `security` is
/// `additionalProperties: false`, so without a row an operator cannot say it.
#[test]
fn hsts_preload_is_sayable_and_off_by_default() {
    let schema = Schema::parse();
    assert!(schema.has("security.hstsPreload"));
    assert_eq!(
        schema
            .root
            .pointer("/properties/security/properties/hstsPreload/default")
            .and_then(Value::as_bool),
        Some(false),
        "preload is a one-way submission to browser lists"
    );
    for value in [true, false] {
        accepts(serde_json::json!({ "name": "Acme", "security": { "hstsPreload": value } }))
            .unwrap_or_else(|e| panic!("hstsPreload: {value}: {e}"));
    }
}

/// RFC 1201: one spelling per ANA-60/61 vendor, so the CSP registry in
/// `liyasa-build` and the config agree on the name.
#[test]
fn every_vendor_the_csp_registry_knows_has_a_spelling() {
    let schema = Schema::parse();
    // ANA-60 analytics, then ANA-61 support.
    for vendor in [
        "adobeAnalytics",
        "amplitude",
        "clarity",
        "clearbit",
        "fathom",
        "ga4",
        "gtm",
        "heap",
        "hightouch",
        "hotjar",
        "koala",
        "logrocket",
        "mixpanel",
        "pirsch",
        "plausible",
        "posthog",
        "segment",
        "intercom",
        "front",
        "crisp",
        "zendesk",
        "plain",
    ] {
        assert!(
            schema.has(&format!("integrations.{vendor}")),
            "`{vendor}` has no row"
        );
    }
}

/// RFC 1201: a vendor key is enabled when present and not `false` or `null`,
/// and its value is CFG-81's `{ id, consent }`.
#[test]
fn a_vendor_key_takes_its_settings_or_switches_itself_off() {
    for value in [
        serde_json::json!({ "id": "abc123", "consent": "required" }),
        serde_json::json!({ "id": "abc123", "consent": "none" }),
        serde_json::json!(true),
        serde_json::json!(false),
        serde_json::json!(null),
    ] {
        accepts(serde_json::json!({ "name": "Acme", "integrations": { "plausible": value } }))
            .unwrap_or_else(|e| panic!("integrations.plausible = {value}: {e}"));
    }
}

/// RFC 1201: a provider name, `{ "provider": … }`, or `true` for the built-in
/// banner.
#[test]
fn cookie_consent_names_a_provider_or_asks_for_the_built_in_banner() {
    for value in [
        serde_json::json!("osano"),
        serde_json::json!("transcend"),
        serde_json::json!("onetrust"),
        serde_json::json!("cookiebot"),
        serde_json::json!("builtin"),
        serde_json::json!({ "provider": "osano" }),
        serde_json::json!(true),
    ] {
        accepts(serde_json::json!({ "name": "Acme", "integrations": { "cookieConsent": value } }))
            .unwrap_or_else(|e| panic!("cookieConsent = {value}: {e}"));
    }
}
