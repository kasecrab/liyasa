//! Endpoints documented without a spec (API-20, API-21).
//!
//! Not every API has an OpenAPI document, and the ones that do rarely have one
//! on the first day. A page may describe an endpoint with the `endpoint`,
//! `param`, `response-field`, and example components, get a playground from
//! `api.baseUrl` and `api.auth`, and be promoted to spec-backed later without
//! its route changing.
//!
//! Once both exist, they can disagree, and a page that disagrees with the API
//! is worse than no page: [`drift`] is what the verifier reports.

use liyasa_core::components::{PropDef, PropSchema, PropType, SlotDef};
use liyasa_core::diagnostics::{Diagnostic, code};
use serde::{Deserialize, Serialize};

use crate::config::ApiConfig;
use crate::model::{Method, ParameterIn, Spec};
use crate::playground::{AuthControl, manual_auth};

/// One endpoint a page describes by hand.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Endpoint {
    pub method: String,
    pub path: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub deprecated: bool,
    pub params: Vec<ManualField>,
    pub response_fields: Vec<ManualField>,
    /// `api.baseUrl` unless the page overrides it.
    pub base_url: Option<String>,
}

impl Endpoint {
    pub fn method(&self) -> Option<Method> {
        Method::parse(&self.method)
    }

    /// `GET /users/{id}`, the same selector a spec-backed page has, so the two
    /// can be compared and one can replace the other (API-21).
    pub fn selector(&self) -> String {
        format!(
            "{} {}",
            self.method()
                .map_or_else(|| self.method.to_uppercase(), |m| m.as_str().to_owned()),
            self.path
        )
    }

    /// The auth control this page's playground offers (API-20).
    pub fn auth(&self, config: &ApiConfig) -> Option<AuthControl> {
        config.auth.as_ref().and_then(manual_auth)
    }

    pub fn server(&self, config: &ApiConfig) -> Option<String> {
        self.base_url.clone().or_else(|| config.base_url.clone())
    }
}

/// One `param` or `response-field`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ManualField {
    pub name: String,
    #[serde(rename = "in")]
    pub location: Option<ParameterIn>,
    /// Free text: `string`, `array of string`, whatever the author wrote.
    #[serde(rename = "type")]
    pub type_label: Option<String>,
    pub required: bool,
    pub deprecated: bool,
    pub default: Option<String>,
    pub description: Option<String>,
}

/// The prop schemas of the four components, declared here because this crate
/// is what knows what they mean; `liyasa-components` registers them.
pub fn prop_schemas() -> Vec<(&'static str, PropSchema)> {
    vec![
        (
            "endpoint",
            PropSchema {
                props: vec![
                    PropDef {
                        name: "method",
                        ty: PropType::Enum(
                            Method::ALL.iter().map(|m| m.as_str().to_owned()).collect(),
                        ),
                        required: true,
                        default: None,
                        doc: "The HTTP method.",
                    },
                    PropDef {
                        name: "path",
                        ty: PropType::Str,
                        required: true,
                        default: None,
                        doc: "The path, with `{name}` for each parameter.",
                    },
                    PropDef {
                        name: "baseUrl",
                        ty: PropType::Str,
                        required: false,
                        default: None,
                        doc: "Overrides `api.baseUrl` for this endpoint.",
                    },
                    PropDef {
                        name: "deprecated",
                        ty: PropType::Bool,
                        required: false,
                        default: Some(false.into()),
                        doc: "Marks the endpoint deprecated.",
                    },
                ],
                slots: vec![SlotDef {
                    name: "description",
                    required: false,
                    doc: "Prose shown above the parameters.",
                }],
            },
        ),
        ("param", field_schema("Where the parameter goes.")),
        (
            "response-field",
            field_schema("Unused: a response field has no location."),
        ),
        (
            "request-example",
            PropSchema {
                props: vec![PropDef {
                    name: "lang",
                    ty: PropType::Str,
                    required: false,
                    default: None,
                    doc: "The language the sample is highlighted as.",
                }],
                slots: vec![SlotDef {
                    name: "default",
                    required: true,
                    doc: "The sample itself, as a fenced block.",
                }],
            },
        ),
    ]
}

