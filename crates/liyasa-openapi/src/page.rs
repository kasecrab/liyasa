//! One endpoint page (API-10, API-11, API-12, API-04).
//!
//! Everything a renderer needs for one operation, decided once: the sections
//! and their anchors, the right rail's samples and examples, and the places a
//! Markdown page may inject its own content.

use serde::Serialize;

use crate::codegen::{Registry, Sample};
use crate::config::Display;
use crate::example::{self, Fill, Side};
use crate::field::Field;
use crate::model::{
    Callback, ExternalDocs, Link, Method, OperationRef, OrderedMap, ParameterIn, SecurityScheme,
    SecuritySchemeKind, Server, Spec,
};
use crate::sample::{Options, Request};
use crate::tree::Value;

/// The points a Markdown page may inject content at (API-04).
pub const SLOTS: &[&str] = &[
    "before-request",
    "after-params",
    "before-responses",
    "after-responses",
    "rail-top",
    "rail-bottom",
];

/// Content a Markdown page contributes, in both sinks: an endpoint page has an
/// HTML form and a Markdown one (API-14), and injected content has to reach
/// both.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rendered {
    pub html: String,
    pub markdown: String,
}

impl Rendered {
    pub fn is_empty(&self) -> bool {
        self.html.is_empty() && self.markdown.is_empty()
    }
}

/// What a Markdown page adds to a generated one (API-04).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Augmentation {
    /// The page's body, which renders above the generated parameters.
    pub intro: Option<Rendered>,
    /// Slot name to already-rendered content.
    pub slots: OrderedMap<Rendered>,
}

impl Augmentation {
    /// Whether `name` is a slot this page defines. An unknown one is the
    /// caller's to report, with the list of the ones that exist.
    pub fn is_slot(name: &str) -> bool {
        SLOTS.contains(&name)
    }
}

/// Front matter's `openapi: "api GET /users/{id}"`: the spec id, then the
/// selector.
pub fn parse_front_matter(value: &str) -> Option<(&str, &str)> {
    let value = value.trim();
    let (id, rest) = value.split_once(char::is_whitespace)?;
    let rest = rest.trim_start();
    let (method, _) = rest.split_once(' ')?;
    Method::parse(method)?;
    (!id.is_empty()).then_some((id, rest))
}

