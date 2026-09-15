//! Liyasa's OpenAPI document model (API-01).
//!
//! One shape for every consumer: whatever dialect a spec arrived in, a
//! renderer, a sample generator, and the playground all read this.

pub mod ext;
pub mod map;
pub mod schema;
pub mod security;

use serde::Serialize;

pub use ext::{Extensions, XLiyasa};
pub use map::OrderedMap;
pub use schema::{AdditionalProperties, Discriminator, Number, Schema, SchemaType, Xml};
pub use security::{
    OAuthFlow, OAuthFlows, SecurityRequirement, SecurityScheme, SecuritySchemeKind,
};

use crate::tree::Value;
use crate::version::SpecVersion;

/// One loaded specification, after normalizing, dereferencing, overlays, and
/// visibility filtering.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Spec {
    /// The `openapi[].id` this spec was configured under (API-01).
    pub id: String,
    /// The dialect the document was written in, before normalizing.
    pub version: SpecVersion,
    pub info: Info,
    pub servers: Vec<Server>,
    /// Templated path to the operations declared on it, in document order.
    pub paths: OrderedMap<PathItem>,
    /// 3.1 webhooks, which have no path (API-10).
    pub webhooks: OrderedMap<PathItem>,
    pub components: Components,
    pub security: Vec<SecurityRequirement>,
    pub tags: Vec<Tag>,
    pub external_docs: Option<ExternalDocs>,
    pub extensions: Extensions,
}

impl Spec {
    /// Every operation in the document, paths then webhooks, in document
    /// order. The `path` of a webhook is its name.
    pub fn operations(&self) -> impl Iterator<Item = OperationRef<'_>> {
        let paths = self.paths.iter().map(|(p, item)| (p, item, false));
        let hooks = self.webhooks.iter().map(|(p, item)| (p, item, true));
        paths.chain(hooks).flat_map(|(path, item, webhook)| {
            item.operations()
                .map(move |(method, operation)| OperationRef {
                    path,
                    method,
                    operation,
                    item,
                    webhook,
                })
        })
    }

    pub fn operation(&self, method: Method, path: &str) -> Option<OperationRef<'_>> {
        self.operations()
            .find(|op| op.method == method && op.path == path)
    }

    /// Looks an operation up by its `operationId`.
    pub fn by_operation_id(&self, id: &str) -> Option<OperationRef<'_>> {
        self.operations()
            .find(|op| op.operation.operation_id.as_deref() == Some(id))
    }

    pub fn tag(&self, name: &str) -> Option<&Tag> {
        self.tags.iter().find(|tag| tag.name == name)
    }
}

/// One operation with everything a page needs to identify it.
#[derive(Debug, Clone, Copy)]
pub struct OperationRef<'a> {
    pub path: &'a str,
    pub method: Method,
    pub operation: &'a Operation,
    /// The path item, for the parameters and servers an operation inherits.
    pub item: &'a PathItem,
    pub webhook: bool,
}

