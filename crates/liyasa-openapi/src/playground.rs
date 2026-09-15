//! The "Try it" playground (API-40 to API-45).
//!
//! The form itself is the reader runtime's; what is here is everything that
//! has to be decided before the browser sees it — which controls an operation
//! offers, which server it calls, and, the part that matters, which hosts the
//! server-side proxy will ever connect to.

use std::collections::BTreeMap;

use liyasa_core::net::{HostPattern, HostSet};
use serde::{Deserialize, Serialize};

use crate::config::{Auth, AuthMethod, Display, ProxyConfig};
use crate::example::{self, Fill, Side};
use crate::field::Field;
use crate::model::{
    Method, OperationRef, ParameterIn, SecuritySchemeKind, Server, ServerVariable, Spec,
};

/// One server the reader may point the form at (API-42).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerChoice {
    pub url: String,
    pub description: Option<String>,
    pub variables: Vec<ServerVariableChoice>,
    /// The URL with every variable at its default, which is what the form
    /// starts on.
    pub resolved: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerVariableChoice {
    pub name: String,
    pub default: String,
    pub options: Vec<String>,
    pub description: Option<String>,
}

/// One control the form shows for authentication (API-43).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthControl {
    pub scheme: String,
    pub kind: String,
    /// Where an API key goes.
    pub location: Option<ParameterIn>,
    /// The header, query, or cookie name for an API key.
    pub name: Option<String>,
    pub scopes: Vec<String>,
    /// OAuth 2.0 and OpenID Connect need these; the rest do not.
    pub authorization_url: Option<String>,
    pub token_url: Option<String>,
    pub discovery_url: Option<String>,
    /// The flow the form drives, when the scheme offers more than one.
    pub flow: Option<String>,
    /// Some schemes a browser cannot present at all (API-45).
    pub usable: bool,
    pub notice: Option<String>,
    /// The credential the reader's own identity supplied (API-44): the token,
    /// the key, or basic's password.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefilled: Option<String>,
    /// Basic authentication's other half.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefilled_user: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormField {
    #[serde(rename = "in")]
    pub location: ParameterIn,
    pub name: String,
    pub required: bool,
    pub deprecated: bool,
    /// What the control is prefilled with (API-31).
    pub value: String,
    pub description: Option<String>,
    pub enumeration: Vec<String>,
    /// `checkbox`, `number`, `select`, `text`.
    pub control: &'static str,
    /// The value came from the reader's identity rather than the spec
    /// (API-44), which a form marks so nobody mistakes it for a placeholder.
    pub from_reader: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BodyForm {
    pub media_type: String,
    /// A JSON editor's starting text, validated against `fields` in the
    /// browser (API-40).
    pub value: String,
    pub required: bool,
    pub fields: Vec<Field>,
    /// The form is a file upload rather than an editor.
    pub multipart: bool,
}

/// Everything the browser needs for one operation.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Playground {
    pub display: Display,
    pub operation_id: Option<String>,
    pub selector: String,
    pub method: Method,
    pub path: String,
    pub servers: Vec<ServerChoice>,
    pub fields: Vec<FormField>,
    pub body: Option<BodyForm>,
    pub auth: Vec<AuthControl>,
    /// Notices for what this deployment cannot do (API-45).
    pub notices: Vec<String>,
    /// Something in here belongs to one reader (API-44), so the page it is
    /// rendered into is that reader's and may not be cached for anyone else.
    pub personalized: bool,
}

impl Playground {
    pub fn build(
        spec: &Spec,
        operation: &OperationRef<'_>,
        display: Display,
        base_url: Option<&str>,
    ) -> Self {
        let display = if operation.operation.liyasa.hidden {
            Display::None
        } else {
            display
        };
        let servers = match base_url {
            Some(url) => vec![ServerChoice {
                url: url.to_owned(),
                description: None,
                variables: Vec::new(),
                resolved: url.to_owned(),
            }],
            None => operation.servers(spec).iter().map(choice).collect(),
        };
        let auth = auth(spec, operation);
        let mut notices = Vec::new();
        if auth.iter().any(|control| !control.usable) {
            notices.push(
                "This operation's authentication cannot be completed in the browser.".to_owned(),
            );
        }
        Self {
            display,
            operation_id: operation.operation.operation_id.clone(),
            selector: operation.selector(),
            method: operation.method,
            path: operation.path.to_owned(),
            servers,
            fields: fields(operation),
            body: body(operation),
            auth,
            notices,
            personalized: false,
        }
    }

