//! What an MCP method means. Knows nothing about HTTP.
//!
//! Transport is [`super::http`] for `liyasa serve` and [`super::stdio`] for
//! `liyasa mcp --dist` (MCP-04); both reduce a message to a
//! [`jsonrpc::Request`] and call [`dispatch`]. That is what makes the two
//! hosts the same server rather than two implementations of one document.

use serde_json::{Value, json};

use super::jsonrpc::{self, Request, Response};
use super::reader::Scope;
use super::tools::Host;
use super::{prompts, resources};

/// The revision this server implements.
pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// Revisions it will also speak, newest first.
///
/// A client that asks for one of these gets it back in `initialize` and is
/// served. A client that asks for anything else is answered with
/// [`PROTOCOL_VERSION`] and decides for itself whether to continue, which is
/// what the specification asks for — refusing the connection would strand a
/// client that is merely newer than this build.
pub const SUPPORTED: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

/// MCP's own code for a resource that does not exist. Outside the JSON-RPC
/// reserved range on purpose, which is why it is not in `jsonrpc`.
pub const RESOURCE_NOT_FOUND: i32 = -32002;

/// One connection's view of the server.
pub struct Server<'a> {
    pub host: Host<'a>,
    pub scope: Scope,
}

impl Server<'_> {
    fn site_name(&self) -> String {
        self.host.reader.site().name.clone()
    }

    /// What `initialize` tells the client about itself.
    fn server_info(&self) -> Value {
        let site = self.host.reader.site();
        let mut info = json!({
            "name": format!("liyasa:{}", site.name),
            "title": site.name,
            "version": env!("CARGO_PKG_VERSION"),
        });
        if let Some(origin) = &site.origin
            && let Some(object) = info.as_object_mut()
        {
            object.insert("websiteUrl".to_owned(), json!(origin));
        }
        info
    }

    /// The one paragraph a client shows its model before anything else.
    ///
    /// It says what the corpus is and that it is authoritative for this
    /// product, because the failure this server exists to prevent is a model
    /// answering from recollection when the current answer was one tool call
    /// away.
    fn instructions(&self) -> String {
        let site = self.host.reader.site();
        let mut text = format!(
            "This server serves the documentation for {}. It is the current text of that \
             documentation, generated from the same build the site serves, so prefer it over \
             anything you recall about this product.\n\n\
             Start with `search`. Use `fetch` to read a page or one of its sections, \
             `list_pages` when you need the shape of the site rather than an answer, and \
             `get_openapi_operation` for an API operation's real parameters. `report_issue` \
             is the only tool that writes, and it files feedback for the maintainers.",
            site.name
        );
        if let Some(description) = &site.description {
            text = format!("{text}\n\nThe site describes itself as: {description}");
        }
        text
    }
}

/// Answers one request. `None` for a notification, which takes no response.
pub async fn dispatch(server: &Server<'_>, request: &Request) -> Option<Response> {
    let id = request.id.clone();
    let params = request.params();

    let response = match request.method.as_str() {
        "initialize" => Response::result(id, initialize(server, &params)),

        // The client telling us it is ready, and the cancellation and
        // progress notifications. Nothing here keeps per-request state that
        // a cancellation could release, so acknowledging is the whole of it.
        method if method.starts_with("notifications/") => return None,

        "ping" => Response::result(id, json!({})),

        "tools/list" => Response::result(id, server.host.list()),
        "tools/call" => {
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Some(Response::error(
                    id,
                    jsonrpc::INVALID_PARAMS,
                    "`tools/call` needs a `name`",
                ));
            };
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            Response::result(id, server.host.call(name, &arguments, &server.scope).await)
        }

        "resources/list" => Response::result(
            id,
            resources::list(
                server.host.reader,
                &server.scope,
                params.get("cursor").and_then(Value::as_str),
            ),
        ),
        // Every resource here has a fixed URI, so there is no template to
        // expand. Answering with an empty list rather than "method not found"
        // is the difference between a client moving on and a client reporting
        // this server as broken.
        "resources/templates/list" => Response::result(id, json!({ "resourceTemplates": [] })),
        "resources/read" => {
            let Some(uri) = params.get("uri").and_then(Value::as_str) else {
                return Some(Response::error(
                    id,
                    jsonrpc::INVALID_PARAMS,
                    "`resources/read` needs a `uri`",
                ));
            };
            match resources::read(server.host.reader, &server.scope, uri) {
                Some(contents) => Response::result(id, contents),
                None => Response::error(id, RESOURCE_NOT_FOUND, format!("no resource at `{uri}`")),
            }
        }

        "prompts/list" => Response::result(id, prompts::list()),
        "prompts/get" => {
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Some(Response::error(
                    id,
                    jsonrpc::INVALID_PARAMS,
                    "`prompts/get` needs a `name`",
                ));
            };
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            match prompts::get(name, &arguments, &server.site_name()) {
                Some(prompt) => Response::result(id, prompt),
                None => Response::error(
                    id,
                    jsonrpc::INVALID_PARAMS,
                    format!(
                        "no prompt `{name}`, or a required argument was missing. \
                         `prompts/list` names them and what each one needs."
                    ),
                ),
            }
        }

        other => Response::error(
            id,
            jsonrpc::METHOD_NOT_FOUND,
            format!("`{other}` is not a method this server implements"),
        ),
    };
    Some(response)
}

