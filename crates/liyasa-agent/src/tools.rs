//! The agent's tool surface (AGT-03).
//!
//! Sixteen tools, each with a JSON Schema for its input and a `min_trust` below
//! which it does not exist. [`specs`] is the whole surface: a tool that is not
//! here cannot be called, and [`crate::dispatch`] is the only way to call one.
//!
//! `min_trust` is the coarse gate and it is not the only one. Seven of these tools
//! write, and what they may write is decided per call by
//! [`crate::trust::Restrictions`] — `write_page` is available to an
//! untrusted-trigger run because AGT-04 gives such a run a write scope rather
//! than no writes at all, and the scope is checked when the page is named, not
//! when the tool is offered.
//!
//! Six tools are `member` and above, and each for a reason AGT-04 states:
//!
//! - `move_page` changes a route, which changes navigation and redirects.
//! - `edit_navigation` is navigation.
//! - `propose_fact_update` is facts.
//! - `read_repo_file` and `search_repo` read a private source tree, and a run that
//!   can read source and write pages can copy one into the other.
//! - `web_fetch` puts whatever the run has in a URL, so it is an outbound channel
//!   whatever its allow list says.
//!
//! `TrustLevel` is ordered most-trusted first, so `min_trust: External` means
//! "everyone" and `min_trust: Member` means "operator and member". That reads
//! backwards the first time; `liyasa_ai::prompt::tools_for` is the filter, and its
//! comparison is `trust <= min_trust`.

use liyasa_core::ai::{ToolSpec, TrustLevel};
use serde_json::{Value, json};

pub const SEARCH_DOCS: &str = "search_docs";
pub const READ_PAGE: &str = "read_page";
pub const WRITE_PAGE: &str = "write_page";
pub const MOVE_PAGE: &str = "move_page";
pub const EDIT_NAVIGATION: &str = "edit_navigation";
pub const READ_REPO_FILE: &str = "read_repo_file";
pub const SEARCH_REPO: &str = "search_repo";
pub const GET_OPENAPI: &str = "get_openapi";
pub const GET_FACT: &str = "get_fact";
pub const PROPOSE_FACT_UPDATE: &str = "propose_fact_update";
pub const RUN_VALIDATE: &str = "run_validate";
pub const RUN_VERIFY: &str = "run_verify";
pub const RENDER_PREVIEW: &str = "render_preview";
pub const LIST_DRIFT: &str = "list_drift";
pub const WEB_FETCH: &str = "web_fetch";
pub const ASK_REVIEWER: &str = "ask_reviewer";

/// Every tool, in the order AGT-03 lists them.
pub const NAMES: [&str; 16] = [
    SEARCH_DOCS,
    READ_PAGE,
    WRITE_PAGE,
    MOVE_PAGE,
    EDIT_NAVIGATION,
    READ_REPO_FILE,
    SEARCH_REPO,
    GET_OPENAPI,
    GET_FACT,
    PROPOSE_FACT_UPDATE,
    RUN_VALIDATE,
    RUN_VERIFY,
    RENDER_PREVIEW,
    LIST_DRIFT,
    WEB_FETCH,
    ASK_REVIEWER,
];

/// The tools that write something, as opposed to reading or asking.
///
/// Named so the per-call scope check in [`crate::dispatch`] cannot be forgotten
/// for a tool added later: a write tool absent from this list fails a test.
pub const WRITES: [&str; 4] = [WRITE_PAGE, MOVE_PAGE, EDIT_NAVIGATION, PROPOSE_FACT_UPDATE];

fn spec_of(
    name: &'static str,
    description: &'static str,
    min_trust: TrustLevel,
    input_schema: Value,
) -> ToolSpec {
    ToolSpec {
        name: name.to_owned(),
        description: description.to_owned(),
        input_schema,
        min_trust,
    }
}

/// An object schema that refuses keys it does not name.
fn object(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    })
}

fn route() -> Value {
    json!({ "type": "string", "pattern": "^/" })
}

/// Every tool the agent may be offered.
pub fn specs() -> &'static [ToolSpec] {
    static COMPILED: std::sync::OnceLock<Vec<ToolSpec>> = std::sync::OnceLock::new();
    COMPILED.get_or_init(build)
}

/// One tool by name.
pub fn spec(name: &str) -> Option<&'static ToolSpec> {
    specs().iter().find(|s| s.name == name)
}

