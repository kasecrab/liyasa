//! The request a sample describes (API-30, API-31).
//!
//! Every generator renders the same value, so `curl` and Python cannot drift
//! apart about which header an operation sends: the work of reading the
//! operation happens once, here, and a template only decides how to spell it.

use serde::Serialize;

use crate::config::{Auth, AuthMethod};
use crate::example::{self, Fill, Side};
use crate::model::{Method, OperationRef, Parameter, ParameterIn, SecuritySchemeKind, Spec, Style};
use crate::tree::Value;

/// A placeholder that is obviously one, so a reader does not paste a sample
/// and wonder why it answers 401.
pub const TOKEN: &str = "$ACCESS_TOKEN";
pub const API_KEY: &str = "$API_KEY";
pub const USERNAME: &str = "$USERNAME";
pub const PASSWORD: &str = "$PASSWORD";

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    pub fill: OptionsFill,
    /// Overrides the server the sample calls (`api.baseUrl`, API-20, API-42).
    pub base_url: Option<String>,
    /// Auth for a page with no spec to take it from (API-20).
    pub manual_auth: Option<Auth>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OptionsFill {
    #[default]
    All,
    Required,
}

impl From<OptionsFill> for Fill {
    fn from(fill: OptionsFill) -> Self {
        match fill {
            OptionsFill::All => Self::All,
            OptionsFill::Required => Self::Required,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BodyKind {
    Json,
    Form,
    Multipart,
    Raw,
}

/// The body as both a string and a field list, because a template needs one or
/// the other and never has to work the second out from the first.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Body {
    pub kind: BodyKind,
    pub media_type: String,
    /// Pretty-printed JSON, an encoded form, or the raw value. Empty for
    /// multipart, whose bytes are the client's to assemble.
    pub text: String,
    pub fields: Vec<Field>,
}

impl Body {
    pub fn media_type(&self) -> &str {
        &self.media_type
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub name: String,
    pub value: String,
    /// A `format: binary` field, which a generator sends as a file rather than
    /// as text.
    pub file: bool,
}

/// One request, ready for a template.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub method: String,
    /// The server, with no trailing slash.
    pub base_url: String,
    /// The path with its parameters filled in, starting with `/`.
    pub path: String,
    pub query: Vec<Pair>,
    pub headers: Vec<Pair>,
    pub cookies: Vec<Pair>,
    pub body: Option<Body>,
    /// `base_url` + `path` + the encoded query, which most templates want
    /// whole.
    pub url: String,
    /// The cookies as one header value, because that is how they are sent;
    /// `cookies` stays separate for the playground's form.
    pub cookie_header: Option<String>,
    /// The operation this came from, for a template that labels its output.
    pub operation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pair {
    pub name: String,
    pub value: String,
}

impl Request {
    /// Reads one operation into the request its samples describe.
    pub fn build(spec: &Spec, operation: &OperationRef<'_>, options: &Options) -> Self {
        let fill: Fill = options.fill.into();
        let base_url = options
            .base_url
            .clone()
            .or_else(|| {
                operation
                    .servers(spec)
                    .first()
                    .map(|server| server.resolve(&Default::default()))
            })
            .unwrap_or_else(|| "https://api.example.com".to_owned());

        let mut path = operation.path.to_owned();
        let mut query = Vec::new();
        let mut headers = Vec::new();
        let mut cookies = Vec::new();

        for parameter in operation.parameters() {
            if !parameter.required && fill == Fill::Required {
                continue;
            }
            if parameter.liyasa.hidden {
                continue;
            }
            match parameter.location {
                ParameterIn::Path => {
                    path = path.replace(
                        &format!("{{{}}}", parameter.name),
                        &encode_path(&value_of(parameter, fill)),
                    );
                }
                ParameterIn::Query => query.extend(expand(parameter, fill)),
                ParameterIn::Header => headers.extend(expand(parameter, fill)),
                ParameterIn::Cookie => cookies.extend(expand(parameter, fill)),
            }
        }

        let body =
            operation.operation.request_body.as_ref().and_then(|body| {
                self::body(body, fill, operation.operation.liyasa.examples.first())
            });
        if let Some(body) = &body
            && body.kind != BodyKind::Multipart
        {
            headers.push(Pair {
                name: "Content-Type".to_owned(),
                value: body.media_type().to_owned(),
            });
        }
        auth(
            spec,
            operation,
            options,
            &mut headers,
            &mut query,
            &mut cookies,
        );

        let base_url = base_url.trim_end_matches('/').to_owned();
        let url = format!("{base_url}{path}{}", query_string(&query));
        let cookie_header = (!cookies.is_empty()).then(|| {
            cookies
                .iter()
                .map(|pair| format!("{}={}", pair.name, pair.value))
                .collect::<Vec<_>>()
                .join("; ")
        });
        Self {
            method: operation.method.as_str().to_owned(),
            base_url,
            path,
            query,
            headers,
            cookies,
            body,
            url,
            cookie_header,
            operation_id: operation.operation.operation_id.clone(),
        }
    }

    /// Whether this request carries a body a generator has to write.
    pub fn has_body(&self) -> bool {
        self.body.is_some()
    }

    pub fn method_enum(&self) -> Option<Method> {
        Method::parse(&self.method)
    }
}

fn value_of(parameter: &Parameter, fill: Fill) -> String {
    if let Some(written) = parameter
        .example
        .clone()
        .or_else(|| parameter.examples.values().find_map(|e| e.value.clone()))
    {
        return example::as_text(&written);
    }
    match &parameter.schema {
        Some(schema) => example::as_text(&example::of(schema, Side::Request, fill)),
        None => parameter.name.clone(),
    }
}

/// One parameter becomes one pair, or several when it is an exploded array.
fn expand(parameter: &Parameter, fill: Fill) -> Vec<Pair> {
    let array = parameter
        .schema
        .as_ref()
        .is_some_and(|s| s.is(crate::model::SchemaType::Array));
    if array && parameter.explodes() {
        let items = parameter
            .schema
            .as_ref()
            .and_then(|s| s.items.as_deref())
            .map(|items| example::of(items, Side::Request, fill))
            .unwrap_or(Value::Null);
        return vec![Pair {
            name: parameter.name.clone(),
            value: example::as_text(&items),
        }];
    }
    if array {
        let separator = match parameter.style.or_default(parameter.location) {
            Style::SpaceDelimited => " ",
            Style::PipeDelimited => "|",
            _ => ",",
        };
        let items = parameter.schema.as_ref().and_then(|s| s.items.as_deref());
        // Two values when the schema names two, so the sample shows the
        // separator; one otherwise, because repeating the same value twice
        // reads as a mistake rather than as a list.
        let values: Vec<String> = match items {
            Some(items) if items.enumeration.len() > 1 => items
                .enumeration
                .iter()
                .take(2)
                .map(example::as_text)
                .collect(),
            Some(items) => vec![example::as_text(&example::of(items, Side::Request, fill))],
            None => Vec::new(),
        };
        return vec![Pair {
            name: parameter.name.clone(),
            value: values.join(separator),
        }];
    }
    vec![Pair {
        name: parameter.name.clone(),
        value: value_of(parameter, fill),
    }]
}

/// `hint` is `x-liyasa.examples`, which outranks the spec's own example: it is
/// the operator's correction of a body the API team wrote badly (API-05).
fn body(body: &crate::model::RequestBody, fill: Fill, hint: Option<&Value>) -> Option<Body> {
    let (media_type, media) = body.preferred()?;
    let schema = media.schema.as_ref();
    let written = hint.cloned().or_else(|| {
        media
            .example
            .clone()
            .or_else(|| media.examples.values().find_map(|e| e.value.clone()))
    });
    let value = written.or_else(|| schema.map(|s| example::of(s, Side::Request, fill)))?;

    let base = media_type.split(';').next().unwrap_or(media_type).trim();
    if base.starts_with("multipart/") {
        return Some(Body {
            kind: BodyKind::Multipart,
            media_type: base.to_owned(),
            text: String::new(),
            fields: fields(schema, &value),
        });
    }
    if base == "application/x-www-form-urlencoded" {
        let fields = fields(schema, &value);
        return Some(Body {
            kind: BodyKind::Form,
            media_type: base.to_owned(),
            text: fields
                .iter()
                .map(|f| format!("{}={}", encode_query(&f.name), encode_query(&f.value)))
                .collect::<Vec<_>>()
                .join("&"),
            fields,
        });
    }
    if base == "application/json" || base.ends_with("+json") {
        return Some(Body {
            kind: BodyKind::Json,
            media_type: base.to_owned(),
            text: serde_json::to_string_pretty(&value).unwrap_or_default(),
            fields: Vec::new(),
        });
    }
    Some(Body {
        kind: BodyKind::Raw,
        media_type: base.to_owned(),
        text: example::as_text(&value),
        fields: Vec::new(),
    })
}

/// A form or multipart body's object becomes one field per property, with the
/// binary ones marked so a generator sends them as files.
fn fields(schema: Option<&crate::model::Schema>, value: &Value) -> Vec<Field> {
    crate::tree::entries(value)
        .map(|(name, item)| {
            let file = schema
                .and_then(|s| s.properties.get(name))
                .is_some_and(is_binary);
            Field {
                name: name.to_owned(),
                value: example::as_text(item),
                file,
            }
        })
        .collect()
}

fn is_binary(schema: &crate::model::Schema) -> bool {
    schema.format.as_deref() == Some("binary")
        || schema.content_encoding.as_deref() == Some("base64")
        || schema
            .content_media_type
            .as_deref()
            .is_some_and(|media| media == "application/octet-stream")
}

/// Fills in the credentials the operation's security asks for, with
/// placeholders rather than anything that could be mistaken for a real one.
fn auth(
    spec: &Spec,
    operation: &OperationRef<'_>,
    options: &Options,
    headers: &mut Vec<Pair>,
    query: &mut Vec<Pair>,
    cookies: &mut Vec<Pair>,
) {
    let requirements = operation.security(spec);
    let Some(first) = requirements.iter().find(|r| !r.is_anonymous()) else {
        if let Some(manual) = &options.manual_auth {
            manual_auth(manual, headers, query, cookies);
        }
        return;
    };
    for (name, _) in first.schemes() {
        let Some(scheme) = spec.components.security_schemes.get(name) else {
            continue;
        };
        match &scheme.kind {
            SecuritySchemeKind::Http { scheme, .. } if scheme.eq_ignore_ascii_case("basic") => {
                headers.push(Pair {
                    name: "Authorization".to_owned(),
                    value: format!("Basic {USERNAME}:{PASSWORD}"),
                });
            }
            SecuritySchemeKind::Http { scheme, .. } => headers.push(Pair {
                name: "Authorization".to_owned(),
                value: format!("{} {TOKEN}", titlecase(scheme)),
            }),
            SecuritySchemeKind::ApiKey { name, location } => {
                let pair = Pair {
                    name: name.clone(),
                    value: API_KEY.to_owned(),
                };
                match location {
                    ParameterIn::Query => query.push(pair),
                    ParameterIn::Cookie => cookies.push(pair),
                    _ => headers.push(pair),
                }
            }
            SecuritySchemeKind::OAuth2 { .. } | SecuritySchemeKind::OpenIdConnect { .. } => {
                headers.push(Pair {
                    name: "Authorization".to_owned(),
                    value: format!("Bearer {TOKEN}"),
                });
            }
            SecuritySchemeKind::MutualTls => {}
        }
    }
}

fn manual_auth(
    auth: &Auth,
    headers: &mut Vec<Pair>,
    query: &mut Vec<Pair>,
    cookies: &mut Vec<Pair>,
) {
    match auth.method {
        AuthMethod::Bearer => headers.push(Pair {
            name: "Authorization".to_owned(),
            value: format!("Bearer {TOKEN}"),
        }),
        AuthMethod::Basic => headers.push(Pair {
            name: "Authorization".to_owned(),
            value: format!("Basic {USERNAME}:{PASSWORD}"),
        }),
        AuthMethod::ApiKey => {
            let pair = Pair {
                name: auth.name.clone().unwrap_or_else(|| "X-API-Key".to_owned()),
                value: API_KEY.to_owned(),
            };
            match auth.location {
                Some(ParameterIn::Query) => query.push(pair),
                Some(ParameterIn::Cookie) => cookies.push(pair),
                _ => headers.push(pair),
            }
        }
        AuthMethod::None => {}
    }
}

fn titlecase(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn query_string(query: &[Pair]) -> String {
    if query.is_empty() {
        return String::new();
    }
    let pairs: Vec<String> = query
        .iter()
        .map(|pair| format!("{}={}", encode_query(&pair.name), encode_query(&pair.value)))
        .collect();
    format!("?{}", pairs.join("&"))
}

/// Percent-encodes everything outside RFC 3986's unreserved set, leaving the
/// `$` of a placeholder alone so `$ACCESS_TOKEN` stays readable.
fn encode(text: &str, extra_safe: &[u8]) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'$')
            || extra_safe.contains(&byte)
        {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn encode_path(text: &str) -> String {
    encode(text, b"!*'()@:")
}

fn encode_query(text: &str) -> String {
    encode(text, b"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load;

    const SPEC: &str = r##"
openapi: 3.1.0
info: { title: Test, version: "1" }
servers:
  - url: https://api.example.com/v1/
security:
  - bearer: []
paths:
  /users/{id}/notes:
    post:
      operationId: addNote
      parameters:
        - { name: id, in: path, required: true, schema: { type: string }, example: "u_1" }
        - { name: draft, in: query, schema: { type: boolean } }
        - { name: X-Trace, in: header, schema: { type: string }, example: "abc" }
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: object
              required: [text]
              properties:
                text: { type: string, examples: ["hello"] }
                pinned: { type: boolean }
      responses:
        "201": { description: made }
components:
  securitySchemes:
    bearer: { type: http, scheme: bearer }
"##;

    fn request(fill: OptionsFill) -> Request {
        let loaded = load::from_bytes("api", "api.yaml", SPEC.as_bytes()).expect("the spec loads");
        let spec = loaded.spec;
        let operation = spec
            .by_operation_id("addNote")
            .expect("the operation is there");
        Request::build(
            &spec,
            &operation,
            &Options {
                fill,
                ..Options::default()
            },
        )
    }

    #[test]
    fn the_path_parameter_is_substituted_and_the_server_loses_its_slash() {
        let request = request(OptionsFill::All);
        assert_eq!(request.base_url, "https://api.example.com/v1");
        assert_eq!(request.path, "/users/u_1/notes");
        assert!(
            request
                .url
                .starts_with("https://api.example.com/v1/users/u_1/notes?")
        );
    }

    #[test]
    fn a_query_parameter_is_encoded_into_the_url() {
        let request = request(OptionsFill::All);
        assert_eq!(request.query.len(), 1);
        assert_eq!(request.query[0].name, "draft");
        assert!(request.url.contains("?draft=true"), "{}", request.url);
    }

    #[test]
    fn required_only_drops_the_optional_parameters_and_body_properties() {
        let request = request(OptionsFill::Required);
        assert!(
            request.query.is_empty(),
            "the optional query parameter is gone"
        );
        assert!(
            !request.headers.iter().any(|h| h.name == "X-Trace"),
            "the optional header is gone"
        );
        let body = request.body.as_ref().expect("the body is required");
        assert!(body.text().contains("\"text\""));
        assert!(!body.text().contains("pinned"), "{}", body.text());
    }

    #[test]
    fn the_security_scheme_becomes_a_placeholder_authorization_header() {
        let request = request(OptionsFill::All);
        let header = request
            .headers
            .iter()
            .find(|h| h.name == "Authorization")
            .expect("the bearer scheme produced a header");
        assert_eq!(header.value, format!("Bearer {TOKEN}"));
    }

    #[test]
    fn a_json_body_gets_a_content_type_and_is_pretty_printed() {
        let request = request(OptionsFill::All);
        assert!(
            request
                .headers
                .iter()
                .any(|h| h.name == "Content-Type" && h.value == "application/json")
        );
        let body = request.body.as_ref().expect("there is a body");
        assert!(
            body.text().contains('\n'),
            "a sample body is read, so it is indented"
        );
    }

    #[test]
    fn a_multipart_body_becomes_fields_with_the_binary_one_marked() {
        let loaded = load::from_bytes(
            "api",
            "api.yaml",
            br##"
openapi: 3.1.0
info: { title: Test, version: "1" }
paths:
  /upload:
    post:
      operationId: upload
      requestBody:
        content:
          multipart/form-data:
            schema:
              type: object
              properties:
                file: { type: string, format: binary }
                note: { type: string }
      responses:
        "200": { description: ok }
"##,
        )
        .expect("the spec loads");
        let spec = loaded.spec;
        let operation = spec
            .by_operation_id("upload")
            .expect("the operation is there");
        let request = Request::build(&spec, &operation, &Options::default());
        let body = request.body.as_ref().expect("there is a body");
        assert_eq!(body.kind, BodyKind::Multipart);
        assert_eq!(body.fields.len(), 2);
        assert!(body.fields[0].file, "the binary field is sent as a file");
        assert!(!body.fields[1].file);
        assert!(
            !request.headers.iter().any(|h| h.name == "Content-Type"),
            "multipart's boundary is the client's to write, not the sample's"
        );
    }

    #[test]
    fn a_placeholder_survives_url_encoding_legibly() {
        assert_eq!(encode_query("$API_KEY"), "$API_KEY");
        assert_eq!(encode_query("a b&c"), "a%20b%26c");
        assert_eq!(encode_path("u/1"), "u%2F1");
    }
}
