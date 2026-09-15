//! Redirects (PRD §7.9, CM-82).
//!
//! One compiled table answers three questions: what the server should send for
//! a request, what `dist/_redirects` and `vercel.json` say, and what the
//! manifest records. A destination is path-relative unless its host is on
//! `redirects.externalAllow`, and an interpolated parameter never reaches a
//! scheme or a host — that is the rule that keeps the docs domain from becoming
//! an open redirect (`E0109`).

use std::collections::BTreeMap;

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use serde::{Deserialize, Serialize};

/// A rule as written in `liyasa.json`, before validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Input {
    pub source: String,
    pub destination: String,
    /// `301` unless the author asked otherwise; `permanent: false` is `302`.
    pub status: Option<u16>,
}

pub const PERMANENT: u16 = 301;
pub const TEMPORARY: u16 = 302;

/// The `:splat` of a `*` source, named so a destination can interpolate it.
pub const SPLAT: &str = "splat";

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Literal(String),
    Param(String),
    Splat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub source: String,
    pub destination: String,
    pub status: u16,
    pattern: Vec<Segment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub destination: String,
    pub status: u16,
}

/// What the manifest records per rule (§6.6, "output bundle manifest").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub source: String,
    pub destination: String,
    pub status: u16,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Table {
    rules: Vec<Rule>,
}

impl Table {
    /// Validates every rule and keeps the ones that survive, in order: the
    /// first match wins, which is what every static host implements.
    pub fn compile(inputs: &[Input], external_allow: &[String]) -> (Self, Diagnostics) {
        let mut diagnostics = Diagnostics::new();
        let mut rules: Vec<Rule> = Vec::new();

        for input in inputs {
            let source = normalize_source(&input.source);
            if rules.iter().any(|rule| rule.source == source) {
                diagnostics.push(
                    Diagnostic::new(code::E0106, format!("two redirects both claim `{source}`"))
                        .help("the first rule wins; remove or merge the later one"),
                );
                continue;
            }
            let pattern = parse_source(&source);
            let names = declared(&pattern);
            if let Err(diagnostic) = check_destination(&input.destination, &names, external_allow) {
                diagnostics.push(diagnostic);
                continue;
            }
            rules.push(Rule {
                source,
                destination: input.destination.clone(),
                status: input.status.unwrap_or(PERMANENT),
                pattern,
            });
        }
        (Self { rules }, diagnostics)
    }

    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// The destination for a request path, or `None` when nothing matches.
    pub fn resolve(&self, path: &str) -> Option<Hit> {
        let path = normalize_source(path);
        for rule in &self.rules {
            if let Some(bindings) = match_pattern(&rule.pattern, &path) {
                return Some(Hit {
                    destination: interpolate(&rule.destination, &bindings),
                    status: rule.status,
                });
            }
        }
        None
    }

    pub fn manifest_entries(&self) -> Vec<ManifestEntry> {
        self.rules
            .iter()
            .map(|rule| ManifestEntry {
                source: rule.source.clone(),
                destination: rule.destination.clone(),
                status: rule.status,
            })
            .collect()
    }
}

/// `dist/_redirects`, the Netlify format: source, destination, status.
pub fn netlify(table: &Table) -> String {
    let mut out = String::new();
    for rule in table.rules() {
        out.push_str(&format!(
            "{} {} {}\n",
            rule.source, rule.destination, rule.status
        ));
    }
    out
}

/// `dist/vercel.json`. Vercel spells a wildcard as a named catch-all, so `/*`
/// becomes `/:splat*` on the way out and the destination keeps its `:splat`.
pub fn vercel(table: &Table) -> String {
    #[derive(Serialize)]
    struct Redirect {
        source: String,
        destination: String,
        permanent: bool,
    }
    #[derive(Serialize)]
    struct File {
        redirects: Vec<Redirect>,
    }

    let file = File {
        redirects: table
            .rules()
            .iter()
            .map(|rule| Redirect {
                source: rule.source.replace("/*", "/:splat*"),
                destination: rule.destination.clone(),
                permanent: rule.status == PERMANENT,
            })
            .collect(),
    };
    serde_json::to_string_pretty(&file).unwrap_or_else(|_| "{\"redirects\":[]}".to_owned())
}

