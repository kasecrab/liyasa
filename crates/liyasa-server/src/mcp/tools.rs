//! The six tools of MCP-01, and the call that answers one.
//!
//! ## A tool that cannot run is advertised, not hidden
//!
//! MCP-04 is explicit about it: under `liyasa mcp --dist` there are no
//! vectors and no server, so `ask` degrades and `report_issue` cannot run at
//! all — and both still appear in `tools/list`, each carrying the reason.
//! Omitting them would make a model conclude the site cannot do those things,
//! which is a different and wrong statement: the site can, this transport
//! cannot. The reason goes in two places on purpose. `_meta` is where the
//! protocol says an implementation may put its own fields, so a client that
//! wants to grey the tool out has somewhere to read; the description carries
//! it too, because the model reads the description and does not read `_meta`.
//!
//! ## A failing tool is not a failing call
//!
//! Every refusal here comes back as a result with `isError: true`, never as a
//! JSON-RPC error. A model can read a tool result and try something else; a
//! transport error it cannot see at all.

use liyasa_core::net::BoxFut;
use serde_json::{Value, json};

use super::reader::{Hit, PageRef, Scope, SiteReader, ToolFailure};

pub const SEARCH: &str = "search";
pub const FETCH: &str = "fetch";
pub const LIST_PAGES: &str = "list_pages";
pub const GET_OPENAPI_OPERATION: &str = "get_openapi_operation";
pub const ASK: &str = "ask";
pub const REPORT_ISSUE: &str = "report_issue";

/// The longest `report_issue` body this server accepts (MCP-03, "size
/// capped"). Long enough for a stack trace and a paragraph, short enough that
/// the one write tool cannot be used as free storage.
pub const MAX_REPORT_BYTES: usize = 8 * 1024;

/// The reader-facing assistant behind the `ask` tool (AST-32).
///
/// A host with no model configured passes `None` and `ask` degrades to search
/// with extractive snippets, saying so in its own result — which is what
/// MCP-04 asks of `--dist` and is equally the honest answer on a server whose
/// operator configured no provider.
pub trait Assistant: Send + Sync {
    fn answer<'a>(
        &'a self,
        question: &'a str,
        scope: &'a Scope,
    ) -> BoxFut<'a, Result<String, ToolFailure>>;
}

/// What an agent reported, already validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// The page the report is about, when the agent named one.
    pub route: Option<String>,
    pub summary: String,
    pub detail: String,
}

/// Where `report_issue` writes (MCP-03: the one write tool).
pub trait Issues: Send + Sync {
    /// Returns what to tell the agent — an identifier it can quote, or a
    /// confirmation. Never a URL into an operator's tracker that an anonymous
    /// caller could not open.
    fn report<'a>(
        &'a self,
        issue: &'a Issue,
        scope: &'a Scope,
    ) -> BoxFut<'a, Result<String, ToolFailure>>;
}

/// Everything a call needs. Built per request, because `scope` is.
pub struct Host<'a> {
    pub reader: &'a dyn SiteReader,
    pub assistant: Option<&'a dyn Assistant>,
    pub issues: Option<&'a dyn Issues>,
}