impl OperationRef<'_> {
    /// `GET /users/{id}`: how navigation and front matter name an operation
    /// (API-03, API-04).
    pub fn selector(&self) -> String {
        format!("{} {}", self.method.as_str(), self.path)
    }

    /// Path-item parameters first, then the operation's own, with an
    /// operation parameter shadowing an inherited one of the same name and
    /// location (the spec's override rule).
    pub fn parameters(&self) -> Vec<&Parameter> {
        let mut out: Vec<&Parameter> = self.item.parameters.iter().collect();
        for own in &self.operation.parameters {
            match out
                .iter()
                .position(|p| p.name == own.name && p.location == own.location)
            {
                Some(at) => out[at] = own,
                None => out.push(own),
            }
        }
        out
    }

    /// The servers that apply: the operation's, else the path item's, else the
    /// document's (API-42).
    pub fn servers<'s>(&'s self, spec: &'s Spec) -> &'s [Server] {
        if !self.operation.servers.is_empty() {
            &self.operation.servers
        } else if !self.item.servers.is_empty() {
            &self.item.servers
        } else {
            &spec.servers
        }
    }

    /// The security that applies: the operation's when it declares any (an
    /// empty list means "no auth"), else the document's.
    pub fn security<'s>(&'s self, spec: &'s Spec) -> &'s [SecurityRequirement] {
        match &self.operation.security {
            Some(own) => own,
            None => &spec.security,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub title: String,
    pub version: String,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub terms_of_service: Option<String>,
    pub contact: Option<Contact>,
    pub license: Option<License>,
    pub extensions: Extensions,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Contact {
    pub name: Option<String>,
    pub url: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct License {
    pub name: String,
    pub identifier: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Server {
    pub url: String,
    pub description: Option<String>,
    pub variables: OrderedMap<ServerVariable>,
}

impl Server {
    /// The URL with every variable replaced by `values`, falling back to each
    /// variable's default (API-42).
    pub fn resolve(&self, values: &OrderedMap<String>) -> String {
        let mut out = String::with_capacity(self.url.len());
        let mut rest = self.url.as_str();
        while let Some(open) = rest.find('{') {
            out.push_str(&rest[..open]);
            let Some(close) = rest[open..].find('}').map(|at| open + at) else {
                break;
            };
            let name = &rest[open + 1..close];
            let chosen = values
                .get(name)
                .or_else(|| self.variables.get(name).map(|v| &v.default));
            match chosen {
                Some(value) => out.push_str(value),
                None => out.push_str(&rest[open..=close]),
            }
            rest = &rest[close + 1..];
        }
        out.push_str(rest);
        out
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerVariable {
    pub default: String,
    pub enumeration: Vec<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub name: String,
    pub description: Option<String>,
    pub external_docs: Option<ExternalDocs>,
    pub extensions: Extensions,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalDocs {
    pub url: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Method {
    Get,
    Put,
    Post,
    Delete,
    Options,
    Head,
    Patch,
    Trace,
    /// 3.1's `query` method, reserved by the HTTP working group.
    Query,
}

impl Method {
    /// Every method in the order OpenAPI lists them, which is the order an
    /// operation list is rendered in when a path declares several.
    pub const ALL: [Self; 9] = [
        Self::Get,
        Self::Put,
        Self::Post,
        Self::Delete,
        Self::Options,
        Self::Head,
        Self::Patch,
        Self::Trace,
        Self::Query,
    ];

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|m| m.as_str().eq_ignore_ascii_case(text))
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Put => "PUT",
            Self::Post => "POST",
            Self::Delete => "DELETE",
            Self::Options => "OPTIONS",
            Self::Head => "HEAD",
            Self::Patch => "PATCH",
            Self::Trace => "TRACE",
            Self::Query => "QUERY",
        }
    }

    pub const fn lowercase(self) -> &'static str {
        match self {
            Self::Get => "get",
            Self::Put => "put",
            Self::Post => "post",
            Self::Delete => "delete",
            Self::Options => "options",
            Self::Head => "head",
            Self::Patch => "patch",
            Self::Trace => "trace",
            Self::Query => "query",
        }
    }

    /// Whether a request of this method carries a body by default, which the
    /// sample generators use to decide whether to write one.
    pub const fn takes_body(self) -> bool {
        matches!(self, Self::Put | Self::Post | Self::Patch | Self::Delete)
    }
}

impl std::fmt::Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathItem {
    pub summary: Option<String>,
    pub description: Option<String>,
    pub operations: OrderedMap<Operation>,
    pub servers: Vec<Server>,
    pub parameters: Vec<Parameter>,
    pub extensions: Extensions,
}

impl PathItem {
    /// The operations on this path, in OpenAPI's method order rather than the
    /// document's, so two specs that list `get` and `post` in either order
    /// render the same page.
    pub fn operations(&self) -> impl Iterator<Item = (Method, &Operation)> {
        Method::ALL
            .into_iter()
            .filter_map(|method| Some((method, self.operations.get(method.lowercase())?)))
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    pub operation_id: Option<String>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub deprecated: bool,
    pub parameters: Vec<Parameter>,
    pub request_body: Option<RequestBody>,
    /// Status code (or `default`) to response, in document order.
    pub responses: OrderedMap<Response>,
    pub callbacks: OrderedMap<Callback>,
    /// `None` means "inherit the document's"; `Some(vec![])` means "no auth".
    pub security: Option<Vec<SecurityRequirement>>,
    pub servers: Vec<Server>,
    pub external_docs: Option<ExternalDocs>,
    pub extensions: Extensions,
    /// `x-liyasa` on this operation, already parsed (API-05).
    pub liyasa: XLiyasa,
    /// `x-codeSamples`, which turns generation off for this operation
    /// (API-31).
    pub code_samples: Vec<CodeSample>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeSample {
    pub lang: String,
    pub label: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ParameterIn {
    #[default]
    Query,
    Header,
    Path,
    Cookie,
}

impl ParameterIn {
    /// The order the sections appear on a page (API-10).
    pub const ALL: [Self; 4] = [Self::Path, Self::Query, Self::Header, Self::Cookie];

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "query" => Self::Query,
            "header" => Self::Header,
            "path" => Self::Path,
            "cookie" => Self::Cookie,
            _ => return None,
        })
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Query => "query",
            Self::Header => "header",
            Self::Path => "path",
            Self::Cookie => "cookie",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Style {
    #[default]
    Unset,
    Matrix,
    Label,
    Form,
    Simple,
    SpaceDelimited,
    PipeDelimited,
    DeepObject,
}

impl Style {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "matrix" => Self::Matrix,
            "label" => Self::Label,
            "form" => Self::Form,
            "simple" => Self::Simple,
            "spaceDelimited" => Self::SpaceDelimited,
            "pipeDelimited" => Self::PipeDelimited,
            "deepObject" => Self::DeepObject,
            _ => return None,
        })
    }

    /// The style a location defaults to when the spec does not say.
    pub const fn or_default(self, location: ParameterIn) -> Self {
        match self {
            Self::Unset => match location {
                ParameterIn::Query | ParameterIn::Cookie => Self::Form,
                ParameterIn::Path | ParameterIn::Header => Self::Simple,
            },
            other => other,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Parameter {
    pub name: String,
    #[serde(rename = "in")]
    pub location: ParameterIn,
    pub description: Option<String>,
    pub required: bool,
    pub deprecated: bool,
    pub allow_empty_value: bool,
    pub style: Style,
    /// `None` means "the default for the style", which differs per style.
    pub explode: Option<bool>,
    pub allow_reserved: bool,
    pub schema: Option<Schema>,
    /// Set instead of `schema` when the parameter is described by media type.
    pub content: OrderedMap<MediaType>,
    pub example: Option<Value>,
    pub examples: OrderedMap<Example>,
    pub extensions: Extensions,
    pub liyasa: XLiyasa,
}

impl Parameter {
    /// The stable anchor a page links a parameter by (API-10).
    pub fn anchor(&self) -> String {
        format!("{}-{}", self.location.as_str(), slug(&self.name))
    }

    /// `explode` as it applies, which is `true` for `form` and `false`
    /// otherwise when the spec is silent.
    pub fn explodes(&self) -> bool {
        self.explode
            .unwrap_or(self.style.or_default(self.location) == Style::Form)
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestBody {
    pub description: Option<String>,
    pub required: bool,
    pub content: OrderedMap<MediaType>,
    pub extensions: Extensions,
}

impl RequestBody {
    /// The media type a sample is generated from: JSON when it is offered,
    /// otherwise the first one written.
    pub fn preferred(&self) -> Option<(&str, &MediaType)> {
        self.content
            .iter()
            .find(|(name, _)| is_json(name))
            .or_else(|| self.content.iter().next())
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaType {
    pub schema: Option<Schema>,
    pub example: Option<Value>,
    pub examples: OrderedMap<Example>,
    pub encoding: OrderedMap<Encoding>,
    pub extensions: Extensions,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Encoding {
    pub content_type: Option<String>,
    pub headers: OrderedMap<Header>,
    pub style: Style,
    pub explode: Option<bool>,
    pub allow_reserved: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Example {
    pub summary: Option<String>,
    pub description: Option<String>,
    pub value: Option<Value>,
    pub external_value: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub description: String,
    pub headers: OrderedMap<Header>,
    pub content: OrderedMap<MediaType>,
    pub links: OrderedMap<Link>,
    pub extensions: Extensions,
}

impl Response {
    pub fn preferred(&self) -> Option<(&str, &MediaType)> {
        self.content
            .iter()
            .find(|(name, _)| is_json(name))
            .or_else(|| self.content.iter().next())
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
    pub description: Option<String>,
    pub required: bool,
    pub deprecated: bool,
    pub style: Style,
    pub explode: Option<bool>,
    pub schema: Option<Schema>,
    pub content: OrderedMap<MediaType>,
    pub example: Option<Value>,
    pub examples: OrderedMap<Example>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Link {
    pub operation_ref: Option<String>,
    pub operation_id: Option<String>,
    pub description: Option<String>,
    pub parameters: OrderedMap<Value>,
    pub request_body: Option<Value>,
    pub server: Option<Server>,
}

/// A callback: an expression such as `{$request.body#/callbackUrl}` to the
/// path item the API calls back on (API-10).
#[derive(Debug, Clone, Default, Serialize)]
#[serde(transparent)]
pub struct Callback(pub OrderedMap<PathItem>);

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Components {
    pub schemas: OrderedMap<Schema>,
    pub responses: OrderedMap<Response>,
    pub parameters: OrderedMap<Parameter>,
    pub examples: OrderedMap<Example>,
    pub request_bodies: OrderedMap<RequestBody>,
    pub headers: OrderedMap<Header>,
    pub security_schemes: OrderedMap<SecurityScheme>,
    pub links: OrderedMap<Link>,
    pub callbacks: OrderedMap<Callback>,
    pub path_items: OrderedMap<PathItem>,
    pub extensions: Extensions,
}

fn is_json(media_type: &str) -> bool {
    let base = media_type.split(';').next().unwrap_or(media_type).trim();
    base == "application/json" || base.ends_with("+json")
}

/// The anchor form of a name: lowercase, non-alphanumerics folded to one dash.
pub fn slug(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            out.extend(ch.to_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_method_round_trips_through_its_name() {
        for method in Method::ALL {
            assert_eq!(Method::parse(method.as_str()), Some(method));
            assert_eq!(Method::parse(method.lowercase()), Some(method));
        }
        assert_eq!(Method::parse("connect"), None);
    }

    #[test]
    fn a_path_renders_its_operations_in_openapi_order_not_the_documents() {
        let mut item = PathItem::default();
        item.operations.insert("post", Operation::default());
        item.operations.insert("get", Operation::default());
        assert_eq!(
            item.operations().map(|(m, _)| m).collect::<Vec<_>>(),
            vec![Method::Get, Method::Post]
        );
    }

    #[test]
    fn an_operation_parameter_shadows_the_path_items_of_the_same_name_and_place() {
        let mut item = PathItem::default();
        item.parameters.push(Parameter {
            name: "id".to_owned(),
            location: ParameterIn::Path,
            description: Some("inherited".to_owned()),
            ..Parameter::default()
        });
        item.parameters.push(Parameter {
            name: "id".to_owned(),
            location: ParameterIn::Query,
            ..Parameter::default()
        });
        let mut operation = Operation::default();
        operation.parameters.push(Parameter {
            name: "id".to_owned(),
            location: ParameterIn::Path,
            description: Some("overridden".to_owned()),
            ..Parameter::default()
        });
        item.operations.insert("get", operation);

        let (method, operation) = item.operations().next().expect("one operation");
        let reference = OperationRef {
            path: "/users/{id}",
            method,
            operation,
            item: &item,
            webhook: false,
        };
        let names: Vec<_> = reference
            .parameters()
            .iter()
            .map(|p| (p.location, p.description.clone()))
            .collect();
        assert_eq!(
            names,
            vec![
                (ParameterIn::Path, Some("overridden".to_owned())),
                (ParameterIn::Query, None),
            ],
            "the override replaces in place; the query parameter is untouched"
        );
        assert_eq!(reference.selector(), "GET /users/{id}");
    }

    #[test]
    fn a_server_substitutes_variables_and_falls_back_to_their_defaults() {
        let mut server = Server {
            url: "https://{region}.example.com/{version}".to_owned(),
            ..Server::default()
        };
        server.variables.insert(
            "region",
            ServerVariable {
                default: "eu".to_owned(),
                ..ServerVariable::default()
            },
        );
        server.variables.insert(
            "version",
            ServerVariable {
                default: "v1".to_owned(),
                ..ServerVariable::default()
            },
        );

        assert_eq!(
            server.resolve(&OrderedMap::new()),
            "https://eu.example.com/v1"
        );
        let chosen = [("region".to_owned(), "us".to_owned())]
            .into_iter()
            .collect();
        assert_eq!(server.resolve(&chosen), "https://us.example.com/v1");
    }

    #[test]
    fn an_unknown_variable_is_left_as_written_rather_than_dropped() {
        let server = Server {
            url: "https://{host}/v1".to_owned(),
            ..Server::default()
        };
        assert_eq!(server.resolve(&OrderedMap::new()), "https://{host}/v1");
    }

    #[test]
    fn a_json_media_type_is_preferred_over_the_first_one_written() {
        let mut body = RequestBody::default();
        body.content.insert("text/plain", MediaType::default());
        body.content
            .insert("application/vnd.api+json", MediaType::default());
        assert_eq!(
            body.preferred().map(|(name, _)| name),
            Some("application/vnd.api+json")
        );
    }

    #[test]
    fn an_anchor_folds_punctuation_to_single_dashes() {
        assert_eq!(slug("X-Request-ID"), "x-request-id");
        assert_eq!(slug("user[address][city]"), "user-address-city");
        assert_eq!(slug("  spaced  out  "), "spaced-out");
    }
}
