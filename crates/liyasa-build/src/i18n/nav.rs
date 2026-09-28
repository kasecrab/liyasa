//! The region chain of a navigation node (AUTH-51, third gating level).
//!
//! `nav::access_of` walks the navigation for the `groups:` each route inherits
//! and `auth::groups::decide` reads the chain it produces. `regions:` on the
//! same nodes is the other half of §8.4's gate and had no walk at all: a
//! sidebar section declared `regions: ["us"]` was served to every reader in
//! every region.
//!
//! This is that walk. It resolves nothing itself — [`Routes`] is passed in, so
//! the renderer, the access walk and this cannot turn one navigation reference
//! into three different pages.
//!
//! **A chain is about the route, not about the sidebar.** Which entries a
//! reader's sidebar lists is `plan/rfcs/2701-a-region-gated-navigation-node.md`
//! and needs a field `liyasa_theme::nav` does not have yet. What this closes is
//! the hole: whether the route is served at all.

use std::collections::BTreeMap;

use liyasa_core::frontmatter::RegionGate;
use liyasa_core::ids::Route;
use serde_json::Value;

use super::regions;

/// How a navigation reference becomes a route.
///
/// The navigation renderer owns both answers. Taking them as a parameter is
/// what keeps this from becoming a second resolver that can disagree with
/// `nav::find_page` — the hazard `nav.rs` calls out where `item_of` and
/// `access_of` share one.
pub trait Routes {
    /// The route a string node names, by route or by source path.
    fn named(&self, reference: &str) -> Option<Route>;
    /// Every route a `directory` node covers.
    fn under(&self, directory: &str) -> Vec<Route>;
}

/// Each route's ancestor region gates, root-first.
///
/// The page's own `regions:` front matter is not here, for the reason
/// `nav::access_of` leaves the page's own access level out: a page the
/// navigation never names still has one, and the caller appends it.
///
/// A route named twice keeps the chain of its first occurrence in document
/// order, which is the entry the renderer would show first.
///
/// Every shape that can hold children is recursed, including the node kinds
/// the sidebar renderer ignores. That asymmetry is deliberate and is
/// `nav::access_of`'s: a subtree missing from the sidebar is cosmetic, a
/// subtree missing its restriction is a hole, so this walk is the more
/// thorough of the two.
pub fn chains(nodes: &[Value], routes: &dyn Routes) -> BTreeMap<Route, Vec<RegionGate>> {
    let mut out = BTreeMap::new();
    walk(nodes, routes, &[], &mut out);
    out
}

fn walk(
    nodes: &[Value],
    routes: &dyn Routes,
    chain: &[RegionGate],
    out: &mut BTreeMap<Route, Vec<RegionGate>>,
) {
    for node in nodes {
        match node {
            Value::String(reference) => {
                if let Some(route) = routes.named(reference) {
                    out.entry(route).or_insert_with(|| chain.to_vec());
                }
            }
            Value::Object(map) => {
                let mut next = chain.to_vec();
                next.extend(regions::node_gate(node).filter(|gate| !is_open(gate)));
                if let Some(directory) = map.get("directory").and_then(Value::as_str) {
                    for route in routes.under(directory) {
                        out.entry(route).or_insert_with(|| next.clone());
                    }
                }
                // `pages` on group, tab, product, version and language nodes;
                // `items` on menu and dropdown.
                for key in ["pages", "items"] {
                    if let Some(children) = map.get(key).and_then(Value::as_array) {
                        walk(children, routes, &next, out);
                    }
                }
            }
            _ => {}
        }
    }
}

/// A gate that restricts nothing, which is left out of the chain the way
/// `AccessLevel::is_open` levels are.
fn is_open(gate: &RegionGate) -> bool {
    gate.only.as_ref().is_none_or(Vec::is_empty) && gate.except.as_ref().is_none_or(Vec::is_empty)
}

/// Whether a reader in `region` may be served a route with this chain.
///
/// Every level must admit, the way `auth::groups::decide` requires every level
/// to grant: a page that names no region of its own does not escape the
/// region-restricted section it sits in.
///
/// A build that does not know the reader's region admits no restricted level,
/// which is RFC 0401's rule — a gate that cannot be checked is a gate that did
/// not hold.
pub fn admits_chain(chain: &[RegionGate], region: Option<&str>) -> bool {
    chain
        .iter()
        .all(|gate| regions::admits_page(Some(gate), region))
}

