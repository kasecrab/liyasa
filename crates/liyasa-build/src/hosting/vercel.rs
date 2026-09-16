//! `vercel.json`: the same headers and redirects in Vercel's spelling.
//!
//! Vercel reads neither `_headers` nor `_redirects`. Its `source` patterns are
//! path-to-regexp, so a `_headers` glob becomes a capture group and a `.` is
//! escaped. `trailingSlash: true` makes `/guides/install` redirect to the
//! directory the build wrote, which is what every other host does on its own.

use serde::Serialize;

use super::headers::Rules;
use super::redirects::Redirect;

#[derive(Serialize)]
struct Header {
    key: String,
    value: String,
}

#[derive(Serialize)]
struct HeaderRule {
    source: String,
    headers: Vec<Header>,
}

#[derive(Serialize)]
struct RedirectRule {
    source: String,
    destination: String,
    permanent: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct File {
    trailing_slash: bool,
    headers: Vec<HeaderRule>,
    redirects: Vec<RedirectRule>,
}

/// A `_headers` path as a Vercel `source`.
pub fn source(path: &str) -> String {
    let mut out = String::with_capacity(path.len() + 8);
    for ch in path.chars() {
        match ch {
            '*' => out.push_str("(.*)"),
            '.' | '(' | ')' | '+' | '?' | '[' | ']' | '{' | '}' | '\\' | '^' | '$' | '|' => {
                out.push('\\');
                out.push(ch);
            }
            other => out.push(other),
        }
    }
    out
}

pub fn render(rules: &Rules, redirects: &[Redirect]) -> String {
    let file = File {
        trailing_slash: true,
        headers: rules
            .iter()
            .map(|rule| HeaderRule {
                source: source(&rule.path),
                headers: rule
                    .headers
                    .iter()
                    .map(|(key, value)| Header {
                        key: key.clone(),
                        value: value.clone(),
                    })
                    .collect(),
            })
            .collect(),
        redirects: redirects
            .iter()
            .map(|redirect| RedirectRule {
                source: redirect.source.replace("/*", "/:splat*"),
                destination: redirect.destination.clone(),
                permanent: redirect.status == crate::redirects::PERMANENT,
            })
            .collect(),
    };
    let mut text = serde_json::to_string_pretty(&file)
        .unwrap_or_else(|_| "{\"trailingSlash\":true,\"headers\":[],\"redirects\":[]}".to_owned());
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hosting::headers::Rule;

    #[test]
    fn a_glob_becomes_a_capture_and_a_dot_is_escaped() {
        assert_eq!(source("/*"), "/(.*)");
        assert_eq!(source("/docs/_liyasa/*"), "/docs/_liyasa/(.*)");
        assert_eq!(source("/*.md"), "/(.*)\\.md");
        assert_eq!(source("/embed/widget"), "/embed/widget");
    }

    #[test]
    fn the_file_carries_headers_redirects_and_the_slash_policy() {
        let rules = Rules(vec![Rule {
            path: "/*".to_owned(),
            headers: vec![("X-Content-Type-Options".to_owned(), "nosniff".to_owned())],
        }]);
        let redirects = [Redirect {
            source: "/v1/*".to_owned(),
            destination: "/v2/:splat".to_owned(),
            status: 301,
        }];
        let text = render(&rules, &redirects);
        let value: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(value["trailingSlash"], true);
        assert_eq!(value["headers"][0]["source"], "/(.*)");
        assert_eq!(
            value["headers"][0]["headers"][0]["key"],
            "X-Content-Type-Options"
        );
        assert_eq!(value["redirects"][0]["source"], "/v1/:splat*");
        assert_eq!(value["redirects"][0]["permanent"], true);
        assert!(text.ends_with('\n'));
    }
}