impl Host<'_> {
    /// Why `ask` cannot answer properly here, if it cannot.
    fn ask_degraded(&self) -> Option<&'static str> {
        self.assistant.is_none().then_some(
            "no model is configured on this host, so `ask` answers with keyword search and \
             extracted snippets rather than a written answer. Its results are passages, not \
             prose.",
        )
    }

    fn report_unavailable(&self) -> Option<&'static str> {
        self.issues.is_none().then_some(
            "reporting needs a running `liyasa serve` to file against; this host is serving a \
             static build and has nowhere to put a report.",
        )
    }

    /// `tools/list`.
    pub fn list(&self) -> Value {
        let tools: Vec<Value> = vec![
            declare(
                SEARCH,
                "Search the documentation",
                "Search this site's documentation. Returns passages with the route and anchor \
                 each one came from, so a result can be fetched or linked without guessing.",
                json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "What to look for." },
                        "limit": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": super::bundle_reader::MAX_LIMIT,
                            "description": "How many results to return. Default 10."
                        }
                    },
                    "required": ["query"],
                    "additionalProperties": false
                }),
                true,
                None,
            ),
            declare(
                FETCH,
                "Read a page",
                "Read one page as Markdown, or one section of it. `route` accepts a route \
                 (`/guides/install`), that page's `.md` twin, or its absolute URL.",
                json!({
                    "type": "object",
                    "properties": {
                        "route": { "type": "string", "description": "Route, `.md` twin, or URL." },
                        "section": {
                            "type": "string",
                            "description": "A heading's anchor or its text. Omit for the whole page."
                        }
                    },
                    "required": ["route"],
                    "additionalProperties": false
                }),
                true,
                None,
            ),
            declare(
                LIST_PAGES,
                "List every page",
                "Every page of this site, as routes and titles with their depth in the tree. \
                 The tree is derived from the routes: a built site carries no navigation, so \
                 `/guides/install` is under `/guides` by its path and not by a declaration.",
                json!({ "type": "object", "properties": {}, "additionalProperties": false }),
                true,
                None,
            ),
            declare(
                GET_OPENAPI_OPERATION,
                "Read an API operation",
                "Read one API operation from this site's published OpenAPI specification. \
                 Name it `GET /pets/{id}` or by its operationId.",
                json!({
                    "type": "object",
                    "properties": {
                        "operation": {
                            "type": "string",
                            "description": "`METHOD /path`, or an operationId."
                        }
                    },
                    "required": ["operation"],
                    "additionalProperties": false
                }),
                true,
                None,
            ),
            declare(
                ASK,
                "Ask the documentation",
                "Ask this documentation site a question and get an answer with citations that \
                 deep-link to the sections it used.",
                json!({
                    "type": "object",
                    "properties": {
                        "question": { "type": "string" }
                    },
                    "required": ["question"],
                    "additionalProperties": false
                }),
                true,
                self.ask_degraded(),
            ),
            declare(
                REPORT_ISSUE,
                "Report a documentation problem",
                "Report something wrong or missing in this documentation. This is the only \
                 tool that writes. It files feedback for the documentation's maintainers; it \
                 does not change the site and nothing it writes is served back to anyone.",
                json!({
                    "type": "object",
                    "properties": {
                        "summary": {
                            "type": "string",
                            "description": "One line: what is wrong."
                        },
                        "detail": {
                            "type": "string",
                            "description": "What you expected and what you found."
                        },
                        "route": {
                            "type": "string",
                            "description": "The page it is about, when it is about one."
                        }
                    },
                    "required": ["summary"],
                    "additionalProperties": false
                }),
                false,
                self.report_unavailable(),
            ),
        ];
        json!({ "tools": tools })
    }

    /// `tools/call`.
    pub async fn call(&self, name: &str, arguments: &Value, scope: &Scope) -> Value {
        let outcome = match name {
            SEARCH => self.search(arguments, scope),
            FETCH => self.fetch(arguments, scope),
            LIST_PAGES => self.list_pages(scope),
            GET_OPENAPI_OPERATION => self.operation(arguments, scope),
            ASK => self.ask(arguments, scope).await,
            REPORT_ISSUE => self.report(arguments, scope).await,
            other => Err(ToolFailure::NotFound(format!(
                "`{other}` is not a tool this server has. It has: {}",
                [
                    SEARCH,
                    FETCH,
                    LIST_PAGES,
                    GET_OPENAPI_OPERATION,
                    ASK,
                    REPORT_ISSUE
                ]
                .join(", ")
            ))),
        };
        match outcome {
            Ok(result) => result,
            Err(failure) => error_result(&failure.to_string()),
        }
    }

    fn search(&self, arguments: &Value, scope: &Scope) -> Result<Value, ToolFailure> {
        let query = required_str(arguments, "query")?;
        let limit = arguments
            .get("limit")
            .and_then(Value::as_u64)
            .map(|n| n as usize)
            .unwrap_or(super::bundle_reader::DEFAULT_LIMIT);
        let hits = self.reader.search(query, limit, scope)?;
        Ok(result(
            hit_transcript(query, &hits),
            json!({
                "query": query,
                "results": hits.iter().map(hit_value).collect::<Vec<_>>()
            }),
        ))
    }

    fn fetch(&self, arguments: &Value, scope: &Scope) -> Result<Value, ToolFailure> {
        let route = required_str(arguments, "route")?;
        let section = arguments.get("section").and_then(Value::as_str);
        let page = self.reader.page(route, section, scope)?;
        Ok(result(
            page.markdown.clone(),
            json!({
                "route": page.route,
                "title": page.title,
                "anchor": page.anchor,
                "markdown": page.markdown
            }),
        ))
    }

    fn list_pages(&self, scope: &Scope) -> Result<Value, ToolFailure> {
        let pages = self.reader.pages(scope);
        Ok(result(
            page_transcript(&pages),
            json!({
                "pages": pages
                    .iter()
                    .map(|page| json!({
                        "route": page.route,
                        "title": page.title,
                        "depth": page.depth
                    }))
                    .collect::<Vec<_>>()
            }),
        ))
    }

    fn operation(&self, arguments: &Value, scope: &Scope) -> Result<Value, ToolFailure> {
        let name = required_str(arguments, "operation")?;
        let page = self.reader.operation(name, scope)?;
        Ok(result(
            page.markdown.clone(),
            json!({
                "operation": name,
                "specification": page.route,
                "title": page.title,
                "markdown": page.markdown
            }),
        ))
    }

    async fn ask(&self, arguments: &Value, scope: &Scope) -> Result<Value, ToolFailure> {
        let question = required_str(arguments, "question")?;
        if let Some(assistant) = self.assistant {
            let answer = assistant.answer(question, scope).await?;
            return Ok(result(
                answer.clone(),
                json!({ "question": question, "answer": answer, "degraded": false }),
            ));
        }
        // MCP-04's degradation. It says so in the RESULT and not only in the
        // tool list, because the caller that reads the result is often not
        // the one that read the list.
        let hits = self
            .reader
            .search(question, super::bundle_reader::DEFAULT_LIMIT, scope)?;
        let notice = self.ask_degraded().unwrap_or_default();
        let text = format!("{notice}\n\n{}", hit_transcript(question, &hits));
        Ok(result(
            text,
            json!({
                "question": question,
                "degraded": true,
                "reason": notice,
                "results": hits.iter().map(hit_value).collect::<Vec<_>>()
            }),
        ))
    }

    async fn report(&self, arguments: &Value, scope: &Scope) -> Result<Value, ToolFailure> {
        let Some(issues) = self.issues else {
            return Err(ToolFailure::Unavailable(
                self.report_unavailable().unwrap_or_default().to_owned(),
            ));
        };
        let issue = read_issue(arguments)?;
        let receipt = issues.report(&issue, scope).await?;
        Ok(result(
            receipt.clone(),
            json!({ "accepted": true, "receipt": receipt }),
        ))
    }
}