    /// Fills in what the site's auth (§19) already knows about this reader,
    /// and returns how many controls and fields it filled (API-44).
    ///
    /// Nothing here reaches Liyasa: the values come from the reader's own
    /// session on this site and go into their own page. [`Self::personalized`]
    /// is what stops that page being served to the next reader.
    pub fn prefill(&mut self, identity: &Identity) -> usize {
        let mut filled = 0;
        for control in &mut self.auth {
            let secret = identity
                .get(&control.scheme)
                .or_else(|| control.name.as_deref().and_then(|name| identity.get(name)))
                .or_else(|| identity.get(generic(&control.kind)));
            if let Some(secret) = secret {
                control.prefilled = Some(secret.to_owned());
                filled += 1;
            }
            if control.kind == "basic"
                && let Some(user) = identity.get("username")
            {
                control.prefilled_user = Some(user.to_owned());
            }
        }
        for field in &mut self.fields {
            if let Some(value) = identity.get(&field.name) {
                field.value = value.to_owned();
                field.from_reader = true;
                filled += 1;
            }
        }
        self.personalized = filled > 0;
        filled
    }

    /// The notice a static export shows when an operation needs the proxy
    /// (API-45).
    pub fn static_export_notice(&self) -> Option<String> {
        self.display.sends_requests().then(|| {
            "This site is a static export: requests go straight from your browser, so an API \
             that does not allow cross-origin requests will refuse them."
                .to_owned()
        })
    }
}

fn choice(server: &Server) -> ServerChoice {
    ServerChoice {
        url: server.url.clone(),
        description: server.description.clone(),
        variables: server
            .variables
            .iter()
            .map(|(name, variable)| variable_choice(name, variable))
            .collect(),
        resolved: server.resolve(&Default::default()),
    }
}

fn variable_choice(name: &str, variable: &ServerVariable) -> ServerVariableChoice {
    ServerVariableChoice {
        name: name.to_owned(),
        default: variable.default.clone(),
        options: variable.enumeration.clone(),
        description: variable.description.clone(),
    }
}

fn fields(operation: &OperationRef<'_>) -> Vec<FormField> {
    let mut out = Vec::new();
    for location in ParameterIn::ALL {
        for parameter in operation.parameters() {
            if parameter.location != location || parameter.liyasa.hidden {
                continue;
            }
            let schema = parameter.schema.clone().unwrap_or_default();
            let value = parameter
                .example
                .clone()
                .or_else(|| parameter.examples.values().find_map(|e| e.value.clone()))
                .unwrap_or_else(|| example::of(&schema, Side::Request, Fill::All));
            out.push(FormField {
                location,
                name: parameter.name.clone(),
                required: parameter.required,
                deprecated: parameter.deprecated,
                value: example::as_text(&value),
                description: parameter.description.clone(),
                enumeration: schema.enumeration.iter().map(example::as_text).collect(),
                control: control_for(&schema),
                from_reader: false,
            });
        }
    }
    out
}

fn control_for(schema: &crate::model::Schema) -> &'static str {
    use crate::model::SchemaType::{Boolean, Integer, Number};
    if !schema.enumeration.is_empty() {
        return "select";
    }
    if schema.is(Boolean) {
        return "checkbox";
    }
    if schema.is(Integer) || schema.is(Number) {
        return "number";
    }
    "text"
}

fn body(operation: &OperationRef<'_>) -> Option<BodyForm> {
    let body = operation.operation.request_body.as_ref()?;
    let (media_type, media) = body.preferred()?;
    let schema = media.schema.clone().unwrap_or_default();
    let value = media
        .example
        .clone()
        .or_else(|| media.examples.values().find_map(|e| e.value.clone()))
        .unwrap_or_else(|| example::of(&schema, Side::Request, Fill::All));
    let base = media_type.split(';').next().unwrap_or(media_type).trim();
    let root = Field::of_schema("body", &schema, body.required, crate::field::DEPTH);
    Some(BodyForm {
        media_type: base.to_owned(),
        value: serde_json::to_string_pretty(&value).unwrap_or_default(),
        required: body.required,
        fields: if root.children.is_empty() {
            vec![root]
        } else {
            root.children
        },
        multipart: base.starts_with("multipart/"),
    })
}