#[derive(Debug, Clone)]
pub struct BuildOptions {
    pub route: String,
    /// The sample languages, in the order the selector shows them.
    pub languages: Vec<String>,
    pub display: Display,
    pub sample: Options,
    /// Where the processed spec is served (API-50).
    pub download_base: String,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            route: String::new(),
            languages: crate::codegen::DEFAULT_LANGUAGES
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
            display: Display::default(),
            sample: Options::default(),
            download_base: "/openapi".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub spec: String,
    pub selector: String,
    pub route: String,
    pub method: Method,
    pub path: String,
    pub webhook: bool,
    pub title: String,
    pub summary: Option<String>,
    /// Markdown, from `x-liyasa.description` when there is one.
    pub description: Option<String>,
    pub deprecated: bool,
    pub deprecated_note: Option<String>,
    pub tags: Vec<String>,
    pub servers: Vec<Server>,
    /// The alternatives an operation accepts; an empty list is "no auth".
    pub auth: Vec<AuthOption>,
    pub parameters: Vec<Section>,
    pub body: Option<BodySection>,
    pub responses: Vec<ResponseSection>,
    pub callbacks: Vec<CallbackSection>,
    pub external_docs: Option<ExternalDocs>,
    pub rail: Rail,
    pub augmentation: Augmentation,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    #[serde(rename = "in")]
    pub location: ParameterIn,
    pub title: String,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BodySection {
    pub required: bool,
    pub description: Option<String>,
    /// One entry per media type the body accepts, in spec order.
    pub media_types: Vec<MediaSection>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaSection {
    pub media_type: String,
    pub fields: Vec<Field>,
    pub example: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseSection {
    pub status: String,
    pub description: String,
    pub headers: Vec<Field>,
    pub media_types: Vec<MediaSection>,
    pub links: Vec<LinkSection>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkSection {
    pub name: String,
    pub description: Option<String>,
    /// The operation this link points at, when it names one.
    pub operation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallbackSection {
    pub name: String,
    /// The runtime expression the API calls back on.
    pub expression: String,
    pub method: Method,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthOption {
    pub scheme: String,
    pub kind: String,
    pub description: Option<String>,
    pub scopes: Vec<String>,
    /// Whether the playground can drive it from a browser (API-45).
    pub drivable: bool,
}

/// The right rail (API-12).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rail {
    pub samples: Vec<Sample>,
    pub request_example: Option<ExampleBlock>,
    pub responses: Vec<ExampleBlock>,
    /// How much of the playground this operation offers.
    pub display: Display,
    pub spec_download: Download,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExampleBlock {
    /// `200`, or empty for the request.
    pub status: String,
    pub media_type: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Download {
    pub json: String,
    pub yaml: String,
}

impl Page {
    pub fn build(
        spec: &Spec,
        operation: &OperationRef<'_>,
        registry: &Registry,
        options: &BuildOptions,
    ) -> Self {
        let hints = &operation.operation.liyasa;
        let title = hints
            .title
            .clone()
            .or_else(|| operation.operation.summary.clone())
            .unwrap_or_else(|| operation.selector());
        let display = if hints.hidden {
            Display::None
        } else {
            options.display
        };

        Self {
            spec: spec.id.clone(),
            selector: operation.selector(),
            route: options.route.clone(),
            method: operation.method,
            path: operation.path.to_owned(),
            webhook: operation.webhook,
            title,
            summary: operation.operation.summary.clone(),
            description: hints
                .description
                .clone()
                .or_else(|| operation.operation.description.clone()),
            deprecated: operation.operation.deprecated,
            deprecated_note: hints.deprecated_note.clone(),
            tags: operation.operation.tags.clone(),
            servers: operation.servers(spec).to_vec(),
            auth: auth(spec, operation),
            parameters: sections(operation),
            body: body(operation),
            responses: responses(spec, operation),
            callbacks: callbacks(operation),
            external_docs: operation.operation.external_docs.clone(),
            rail: rail(spec, operation, registry, options, display),
            augmentation: Augmentation::default(),
        }
    }

    /// Adds a Markdown page's own content (API-04).
    pub fn augment(&mut self, augmentation: Augmentation) {
        self.augmentation = augmentation;
    }

    /// Every field on the page, which is what search and the Markdown
    /// representation walk.
    pub fn fields(&self) -> Vec<&Field> {
        let parameters = self.parameters.iter().flat_map(|s| s.fields.iter());
        let body = self
            .body
            .iter()
            .flat_map(|b| b.media_types.iter())
            .flat_map(|m| m.fields.iter());
        let responses = self.responses.iter().flat_map(|r| {
            r.headers
                .iter()
                .chain(r.media_types.iter().flat_map(|m| m.fields.iter()))
        });
        parameters
            .chain(body)
            .chain(responses)
            .flat_map(Field::flatten)
            .collect()
    }
}

fn sections(operation: &OperationRef<'_>) -> Vec<Section> {
    let parameters = operation.parameters();
    ParameterIn::ALL
        .into_iter()
        .filter_map(|location| {
            let fields: Vec<Field> = parameters
                .iter()
                .filter(|p| p.location == location && !p.liyasa.hidden)
                .map(|p| Field::of_parameter(p))
                .collect();
            (!fields.is_empty()).then(|| Section {
                location,
                title: title_of(location),
                fields,
            })
        })
        .collect()
}

fn title_of(location: ParameterIn) -> String {
    match location {
        ParameterIn::Path => "Path parameters",
        ParameterIn::Query => "Query parameters",
        ParameterIn::Header => "Headers",
        ParameterIn::Cookie => "Cookies",
    }
    .to_owned()
}

fn body(operation: &OperationRef<'_>) -> Option<BodySection> {
    let body = operation.operation.request_body.as_ref()?;
    Some(BodySection {
        required: body.required,
        description: body.description.clone(),
        media_types: media_sections(&body.content, Side::Request),
    })
}

fn media_sections(content: &OrderedMap<crate::model::MediaType>, side: Side) -> Vec<MediaSection> {
    content
        .iter()
        .map(|(media_type, media)| {
            let schema = media.schema.clone().unwrap_or_default();
            let root = Field::of_schema("body", &schema, true, crate::field::DEPTH);
            // An object body is shown as its properties rather than as one row
            // called "body"; anything else keeps its single row.
            let fields = if root.children.is_empty() && root.variants.is_empty() {
                vec![root]
            } else if root.variants.is_empty() {
                root.children
            } else {
                vec![root]
            };
            MediaSection {
                media_type: media_type.to_owned(),
                fields,
                example: example_text(media, side),
            }
        })
        .collect()
}

/// The example a rail shows for one media type: the spec's, else one built
/// from the schema.
fn example_text(media: &crate::model::MediaType, side: Side) -> Option<String> {
    let written = media
        .example
        .clone()
        .or_else(|| media.examples.values().find_map(|e| e.value.clone()));
    let value = written.or_else(|| {
        media
            .schema
            .as_ref()
            .map(|schema| example::of(schema, side, Fill::All))
    })?;
    render(&value)
}

fn render(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        other => serde_json::to_string_pretty(other).ok(),
    }
}

fn responses(spec: &Spec, operation: &OperationRef<'_>) -> Vec<ResponseSection> {
    operation
        .operation
        .responses
        .iter()
        .map(|(status, response)| ResponseSection {
            status: status.to_owned(),
            description: response.description.clone(),
            headers: response
                .headers
                .iter()
                .map(|(name, header)| {
                    let schema = header.schema.clone().unwrap_or_default();
                    let mut field =
                        Field::of_schema(name, &schema, header.required, crate::field::DEPTH);
                    if header.description.is_some() {
                        field.description = header.description.clone();
                    }
                    field.deprecated |= header.deprecated;
                    field
                })
                .collect(),
            media_types: media_sections(&response.content, Side::Response),
            links: response
                .links
                .iter()
                .map(|(name, link)| link_section(spec, name, link))
                .collect(),
        })
        .collect()
}

fn link_section(spec: &Spec, name: &str, link: &Link) -> LinkSection {
    let operation = link
        .operation_id
        .as_ref()
        .and_then(|id| spec.by_operation_id(id))
        .map(|found| found.selector())
        .or_else(|| link.operation_ref.clone());
    LinkSection {
        name: name.to_owned(),
        description: link.description.clone(),
        operation,
    }
}

fn callbacks(operation: &OperationRef<'_>) -> Vec<CallbackSection> {
    let mut out = Vec::new();
    for (name, Callback(paths)) in operation.operation.callbacks.iter() {
        for (expression, item) in paths.iter() {
            for (method, called) in item.operations() {
                out.push(CallbackSection {
                    name: name.to_owned(),
                    expression: expression.to_owned(),
                    method,
                    summary: called.summary.clone(),
                });
            }
        }
    }
    out
}

fn auth(spec: &Spec, operation: &OperationRef<'_>) -> Vec<AuthOption> {
    let mut out = Vec::new();
    for requirement in operation.security(spec) {
        for (name, scopes) in requirement.schemes() {
            let Some(scheme) = spec.components.security_schemes.get(name) else {
                continue;
            };
            out.push(AuthOption {
                scheme: name.to_owned(),
                kind: kind_of(scheme).to_owned(),
                description: scheme.description.clone(),
                scopes: scopes.to_vec(),
                drivable: scheme.drivable_in_a_browser(),
            });
        }
    }
    out
}

fn kind_of(scheme: &SecurityScheme) -> &'static str {
    match &scheme.kind {
        SecuritySchemeKind::Http { scheme, .. } if scheme.eq_ignore_ascii_case("basic") => "basic",
        SecuritySchemeKind::Http { .. } => "bearer",
        SecuritySchemeKind::ApiKey { .. } => "apiKey",
        SecuritySchemeKind::OAuth2 { .. } => "oauth2",
        SecuritySchemeKind::OpenIdConnect { .. } => "openIdConnect",
        SecuritySchemeKind::MutualTls => "mutualTLS",
    }
}

fn rail(
    spec: &Spec,
    operation: &OperationRef<'_>,
    registry: &Registry,
    options: &BuildOptions,
    display: Display,
) -> Rail {
    let request = Request::build(spec, operation, &options.sample);
    Rail {
        samples: registry.samples(spec, operation, &options.languages, &options.sample),
        request_example: request.body.as_ref().map(|body| ExampleBlock {
            status: String::new(),
            media_type: body.media_type().to_owned(),
            text: body.text().to_owned(),
        }),
        responses: operation
            .operation
            .responses
            .iter()
            .filter_map(|(status, response)| {
                let (media_type, media) = response.preferred()?;
                Some(ExampleBlock {
                    status: status.to_owned(),
                    media_type: media_type.to_owned(),
                    text: example_text(media, Side::Response)?,
                })
            })
            .collect(),
        display,
        spec_download: Download {
            json: format!(
                "{}/{}.json",
                options.download_base.trim_end_matches('/'),
                spec.id
            ),
            yaml: format!(
                "{}/{}.yaml",
                options.download_base.trim_end_matches('/'),
                spec.id
            ),
        },
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
  - url: https://api.example.com/v1
security:
  - bearer: [read]
paths:
  /users/{id}:
    get:
      operationId: getUser
      summary: Fetch one user
      description: Returns the user.
      parameters:
        - { name: id, in: path, required: true, schema: { type: string }, description: The id }
        - { name: verbose, in: query, schema: { type: boolean } }
        - { name: X-Trace, in: header, schema: { type: string } }
        - { name: session, in: cookie, schema: { type: string } }
      responses:
        "200":
          description: The user
          headers:
            X-Rate-Limit: { schema: { type: integer }, description: What is left }
          content:
            application/json:
              schema:
                type: object
                required: [id]
                properties:
                  id: { type: string }
                  name: { type: [string, "null"] }
          links:
            notes: { operationId: listNotes, description: The user's notes }
        "404": { description: Not there }
    put:
      operationId: replaceUser
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: object
              required: [name]
              properties:
                name: { type: string, examples: ["Ada"] }
      responses:
        "200": { description: ok }
  /users/{id}/notes:
    get:
      operationId: listNotes
      responses:
        "200": { description: ok }
components:
  securitySchemes:
    bearer: { type: http, scheme: bearer, description: A token }
"##;

    fn page(operation_id: &str) -> (Spec, Page) {
        let loaded = load::from_bytes("api", "api.yaml", SPEC.as_bytes()).expect("the spec loads");
        assert!(
            !loaded.diagnostics.has_errors(),
            "{:?}",
            loaded.diagnostics.as_slice()
        );
        let spec = loaded.spec;
        let registry = Registry::new();
        let built = {
            let operation = spec
                .by_operation_id(operation_id)
                .expect("the operation is there");
            Page::build(
                &spec,
                &operation,
                &registry,
                &BuildOptions {
                    route: format!("/api-reference/{operation_id}"),
                    ..BuildOptions::default()
                },
            )
        };
        (spec, built)
    }

    #[test]
    fn every_parameter_location_gets_its_own_section_in_a_fixed_order() {
        let (_, page) = page("getUser");
        assert_eq!(
            page.parameters
                .iter()
                .map(|s| s.title.as_str())
                .collect::<Vec<_>>(),
            vec!["Path parameters", "Query parameters", "Headers", "Cookies"]
        );
        assert_eq!(page.parameters[0].fields[0].anchor, "path-id");
        assert_eq!(
            page.parameters[0].fields[0].description.as_deref(),
            Some("The id")
        );
        assert!(page.parameters[0].fields[0].required);
    }

    #[test]
    fn a_response_carries_its_headers_schema_and_links() {
        let (_, page) = page("getUser");
        assert_eq!(
            page.responses
                .iter()
                .map(|r| r.status.as_str())
                .collect::<Vec<_>>(),
            vec!["200", "404"]
        );
        let ok = &page.responses[0];
        assert_eq!(ok.headers[0].name, "X-Rate-Limit");
        assert_eq!(ok.media_types[0].media_type, "application/json");
        assert_eq!(
            ok.media_types[0]
                .fields
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>(),
            vec!["id", "name"],
            "an object body is shown as its properties"
        );
        assert!(ok.media_types[0].fields[1].nullable);
        assert_eq!(
            ok.links[0].operation.as_deref(),
            Some("GET /users/{id}/notes")
        );
    }

    #[test]
    fn the_operations_auth_is_listed_with_its_scopes() {
        let (_, page) = page("getUser");
        assert_eq!(page.auth.len(), 1);
        assert_eq!(page.auth[0].scheme, "bearer");
        assert_eq!(page.auth[0].kind, "bearer");
        assert_eq!(page.auth[0].scopes, vec!["read".to_owned()]);
        assert!(page.auth[0].drivable);
    }

    #[test]
    fn the_rail_carries_samples_examples_and_the_download_links() {
        let (_, page) = page("replaceUser");
        assert_eq!(
            page.rail
                .samples
                .iter()
                .map(|s| s.language.as_str())
                .collect::<Vec<_>>(),
            vec!["curl", "javascript", "python", "go"]
        );
        let request = page.rail.request_example.as_ref().expect("there is a body");
        assert_eq!(request.media_type, "application/json");
        assert!(request.text.contains("Ada"), "{}", request.text);
        assert_eq!(page.rail.spec_download.json, "/openapi/api.json");
        assert_eq!(page.rail.spec_download.yaml, "/openapi/api.yaml");
    }

    #[test]
    fn a_response_with_a_schema_and_no_example_still_gets_one_in_the_rail() {
        let (_, page) = page("getUser");
        let ok = page
            .rail
            .responses
            .iter()
            .find(|block| block.status == "200")
            .expect("the 200 has an example");
        assert!(ok.text.contains("\"id\""), "{}", ok.text);
    }

    #[test]
    fn the_title_falls_back_from_the_hint_to_the_summary_to_the_selector() {
        let (_, titled) = page("getUser");
        assert_eq!(titled.title, "Fetch one user");
        let (_, untitled) = page("listNotes");
        assert_eq!(untitled.title, "GET /users/{id}/notes");
    }

    #[test]
    fn front_matter_names_a_spec_and_a_selector() {
        assert_eq!(
            parse_front_matter("api GET /users/{id}"),
            Some(("api", "GET /users/{id}"))
        );
        assert_eq!(
            parse_front_matter("  api   POST /a  "),
            Some(("api", "POST /a"))
        );
        assert_eq!(parse_front_matter("api"), None);
        assert_eq!(parse_front_matter("api FETCH /a"), None);
    }

    #[test]
    fn a_page_takes_a_markdown_pages_body_and_slots() {
        let (_, mut page) = page("getUser");
        let mut slots = OrderedMap::new();
        slots.insert(
            "after-params",
            Rendered {
                html: "<p>Rate limits apply.</p>".to_owned(),
                markdown: "Rate limits apply.".to_owned(),
            },
        );
        page.augment(Augmentation {
            intro: Some(Rendered {
                html: "<p>Introduction.</p>".to_owned(),
                markdown: "Introduction.".to_owned(),
            }),
            slots,
        });
        assert_eq!(
            page.augmentation
                .slots
                .get("after-params")
                .map(|rendered| rendered.markdown.as_str()),
            Some("Rate limits apply.")
        );
        assert!(Augmentation::is_slot("after-params"));
        assert!(!Augmentation::is_slot("wherever"));
    }

    #[test]
    fn every_field_on_the_page_is_reachable_for_search_and_markdown() {
        let (_, page) = page("getUser");
        let names: Vec<&str> = page.fields().iter().map(|f| f.name.as_str()).collect();
        for want in [
            "id",
            "verbose",
            "X-Trace",
            "session",
            "X-Rate-Limit",
            "name",
        ] {
            assert!(names.contains(&want), "`{want}` is missing from {names:?}");
        }
    }
}
