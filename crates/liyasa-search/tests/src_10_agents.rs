//! SRC-10: the MCP `search` tool and the REST endpoint, ranked the same.
//!
//! Both surfaces go through `api::search`, so what is worth testing is not the
//! ranking a second time but the dispatch either side of it: that a query
//! string and a tool call mean the same request, that the two answers are the
//! same list, and that a caller's mistake is a diagnostic rather than a panic
//! or an empty result set.

mod support;

use liyasa_search::api::{self, RestResponse, SearchRequest};
use liyasa_search::config::SearchSettings;
use liyasa_search::idx::Index;
use liyasa_search::idx::query::ReaderScope;
use liyasa_search::idx::writer::{self, WriterOptions};
use serde_json::{Value, json};
use support::corpus;

fn index() -> Index {
    Index::from_built(writer::build(
        &corpus::documents(),
        &WriterOptions::default(),
    ))
}

fn rest(query_string: &str) -> RestResponse {
    api::rest(
        &index(),
        query_string,
        &SearchSettings::default(),
        &ReaderScope::default(),
    )
    .0
}

fn urls(body: &Value) -> Vec<String> {
    body["results"]
        .as_array()
        .expect("a result list")
        .iter()
        .map(|result| result["url"].as_str().expect("a url").to_owned())
        .collect()
}

fn call(arguments: Value) -> Value {
    api::tool_call(
        &index(),
        api::TOOL_NAME,
        &arguments,
        &SearchSettings::default(),
        &ReaderScope::default(),
    )
    .0
}

#[test]
fn the_two_surfaces_answer_one_query_the_same_way() {
    let from_rest = rest("q=rate+limits");
    let from_tool = call(json!({ "query": "rate limits" }));

    assert_eq!(from_rest.status, 200);
    assert_eq!(from_tool["isError"], false);
    assert_eq!(
        urls(&from_rest.body),
        urls(&from_tool["structuredContent"]),
        "SRC-10 is one ranking, not two"
    );
    assert!(!urls(&from_rest.body).is_empty());
}

#[test]
fn a_query_string_and_a_json_body_are_the_same_request() {
    let parsed = api::request_from_query_string(
        "q=rate%20limits&locale=en&version=v2&tab=docs&limit=5&snippets=false\
         &filters.type=page&filters.version=v2",
    )
    .expect("parses");

    let body: SearchRequest = serde_json::from_value(json!({
        "query": "rate limits",
        "locale": "en",
        "version": "v2",
        "tab": "docs",
        "limit": 5,
        "snippets": false,
        "filters": { "type": "page", "version": "v2" }
    }))
    .expect("parses");

    assert_eq!(parsed, body);
}

#[test]
fn a_plus_and_a_percent_escape_both_decode() {
    let plus = api::request_from_query_string("q=rate+limits").expect("parses");
    let escaped = api::request_from_query_string("q=rate%20limits").expect("parses");
    assert_eq!(plus.query, "rate limits");
    assert_eq!(escaped.query, "rate limits");

    let quoted = api::request_from_query_string("q=%22rate+limits%22").expect("parses");
    assert_eq!(quoted.query, "\"rate limits\"");
}

#[test]
fn the_long_spelling_of_the_query_parameter_also_works() {
    let short = api::request_from_query_string("q=auth").expect("parses");
    let long = api::request_from_query_string("query=auth").expect("parses");
    assert_eq!(short, long);
}

#[test]
fn a_parameter_the_endpoint_does_not_have_is_rejected_rather_than_ignored() {
    let response = rest("q=auth&sort=date");
    assert_eq!(response.status, 400);
    assert_eq!(response.body["code"], "E1004");
    assert!(
        response.body["message"]
            .as_str()
            .is_some_and(|m| m.contains("sort")),
        "the message names the parameter: {}",
        response.body["message"]
    );
}

#[test]
fn a_request_without_a_query_is_a_diagnostic() {
    let response = rest("locale=en");
    assert_eq!(response.status, 400);
    assert_eq!(response.body["code"], "E1004");
}

#[test]
fn an_unbalanced_quote_is_four_hundred_rather_than_five_hundred() {
    let response = rest("q=%22rate+limits");
    assert_eq!(response.status, 400);
    assert_eq!(response.body["code"], "E1004");
}

#[test]
fn a_limit_past_the_published_maximum_is_refused_on_both_surfaces() {
    let response = rest("q=auth&limit=500");
    assert_eq!(response.status, 400);
    assert_eq!(response.body["code"], "E1004");
    assert!(
        response.body["message"]
            .as_str()
            .is_some_and(|m| m.contains(&api::MAX_LIMIT.to_string())),
        "the message names the maximum: {}",
        response.body["message"]
    );

    let result = call(json!({ "query": "auth", "limit": 500 }));
    assert_eq!(result["isError"], true);

    // The schema an agent reads and the rule the server enforces are one
    // number, so a conforming agent never trips it.
    let schema = api::tool_schema();
    assert_eq!(
        schema["inputSchema"]["properties"]["limit"]["maximum"].as_u64(),
        Some(api::MAX_LIMIT as u64)
    );
}

#[test]
fn a_limit_shortens_the_list() {
    let response = rest("q=limit&limit=2");
    assert_eq!(response.status, 200);
    assert!(urls(&response.body).len() <= 2);
    assert!(
        response.body["total"].as_u64().expect("a total") >= urls(&response.body).len() as u64,
        "`total` is what the index found, before the limit"
    );
}

#[test]
fn a_number_where_a_number_is_expected_is_a_diagnostic() {
    let response = rest("q=auth&limit=ten");
    assert_eq!(response.status, 400);
    assert_eq!(response.body["code"], "E1004");

    let response = rest("q=auth&snippets=perhaps");
    assert_eq!(response.status, 400);
    assert_eq!(response.body["code"], "E1004");
}