/// The rule the editor adds when a page moves (CM-82, last sentence).
pub fn for_move(from: &str, to: &str) -> Input {
    Input {
        source: normalize_source(from),
        destination: normalize_source(to),
        status: Some(PERMANENT),
    }
}

fn normalize_source(path: &str) -> String {
    let trimmed = path.trim();
    let with_slash = if trimmed.starts_with('/') || trimmed.contains("://") {
        trimmed.to_owned()
    } else {
        format!("/{trimmed}")
    };
    match with_slash.len() > 1 && with_slash.ends_with('/') {
        true => with_slash.trim_end_matches('/').to_owned(),
        false => with_slash,
    }
}

fn parse_source(source: &str) -> Vec<Segment> {
    source
        .trim_start_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| match segment {
            "*" => Segment::Splat,
            other => match other.strip_prefix(':') {
                Some(name) => Segment::Param(name.to_owned()),
                None => Segment::Literal(other.to_owned()),
            },
        })
        .collect()
}

fn declared(pattern: &[Segment]) -> Vec<String> {
    pattern
        .iter()
        .filter_map(|segment| match segment {
            Segment::Param(name) => Some(name.clone()),
            Segment::Splat => Some(SPLAT.to_owned()),
            Segment::Literal(_) => None,
        })
        .collect()
}

/// A destination is a path unless it carries a scheme or starts `//`, in which
/// case its host must be allow-listed and may not be interpolated.
fn check_destination(
    destination: &str,
    _names: &[String],
    external_allow: &[String],
) -> Result<(), Diagnostic> {
    let Some((scheme, authority)) = split_absolute(destination) else {
        return Ok(());
    };
    let refuse = |reason: &str| {
        Err(
            Diagnostic::new(code::E0109, format!("`{destination}` {reason}")).help(
                "a parameter may appear in the path of a destination, never in its scheme or host",
            ),
        )
    };
    if scheme.is_some_and(|scheme| !is_scheme(scheme)) {
        return refuse("interpolates a parameter into its scheme");
    }
    if authority.starts_with(':') || has_non_port_colon(authority) {
        return refuse("interpolates a parameter into its host");
    }

    let host = authority.rsplit('@').next().unwrap_or(authority);
    let host = host.split(':').next().unwrap_or(host);
    if external_allow.iter().any(|allowed| allowed == host) {
        return Ok(());
    }
    Err(Diagnostic::new(
        code::E0109,
        format!("`{destination}` points at `{host}`, which is not in `redirects.externalAllow`"),
    )
    .help("add the host to `redirects.externalAllow`, or make the destination path-relative"))
}

/// `(scheme, authority)` of an absolute destination, or `None` for a path.
/// A scheme-relative `//host/path` has no scheme and is still absolute.
fn split_absolute(destination: &str) -> Option<(Option<&str>, &str)> {
    if let Some((scheme, rest)) = destination.split_once("://") {
        return Some((Some(scheme), rest.split('/').next().unwrap_or(rest)));
    }
    let rest = destination.strip_prefix("//")?;
    Some((None, rest.split('/').next().unwrap_or(rest)))
}

fn is_scheme(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'))
}

/// A colon in an authority is a port or it is a parameter; a port is digits.
fn has_non_port_colon(authority: &str) -> bool {
    match authority.split_once(':') {
        Some((_, after)) => after.is_empty() || !after.chars().all(|ch| ch.is_ascii_digit()),
        None => false,
    }
}

fn match_pattern(pattern: &[Segment], path: &str) -> Option<BTreeMap<String, String>> {
    let segments: Vec<&str> = path
        .trim_start_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let mut bindings = BTreeMap::new();
    let mut at = 0usize;

    for (index, segment) in pattern.iter().enumerate() {
        match segment {
            Segment::Splat => {
                // A splat is only ever last, and it may match nothing.
                if index + 1 != pattern.len() {
                    return None;
                }
                bindings.insert(SPLAT.to_owned(), segments[at..].join("/"));
                return Some(bindings);
            }
            Segment::Literal(text) => {
                if segments.get(at)? != text {
                    return None;
                }
                at += 1;
            }
            Segment::Param(name) => {
                bindings.insert(name.clone(), (*segments.get(at)?).to_owned());
                at += 1;
            }
        }
    }
    (at == segments.len()).then_some(bindings)
}