fn auth(spec: &Spec, operation: &OperationRef<'_>) -> Vec<AuthControl> {
    let mut out = Vec::new();
    for requirement in operation.security(spec) {
        for (name, scopes) in requirement.schemes() {
            let Some(scheme) = spec.components.security_schemes.get(name) else {
                continue;
            };
            let mut control = AuthControl {
                scheme: name.to_owned(),
                kind: String::new(),
                location: None,
                name: None,
                scopes: scopes.to_vec(),
                authorization_url: None,
                token_url: None,
                discovery_url: None,
                flow: None,
                usable: scheme.drivable_in_a_browser(),
                notice: None,
                prefilled: None,
                prefilled_user: None,
            };
            match &scheme.kind {
                SecuritySchemeKind::Http { scheme, .. } if scheme.eq_ignore_ascii_case("basic") => {
                    control.kind = "basic".to_owned();
                }
                SecuritySchemeKind::Http { .. } => control.kind = "bearer".to_owned(),
                SecuritySchemeKind::ApiKey { name, location } => {
                    control.kind = "apiKey".to_owned();
                    control.name = Some(name.clone());
                    control.location = Some(*location);
                }
                SecuritySchemeKind::OAuth2 { flows } => {
                    control.kind = "oauth2".to_owned();
                    // Authorization code with PKCE first, then client
                    // credentials; implicit only when it is all there is, and
                    // then with a warning (API-43).
                    if let Some(flow) = &flows.authorization_code {
                        control.flow = Some("authorizationCode".to_owned());
                        control.authorization_url = flow.authorization_url.clone();
                        control.token_url = flow.token_url.clone();
                    } else if let Some(flow) = &flows.client_credentials {
                        control.flow = Some("clientCredentials".to_owned());
                        control.token_url = flow.token_url.clone();
                    } else if let Some(flow) = &flows.implicit {
                        control.flow = Some("implicit".to_owned());
                        control.authorization_url = flow.authorization_url.clone();
                        control.notice = Some(
                            "This API offers only the implicit flow, which returns the token in \
                             the URL; it is deprecated and the token is easier to leak."
                                .to_owned(),
                        );
                    } else if let Some(flow) = &flows.password {
                        control.flow = Some("password".to_owned());
                        control.token_url = flow.token_url.clone();
                    }
                }
                SecuritySchemeKind::OpenIdConnect { url } => {
                    control.kind = "openIdConnect".to_owned();
                    control.discovery_url = Some(url.clone());
                }
                SecuritySchemeKind::MutualTls => {
                    control.kind = "mutualTLS".to_owned();
                    control.notice = Some(
                        "This operation needs a client certificate, which a browser cannot \
                         present here."
                            .to_owned(),
                    );
                }
            }
            out.push(control);
        }
    }
    out
}

/// The manual-page equivalent: `api.auth` becomes one control (API-20).
pub fn manual_auth(auth: &Auth) -> Option<AuthControl> {
    let kind = match auth.method {
        AuthMethod::None => return None,
        AuthMethod::Bearer => "bearer",
        AuthMethod::Basic => "basic",
        AuthMethod::ApiKey => "apiKey",
    };
    Some(AuthControl {
        scheme: "api".to_owned(),
        kind: kind.to_owned(),
        location: auth.location,
        name: auth.name.clone(),
        scopes: Vec::new(),
        authorization_url: None,
        token_url: None,
        discovery_url: None,
        flow: None,
        usable: true,
        notice: None,
        prefilled: None,
        prefilled_user: None,
    })
}

/// What the site's auth knows about the reader, for the playground to start
/// from (API-44).
///
/// The keys are whatever §19 hands over. A control is matched by its scheme id
/// first, then by the header, query, or cookie name an API key travels in,
/// then by the generic name for its kind — `token`, `apiKey`, `password` —
/// so a site that calls its key one thing and its spec another still matches.
#[derive(Debug, Clone, Default)]
pub struct Identity {
    values: BTreeMap<String, String>,
}

impl Identity {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.insert(key, value);
        self
    }

    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.values.insert(key.into().to_lowercase(), value.into());
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(&key.to_lowercase()).map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// The name the rest of Liyasa uses for one kind of credential.
fn generic(kind: &str) -> &'static str {
    match kind {
        "apiKey" => "apiKey",
        "basic" => "password",
        _ => "token",
    }
}

// ---- the proxy (API-41) ----

/// Which deployment is asking. A preview of a branch nobody has vouched for
/// gets production's allow list, never its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Deployment {
    Production,
    Preview { trusted: bool },
}