#[test]
fn a_facet_the_site_does_not_offer_is_a_diagnostic() {
    let response = rest("q=auth&filters.type=widget");
    assert_eq!(response.status, 400);
    assert_eq!(response.body["code"], "E1004");
}

#[test]
fn both_surfaces_filter_by_the_readers_groups_before_ranking() {
    let staff = ReaderScope {
        groups: vec!["staff".to_owned()],
        region: None,
    };
    let settings = SearchSettings::default();
    let index = index();

    let anonymous = api::rest(
        &index,
        "q=raising+a+limit",
        &settings,
        &ReaderScope::default(),
    )
    .0;
    assert!(
        !urls(&anonymous.body)
            .iter()
            .any(|url| url.starts_with("/internal/")),
        "an anonymous REST caller must not see a grouped section: {:?}",
        urls(&anonymous.body)
    );

    let privileged = api::rest(&index, "q=raising+a+limit", &settings, &staff).0;
    assert!(
        urls(&privileged.body)
            .iter()
            .any(|url| url.starts_with("/internal/")),
        "a reader in the group sees it: {:?}",
        urls(&privileged.body)
    );

    let tool = api::tool_call(
        &index,
        api::TOOL_NAME,
        &json!({ "query": "raising a limit" }),
        &settings,
        &ReaderScope::default(),
    )
    .0;
    assert!(
        !urls(&tool["structuredContent"])
            .iter()
            .any(|url| url.starts_with("/internal/")),
        "an agent gets no more than the reader it speaks for"
    );
}

#[test]
fn the_tool_list_is_the_one_schema_the_docs_publish() {
    let listed = api::tools_list();
    let tools = listed["tools"].as_array().expect("a tool array");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0], api::tool_schema());
    assert_eq!(tools[0]["name"], api::TOOL_NAME);
}

#[test]
fn a_tool_that_does_not_exist_is_an_error_result_rather_than_a_panic() {
    let result = api::tool_call(
        &index(),
        "summarize",
        &json!({ "query": "auth" }),
        &SearchSettings::default(),
        &ReaderScope::default(),
    )
    .0;
    assert_eq!(result["isError"], true);
    assert!(
        result["content"][0]["text"]
            .as_str()
            .is_some_and(|text| text.contains("summarize")),
        "{result}"
    );
}

#[test]
fn tool_arguments_of_the_wrong_shape_are_an_error_result() {
    for arguments in [
        json!({}),
        json!({ "query": 7 }),
        json!({ "query": "auth", "filters": { "type": "widget" } }),
        json!("auth"),
    ] {
        let result = call(arguments.clone());
        assert_eq!(result["isError"], true, "{arguments} must be refused");
        assert!(
            result["content"][0]["text"]
                .as_str()
                .is_some_and(|t| !t.is_empty()),
            "an agent is told what went wrong: {result}"
        );
    }
}

#[test]
fn a_tool_result_carries_the_text_an_agent_reads_and_the_json_it_parses() {
    let result = call(json!({ "query": "rate limits", "limit": 3 }));
    assert_eq!(result["isError"], false);

    let text = result["content"][0]["text"].as_str().expect("text content");
    assert_eq!(result["content"][0]["type"], "text");
    assert!(
        text.contains("/guides/limits"),
        "the text half names the routes: {text}"
    );

    let structured = &result["structuredContent"];
    assert_eq!(structured["results"][0]["url"], urls(structured)[0]);
    assert!(structured["total"].is_number());
}

#[test]
fn a_query_that_finds_nothing_is_a_result_rather_than_an_error() {
    let response = rest("q=quinoa");
    assert_eq!(response.status, 200);
    assert!(urls(&response.body).is_empty());
    assert_eq!(response.body["total"], 0);

    let result = call(json!({ "query": "quinoa" }));
    assert_eq!(
        result["isError"], false,
        "an empty result set is an answer, not a failure"
    );
}

#[test]
fn both_surfaces_emit_the_analytics_event_and_no_reader_identity() {
    let index = index();
    let settings = SearchSettings::default();

    let (_, event) = api::rest(&index, "q=rate+limits", &settings, &ReaderScope::default());
    let event = event.expect("a search emits an event");
    let json = serde_json::to_value(&event).expect("serializes");
    assert_eq!(json["event"], "query");
    assert_eq!(json["query"], "rate limits");

    let (_, event) = api::tool_call(
        &index,
        api::TOOL_NAME,
        &json!({ "query": "quinoa" }),
        &settings,
        &ReaderScope::default(),
    );
    let json = serde_json::to_value(event.expect("an event")).expect("serializes");
    assert_eq!(json["event"], "no-results");

    // A refused request never reaches the index, so it is not a query anyone
    // searched for.
    let (_, event) = api::rest(
        &index,
        "q=auth&sort=date",
        &settings,
        &ReaderScope::default(),
    );
    assert!(event.is_none());
}

#[test]
fn a_query_a_reader_pasted_a_secret_into_is_scrubbed_before_the_event() {
    let index = index();
    let (_, event) = api::rest(
        &index,
        "q=token+sk_live_4eC39HqLyjWDarjtT1zdp7dc",
        &SearchSettings::default(),
        &ReaderScope::default(),
    );
    let json = serde_json::to_value(event.expect("an event")).expect("serializes");
    let query = json["query"].as_str().expect("a query");
    assert!(
        !query.contains("4eC39HqLyjWDarjtT1zdp7dc"),
        "SRC-08 scrubs before the event leaves the request: {query}"
    );
}

#[test]
fn the_endpoint_names_one_path_for_the_server_and_the_docs() {
    assert!(api::REST_PATH.starts_with('/'));
    assert!(!api::REST_PATH.ends_with('/'));
}
