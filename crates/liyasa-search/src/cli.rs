//! `liyasa search "<query>"` (SRC-09).
//!
//! The command exists to test ranking locally and in CI, so its exit code is
//! the assertion: zero when the query found something, one when it did not.
//! `liyasa-cli` wraps [`run`]; `src/bin/search.rs` runs it directly while that
//! crate does not exist.

use std::fmt::Write as _;

use crate::api::{self, SearchRequest};
use crate::config::SearchSettings;
use crate::error::SearchError;
use crate::idx::Index;
use crate::idx::query::ReaderScope;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    pub locale: Option<String>,
    pub version: Option<String>,
    pub tab: Option<String>,
    pub limit: Option<usize>,
    /// Machine-readable output, for CI and for an agent.
    pub json: bool,
    /// The result a CI check requires: `--expect /guides/limits`.
    pub expect: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub output: String,
    /// 0 when the query found what it was asked to find.
    pub code: i32,
}

pub fn run(index: &Index, query: &str, options: &Options) -> Result<Outcome, SearchError> {
    let request = SearchRequest {
        query: query.to_owned(),
        locale: options.locale.clone(),
        version: options.version.clone(),
        tab: options.tab.clone(),
        limit: options.limit,
        snippets: Some(!options.json),
        ..SearchRequest::default()
    };
    let settings = SearchSettings::default();
    let (response, _event) = api::search(index, &request, &settings, &ReaderScope::default())?;

    if options.json {
        let output = serde_json::to_string_pretty(&response)
            .map_err(|e| SearchError::Query(e.to_string()))?;
        return Ok(Outcome {
            code: exit_code(
                &response
                    .results
                    .iter()
                    .map(|r| r.url.clone())
                    .collect::<Vec<_>>(),
                options,
            ),
            output,
        });
    }

    let mut output = String::new();
    if response.results.is_empty() {
        let _ = writeln!(output, "no results for `{query}`");
    }
    for (rank, result) in response.results.iter().enumerate() {
        let _ = writeln!(
            output,
            "{:>3}. {}  {}",
            rank + 1,
            result.url,
            heading(result)
        );
        if let Some(snippet) = &result.snippet {
            let _ = writeln!(output, "     {}", snippet.replace('\n', " "));
        }
    }
    let urls: Vec<String> = response.results.iter().map(|r| r.url.clone()).collect();
    if let Some(expected) = &options.expect
        && !urls.iter().any(|url| url == expected)
    {
        let _ = writeln!(output, "expected `{expected}`, which is not in the results");
    }
    Ok(Outcome {
        code: exit_code(&urls, options),
        output,
    })
}

fn heading(result: &crate::api::SearchResult) -> String {
    if result.section == result.title || result.section.is_empty() {
        result.title.clone()
    } else {
        format!("{} › {}", result.title, result.section)
    }
}

fn exit_code(urls: &[String], options: &Options) -> i32 {
    match &options.expect {
        Some(expected) => i32::from(!urls.iter().any(|url| url == expected)),
        None => i32::from(urls.is_empty()),
    }
}

#[cfg(test)]
mod tests {
    use liyasa_core::ids::{Locale, Route};

    use super::*;
    use crate::doc::{DocKind, SectionDocument};
    use crate::idx::writer::{self, WriterOptions};

    fn index() -> Index {
        let documents = vec![SectionDocument {
            route: Route::new("/guides/limits"),
            anchor: String::new(),
            title: "Rate limits".to_owned(),
            section: "Rate limits".to_owned(),
            breadcrumb: vec!["Guides".to_owned()],
            body: "Every API key has a rate limit.".to_owned(),
            code: String::new(),
            keywords: Vec::new(),
            tab: None,
            version: None,
            locale: Locale::new("en"),
            kind: DocKind::Page,
            boost: 1.0,
            groups: Vec::new(),
            regions: Vec::new(),
            updated: None,
        }];
        Index::from_built(writer::build(&documents, &WriterOptions::default()))
    }

    #[test]
    fn a_query_that_finds_a_page_exits_zero() {
        let outcome = run(&index(), "rate limits", &Options::default()).expect("runs");
        assert_eq!(outcome.code, 0);
        assert!(
            outcome.output.contains("/guides/limits"),
            "{}",
            outcome.output
        );
    }

    #[test]
    fn a_query_that_finds_nothing_exits_one() {
        let outcome = run(&index(), "quinoa", &Options::default()).expect("runs");
        assert_eq!(outcome.code, 1);
        assert!(outcome.output.contains("no results"), "{}", outcome.output);
    }

    #[test]
    fn expect_turns_a_query_into_a_ci_assertion() {
        let options = Options {
            expect: Some("/guides/limits".to_owned()),
            ..Options::default()
        };
        assert_eq!(
            run(&index(), "rate limits", &options).expect("runs").code,
            0
        );

        let options = Options {
            expect: Some("/guides/auth".to_owned()),
            ..Options::default()
        };
        let outcome = run(&index(), "rate limits", &options).expect("runs");
        assert_eq!(outcome.code, 1);
        assert!(
            outcome.output.contains("expected `/guides/auth`"),
            "{}",
            outcome.output
        );
    }

    #[test]
    fn json_output_is_the_api_response() {
        let options = Options {
            json: true,
            ..Options::default()
        };
        let outcome = run(&index(), "rate limits", &options).expect("runs");
        let parsed: serde_json::Value = serde_json::from_str(&outcome.output).expect("valid JSON");
        assert_eq!(parsed["total"], 1);
        assert_eq!(parsed["results"][0]["url"], "/guides/limits");
    }

    #[test]
    fn a_malformed_query_is_a_diagnostic_rather_than_an_exit_code() {
        let error = run(&index(), "\"unbalanced", &Options::default()).expect_err("must fail");
        assert_eq!(error.diagnostic().code.as_str(), "E1004");
    }
}