fn interpolate(destination: &str, bindings: &BTreeMap<String, String>) -> String {
    let mut out = destination.to_owned();
    // Longest name first, so `:slug` never eats the front of `:slugline`.
    let mut names: Vec<&String> = bindings.keys().collect();
    names.sort_by_key(|name| std::cmp::Reverse(name.len()));
    for name in names {
        if let Some(value) = bindings.get(name) {
            out = out.replace(&format!(":{name}"), value);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compile(rules: &[(&str, &str)]) -> Table {
        let inputs: Vec<Input> = rules
            .iter()
            .map(|(source, destination)| Input {
                source: (*source).to_owned(),
                destination: (*destination).to_owned(),
                status: None,
            })
            .collect();
        Table::compile(&inputs, &[]).0
    }

    #[test]
    fn a_literal_source_matches_exactly() {
        let table = compile(&[("/old", "/new")]);
        assert_eq!(
            table.resolve("/old"),
            Some(Hit {
                destination: "/new".to_owned(),
                status: 301
            })
        );
        assert!(table.resolve("/older").is_none());
    }

    #[test]
    fn a_trailing_slash_does_not_change_the_match() {
        let table = compile(&[("/old/", "/new")]);
        assert!(table.resolve("/old").is_some());
        assert!(table.resolve("/old/").is_some());
    }

    #[test]
    fn a_splat_may_match_nothing() {
        let table = compile(&[("/v1/*", "/v2/:splat")]);
        assert_eq!(
            table.resolve("/v1").map(|hit| hit.destination),
            Some("/v2/".to_owned())
        );
    }

    #[test]
    fn several_parameters_are_bound_by_position() {
        let table = compile(&[("/:section/:page", "/docs/:section-:page")]);
        assert_eq!(
            table.resolve("/guides/install").map(|hit| hit.destination),
            Some("/docs/guides-install".to_owned())
        );
    }

    #[test]
    fn a_longer_name_wins_over_its_prefix() {
        let mut bindings = BTreeMap::new();
        bindings.insert("slug".to_owned(), "a".to_owned());
        bindings.insert("slugline".to_owned(), "b".to_owned());
        assert_eq!(interpolate("/:slugline/:slug", &bindings), "/b/a");
    }

    #[test]
    fn the_first_of_two_matching_rules_answers() {
        let table = compile(&[("/docs/*", "/guides/:splat"), ("/docs/install", "/setup")]);
        assert_eq!(
            table.resolve("/docs/install").map(|hit| hit.destination),
            Some("/guides/install".to_owned())
        );
    }

    #[test]
    fn a_relative_destination_never_needs_the_allow_list() {
        assert!(check_destination("/guides/install", &[], &[]).is_ok());
        assert!(check_destination("/guides/:slug", &["slug".to_owned()], &[]).is_ok());
    }

    #[test]
    fn a_scheme_relative_destination_is_still_a_host() {
        let error = check_destination("//evil.example/docs", &[], &[]).expect_err("refused");
        assert_eq!(error.code.as_str(), "E0109");
        assert!(check_destination("//ok.example/docs", &[], &["ok.example".to_owned()]).is_ok());
    }

    #[test]
    fn a_port_does_not_defeat_the_allow_list() {
        assert!(
            check_destination("https://ok.example:8443/d", &[], &["ok.example".to_owned()]).is_ok()
        );
    }

    #[test]
    fn userinfo_does_not_smuggle_a_host_past_the_allow_list() {
        let error = check_destination(
            "https://ok.example@evil.example/d",
            &[],
            &["ok.example".to_owned()],
        )
        .expect_err("refused");
        assert_eq!(error.code.as_str(), "E0109");
    }
}
