//! `DOCOWNERS`: who is responsible for reviewing a page (VER-77).
//!
//! `liyasa-config` already treats the file as part of the trust plane
//! (`trust::FILES`), which settles where it may be read from and who may change
//! it. Nothing parsed it. VER-77 names the file and not its format, so RFC 2064
//! settles the format, and it is CODEOWNERS' — a pattern, whitespace, the
//! owners — with route globs that follow §8.4's rather than git's pathspecs,
//! because the thing being matched is a route.

use liyasa_core::ids::Route;

/// One rule: a route pattern and the owners it assigns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub pattern: String,
    /// Empty for a rule that deliberately leaves a directory unowned.
    pub owners: Vec<String>,
}

/// The parsed file, in the order it was written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Docowners {
    rules: Vec<Rule>,
}

impl Docowners {
    /// Never fails. A line that is not a rule is not a rule, and an
    /// unparseable `DOCOWNERS` that refused to load would leave every page
    /// unowned, which is worse than ignoring one line.
    pub fn parse(text: &str) -> Self {
        let rules = text
            .lines()
            .filter_map(|line| {
                let line = line.split('#').next().unwrap_or_default().trim();
                let mut parts = line.split_whitespace();
                let pattern = parts.next()?;
                Some(Rule {
                    pattern: pattern.to_owned(),
                    owners: parts.map(ToOwned::to_owned).collect(),
                })
            })
            .collect();
        Self { rules }
    }

    /// The owners of a route, or `None` when no rule matches it.
    ///
    /// The **last** matching rule wins, which is CODEOWNERS' rule and the one
    /// an author already expects: the general rule goes at the top and the
    /// specific override below it. A rule that matches and names nobody
    /// answers `Some(&[])` — deliberately unowned is an answer, and it is not
    /// the same as unmatched.
    pub fn owners_of(&self, route: &Route) -> Option<&[String]> {
        self.rules
            .iter()
            .rev()
            .find(|rule| glob(&rule.pattern, route.as_str()))
            .map(|rule| rule.owners.as_slice())
    }

    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

/// Segment-wise glob: `*` spans one segment, `**` spans any number, and a
/// trailing `/` means the directory and everything under it.
///
/// The same two wildcards as §8.4's navigation globs, so an author learns one
/// syntax. It is not `liyasa-config`'s implementation because `liyasa-verify`
/// does not depend on that crate and the function there is private; the rules
/// are deliberately identical.
fn glob(pattern: &str, route: &str) -> bool {
    let owned = if pattern.ends_with('/') {
        format!("{pattern}**")
    } else {
        pattern.to_owned()
    };
    matches_from(&segments(&owned), &segments(route))
}

fn segments(path: &str) -> Vec<&str> {
    path.split('/').filter(|part| !part.is_empty()).collect()
}

fn matches_from(pattern: &[&str], route: &[&str]) -> bool {
    match (pattern.first(), route.first()) {
        (None, None) => true,
        (None, Some(_)) => false,
        (Some(&"**"), _) => {
            matches_from(&pattern[1..], route)
                || (!route.is_empty() && matches_from(pattern, &route[1..]))
        }
        (Some(_), None) => false,
        (Some(&"*"), Some(_)) => matches_from(&pattern[1..], &route[1..]),
        (Some(segment), Some(part)) => segment == part && matches_from(&pattern[1..], &route[1..]),
    }
}

#[cfg(test)]
mod tests;