fn build() -> Vec<ToolSpec> {
    vec![
        spec_of(
            SEARCH_DOCS,
            "Search this site's documentation. Returns passages with their routes.",
            TrustLevel::External,
            object(
                json!({
                    "query": { "type": "string", "minLength": 1 },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 50 },
                    "version": { "type": "string" },
                    "locale": { "type": "string" }
                }),
                &["query"],
            ),
        ),
        spec_of(
            READ_PAGE,
            "Read one page, or one section of it, as Markdown.",
            TrustLevel::External,
            object(
                json!({ "route": route(), "section": { "type": "string" } }),
                &["route"],
            ),
        ),
        spec_of(
            WRITE_PAGE,
            "Replace a page's Markdown in the sandboxed working copy. The page must \
             be inside this run's write scope.",
            TrustLevel::External,
            object(
                json!({
                    "route": route(),
                    "markdown": { "type": "string" },
                    "message": { "type": "string" }
                }),
                &["route", "markdown"],
            ),
        ),
        spec_of(
            MOVE_PAGE,
            "Move a page to a new route, leaving a redirect behind.",
            TrustLevel::Member,
            object(json!({ "from": route(), "to": route() }), &["from", "to"]),
        ),
        spec_of(
            EDIT_NAVIGATION,
            "Change the site's navigation tree.",
            TrustLevel::Member,
            object(
                json!({
                    "operation": { "type": "string", "enum": ["insert", "move", "remove", "rename"] },
                    "route": route(),
                    "parent": route(),
                    "position": { "type": "integer", "minimum": 0 },
                    "title": { "type": "string" }
                }),
                &["operation", "route"],
            ),
        ),
        spec_of(
            READ_REPO_FILE,
            "Read one file from a configured context repository.",
            TrustLevel::Member,
            object(
                json!({
                    "repo": { "type": "string", "minLength": 1 },
                    "path": { "type": "string", "minLength": 1 },
                    "ref": { "type": "string" }
                }),
                &["repo", "path"],
            ),
        ),
        spec_of(
            SEARCH_REPO,
            "Search a configured context repository for a string.",
            TrustLevel::Member,
            object(
                json!({
                    "repo": { "type": "string", "minLength": 1 },
                    "query": { "type": "string", "minLength": 1 },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 100 }
                }),
                &["repo", "query"],
            ),
        ),
        spec_of(
            GET_OPENAPI,
            "Read one API operation, named `METHOD /path` or by its operationId.",
            TrustLevel::External,
            object(
                json!({ "operation": { "type": "string", "minLength": 1 }, "spec": { "type": "string" } }),
                &["operation"],
            ),
        ),
        spec_of(
            GET_FACT,
            "Read one fact's current value from the truth graph.",
            TrustLevel::External,
            object(
                json!({ "fact": { "type": "string", "minLength": 1 } }),
                &["fact"],
            ),
        ),
        spec_of(
            PROPOSE_FACT_UPDATE,
            "Propose a new value for a fact, for a reviewer to accept.",
            TrustLevel::Member,
            object(
                json!({
                    "fact": { "type": "string", "minLength": 1 },
                    "value": {},
                    "evidence": { "type": "string" }
                }),
                &["fact", "value", "evidence"],
            ),
        ),
        spec_of(
            RUN_VALIDATE,
            "Run `liyasa validate` over the working copy and return its diagnostics.",
            TrustLevel::External,
            object(
                json!({ "paths": { "type": "array", "items": { "type": "string" } } }),
                &[],
            ),
        ),
        spec_of(
            RUN_VERIFY,
            "Run `liyasa verify --changed` over the working copy.",
            TrustLevel::External,
            object(json!({ "changed_only": { "type": "boolean" } }), &[]),
        ),
        spec_of(
            RENDER_PREVIEW,
            "Render one page of the working copy and return its preview URL.",
            TrustLevel::External,
            object(json!({ "route": route() }), &["route"]),
        ),
        spec_of(
            LIST_DRIFT,
            "List the project's open drift records.",
            TrustLevel::External,
            object(
                json!({
                    "open": { "type": "boolean" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 200 }
                }),
                &[],
            ),
        ),
        spec_of(
            WEB_FETCH,
            "Fetch one allow-listed web page and return its text.",
            TrustLevel::Member,
            object(
                json!({ "url": { "type": "string", "pattern": "^https?://" } }),
                &["url"],
            ),
        ),
        spec_of(
            ASK_REVIEWER,
            "Ask the reviewer a question, surfaced in the proposal.",
            TrustLevel::External,
            object(
                json!({
                    "question": { "type": "string", "minLength": 1 },
                    "about": route()
                }),
                &["question"],
            ),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_agt_03_names_is_present_and_nothing_else_is() {
        let names: Vec<&str> = specs().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, NAMES);
    }

    #[test]
    fn every_input_schema_refuses_keys_it_does_not_declare() {
        // An open schema is not a typed tool: a model that passes an extra key
        // would have it reach whatever deserializes the input.
        for spec in specs() {
            assert_eq!(
                spec.input_schema["additionalProperties"],
                Value::Bool(false),
                "`{}` accepts keys it does not declare",
                spec.name
            );
        }
    }

    #[test]
    fn every_input_schema_compiles() {
        for spec in specs() {
            jsonschema::options()
                .with_draft(jsonschema::Draft::Draft202012)
                .build(&spec.input_schema)
                .unwrap_or_else(|e| {
                    panic!("`{}` has a schema that does not compile: {e}", spec.name)
                });
        }
    }

    #[test]
    fn the_six_tools_agt_04_keeps_from_a_stranger_are_member_and_above() {
        const RESTRICTED: [&str; 6] = [
            MOVE_PAGE,
            EDIT_NAVIGATION,
            PROPOSE_FACT_UPDATE,
            READ_REPO_FILE,
            SEARCH_REPO,
            WEB_FETCH,
        ];
        for name in RESTRICTED {
            let spec = spec(name).unwrap_or_else(|| panic!("`{name}` is missing"));
            assert_eq!(
                spec.min_trust,
                TrustLevel::Member,
                "`{name}` is offered below member"
            );
        }
        for spec in specs() {
            if !RESTRICTED.contains(&spec.name.as_str()) {
                assert_eq!(
                    spec.min_trust,
                    TrustLevel::External,
                    "`{}` is restricted and is not in the list that says why",
                    spec.name
                );
            }
        }
    }

    #[test]
    fn a_stranger_is_offered_write_page_because_the_scope_is_the_gate() {
        // AGT-04 gives an untrusted-trigger run a write scope, not no writes. If
        // this ever became member-only, feedback-triggered runs would produce
        // empty proposals and the scope logic would be dead code.
        let offered = liyasa_ai::prompt::tools_for(specs(), TrustLevel::Anonymous);
        assert!(offered.iter().any(|s| s.name == WRITE_PAGE));
        assert!(!offered.iter().any(|s| s.name == EDIT_NAVIGATION));
    }

    #[test]
    fn an_operator_is_offered_every_tool() {
        assert_eq!(
            liyasa_ai::prompt::tools_for(specs(), TrustLevel::Operator).len(),
            NAMES.len()
        );
    }

    #[test]
    fn every_write_tool_is_listed_as_one() {
        // The per-call scope check keys off WRITES. A write tool added without a
        // row there would be offered and never scope-checked.
        for name in WRITES {
            assert!(spec(name).is_some(), "`{name}` is not a tool");
        }
        const WRITE_WORDS: [&str; 5] = ["write", "move", "edit", "propose", "update"];
        for spec in specs() {
            let looks_like_a_write = WRITE_WORDS.iter().any(|w| spec.name.contains(w));
            assert_eq!(
                looks_like_a_write,
                WRITES.contains(&spec.name.as_str()),
                "`{}` reads as a write tool and is not in WRITES, or the reverse",
                spec.name
            );
        }
    }

    #[test]
    fn every_tool_has_a_description_a_model_can_act_on() {
        for spec in specs() {
            assert!(
                spec.description.len() > 20,
                "`{}` has no usable description",
                spec.name
            );
            assert!(
                spec.description.ends_with('.'),
                "`{}`'s description is not a sentence",
                spec.name
            );
        }
    }

    #[test]
    fn a_route_taking_tool_requires_a_leading_slash() {
        for name in [READ_PAGE, WRITE_PAGE, RENDER_PREVIEW] {
            let schema = &spec(name).expect("a tool").input_schema;
            assert_eq!(
                schema["properties"]["route"]["pattern"], "^/",
                "`{name}` accepts a route that is not one"
            );
        }
    }

    #[test]
    fn web_fetch_takes_an_http_url_and_nothing_else() {
        // The allow list is checked at call time; the schema keeps `file://` and
        // `gopher://` from reaching the code that checks it.
        let schema = &spec(WEB_FETCH).expect("web_fetch").input_schema;
        assert_eq!(schema["properties"]["url"]["pattern"], "^https?://");
    }

    #[test]
    fn no_two_tools_share_a_name() {
        let mut names: Vec<&str> = specs().iter().map(|s| s.name.as_str()).collect();
        let before = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), before);
    }
}
