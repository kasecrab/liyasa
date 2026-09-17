//! The five read-only tools (AST-10).
//!
//! There are no write tools, at any trust level. That is not a policy this
//! module enforces at call time but a property of the list: [`SPECS`] is the
//! whole surface, and a test asserts that every entry is read-only, so a write
//! tool cannot be added without the test that forbids it failing.
//!
//! Every tool's OUTPUT is untrusted. A page body is `member` text and a
//! reader's own current page is `anonymous`; both come back as a
//! [`DataBlock`](liyasa_core::ai::DataBlock) and never as a plain string the
//! caller might paste into a prompt.

use liyasa_core::ai::{DataBlock, ToolSpec, TrustLevel};
use liyasa_core::ids::Route;
use liyasa_core::net::BoxFut;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::index::{ChunkQuery, Hit};

pub const SEARCH: &str = "search";
pub const GET_PAGE: &str = "get_page";
pub const GET_OPENAPI: &str = "get_openapi";
pub const LIST_NAVIGATION: &str = "list_navigation";
pub const GET_CURRENT_PAGE: &str = "get_current_page";

/// Every tool the assistant may offer, in the order AST-10 lists them.
pub fn specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: SEARCH.to_owned(),
            description: "Search this site's documentation. Returns passages with their routes."
                .to_owned(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "filters": {
                        "type": "object",
                        "properties": {
                            "version": { "type": "string" },
                            "locale": { "type": "string" },
                            "kind": { "type": "string", "enum": ["prose", "operation"] }
                        },
                        "additionalProperties": false
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
            min_trust: TrustLevel::External,
        },
        ToolSpec {
            name: GET_PAGE.to_owned(),
            description: "Read one page, or one section of it, as Markdown.".to_owned(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "route": { "type": "string" },
                    "section": { "type": "string" }
                },
                "required": ["route"],
                "additionalProperties": false
            }),
            min_trust: TrustLevel::External,
        },
        ToolSpec {
            name: GET_OPENAPI.to_owned(),
            description: "Read one API operation, named `METHOD /path` or by its operationId."
                .to_owned(),
            input_schema: json!({
                "type": "object",
                "properties": { "operation": { "type": "string" } },
                "required": ["operation"],
                "additionalProperties": false
            }),
            min_trust: TrustLevel::External,
        },
        ToolSpec {
            name: LIST_NAVIGATION.to_owned(),
            description: "List the site's navigation, as routes and titles.".to_owned(),
            input_schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
            min_trust: TrustLevel::External,
        },
        ToolSpec {
            name: GET_CURRENT_PAGE.to_owned(),
            description: "Read the page the reader is on, and their selection if they made one."
                .to_owned(),
            input_schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
            min_trust: TrustLevel::External,
        },
    ]
}

/// A page, or a section of one, as the model is shown it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageExcerpt {
    pub route: String,
    pub title: String,
    /// `""` for the whole page.
    pub anchor: String,
    pub markdown: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavEntry {
    pub route: String,
    pub title: String,
    pub depth: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ToolError {
    #[error("`{0}` is not a tool this assistant has")]
    Unknown(String),
    #[error("{tool}: {message}")]
    BadInput { tool: String, message: String },
    #[error("{0}")]
    Unavailable(String),
}

/// What the server supplies. Every method is a read.
pub trait Tools: Send + Sync {
    fn search<'a>(
        &'a self,
        query: &'a str,
        filter: &'a ChunkQuery,
    ) -> BoxFut<'a, Result<Vec<Hit>, ToolError>>;

    fn get_page<'a>(
        &'a self,
        route: &'a Route,
        section: Option<&'a str>,
    ) -> BoxFut<'a, Result<Option<PageExcerpt>, ToolError>>;

    fn get_openapi<'a>(
        &'a self,
        operation: &'a str,
    ) -> BoxFut<'a, Result<Option<PageExcerpt>, ToolError>>;

    fn list_navigation<'a>(&'a self) -> BoxFut<'a, Result<Vec<NavEntry>, ToolError>>;

    fn get_current_page<'a>(&'a self) -> BoxFut<'a, Result<Option<PageExcerpt>, ToolError>>;
}

/// A tool result, wrapped as data before it can reach the model.
pub fn result_block(tool: &str, label: &str, content: String, trust: TrustLevel) -> DataBlock {
    DataBlock {
        label: format!("{tool}: {label}"),
        trust,
        content,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_assistant_has_no_write_tools() {
        // AST-10 and §30.2.2 item 3. The list IS the surface, so this is the
        // check rather than a runtime refusal.
        const WRITE_WORDS: &[&str] = &[
            "write", "create", "update", "delete", "edit", "put", "post", "set", "publish",
            "merge", "propose", "run", "exec",
        ];
        for spec in specs() {
            for word in WRITE_WORDS {
                assert!(
                    !spec.name.contains(word),
                    "`{}` looks like a write tool",
                    spec.name
                );
            }
        }
    }

    #[test]
    fn every_tool_ast_10_names_is_present_and_nothing_else_is() {
        let names: Vec<String> = specs().into_iter().map(|s| s.name).collect();
        assert_eq!(
            names,
            [
                SEARCH,
                GET_PAGE,
                GET_OPENAPI,
                LIST_NAVIGATION,
                GET_CURRENT_PAGE
            ]
        );
    }

    #[test]
    fn every_tool_is_available_to_an_anonymous_reader() {
        // The assistant answers readers who are not signed in; a tool gated
        // above `external` would make the whole feature unavailable to them.
        for spec in specs() {
            assert_eq!(spec.min_trust, TrustLevel::External, "{}", spec.name);
        }
    }

    #[test]
    fn every_input_schema_is_closed() {
        for spec in specs() {
            assert_eq!(
                spec.input_schema["additionalProperties"],
                serde_json::Value::Bool(false),
                "`{}` accepts keys it does not declare",
                spec.name
            );
        }
    }
}
