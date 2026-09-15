//! The Liyasa style that ships with the binary (VER-61).
//!
//! VER-61 names three packages: this one, Google, and Microsoft. The other two
//! are third-party rule sets an operator installs under `StylesPath`; this one
//! is compiled in, so `liyasa verify` has something to say on a project that
//! has configured nothing.
//!
//! The rules are real `.yml` files in Vale's own format rather than Rust
//! literals, so an operator can read one, copy it into their own style, and
//! change it.

use super::rule::Rule;

/// `(name, source)` for every bundled rule.
pub const RULES: &[(&str, &str)] = &[
    (
        "Liyasa.Consistency",
        include_str!("styles/Liyasa/Consistency.yml"),
    ),
    (
        "Liyasa.Exclamation",
        include_str!("styles/Liyasa/Exclamation.yml"),
    ),
    (
        "Liyasa.Headings",
        include_str!("styles/Liyasa/Headings.yml"),
    ),
    (
        "Liyasa.Inclusive",
        include_str!("styles/Liyasa/Inclusive.yml"),
    ),
    ("Liyasa.Latin", include_str!("styles/Liyasa/Latin.yml")),
    (
        "Liyasa.Spelling",
        include_str!("styles/Liyasa/Spelling.yml"),
    ),
    ("Liyasa.Terms", include_str!("styles/Liyasa/Terms.yml")),
    ("Liyasa.Weasel", include_str!("styles/Liyasa/Weasel.yml")),
];

/// The bundled style, parsed.
///
/// A rule that fails to parse is a bug in this crate, not in an operator's
/// project, and the test below is what catches it; at run time the broken rule
/// is dropped rather than taking the whole style down with it.
pub fn liyasa() -> Vec<Rule> {
    RULES
        .iter()
        .filter_map(|(name, source)| Rule::parse(name, source).ok())
        .collect()
}

#[cfg(test)]
mod tests;