/// Validates a report before anything is written.
///
/// The cap is on the whole submission rather than per field, because two
/// fields each under the limit are the same bytes on disk as one over it.
pub fn read_issue(arguments: &Value) -> Result<Issue, ToolFailure> {
    let summary = required_str(arguments, "summary")?.trim().to_owned();
    let detail = arguments
        .get("detail")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_owned();
    let route = arguments
        .get("route")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|route| !route.is_empty())
        .map(str::to_owned);
    let size = summary.len() + detail.len() + route.as_ref().map_or(0, String::len);
    if size > MAX_REPORT_BYTES {
        return Err(ToolFailure::BadInput(format!(
            "a report may be {MAX_REPORT_BYTES} bytes and this one is {size}; \
             send the shortest version that still identifies the problem"
        )));
    }
    Ok(Issue {
        route,
        summary,
        detail,
    })
}

fn required_str<'a>(arguments: &'a Value, key: &str) -> Result<&'a str, ToolFailure> {
    match arguments.get(key).and_then(Value::as_str) {
        Some(text) if !text.trim().is_empty() => Ok(text),
        Some(_) => Err(ToolFailure::BadInput(format!("`{key}` is empty"))),
        None => Err(ToolFailure::BadInput(format!(
            "`{key}` is required and was not given"
        ))),
    }
}

/// One tool's declaration.
///
/// `unavailable` is carried in `_meta` and repeated in the description; see
/// the module comment for why both.
fn declare(
    name: &str,
    title: &str,
    description: &str,
    input_schema: Value,
    read_only: bool,
    unavailable: Option<&str>,
) -> Value {
    let description = match unavailable {
        Some(reason) => format!("{description}\n\nNot fully available here: {reason}"),
        None => description.to_owned(),
    };
    let mut tool = json!({
        "name": name,
        "title": title,
        "description": description,
        "inputSchema": input_schema,
        "annotations": {
            "title": title,
            "readOnlyHint": read_only,
            "destructiveHint": false,
            // Every answer comes out of this site's own build, so a repeated
            // call with the same arguments gives the same answer until the
            // site is rebuilt.
            "openWorldHint": false
        }
    });
    if let Some(reason) = unavailable
        && let Some(object) = tool.as_object_mut()
    {
        object.insert("_meta".to_owned(), json!({ "liyasa/unavailable": reason }));
    }
    tool
}