fn field_schema(location_doc: &'static str) -> PropSchema {
    PropSchema {
        props: vec![
            PropDef {
                name: "name",
                ty: PropType::Str,
                required: true,
                default: None,
                doc: "The field's name.",
            },
            PropDef {
                name: "in",
                ty: PropType::Enum(
                    ParameterIn::ALL
                        .iter()
                        .map(|location| location.as_str().to_owned())
                        .collect(),
                ),
                required: false,
                default: None,
                doc: location_doc,
            },
            PropDef {
                name: "type",
                ty: PropType::Str,
                required: false,
                default: None,
                doc: "What the field accepts.",
            },
            PropDef {
                name: "required",
                ty: PropType::Bool,
                required: false,
                default: Some(false.into()),
                doc: "Whether the field has to be there.",
            },
            PropDef {
                name: "deprecated",
                ty: PropType::Bool,
                required: false,
                default: Some(false.into()),
                doc: "Whether the field is on its way out.",
            },
            PropDef {
                name: "default",
                ty: PropType::Str,
                required: false,
                default: None,
                doc: "The value used when the field is left out.",
            },
        ],
        slots: vec![SlotDef {
            name: "default",
            required: false,
            doc: "The field's description.",
        }],
    }
}

/// One way a hand-written page and a spec disagree (API-21).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Drift {
    /// The spec has nothing at this method and path.
    NoSuchOperation,
    /// The page documents a parameter the spec does not declare.
    Extra { name: String, location: String },
    /// The spec declares a parameter the page does not document.
    Missing { name: String, location: String },
    /// Both declare it; one says it is required and the other does not.
    Required {
        name: String,
        location: String,
        in_spec: bool,
    },
    /// Both declare it; the page's type does not look like the spec's.
    Type {
        name: String,
        page: String,
        in_spec: String,
    },
}

impl Drift {
    pub fn message(&self, selector: &str) -> String {
        match self {
            Self::NoSuchOperation => {
                format!("the page documents `{selector}`, which the spec does not declare")
            }
            Self::Extra { name, location } => format!(
                "`{selector}`: the page documents a {location} parameter `{name}` the spec does not declare"
            ),
            Self::Missing { name, location } => format!(
                "`{selector}`: the spec declares a {location} parameter `{name}` the page does not document"
            ),
            Self::Required {
                name,
                location,
                in_spec,
            } => format!(
                "`{selector}`: the {location} parameter `{name}` is {} in the spec and {} on the page",
                if *in_spec { "required" } else { "optional" },
                if *in_spec { "optional" } else { "required" }
            ),
            Self::Type {
                name,
                page,
                in_spec,
            } => format!(
                "`{selector}`: `{name}` is `{in_spec}` in the spec and `{page}` on the page"
            ),
        }
    }

    pub fn diagnostic(&self, selector: &str) -> Diagnostic {
        Diagnostic::new(code::W0513, self.message(selector)).help(
            "update the page, or promote it to spec-backed with `openapi:` front matter and \
             delete the hand-written parameters",
        )
    }
}