/// Where an allow list is derived from: one deployment's processed spec and
/// config.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProxySource {
    /// Every `servers[].url` of the processed spec, already resolved.
    pub servers: Vec<String>,
    /// `api.baseUrl`.
    pub base_url: Option<String>,
    /// `playground.proxy.allow`.
    pub configured: Vec<String>,
}

impl ProxySource {
    /// Reads a loaded spec's servers, with every variable at its default.
    pub fn of(spec: &Spec, config: &ProxyConfig, base_url: Option<&str>) -> Self {
        Self {
            servers: spec
                .servers
                .iter()
                .map(|server| server.resolve(&Default::default()))
                .collect(),
            base_url: base_url.map(str::to_owned),
            configured: config.allow.clone(),
        }
    }

    fn hosts(&self) -> Vec<String> {
        let mut out = Vec::new();
        for url in self.servers.iter().chain(self.base_url.iter()) {
            if let Some(host) = host_of(url) {
                out.push(host);
            }
        }
        for pattern in &self.configured {
            // A configured entry is already a host, or a URL to take one from.
            out.push(host_of(pattern).unwrap_or_else(|| pattern.trim().to_ascii_lowercase()));
        }
        out.retain(|host| !host.is_empty());
        out.dedup();
        out
    }
}

/// The hosts this deployment's proxy may connect to.
///
/// `None` is "no proxy at all", which is what a preview gets when the project
/// has never had a production deployment: with nothing to derive a list from,
/// the only safe list is no list. A fork's pull request therefore cannot point
/// the proxy anywhere, whatever it writes in its `servers` block.
pub fn allow_list(
    production: Option<&ProxySource>,
    own: &ProxySource,
    deployment: Deployment,
    config: &ProxyConfig,
) -> Option<HostSet> {
    if !config.enabled {
        return None;
    }
    let source = match deployment {
        Deployment::Production => own,
        // A branch someone has vouched for may add a server; one nobody has
        // may not, and neither may one with no production deployment behind
        // it (GIT-31).
        Deployment::Preview { trusted: true } => match production {
            Some(_) => own,
            None => return None,
        },
        Deployment::Preview { trusted: false } => production?,
    };
    let hosts = source.hosts();
    if hosts.is_empty() {
        return None;
    }
    Some(HostSet(hosts.into_iter().map(HostPattern::Exact).collect()))
}

fn host_of(url: &str) -> Option<String> {
    let url = url.trim();
    let rest = url
        .split_once("://")
        .map_or(url, |(_, rest)| rest)
        .trim_start_matches('/');
    let host = rest
        .split(['/', '?', '#'])
        .next()?
        .rsplit('@')
        .next()?
        .split(':')
        .next()?;
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

/// Headers the proxy forwards: the ones the playground form set, and nothing
/// that carries the reader's identity on this site (API-41).
///
/// The rule is an allow list of what the form produces rather than a deny list
/// of what a browser adds, because a deny list is a list of the headers
/// someone thought of.
pub fn forwardable(form_headers: &[(String, String)]) -> Vec<(String, String)> {
    form_headers
        .iter()
        .filter(|(name, _)| !is_identity_header(name))
        .cloned()
        .collect()
}

fn is_identity_header(name: &str) -> bool {
    const NEVER: &[&str] = &[
        "cookie",
        "set-cookie",
        "host",
        "referer",
        "origin",
        "forwarded",
        "x-forwarded-for",
        "x-forwarded-host",
        "x-forwarded-proto",
        "x-real-ip",
    ];
    let lower = name.trim().to_ascii_lowercase();
    NEVER.contains(&lower.as_str()) || lower.starts_with("sec-")
}

/// What the proxy is allowed to record about a request (API-41, ANA-01).
///
/// No URL, no headers, no body: an operation id, which class of status came
/// back, and how long it took.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyEvent {
    pub operation_id: String,
    /// `2xx`, `4xx`, and so on.
    pub status_class: String,
    pub latency_ms: u64,
}

