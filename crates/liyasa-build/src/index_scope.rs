//! Which readers an indexed page is offered to (AUTH-53, defect 157).
//!
//! `regions` is availability rather than access. `section::gates` already keeps
//! every gated *block* out of the index, so what a region gate scopes here is a
//! page's title, route and snippet — and AUTH-54 has region-gated pages publicly
//! indexable in their default variant.
//!
//! The precedence rule between `only` and `except` is not here. It lives in
//! `i18n::regions::allowed`, which returns what the gate admits; this module
//! decides what the build does about it. A gate resolving to no region at all is
//! the case worth the separation: `allowed` is right to call that `Some(empty)`,
//! and `PageMeta.regions` has no way to spell it — `regions: []` means
//! unrestricted, so writing the empty list into the facets would produce the
//! page every reader matches from the gate that admits nobody.

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::frontmatter::RegionGate;
use liyasa_core::ids::Route;

use crate::i18n::config::Regions;
use crate::i18n::regions::allowed;

/// What the search index is told about one page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    /// `PageMeta.regions`. Empty means every region, which is what
    /// `ReaderScope::admits` opens on.
    pub regions: Vec<String>,
    /// `IndexPage.indexed`.
    pub indexed: bool,
    pub diagnostic: Option<Diagnostic>,
}

/// The scope one page's gate produces.
///
/// `indexed` in is `tree::Indexing::search` — whether the page asked to be in
/// search at all. A gate can take that away and never grant it.
pub fn of_page(
    route: &Route,
    gate: Option<&RegionGate>,
    regions: &Regions,
    indexed: bool,
) -> Scope {
    // A site with no region system cannot have meant anything by a gate, and
    // `allowed` answers `Some(empty)` for an `except` against an empty list —
    // the literal truth, and not grounds to drop a page here. Spelled with
    // `enabled` as well as the list because `allowed` never reads `enabled`,
    // while `Detector::detect` and `undeclared` both go inert without it.
    if !regions.enabled || regions.list.is_empty() {
        return Scope {
            regions: Vec::new(),
            indexed,
            diagnostic: None,
        };
    }
    match allowed(gate, regions) {
        None => Scope {
            regions: Vec::new(),
            indexed,
            diagnostic: None,
        },
        Some(list) if list.is_empty() => Scope {
            regions: Vec::new(),
            indexed: false,
            diagnostic: Some(admits_nobody(route)),
        },
        Some(list) => Scope {
            regions: list,
            indexed,
            diagnostic: None,
        },
    }
}

fn admits_nobody(route: &Route) -> Diagnostic {
    Diagnostic::new(
        code::W0729,
        format!("`{route}` has a region gate that admits no region"),
    )
    .help(
        "`only` chooses the set and `except` narrows it, so a region named in both is removed; \
         the page is built and routed but left out of search",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn regions(enabled: bool, list: &[&str]) -> Regions {
        Regions {
            enabled,
            list: list.iter().map(|code| (*code).to_owned()).collect(),
            ..Regions::default()
        }
    }

    fn gate(only: Option<&[&str]>, except: Option<&[&str]>) -> RegionGate {
        let list = |codes: &[&str]| codes.iter().map(|code| (*code).to_owned()).collect();
        RegionGate {
            only: only.map(list),
            except: except.map(list),
        }
    }

    fn route() -> Route {
        Route::new("/pricing")
    }

    #[test]
    fn an_ungated_page_is_offered_to_every_region() {
        let scope = of_page(&route(), None, &regions(true, &["us", "eu"]), true);
        assert_eq!(scope.regions, Vec::<String>::new());
        assert!(scope.indexed);
        assert!(scope.diagnostic.is_none());
    }

    #[test]
    fn an_exception_becomes_the_regions_that_remain() {
        let scope = of_page(
            &route(),
            Some(&gate(None, Some(&["eu"]))),
            &regions(true, &["us", "eu", "apac"]),
            true,
        );
        assert_eq!(scope.regions, vec!["us".to_owned(), "apac".to_owned()]);
        assert!(scope.indexed);
    }

    #[test]
    fn a_gate_that_admits_nobody_is_not_indexed_and_says_so() {
        let scope = of_page(
            &route(),
            Some(&gate(Some(&["us"]), Some(&["us"]))),
            &regions(true, &["us", "eu"]),
            true,
        );
        assert!(!scope.indexed);
        // Not `regions: []` with `indexed: true` — that is the empty list
        // `ReaderScope::admits` reads as "every reader".
        assert_eq!(scope.regions, Vec::<String>::new());
        assert_eq!(
            scope.diagnostic.map(|one| one.code.as_str().to_owned()),
            Some("W0729".to_owned())
        );
    }

    #[test]
    fn a_page_already_out_of_search_stays_out_and_a_gate_cannot_let_it_in() {
        let scope = of_page(
            &route(),
            Some(&gate(None, Some(&["eu"]))),
            &regions(true, &["us", "eu"]),
            false,
        );
        assert!(!scope.indexed);
    }

    #[test]
    fn a_site_with_no_regions_ignores_a_gate_rather_than_warning_about_it() {
        for site in [regions(false, &["us", "eu"]), regions(true, &[])] {
            let scope = of_page(&route(), Some(&gate(None, Some(&["eu"]))), &site, true);
            assert!(
                scope.indexed,
                "a gate cannot withhold what the site never declared"
            );
            assert_eq!(scope.regions, Vec::<String>::new());
            assert!(
                scope.diagnostic.is_none(),
                "W0729 would fire on the sites least equipped to read it"
            );
        }
    }
}