/// Compares a hand-written page with the spec that describes the same path.
///
/// Only the parameters are compared: a page's prose is its own, and a response
/// field a page documents may be one of several the spec's schema allows.
pub fn drift(endpoint: &Endpoint, spec: &Spec) -> Vec<Drift> {
    let Some(method) = endpoint.method() else {
        return vec![Drift::NoSuchOperation];
    };
    let Some(operation) = spec.operation(method, &endpoint.path) else {
        return vec![Drift::NoSuchOperation];
    };
    let declared = operation.parameters();
    let mut out = Vec::new();

    for field in &endpoint.params {
        let location = field.location.unwrap_or(ParameterIn::Query);
        let found = declared
            .iter()
            .find(|p| p.name == field.name && p.location == location);
        let Some(found) = found else {
            out.push(Drift::Extra {
                name: field.name.clone(),
                location: location.as_str().to_owned(),
            });
            continue;
        };
        if found.required != field.required {
            out.push(Drift::Required {
                name: field.name.clone(),
                location: location.as_str().to_owned(),
                in_spec: found.required,
            });
        }
        if let (Some(page), Some(schema)) = (&field.type_label, &found.schema) {
            let in_spec = crate::field::type_label(schema);
            if !same_type(page, &in_spec) {
                out.push(Drift::Type {
                    name: field.name.clone(),
                    page: page.clone(),
                    in_spec,
                });
            }
        }
    }

    for parameter in declared {
        if parameter.liyasa.hidden {
            continue;
        }
        let documented = endpoint.params.iter().any(|field| {
            field.name == parameter.name
                && field.location.unwrap_or(ParameterIn::Query) == parameter.location
        });
        if !documented {
            out.push(Drift::Missing {
                name: parameter.name.clone(),
                location: parameter.location.as_str().to_owned(),
            });
        }
    }
    out
}