impl ProxyEvent {
    pub fn new(operation_id: impl Into<String>, status: u16, latency_ms: u64) -> Self {
        Self {
            operation_id: operation_id.into(),
            status_class: format!("{}xx", status / 100),
            latency_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load;

    const SPEC: &str = r##"
openapi: 3.1.0
info: { title: T, version: "1" }
servers:
  - url: https://{region}.api.example.com/v1
    variables:
      region: { default: eu, enum: [eu, us] }
paths:
  /widgets:
    post:
      operationId: makeWidget
      security:
        - oauth: [write]
      parameters:
        - { name: dryRun, in: query, schema: { type: boolean } }
        - { name: kind, in: query, schema: { type: string, enum: [bolt, nut] } }
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: object
              required: [name]
              properties:
                name: { type: string, examples: ["Bolt"] }
      responses: { "201": { description: made } }
  /mtls:
    get:
      operationId: secure
      security:
        - mtls: []
      responses: { "200": { description: ok } }
components:
  securitySchemes:
    oauth:
      type: oauth2
      flows:
        authorizationCode:
          authorizationUrl: https://example.com/authorize
          tokenUrl: https://example.com/token
          scopes: { write: Write }
        implicit:
          authorizationUrl: https://example.com/implicit
          scopes: { write: Write }
    mtls: { type: mutualTLS }
"##;

    fn spec() -> Spec {
        load::from_bytes("api", "api.yaml", SPEC.as_bytes())
            .expect("the spec loads")
            .spec
    }

    fn playground(operation_id: &str, base_url: Option<&str>) -> Playground {
        let spec = spec();
        let operation = spec
            .by_operation_id(operation_id)
            .expect("the operation is there");
        Playground::build(&spec, &operation, Display::Interactive, base_url)
    }

    #[test]
    fn a_server_variable_becomes_a_control_with_its_options() {
        let playground = playground("makeWidget", None);
        assert_eq!(playground.servers.len(), 1);
        assert_eq!(
            playground.servers[0].resolved,
            "https://eu.api.example.com/v1"
        );
        assert_eq!(playground.servers[0].variables[0].options, vec!["eu", "us"]);
    }

    #[test]
    fn a_base_url_override_replaces_the_specs_servers() {
        let playground = playground("makeWidget", Some("https://localhost:8080"));
        assert_eq!(playground.servers.len(), 1);
        assert_eq!(playground.servers[0].resolved, "https://localhost:8080");
    }

    #[test]
    fn a_parameters_control_follows_its_schema() {
        let playground = playground("makeWidget", None);
        let controls: Vec<(&str, &str)> = playground
            .fields
            .iter()
            .map(|field| (field.name.as_str(), field.control))
            .collect();
        assert_eq!(controls, vec![("dryRun", "checkbox"), ("kind", "select")]);
        assert_eq!(playground.fields[1].enumeration, vec!["bolt", "nut"]);
    }

    #[test]
    fn the_body_editor_starts_from_the_specs_example() {
        let playground = playground("makeWidget", None);
        let body = playground.body.as_ref().expect("there is a body");
        assert_eq!(body.media_type, "application/json");
        assert!(body.value.contains("Bolt"), "{}", body.value);
        assert!(body.required);
        assert!(!body.multipart);
    }

    #[test]
    fn oauth_drives_authorization_code_rather_than_the_deprecated_implicit_flow() {
        let playground = playground("makeWidget", None);
        let control = &playground.auth[0];
        assert_eq!(control.flow.as_deref(), Some("authorizationCode"));
        assert_eq!(
            control.authorization_url.as_deref(),
            Some("https://example.com/authorize")
        );
        assert!(control.notice.is_none());
    }

    #[test]
    fn a_scheme_a_browser_cannot_drive_says_so_rather_than_failing_later() {
        let playground = playground("secure", None);
        assert!(!playground.auth[0].usable);
        assert!(playground.auth[0].notice.is_some());
        assert_eq!(playground.notices.len(), 1);
    }

    #[test]
    fn a_hidden_operation_offers_no_playground_whatever_the_site_configures() {
        let loaded = load::from_bytes(
            "api",
            "api.yaml",
            br##"
openapi: 3.1.0
info: { title: T, version: "1" }
paths:
  /secret:
    get:
      operationId: secret
      x-liyasa: { hidden: true }
      responses: { "200": { description: ok } }
"##,
        )
        .expect("the spec loads");
        let spec = loaded.spec;
        let operation = spec
            .by_operation_id("secret")
            .expect("the operation is read");
        let playground = Playground::build(&spec, &operation, Display::Interactive, None);
        assert_eq!(playground.display, Display::None);
    }

    // ---- the proxy ----

    fn production() -> ProxySource {
        ProxySource {
            servers: vec!["https://api.example.com/v1".to_owned()],
            base_url: None,
            configured: vec!["mirror.example.com".to_owned()],
        }
    }

    fn fork() -> ProxySource {
        ProxySource {
            servers: vec!["https://attacker.example.net".to_owned()],
            base_url: None,
            configured: vec!["attacker.example.net".to_owned()],
        }
    }

    fn enabled() -> ProxyConfig {
        ProxyConfig {
            enabled: true,
            allow: Vec::new(),
        }
    }

    #[test]
    fn a_disabled_proxy_has_no_allow_list_at_all() {
        assert_eq!(
            allow_list(
                Some(&production()),
                &production(),
                Deployment::Production,
                &ProxyConfig::default()
            ),
            None
        );
    }

    #[test]
    fn production_allows_its_own_servers_base_url_and_configured_hosts() {
        let list = allow_list(
            Some(&production()),
            &production(),
            Deployment::Production,
            &enabled(),
        )
        .expect("a list");
        assert!(list.matches("api.example.com"));
        assert!(list.matches("mirror.example.com"));
        assert!(!list.matches("elsewhere.example.net"));
    }

    #[test]
    fn a_preview_of_an_untrusted_branch_cannot_add_a_host_of_its_own() {
        let list = allow_list(
            Some(&production()),
            &fork(),
            Deployment::Preview { trusted: false },
            &enabled(),
        )
        .expect("a list");
        assert!(
            list.matches("api.example.com"),
            "production's list is what it gets"
        );
        assert!(
            !list.matches("attacker.example.net"),
            "the fork's own servers block is never consulted"
        );
    }

    #[test]
    fn a_preview_with_no_production_deployment_gets_no_proxy() {
        assert_eq!(
            allow_list(
                None,
                &fork(),
                Deployment::Preview { trusted: false },
                &enabled()
            ),
            None
        );
        assert_eq!(
            allow_list(
                None,
                &production(),
                Deployment::Preview { trusted: true },
                &enabled()
            ),
            None,
            "a trusted branch is still a branch of a project with nothing to derive from"
        );
    }

    #[test]
    fn a_trusted_preview_may_add_a_server_once_production_exists() {
        let mut own = production();
        own.servers.push("https://staging.example.com".to_owned());
        let list = allow_list(
            Some(&production()),
            &own,
            Deployment::Preview { trusted: true },
            &enabled(),
        )
        .expect("a list");
        assert!(list.matches("staging.example.com"));
    }

    #[test]
    fn a_source_with_nothing_in_it_produces_no_list_rather_than_an_empty_one() {
        assert_eq!(
            allow_list(
                Some(&ProxySource::default()),
                &ProxySource::default(),
                Deployment::Production,
                &enabled()
            ),
            None
        );
    }

    #[test]
    fn a_host_is_read_out_of_whatever_shape_the_url_is_in() {
        assert_eq!(
            host_of("https://api.example.com/v1"),
            Some("api.example.com".to_owned())
        );
        assert_eq!(
            host_of("http://API.example.com:8080/x"),
            Some("api.example.com".to_owned())
        );
        assert_eq!(
            host_of("api.example.com"),
            Some("api.example.com".to_owned())
        );
        assert_eq!(
            host_of("https://user:pass@api.example.com/"),
            Some("api.example.com".to_owned())
        );
        assert_eq!(host_of(""), None);
    }

    #[test]
    fn the_proxy_forwards_the_forms_headers_and_nothing_that_identifies_the_reader() {
        let sent = vec![
            ("Authorization".to_owned(), "Bearer t".to_owned()),
            ("Cookie".to_owned(), "session=1".to_owned()),
            ("X-Trace".to_owned(), "abc".to_owned()),
            ("Sec-Fetch-Site".to_owned(), "same-origin".to_owned()),
            ("X-Forwarded-For".to_owned(), "203.0.113.1".to_owned()),
        ];
        let kept = forwardable(&sent);
        let forwarded: Vec<&str> = kept.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(forwarded, vec!["Authorization", "X-Trace"]);
    }

    #[test]
    fn the_analytics_event_holds_only_what_api_41_allows() {
        let event = ProxyEvent::new("makeWidget", 201, 42);
        assert_eq!(event.status_class, "2xx");
        let json = serde_json::to_string(&event).expect("it serializes");
        assert_eq!(
            json,
            r#"{"operationId":"makeWidget","statusClass":"2xx","latencyMs":42}"#
        );
    }

    #[test]
    fn a_static_export_says_what_it_cannot_do() {
        assert!(
            playground("makeWidget", None)
                .static_export_notice()
                .is_some()
        );
        let spec = spec();
        let operation = spec.by_operation_id("makeWidget").expect("the operation");
        let simple = Playground::build(&spec, &operation, Display::Simple, None);
        assert_eq!(
            simple.static_export_notice(),
            None,
            "it sends nothing to begin with"
        );
    }
}
