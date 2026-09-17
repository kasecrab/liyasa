//! Whether a variant satisfies a block's gate
//! (`plan/rfcs/0401-what-an-unknown-variant-admits.md`).
//!
//! Both predicates read the same way: an empty declaration is no gate and
//! admits everything, and a declaration the variant cannot satisfy — including
//! one it has nothing to say about — admits nothing.
//!
//! `liyasa-server`'s `auth::groups` has the same set intersection for building
//! a page's group variants. This is not a call into it: the dependency runs
//! server to build to components, so an edge back would be a cycle. If it ever
//! wants one home that is `liyasa-core`, and moving it there is a frozen
//! contract and so an RFC rather than an edit.

use std::collections::BTreeSet;

/// Whether `declared` and `held` share a name.
///
/// An anonymous reader holds none, so a block naming any group is withheld —
/// the same answer the page-level gate gives for the same page.
pub fn any_of(declared: &[String], held: &BTreeSet<String>) -> bool {
    declared.is_empty() || declared.iter().any(|name| held.contains(name))
}

/// Whether `value` is one of `declared`.
///
/// `None` is not a match. The build cannot show an unknown region, locale or
/// version is the declared one, and a gate that cannot be checked is a gate
/// that did not hold.
pub fn is_one_of(declared: &[String], value: Option<&str>) -> bool {
    declared.is_empty() || value.is_some_and(|value| declared.iter().any(|one| one == value))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|n| (*n).to_owned()).collect()
    }

    fn list(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| (*n).to_owned()).collect()
    }

    #[test]
    fn no_declaration_is_no_gate() {
        assert!(any_of(&[], &BTreeSet::new()));
        assert!(is_one_of(&[], None));
    }

    #[test]
    fn a_declaration_needs_something_to_match() {
        assert!(!any_of(&list(&["admin"]), &BTreeSet::new()));
        assert!(!is_one_of(&list(&["eu"]), None));
    }

    #[test]
    fn one_name_in_common_admits() {
        assert!(any_of(&list(&["admin", "staff"]), &set(&["staff", "eng"])));
        assert!(!any_of(&list(&["admin"]), &set(&["support"])));
    }

    #[test]
    fn a_scalar_matches_exactly() {
        assert!(is_one_of(&list(&["eu", "uk"]), Some("uk")));
        assert!(!is_one_of(&list(&["eu"]), Some("us")));
        // Not a prefix match: `en` must not open a block gated on `en-GB`.
        assert!(!is_one_of(&list(&["en-GB"]), Some("en")));
    }
}