/// Every region name a navigation declares, for `regions::undeclared`.
///
/// A typo in a sidebar gate withholds the whole section from everyone, so it is
/// worth the same warning a typo in a page's front matter gets.
pub fn named_in(chains: &BTreeMap<Route, Vec<RegionGate>>) -> std::collections::BTreeSet<String> {
    chains
        .values()
        .flatten()
        .flat_map(|gate| {
            gate.only
                .iter()
                .chain(gate.except.iter())
                .flatten()
                .cloned()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    /// The pages of a small site, resolved the way `nav::find_page` resolves
    /// them: by route, or by source path.
    struct Pages(Vec<&'static str>);

    impl Routes for Pages {
        fn named(&self, reference: &str) -> Option<Route> {
            let wanted = format!("/{}", reference.trim_matches('/'));
            self.0
                .iter()
                .find(|route| **route == wanted)
                .map(|route| Route::new(*route))
        }

        fn under(&self, directory: &str) -> Vec<Route> {
            let prefix = format!("/{}", directory.trim_matches('/'));
            self.0
                .iter()
                .filter(|route| **route == prefix || route.starts_with(&format!("{prefix}/")))
                .map(|route| Route::new(*route))
                .collect()
        }
    }

    fn pages() -> Pages {
        Pages(vec![
            "/index",
            "/guides/install",
            "/us/billing",
            "/us/billing/instalments",
            "/partners/rates",
        ])
    }

    fn nodes(json: &str) -> Vec<Value> {
        serde_json::from_str(json).expect("the fixture is JSON")
    }

    fn gate(only: &[&str], except: &[&str]) -> RegionGate {
        RegionGate {
            only: (!only.is_empty()).then(|| only.iter().map(|c| (*c).to_owned()).collect()),
            except: (!except.is_empty()).then(|| except.iter().map(|c| (*c).to_owned()).collect()),
        }
    }

    #[test]
    fn a_navigation_with_no_regions_restricts_nothing() {
        let chains = chains(
            &nodes(r#"["index",{"group":"Guides","pages":["guides/install"]}]"#),
            &pages(),
        );
        assert_eq!(chains[&Route::new("/index")], Vec::new());
        assert_eq!(chains[&Route::new("/guides/install")], Vec::new());
        assert!(admits_chain(&chains[&Route::new("/index")], None));
    }

    #[test]
    fn a_page_inherits_the_gate_of_the_group_it_sits_in() {
        let chains = chains(
            &nodes(r#"[{"group":"Billing","regions":["us"],"pages":["us/billing"]}]"#),
            &pages(),
        );
        let chain = &chains[&Route::new("/us/billing")];
        assert_eq!(chain, &vec![gate(&["us"], &[])]);
        assert!(admits_chain(chain, Some("us")));
        assert!(!admits_chain(chain, Some("eu")));
        assert!(
            !admits_chain(chain, None),
            "a build that does not know the region cannot show the gate held"
        );
    }

    /// The hazard `auth_07_mixed.rs` was written for, in the region half: the
    /// sidebar renderer drops a group nested inside another group's `pages`, so
    /// a walk that mirrored the renderer would give this page no ancestor and
    /// serve it everywhere.
    #[test]
    fn a_gate_nested_inside_another_gate_reaches_the_page_under_it() {
        let chains = chains(
            &nodes(
                r#"[{"group":"Billing","regions":["us","ca"],"pages":[
                     "us/billing",
                     {"group":"Instalments","regions":["us"],
                      "pages":["us/billing/instalments"]}]}]"#,
            ),
            &pages(),
        );
        let inner = &chains[&Route::new("/us/billing/instalments")];
        assert_eq!(inner.len(), 2, "both levels, root-first");
        assert!(admits_chain(inner, Some("us")));
        assert!(
            !admits_chain(inner, Some("ca")),
            "the outer level admits CA and the inner one does not; every level must admit"
        );
    }

    #[test]
    fn a_directory_node_gates_every_route_under_it() {
        let chains = chains(
            &nodes(r#"[{"directory":"partners","regions":{"except":["eu"]}}]"#),
            &pages(),
        );
        let chain = &chains[&Route::new("/partners/rates")];
        assert!(admits_chain(chain, Some("us")));
        assert!(!admits_chain(chain, Some("eu")));
    }

    #[test]
    fn a_menu_items_children_are_walked_too() {
        let chains = chains(
            &nodes(r#"[{"menu":"More","regions":["us"],"items":["us/billing"]}]"#),
            &pages(),
        );
        assert_eq!(chains[&Route::new("/us/billing")], vec![gate(&["us"], &[])]);
    }

    #[test]
    fn a_gate_that_names_nothing_is_not_a_level() {
        let chains = chains(
            &nodes(r#"[{"group":"Guides","regions":[],"pages":["guides/install"]}]"#),
            &pages(),
        );
        assert!(
            chains[&Route::new("/guides/install")].is_empty(),
            "an empty declaration is no gate, so it does not withhold the page"
        );
    }

    #[test]
    fn a_route_named_twice_keeps_its_first_chain() {
        let chains = chains(
            &nodes(
                r#"[{"group":"First","regions":["us"],"pages":["us/billing"]},
                    {"group":"Second","regions":["eu"],"pages":["us/billing"]}]"#,
            ),
            &pages(),
        );
        assert_eq!(chains[&Route::new("/us/billing")], vec![gate(&["us"], &[])]);
    }

    #[test]
    fn a_reference_no_page_answers_is_not_in_the_map() {
        let chains = chains(
            &nodes(r#"[{"group":"Gone","regions":["us"],"pages":["nowhere"]}]"#),
            &pages(),
        );
        assert!(chains.is_empty());
    }

    #[test]
    fn every_region_the_navigation_names_is_collected() {
        let chains = chains(
            &nodes(
                r#"[{"group":"Billing","regions":["us"],"pages":["us/billing"]},
                    {"directory":"partners","regions":{"except":["eu"]}}]"#,
            ),
            &pages(),
        );
        assert_eq!(
            named_in(&chains),
            ["eu".to_owned(), "us".to_owned()]
                .into_iter()
                .collect::<BTreeSet<String>>(),
            "an exclusion names a region as much as an inclusion does"
        );
    }
}
