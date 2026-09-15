//! Serving the spec (API-50).
//!
//! What is served is the *processed* document: after overlays, after the
//! visibility filter for whoever is asking. The original may be offered beside
//! it, clearly labelled, for the reader who wants what the API team published
//! rather than what this site made of it.

use serde::Serialize;

use crate::SpecError;
use crate::tree::{Value, to_json, to_yaml};
use crate::visibility::{self, Audience};

/// Where a spec's documents are served from.
pub const BASE: &str = "/openapi";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Routes {
    pub json: String,
    pub yaml: String,
    /// Present only when the site offers the unprocessed document.
    pub original_json: Option<String>,
    pub original_yaml: Option<String>,
}

impl Routes {
    pub fn of(id: &str, base: &str, with_original: bool) -> Self {
        let base = base.trim_end_matches('/');
        Self {
            json: format!("{base}/{id}.json"),
            yaml: format!("{base}/{id}.yaml"),
            original_json: with_original.then(|| format!("{base}/{id}.original.json")),
            original_yaml: with_original.then(|| format!("{base}/{id}.original.yaml")),
        }
    }
}

/// One spec's downloadable documents.
#[derive(Debug, Clone)]
pub struct Downloads {
    pub id: String,
    /// The document after overlays, before the per-reader filter.
    processed: Value,
    original: Option<Value>,
}

impl Downloads {
    pub fn new(id: impl Into<String>, processed: Value) -> Self {
        Self {
            id: id.into(),
            processed,
            original: None,
        }
    }

    /// Also offer the document as it was fetched (API-50).
    #[must_use]
    pub fn with_original(mut self, original: Value) -> Self {
        self.original = Some(original);
        self
    }

    pub fn routes(&self, base: &str) -> Routes {
        Routes::of(&self.id, base, self.original.is_some())
    }

    /// The processed document for one reader, as JSON.
    pub fn json(&self, audience: &Audience) -> Result<String, SpecError> {
        to_json(&self.for_audience(audience))
    }

    pub fn yaml(&self, audience: &Audience) -> Result<String, SpecError> {
        to_yaml(&self.for_audience(audience))
    }

    /// The original, which is served unfiltered because it is what the API
    /// team published; a site that does not want that does not offer it.
    pub fn original_json(&self) -> Option<Result<String, SpecError>> {
        self.original.as_ref().map(to_json)
    }

    pub fn original_yaml(&self) -> Option<Result<String, SpecError>> {
        self.original.as_ref().map(to_yaml)
    }

    fn for_audience(&self, audience: &Audience) -> Value {
        let mut tree = self.processed.clone();
        visibility::filter_tree(&mut tree, audience);
        tree
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::parse;

    const SPEC: &str = r##"
openapi: 3.1.0
info: { title: T, version: "1" }
paths:
  /open:
    get: { operationId: open, responses: { "200": { description: ok } } }
  /secret:
    get:
      operationId: secret
      x-liyasa: { hidden: true }
      responses: { "200": { description: ok } }
"##;

    fn downloads() -> Downloads {
        let tree = parse(SPEC.as_bytes(), "api.yaml").expect("the fixture parses");
        Downloads::new("api", tree)
    }

    #[test]
    fn the_routes_are_the_id_under_the_base() {
        let routes = downloads().routes(BASE);
        assert_eq!(routes.json, "/openapi/api.json");
        assert_eq!(routes.yaml, "/openapi/api.yaml");
        assert_eq!(routes.original_json, None, "not offered unless it is given");
    }

    #[test]
    fn the_original_gets_its_own_routes_when_it_is_offered() {
        let tree = parse(SPEC.as_bytes(), "api.yaml").expect("parses");
        let routes = downloads().with_original(tree).routes(BASE);
        assert_eq!(
            routes.original_json.as_deref(),
            Some("/openapi/api.original.json")
        );
        assert_eq!(
            routes.original_yaml.as_deref(),
            Some("/openapi/api.original.yaml")
        );
    }

    #[test]
    fn a_hidden_operation_is_not_in_the_processed_download() {
        let json = downloads()
            .json(&Audience::public())
            .expect("it serializes");
        assert!(json.contains("/open"), "{json}");
        assert!(!json.contains("/secret"), "{json}");
    }

    #[test]
    fn the_original_is_served_as_written() {
        let tree = parse(SPEC.as_bytes(), "api.yaml").expect("parses");
        let downloads = downloads().with_original(tree);
        let json = downloads
            .original_json()
            .expect("it is offered")
            .expect("it serializes");
        assert!(
            json.contains("/secret"),
            "the original is what was published"
        );
    }

    #[test]
    fn both_encodings_carry_the_same_document_in_the_same_order() {
        let downloads = downloads();
        let json = downloads.json(&Audience::public()).expect("json");
        let yaml = downloads.yaml(&Audience::public()).expect("yaml");
        assert!(json.find("\"openapi\"") < json.find("\"paths\""));
        assert!(yaml.find("openapi:") < yaml.find("paths:"));
    }
}