/// Whether a hand-written type reads as the spec's.
///
/// The page's is free text, so the comparison is deliberately loose: `int` and
/// `integer` agree, and so do `string[]` and `array of string`. Anything that
/// shares no word is a disagreement worth reporting.
fn same_type(page: &str, in_spec: &str) -> bool {
    let normalize = |text: &str| {
        text.to_ascii_lowercase()
            .replace("[]", " array")
            .replace(['<', '>', '(', ')', ',', '|'], " ")
            .split_whitespace()
            .map(|word| match word {
                "int" | "int32" | "int64" | "long" => "integer",
                "float" | "double" | "decimal" => "number",
                "bool" => "boolean",
                "of" | "an" | "a" | "the" | "or" => "",
                other => other,
            })
            .filter(|word| !word.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let (page, spec) = (normalize(page), normalize(in_spec));
    if page.is_empty() || spec.is_empty() {
        return true;
    }
    page.iter().any(|word| spec.contains(word))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load;

    const SPEC: &str = r##"
openapi: 3.1.0
info: { title: T, version: "1" }
paths:
  /users/{id}:
    get:
      operationId: getUser
      parameters:
        - { name: id, in: path, required: true, schema: { type: string } }
        - { name: verbose, in: query, schema: { type: boolean } }
        - { name: fields, in: query, schema: { type: array, items: { type: string } } }
      responses: { "200": { description: ok } }
"##;

    fn spec() -> Spec {
        load::from_bytes("api", "api.yaml", SPEC.as_bytes())
            .expect("the spec loads")
            .spec
    }

    fn field(name: &str, location: ParameterIn, required: bool, ty: Option<&str>) -> ManualField {
        ManualField {
            name: name.to_owned(),
            location: Some(location),
            type_label: ty.map(str::to_owned),
            required,
            ..ManualField::default()
        }
    }

    fn endpoint(params: Vec<ManualField>) -> Endpoint {
        Endpoint {
            method: "GET".to_owned(),
            path: "/users/{id}".to_owned(),
            params,
            ..Endpoint::default()
        }
    }

    #[test]
    fn a_page_that_matches_the_spec_drifts_in_no_way() {
        let page = endpoint(vec![
            field("id", ParameterIn::Path, true, Some("string")),
            field("verbose", ParameterIn::Query, false, Some("boolean")),
            field("fields", ParameterIn::Query, false, Some("string[]")),
        ]);
        assert_eq!(drift(&page, &spec()), vec![]);
    }

    #[test]
    fn a_parameter_the_spec_does_not_have_is_reported() {
        let page = endpoint(vec![
            field("id", ParameterIn::Path, true, None),
            field("verbose", ParameterIn::Query, false, None),
            field("fields", ParameterIn::Query, false, None),
            field("legacy", ParameterIn::Query, false, None),
        ]);
        let found = drift(&page, &spec());
        assert_eq!(
            found,
            vec![Drift::Extra {
                name: "legacy".to_owned(),
                location: "query".to_owned()
            }]
        );
        assert!(found[0].message("GET /users/{id}").contains("legacy"));
    }

    #[test]
    fn a_parameter_the_page_does_not_document_is_reported() {
        let page = endpoint(vec![field("id", ParameterIn::Path, true, None)]);
        let found = drift(&page, &spec());
        assert_eq!(found.len(), 2);
        assert!(matches!(found[0], Drift::Missing { .. }));
    }

    #[test]
    fn a_requiredness_that_disagrees_is_reported_with_which_side_says_what() {
        let page = endpoint(vec![
            field("id", ParameterIn::Path, true, None),
            field("verbose", ParameterIn::Query, true, None),
            field("fields", ParameterIn::Query, false, None),
        ]);
        let found = drift(&page, &spec());
        assert_eq!(
            found,
            vec![Drift::Required {
                name: "verbose".to_owned(),
                location: "query".to_owned(),
                in_spec: false
            }]
        );
        assert!(
            found[0]
                .message("GET /users/{id}")
                .contains("optional in the spec and required on the page")
        );
    }

    #[test]
    fn a_type_that_disagrees_is_reported_and_a_synonym_is_not() {
        let page = endpoint(vec![
            field("id", ParameterIn::Path, true, Some("int")),
            field("verbose", ParameterIn::Query, false, Some("bool")),
            field("fields", ParameterIn::Query, false, Some("array of string")),
        ]);
        let found = drift(&page, &spec());
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(matches!(&found[0], Drift::Type { name, .. } if name == "id"));
    }

    #[test]
    fn a_page_for_a_path_the_spec_does_not_have_is_reported_once() {
        let page = Endpoint {
            method: "DELETE".to_owned(),
            path: "/users/{id}".to_owned(),
            ..Endpoint::default()
        };
        assert_eq!(drift(&page, &spec()), vec![Drift::NoSuchOperation]);
    }

    #[test]
    fn drift_is_w0513_with_a_way_out_of_it() {
        let found = Drift::NoSuchOperation.diagnostic("GET /a");
        assert_eq!(found.code, code::W0513);
        assert!(found.help.is_some_and(|help| help.contains("openapi:")));
    }

    #[test]
    fn the_playground_for_a_manual_page_comes_from_the_api_config() {
        let config = ApiConfig {
            base_url: Some("https://api.example.com".to_owned()),
            auth: Some(crate::config::Auth {
                method: crate::config::AuthMethod::Bearer,
                ..crate::config::Auth::default()
            }),
        };
        let page = endpoint(Vec::new());
        assert_eq!(
            page.server(&config).as_deref(),
            Some("https://api.example.com")
        );
        let control = page.auth(&config).expect("there is a control");
        assert_eq!(control.kind, "bearer");
    }

    #[test]
    fn a_page_overrides_the_sites_base_url() {
        let config = ApiConfig {
            base_url: Some("https://api.example.com".to_owned()),
            auth: None,
        };
        let mut page = endpoint(Vec::new());
        page.base_url = Some("https://sandbox.example.com".to_owned());
        assert_eq!(
            page.server(&config).as_deref(),
            Some("https://sandbox.example.com")
        );
        assert!(page.auth(&config).is_none());
    }

    #[test]
    fn every_component_declares_its_required_props() {
        let schemas = prop_schemas();
        let names: Vec<&str> = schemas.iter().map(|(name, _)| *name).collect();
        assert_eq!(
            names,
            vec!["endpoint", "param", "response-field", "request-example"]
        );
        let endpoint = &schemas[0].1;
        assert!(endpoint.prop("method").is_some_and(|prop| prop.required));
        assert!(endpoint.prop("path").is_some_and(|prop| prop.required));
        assert_eq!(endpoint.required_props().count(), 2);
    }

    #[test]
    fn the_selector_is_the_one_a_spec_backed_page_would_have() {
        assert_eq!(
            endpoint(Vec::new()).selector(),
            "GET /users/{id}",
            "so promoting the page keeps its identity (API-21)"
        );
    }
}