/// A successful tool result.
pub fn result(text: String, structured: Value) -> Value {
    json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": structured,
        "isError": false
    })
}

/// A refusal the model can read and act on.
pub fn error_result(message: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": message }],
        "isError": true
    })
}

fn hit_value(hit: &Hit) -> Value {
    json!({
        "route": hit.route,
        "title": hit.title,
        "section": hit.section,
        "anchor": hit.anchor,
        "snippet": hit.snippet
    })
}

/// What a model reads. One line per result: an agent that has to parse prose
/// to find a route will parse it wrong.
fn hit_transcript(query: &str, hits: &[Hit]) -> String {
    use std::fmt::Write as _;

    if hits.is_empty() {
        return format!("No results for `{query}`.");
    }
    let mut out = format!(
        "{} result{} for `{query}`:\n",
        hits.len(),
        if hits.len() == 1 { "" } else { "s" }
    );
    for (rank, hit) in hits.iter().enumerate() {
        let target = match hit.anchor.is_empty() {
            true => hit.route.clone(),
            false => format!("{}#{}", hit.route, hit.anchor),
        };
        let name = match hit.section.is_empty() || hit.section == hit.title {
            true => hit.title.clone(),
            false => format!("{} › {}", hit.title, hit.section),
        };
        let _ = write!(out, "\n{}. {target} — {name}", rank + 1);
        if !hit.snippet.is_empty() {
            let _ = write!(out, "\n   {}", hit.snippet);
        }
    }
    out
}