fn initialize(server: &Server<'_>, params: &Value) -> Value {
    let asked = params.get("protocolVersion").and_then(Value::as_str);
    let version = asked
        .filter(|version| SUPPORTED.contains(version))
        .unwrap_or(PROTOCOL_VERSION);
    json!({
        "protocolVersion": version,
        "capabilities": {
            // `listChanged` is false and honest: the tool set is fixed at
            // compile time and the resource list changes only when the site
            // is rebuilt, which drops the connection anyway. Advertising a
            // notification this server never sends would have clients waiting
            // for one.
            "tools": { "listChanged": false },
            "resources": { "subscribe": false, "listChanged": false },
            "prompts": { "listChanged": false }
        },
        "serverInfo": server.server_info(),
        "instructions": server.instructions()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::jsonrpc::{Id, Incoming, parse};
    use crate::mcp::reader::{Hit, PageRef, PageText, SiteInfo, SiteReader, ToolFailure};

    struct Site;

    impl SiteReader for Site {
        fn site(&self) -> &SiteInfo {
            static INFO: std::sync::OnceLock<SiteInfo> = std::sync::OnceLock::new();
            INFO.get_or_init(|| SiteInfo {
                name: "Acme docs".to_owned(),
                description: Some("How Acme works".to_owned()),
                origin: Some("https://docs.acme.com".to_owned()),
            })
        }
        fn pages(&self, _scope: &Scope) -> Vec<PageRef> {
            vec![PageRef {
                route: "/".to_owned(),
                title: "Home".to_owned(),
                depth: 0,
            }]
        }
        /// Only the one page this site has. A fake that answered for every
        /// locator would make `resources/read` succeed for a URI on another
        /// host, and the test for that refusal would pass on the fake's
        /// generosity rather than on the code.
        fn page(
            &self,
            locator: &str,
            _section: Option<&str>,
            _scope: &Scope,
        ) -> Result<PageText, ToolFailure> {
            const MINE: &[&str] = &[
                "/",
                "/index.md",
                "https://docs.acme.com/",
                "https://docs.acme.com/index.md",
            ];
            if !MINE.contains(&locator) {
                return Err(ToolFailure::NotFound(locator.to_owned()));
            }
            Ok(PageText {
                route: "/".to_owned(),
                title: "Home".to_owned(),
                anchor: String::new(),
                markdown: "# Home\n".to_owned(),
            })
        }
        fn search(&self, _q: &str, _n: usize, _s: &Scope) -> Result<Vec<Hit>, ToolFailure> {
            Ok(Vec::new())
        }
        fn operation(&self, _o: &str, _s: &Scope) -> Result<PageText, ToolFailure> {
            Err(ToolFailure::Unavailable("no spec".to_owned()))
        }
        fn llms_txt(&self, _scope: &Scope) -> Option<String> {
            Some("# Acme docs\n".to_owned())
        }
    }

    fn server() -> Server<'static> {
        Server {
            host: Host {
                reader: &Site,
                assistant: None,
                issues: None,
            },
            scope: Scope::anonymous(),
        }
    }

    async fn call(method: &str, params: Value) -> Response {
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
        let Incoming::Call(request) = parse(&body.to_string()) else {
            panic!("`{method}` did not parse as a call");
        };
        dispatch(&server(), &request)
            .await
            .unwrap_or_else(|| panic!("`{method}` answered nothing"))
    }

    async fn result(method: &str, params: Value) -> Value {
        let response = call(method, params).await;
        assert!(response.error.is_none(), "{:?}", response.error);
        response.result.expect("a result")
    }

    #[tokio::test]
    async fn initialize_echoes_a_version_it_speaks_and_substitutes_one_it_does_not() {
        let mine = result("initialize", json!({ "protocolVersion": "2024-11-05" })).await;
        assert_eq!(mine["protocolVersion"], json!("2024-11-05"));

        let theirs = result("initialize", json!({ "protocolVersion": "2099-01-01" })).await;
        assert_eq!(theirs["protocolVersion"], json!(PROTOCOL_VERSION));
    }

    #[tokio::test]
    async fn initialize_declares_exactly_the_capabilities_this_server_has() {
        let answer = result("initialize", json!({})).await;
        for capability in ["tools", "resources", "prompts"] {
            assert!(
                answer["capabilities"][capability].is_object(),
                "{capability} is missing: {answer}"
            );
        }
        // Not advertised, because none is implemented. A client that saw
        // `completions` here would call it and be refused.
        assert!(answer["capabilities"]["completions"].is_null());
        assert!(answer["capabilities"]["logging"].is_null());
        // And nothing claims a notification this server never sends.
        assert_eq!(answer["capabilities"]["tools"]["listChanged"], json!(false));
        assert_eq!(
            answer["capabilities"]["resources"]["subscribe"],
            json!(false)
        );
    }

    #[tokio::test]
    async fn the_instructions_name_the_site_and_the_tools() {
        let answer = result("initialize", json!({})).await;
        let text = answer["instructions"].as_str().expect("instructions");
        assert!(text.contains("Acme docs"), "{text}");
        assert!(text.contains("How Acme works"), "{text}");
        assert!(text.contains("`search`"), "{text}");
    }

    #[tokio::test]
    async fn a_notification_is_answered_with_nothing_at_all() {
        // Sending a response to a notification is a protocol violation that
        // some clients treat as a response to a DIFFERENT request.
        let body = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        let Incoming::Notify(request) = parse(&body.to_string()) else {
            panic!("that is a notification");
        };
        assert!(dispatch(&server(), &request).await.is_none());
    }

    #[tokio::test]
    async fn an_unknown_method_is_refused_by_name() {
        let response = call("tools/delete", json!({})).await;
        let error = response.error.expect("an error");
        assert_eq!(error.code, jsonrpc::METHOD_NOT_FOUND);
        assert!(error.message.contains("tools/delete"), "{}", error.message);
    }

    #[tokio::test]
    async fn a_tool_call_with_no_name_is_a_protocol_error_and_not_a_tool_error() {
        // The distinction matters: a tool error is the model's to recover
        // from, a malformed call is the client's.
        let response = call("tools/call", json!({ "arguments": {} })).await;
        assert_eq!(
            response.error.expect("an error").code,
            jsonrpc::INVALID_PARAMS
        );
    }

    #[tokio::test]
    async fn a_missing_resource_is_the_protocol_s_own_code() {
        let response = call("resources/read", json!({ "uri": "https://nope.example/x" })).await;
        assert_eq!(response.error.expect("an error").code, RESOURCE_NOT_FOUND);
    }

    #[tokio::test]
    async fn templates_list_is_an_empty_list_rather_than_method_not_found() {
        let answer = result("resources/templates/list", json!({})).await;
        assert!(
            answer["resourceTemplates"]
                .as_array()
                .expect("an array")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn every_capability_declared_answers_its_own_list_method() {
        // The pairing is the point: a capability with no working method is
        // the shape this project keeps shipping.
        assert!(result("tools/list", json!({})).await["tools"].is_array());
        assert!(result("resources/list", json!({})).await["resources"].is_array());
        assert!(result("prompts/list", json!({})).await["prompts"].is_array());
    }

    #[tokio::test]
    async fn a_response_carries_the_id_it_was_asked_with() {
        let body = json!({ "jsonrpc": "2.0", "id": "abc", "method": "ping" });
        let Incoming::Call(request) = parse(&body.to_string()) else {
            panic!("a call");
        };
        let response = dispatch(&server(), &request).await.expect("a response");
        assert_eq!(response.id, Some(Id::String("abc".to_owned())));
    }
}
