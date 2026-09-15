//! A page per named component schema (API-15).
//!
//! Optional, and off by default: a site with six schemas does not want six
//! more pages. A site with two hundred does, because "what is a `LineItem`"
//! is then a question with no other answer, and "used by 6 operations" is the
//! answer to the one after it.

use serde::Serialize;

use crate::field::{self, Field};
use crate::model::{Method, OperationRef, Schema, Spec, slug};
use crate::nav::Entry;

/// Where schema pages live under the reference's base.
pub const SEGMENT: &str = "schemas";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaPage {
    pub spec: String,
    pub name: String,
    pub route: String,
    pub title: String,
    pub description: Option<String>,
    pub field: Field,
    /// Every operation that reads or writes this schema.
    pub used_by: Vec<Usage>,
}

impl SchemaPage {
    /// "used by 6 operations", as a renderer would say it.
    pub fn usage_summary(&self) -> String {
        let operations: Vec<&str> = {
            let mut seen: Vec<&str> = Vec::new();
            for usage in &self.used_by {
                if !seen.contains(&usage.selector.as_str()) {
                    seen.push(&usage.selector);
                }
            }
            seen
        };
        match operations.len() {
            0 => "Not used by any operation".to_owned(),
            1 => "Used by 1 operation".to_owned(),
            many => format!("Used by {many} operations"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub selector: String,
    pub method: Method,
    pub path: String,
    /// The page the operation is on, when navigation generated one.
    pub route: Option<String>,
    /// `request body`, `200 response`, `parameter id`.
    pub place: String,
}

/// Builds a page for every named component schema.
///
/// `entries` is what navigation generated, so a usage can link to the page the
/// operation is actually on rather than to one this function guesses.
pub fn pages(spec: &Spec, base: &str, entries: &[Entry]) -> Vec<SchemaPage> {
    spec.components
        .schemas
        .iter()
        .filter(|(_, schema)| !schema.liyasa_hidden())
        .map(|(name, schema)| SchemaPage {
            spec: spec.id.clone(),
            name: name.to_owned(),
            route: route(base, name),
            title: schema.title.clone().unwrap_or_else(|| name.to_owned()),
            description: schema.description.clone(),
            field: Field::of_schema(name, schema, true, field::DEPTH),
            used_by: usages(spec, name, entries),
        })
        .collect()
}

pub fn route(base: &str, name: &str) -> String {
    format!("/{}/{SEGMENT}/{}", base.trim_matches('/'), slug(name))
}

/// Every place an operation reaches a schema by name.
fn usages(spec: &Spec, name: &str, entries: &[Entry]) -> Vec<Usage> {
    let mut out = Vec::new();
    for operation in spec.operations() {
        let mut places = Vec::new();
        for parameter in operation.parameters() {
            if let Some(schema) = &parameter.schema
                && mentions(schema, name)
            {
                places.push(format!("parameter `{}`", parameter.name));
            }
        }
        if let Some(body) = &operation.operation.request_body
            && body
                .content
                .values()
                .any(|media| media.schema.as_ref().is_some_and(|s| mentions(s, name)))
        {
            places.push("request body".to_owned());
        }
        for (status, response) in operation.operation.responses.iter() {
            if response
                .content
                .values()
                .any(|media| media.schema.as_ref().is_some_and(|s| mentions(s, name)))
            {
                places.push(format!("{status} response"));
            }
        }
        out.extend(places.into_iter().map(|place| Usage {
            selector: operation.selector(),
            method: operation.method,
            path: operation.path.to_owned(),
            route: route_of(&operation, entries),
            place,
        }));
    }
    out
}

fn route_of(operation: &OperationRef<'_>, entries: &[Entry]) -> Option<String> {
    let selector = operation.selector();
    entries
        .iter()
        .find(|entry| entry.selector == selector)
        .map(|entry| entry.route.clone())
}

/// Whether a schema is, or contains, the named component.
///
/// The walk stops at a named schema other than the one being looked for: that
/// one has its own page, and `Order` "using" `Money` through `LineItem` is a
/// fact about `LineItem`.
fn mentions(schema: &Schema, name: &str) -> bool {
    if schema.name.as_deref() == Some(name) {
        return true;
    }
    if schema.name.is_some() {
        return false;
    }
    schema
        .items
        .as_deref()
        .is_some_and(|items| mentions(items, name))
        || schema.properties.values().any(|p| mentions(p, name))
        || schema.variants().iter().any(|v| mentions(v, name))
        || schema.all_of.iter().any(|v| mentions(v, name))
        || schema
            .additional_properties
            .schema()
            .is_some_and(|extra| mentions(extra, name))
}

impl Schema {
    /// `x-liyasa.hidden` on a component schema (API-51).
    fn liyasa_hidden(&self) -> bool {
        crate::model::XLiyasa::read(&self.extensions).hidden
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SpecConfig;
    use crate::load;
    use crate::nav::{self, Node};

    const SPEC: &str = r##"
openapi: 3.1.0
info: { title: Shop, version: "1" }
paths:
  /orders:
    get:
      operationId: listOrders
      parameters:
        - { name: status, in: query, schema: { $ref: "#/components/schemas/Status" } }
      responses:
        "200":
          description: ok
          content:
            application/json:
              schema:
                type: array
                items: { $ref: "#/components/schemas/Order" }
    post:
      operationId: createOrder
      requestBody:
        content:
          application/json:
            schema: { $ref: "#/components/schemas/Order" }
      responses:
        "201":
          description: made
          content:
            application/json:
              schema: { $ref: "#/components/schemas/Order" }
components:
  schemas:
    Status:
      type: string
      enum: [open, closed]
    Money:
      type: object
      properties:
        amount: { type: integer }
    LineItem:
      type: object
      properties:
        price: { $ref: "#/components/schemas/Money" }
    Order:
      title: An order
      description: One customer's order.
      type: object
      properties:
        id: { type: string }
        items:
          type: array
          items: { $ref: "#/components/schemas/LineItem" }
    Unused:
      type: object
      x-liyasa: { hidden: true }
"##;

    fn built() -> (Spec, Vec<SchemaPage>) {
        let spec = load::from_bytes("api", "api.yaml", SPEC.as_bytes())
            .expect("the spec loads")
            .spec;
        let config = SpecConfig::parse(&serde_json::json!("openapi/api.yaml")).expect("it reads");
        let (reference, _) = nav::generate(&spec, &config, &Node::default());
        let entries: Vec<_> = reference.entries().cloned().collect();
        let pages = pages(&spec, nav::DEFAULT_BASE, &entries);
        (spec, pages)
    }

    fn page<'a>(pages: &'a [SchemaPage], name: &str) -> &'a SchemaPage {
        pages
            .iter()
            .find(|page| page.name == name)
            .unwrap_or_else(|| panic!("no page for `{name}`"))
    }

    #[test]
    fn every_named_schema_gets_a_route_under_the_reference() {
        let (_, pages) = built();
        assert_eq!(page(&pages, "Order").route, "/api-reference/schemas/order");
        assert_eq!(
            page(&pages, "LineItem").route,
            "/api-reference/schemas/lineitem"
        );
    }

    #[test]
    fn a_hidden_schema_gets_no_page() {
        let (_, pages) = built();
        assert!(pages.iter().all(|page| page.name != "Unused"));
    }

    #[test]
    fn the_title_and_description_come_from_the_schema_when_it_has_them() {
        let (_, pages) = built();
        let order = page(&pages, "Order");
        assert_eq!(order.title, "An order");
        assert_eq!(order.description.as_deref(), Some("One customer's order."));

        let status = page(&pages, "Status");
        assert_eq!(
            status.title, "Status",
            "the name stands in for a missing title"
        );
    }

    #[test]
    fn usages_name_the_operation_and_the_place_in_it() {
        let (_, pages) = built();
        let order = page(&pages, "Order");
        let places: Vec<&str> = order.used_by.iter().map(|u| u.place.as_str()).collect();
        assert!(places.contains(&"request body"), "{places:?}");
        assert!(places.contains(&"200 response"), "{places:?}");
        assert!(places.contains(&"201 response"), "{places:?}");
        assert_eq!(order.usage_summary(), "Used by 2 operations");
        assert_eq!(
            order.used_by[0].route.as_deref(),
            Some("/api-reference/listorders"),
            "a usage links to the page navigation generated"
        );
    }

    #[test]
    fn a_parameter_counts_as_a_usage() {
        let (_, pages) = built();
        let status = page(&pages, "Status");
        assert_eq!(status.used_by.len(), 1);
        assert_eq!(status.used_by[0].place, "parameter `status`");
    }

    #[test]
    fn a_schema_reached_only_through_another_named_one_is_that_ones_business() {
        let (_, pages) = built();
        assert_eq!(
            page(&pages, "Money").usage_summary(),
            "Not used by any operation",
            "`Money` is used by `LineItem`, which has its own page"
        );
        assert_eq!(
            page(&pages, "LineItem").usage_summary(),
            "Not used by any operation"
        );
    }

    #[test]
    fn the_page_carries_the_rows_a_table_renders() {
        let (_, pages) = built();
        let order = page(&pages, "Order");
        assert_eq!(
            order
                .field
                .children
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>(),
            vec!["id", "items"]
        );
    }
}