fn page_transcript(pages: &[PageRef]) -> String {
    use std::fmt::Write as _;

    if pages.is_empty() {
        return "This site has no pages you may read.".to_owned();
    }
    let mut out = format!("{} pages:\n", pages.len());
    for page in pages {
        let indent = "  ".repeat(usize::from(page.depth).min(6));
        let _ = write!(out, "\n{indent}{} — {}", page.route, page.title);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool<'a>(list: &'a Value, name: &str) -> &'a Value {
        list["tools"]
            .as_array()
            .expect("a tools array")
            .iter()
            .find(|tool| tool["name"] == name)
            .unwrap_or_else(|| panic!("`{name}` is not advertised"))
    }

    struct NoReader;

    impl SiteReader for NoReader {
        fn site(&self) -> &super::super::reader::SiteInfo {
            unimplemented!("the list does not read the site")
        }
        fn pages(&self, _scope: &Scope) -> Vec<PageRef> {
            Vec::new()
        }
        fn page(
            &self,
            _locator: &str,
            _section: Option<&str>,
            _scope: &Scope,
        ) -> Result<super::super::reader::PageText, ToolFailure> {
            Err(ToolFailure::NotFound("none".to_owned()))
        }
        fn search(
            &self,
            _query: &str,
            _limit: usize,
            _scope: &Scope,
        ) -> Result<Vec<Hit>, ToolFailure> {
            Ok(Vec::new())
        }
        fn operation(
            &self,
            _operation: &str,
            _scope: &Scope,
        ) -> Result<super::super::reader::PageText, ToolFailure> {
            Err(ToolFailure::Unavailable("none".to_owned()))
        }
        fn llms_txt(&self, _scope: &Scope) -> Option<String> {
            None
        }
    }

    fn bare() -> Host<'static> {
        Host {
            reader: &NoReader,
            assistant: None,
            issues: None,
        }
    }

    #[test]
    fn every_tool_mcp_01_names_is_advertised() {
        let list = bare().list();
        let names: Vec<&str> = list["tools"]
            .as_array()
            .expect("a tools array")
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect();
        assert_eq!(
            names,
            [
                SEARCH,
                FETCH,
                LIST_PAGES,
                GET_OPENAPI_OPERATION,
                ASK,
                REPORT_ISSUE
            ]
        );
    }

    #[test]
    fn a_tool_that_cannot_run_here_is_advertised_with_its_reason() {
        // MCP-04: "advertised with an `unavailable` reason in the tool list
        // rather than omitted". Hiding them says the site cannot do this,
        // which is a different and wrong statement.
        let list = bare().list();
        for name in [ASK, REPORT_ISSUE] {
            let tool = tool(&list, name);
            assert!(
                tool["_meta"]["liyasa/unavailable"].is_string(),
                "`{name}` carries no reason: {tool}"
            );
            assert!(
                tool["description"]
                    .as_str()
                    .expect("a description")
                    .contains("Not fully available here"),
                "a model reads the description and not `_meta`: {tool}"
            );
        }
        // And a tool that works carries neither.
        assert!(tool(&list, SEARCH)["_meta"].is_null());
    }

    #[test]
    fn report_issue_is_the_only_tool_that_is_not_read_only() {
        let list = bare().list();
        for name in [SEARCH, FETCH, LIST_PAGES, GET_OPENAPI_OPERATION, ASK] {
            assert_eq!(
                tool(&list, name)["annotations"]["readOnlyHint"],
                json!(true),
                "{name}"
            );
        }
        assert_eq!(
            tool(&list, REPORT_ISSUE)["annotations"]["readOnlyHint"],
            json!(false)
        );
    }

    #[test]
    fn every_input_schema_is_closed() {
        // An open schema lets an agent pass a key the server silently drops,
        // so the call looks accepted and the filter never ran.
        for tool in bare().list()["tools"].as_array().expect("tools") {
            assert_eq!(
                tool["inputSchema"]["additionalProperties"],
                json!(false),
                "`{}` accepts keys it does not declare",
                tool["name"]
            );
        }
    }

    #[tokio::test]
    async fn an_unknown_tool_is_a_result_with_is_error_and_not_a_transport_failure() {
        let answer = bare()
            .call("delete_everything", &json!({}), &Scope::anonymous())
            .await;
        assert_eq!(answer["isError"], json!(true));
        let text = answer["content"][0]["text"].as_str().expect("text content");
        assert!(text.contains("delete_everything"), "{text}");
        // Naming what does exist is what turns a refusal into a retry.
        assert!(text.contains(SEARCH), "{text}");
    }

    #[tokio::test]
    async fn a_missing_argument_is_named_rather_than_defaulted() {
        let answer = bare().call(SEARCH, &json!({}), &Scope::anonymous()).await;
        assert_eq!(answer["isError"], json!(true));
        assert!(
            answer["content"][0]["text"]
                .as_str()
                .is_some_and(|text| text.contains("`query`")),
            "{answer}"
        );
    }

    #[tokio::test]
    async fn report_issue_without_a_sink_refuses_and_says_what_would_supply_one() {
        let answer = bare()
            .call(
                REPORT_ISSUE,
                &json!({ "summary": "typo" }),
                &Scope::anonymous(),
            )
            .await;
        assert_eq!(answer["isError"], json!(true));
        assert!(
            answer["content"][0]["text"]
                .as_str()
                .is_some_and(|text| text.contains("liyasa serve")),
            "{answer}"
        );
    }

    #[tokio::test]
    async fn ask_without_a_model_degrades_and_says_so_in_the_result() {
        // Not only in the tool list: the caller that reads a result is often
        // not the one that read the list.
        let answer = bare()
            .call(
                ASK,
                &json!({ "question": "how do I install" }),
                &Scope::anonymous(),
            )
            .await;
        assert_eq!(answer["isError"], json!(false));
        assert_eq!(answer["structuredContent"]["degraded"], json!(true));
        assert!(
            answer["content"][0]["text"]
                .as_str()
                .is_some_and(|text| text.contains("no model is configured")),
            "{answer}"
        );
    }

    #[test]
    fn a_report_over_the_cap_is_refused_by_size_and_not_truncated() {
        // Truncating would file a report whose detail stops mid-sentence and
        // looks like the reporter's own words.
        let long = "x".repeat(MAX_REPORT_BYTES);
        let refusal =
            read_issue(&json!({ "summary": "s", "detail": long })).expect_err("over the cap");
        assert!(matches!(refusal, ToolFailure::BadInput(_)), "{refusal:?}");
        assert!(refusal.to_string().contains(&MAX_REPORT_BYTES.to_string()));
    }

    #[test]
    fn a_report_names_its_route_only_when_one_was_given() {
        let with =
            read_issue(&json!({ "summary": "s", "route": "/guides/install" })).expect("valid");
        assert_eq!(with.route.as_deref(), Some("/guides/install"));
        let without = read_issue(&json!({ "summary": "s", "route": "  " })).expect("valid");
        assert_eq!(without.route, None);
    }
}
