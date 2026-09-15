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

pub const USAGE: &str = "usage: search <search-index directory> <query> \
[--locale <tag>] [--version <name>] [--tab <name>] [--limit <n>] [--json] \
[--expect <url>]";

/// One invocation, parsed. Here rather than in `main` so the argument rules
/// are testable: a CI check that silently ignored `--expect` would pass while
/// asserting nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub directory: String,
    pub query: String,
    pub options: Options,
}

/// Reads the arguments after the program name. The error is the message to
/// print; every unknown flag and every missing value is one, because a
/// mistyped assertion that runs anyway is worse than no assertion.
pub fn parse_arguments<I>(arguments: I) -> Result<Invocation, String>
where
    I: IntoIterator<Item = String>,
{
    let arguments: Vec<String> = arguments.into_iter().collect();
    let mut positional = arguments.iter().filter(|a| !a.starts_with("--"));
    let (Some(directory), Some(query)) = (positional.next(), positional.next()) else {
        return Err(USAGE.to_owned());
    };
    let (directory, query) = (directory.clone(), query.clone());

    let mut options = Options::default();
    let mut at = 0;
    let mut positional_seen = 0;
    while at < arguments.len() {
        let argument = &arguments[at];
        let value = |name: &str, at: &mut usize| -> Result<String, String> {
            *at += 1;
            arguments
                .get(*at)
                .filter(|value| !value.starts_with("--"))
                .cloned()
                .ok_or_else(|| format!("`--{name}` needs a value"))
        };
        match argument.as_str() {
            "--json" => options.json = true,
            "--expect" => options.expect = Some(value("expect", &mut at)?),
            "--locale" => options.locale = Some(value("locale", &mut at)?),
            "--version" => options.version = Some(value("version", &mut at)?),
            "--tab" => options.tab = Some(value("tab", &mut at)?),
            "--limit" => {
                let text = value("limit", &mut at)?;
                options.limit = Some(
                    text.parse()
                        .map_err(|_| format!("`--limit` is `{text}`, which is not a number"))?,
                );
            }
            other if other.starts_with("--") => return Err(format!("unknown option `{other}`")),
            _ => positional_seen += 1,
        }
        at += 1;
    }
    if positional_seen > 2 {
        return Err(USAGE.to_owned());
    }

    Ok(Invocation {
        directory,
        query,
        options,
    })
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

    fn arguments(text: &str) -> Result<Invocation, String> {
        parse_arguments(text.split_whitespace().map(str::to_owned))
    }

    #[test]
    fn the_directory_and_the_query_are_the_two_positional_arguments() {
        let parsed = arguments("./dist/search-index limits").expect("parses");
        assert_eq!(parsed.directory, "./dist/search-index");
        assert_eq!(parsed.query, "limits");
        assert_eq!(parsed.options, Options::default());
    }

    #[test]
    fn every_filter_the_options_carry_can_be_set_from_the_command_line() {
        let parsed = arguments(
            "./index limits --locale de --version v2 --tab api --limit 5 --json              --expect /guides/limits",
        )
        .expect("parses");
        assert_eq!(
            parsed.options,
            Options {
                locale: Some("de".to_owned()),
                version: Some("v2".to_owned()),
                tab: Some("api".to_owned()),
                limit: Some(5),
                json: true,
                expect: Some("/guides/limits".to_owned()),
            }
        );
    }

    #[test]
    fn a_limit_that_is_not_a_number_is_refused_rather_than_ignored() {
        let error = arguments("./index limits --limit ten").expect_err("must fail");
        assert!(error.contains("ten"), "{error}");
    }

    #[test]
    fn a_flag_without_its_value_is_refused() {
        for line in [
            "./index limits --expect",
            "./index limits --limit",
            "./index limits --expect --json",
        ] {
            assert!(arguments(line).is_err(), "`{line}` must not parse");
        }
    }

    #[test]
    fn an_unknown_flag_is_refused() {
        let error = arguments("./index limits --sort date").expect_err("must fail");
        assert!(error.contains("--sort"), "{error}");
    }

    #[test]
    fn too_few_arguments_print_the_usage() {
        assert_eq!(arguments("./index"), Err(USAGE.to_owned()));
        assert_eq!(arguments("--json"), Err(USAGE.to_owned()));
    }

    #[test]
    fn a_malformed_query_is_a_diagnostic_rather_than_an_exit_code() {
        let error = run(&index(), "\"unbalanced", &Options::default()).expect_err("must fail");
        assert_eq!(error.diagnostic().code.as_str(), "E1004");
    }
}
