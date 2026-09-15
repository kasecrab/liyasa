//! `navigation`: the config tree resolved against the content tree (PRD §6.6
//! query 7, §8.4).
//!
//! The shape the theme renders is `liyasa_theme::nav::Navigation`; what this
//! module adds is resolution — a string node is a page, a `directory` node is
//! whatever the content tree holds under it, and a site with no navigation at
//! all gets one built from its own directories. Hidden pages never appear
//! (CM-80).

use std::collections::BTreeSet;

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::ids::{Route, Version};
use liyasa_theme::nav::{Breadcrumbs, Choice, Group, Item, Navigation, Tab};
use serde_json::Value;

use crate::tree::Tree;
use crate::versions::Versions;

#[derive(Debug)]
pub struct Resolved {
    pub navigation: Navigation,
    pub diagnostics: Diagnostics,
}

/// Resolves the `navigation` key against the pages a build found.
///
/// `version` narrows the tree to one version's pages, because a version has its
/// own navigation (CM-92).
pub fn resolve(
    config: &Value,
    tree: &Tree,
    versions: &Versions,
    version: Option<&Version>,
) -> Resolved {
    let mut diagnostics = Diagnostics::new();
    let pages: Vec<&crate::tree::Page> = tree
        .pages
        .iter()
        .filter(|page| page.version.as_ref() == version)
        .filter(|page| page.indexing.navigation)
        .collect();

    let node = config.get("navigation");
    let breadcrumbs = node
        .and_then(|node| node.get("breadcrumbs"))
        .and_then(Value::as_str)
        .map(|text| match text {
            "eyebrow" => Breadcrumbs::Eyebrow,
            "none" => Breadcrumbs::None,
            _ => Breadcrumbs::Path,
        })
        .unwrap_or_default();

    let declared = match node {
        Some(Value::Array(nodes)) => nodes.clone(),
        Some(Value::Object(_)) => node
            .and_then(|node| node.get("tabs").or_else(|| node.get("pages")))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        _ => Vec::new(),
    };

    let mut tabs = Vec::new();
    if declared.is_empty() {
        // CFG-31's autofill, and what a project with no `navigation` key gets.
        tabs.push(Tab {
            groups: from_tree(&pages),
            ..Tab::default()
        });
    } else {
        let mut groups = Vec::new();
        let mut loose = Vec::new();
        for entry in &declared {
            match entry {
                Value::Object(map) if map.contains_key("tab") => {
                    tabs.push(tab_of(entry, &pages, &mut diagnostics));
                }
                Value::Object(map) if map.contains_key("group") => {
                    groups.push(group_of(entry, &pages, &mut diagnostics));
                }
                Value::Object(map) if map.contains_key("directory") => {
                    groups.extend(directory_of(entry, &pages));
                }
                Value::String(_) => {
                    if let Some(item) = item_of(entry, &pages, &mut diagnostics) {
                        loose.push(item);
                    }
                }
                _ => {}
            }
        }
        if !loose.is_empty() {
            groups.insert(
                0,
                Group {
                    expanded: true,
                    items: loose,
                    ..Group::default()
                },
            );
        }
        if !groups.is_empty() {
            tabs.insert(
                0,
                Tab {
                    groups,
                    ..Tab::default()
                },
            );
        }
    }

    let navigation = Navigation {
        tabs,
        versions: version_choices(versions, tree, version),
        locales: Vec::new(),
        breadcrumbs,
    };
    Resolved {
        navigation,
        diagnostics,
    }
}

/// The version switcher's entries, with the fallback CM-91 asks for.
fn version_choices(versions: &Versions, tree: &Tree, current: Option<&Version>) -> Vec<Choice> {
    if versions.is_empty() {
        return Vec::new();
    }
    let routes = crate::versions::routes_by_version(&tree.pages);
    let base = tree
        .pages
        .iter()
        .find(|page| page.version.as_ref() == current)
        .map(|page| page.base_route.clone())
        .unwrap_or_else(|| Route::new("/"));
    versions
        .switcher(&base, current, &routes)
        .into_iter()
        .map(|entry| Choice {
            label: entry.label,
            value: entry.version.as_str().to_owned(),
            href: entry.href,
            current: entry.current,
        })
        .collect()
}

/// A navigation built from the directories the content sits in.
fn from_tree(pages: &[&crate::tree::Page]) -> Vec<Group> {
    let mut sections: BTreeSet<String> = BTreeSet::new();
    for page in pages {
        sections.insert(section_of(&page.route));
    }
    sections
        .into_iter()
        .map(|section| Group {
            title: title_of_section(&section),
            expanded: true,
            items: pages
                .iter()
                .filter(|page| section_of(&page.route) == section)
                .map(|page| item(page))
                .collect(),
            ..Group::default()
        })
        .collect()
}

fn tab_of(node: &Value, pages: &[&crate::tree::Page], diagnostics: &mut Diagnostics) -> Tab {
    let title = node
        .get("tab")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let mut groups = Vec::new();
    let mut loose = Vec::new();
    for entry in node
        .get("pages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        match &entry {
            Value::Object(map) if map.contains_key("group") => {
                groups.push(group_of(&entry, pages, diagnostics));
            }
            Value::Object(map) if map.contains_key("directory") => {
                groups.extend(directory_of(&entry, pages));
            }
            _ => {
                if let Some(item) = item_of(&entry, pages, diagnostics) {
                    loose.push(item);
                }
            }
        }
    }
    if !loose.is_empty() {
        groups.insert(
            0,
            Group {
                expanded: true,
                items: loose,
                ..Group::default()
            },
        );
    }
    Tab {
        title,
        icon: node.get("icon").and_then(Value::as_str).map(str::to_owned),
        groups,
        ..Tab::default()
    }
}

fn group_of(node: &Value, pages: &[&crate::tree::Page], diagnostics: &mut Diagnostics) -> Group {
    Group {
        title: node
            .get("group")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        icon: node.get("icon").and_then(Value::as_str).map(str::to_owned),
        expanded: node
            .get("expanded")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        items: node
            .get("pages")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| item_of(entry, pages, diagnostics))
                    .collect()
            })
            .unwrap_or_default(),
        ..Group::default()
    }
}

/// `{ "directory": "guides" }`: every page under it, in route order.
fn directory_of(node: &Value, pages: &[&crate::tree::Page]) -> Vec<Group> {
    let Some(directory) = node.get("directory").and_then(Value::as_str) else {
        return Vec::new();
    };
    let prefix = format!("/{}", directory.trim_matches('/'));
    let items: Vec<Item> = pages
        .iter()
        .filter(|page| {
            page.route.as_str() == prefix || page.route.as_str().starts_with(&format!("{prefix}/"))
        })
        .map(|page| item(page))
        .collect();
    if items.is_empty() {
        return Vec::new();
    }
    vec![Group {
        title: title_of_section(&prefix),
        expanded: true,
        items,
        ..Group::default()
    }]
}

fn item_of(
    node: &Value,
    pages: &[&crate::tree::Page],
    diagnostics: &mut Diagnostics,
) -> Option<Item> {
    let Value::String(reference) = node else {
        // A nested group inside a group's `pages` is resolved as its own group
        // elsewhere; an unknown shape is simply skipped.
        return None;
    };
    let wanted = normalize(reference);
    match pages
        .iter()
        .find(|page| page.route.as_str() == wanted || page.path.as_str() == reference.as_str())
    {
        Some(page) => Some(item(page)),
        None => {
            diagnostics.push(
                Diagnostic::new(
                    code::E0104,
                    format!("navigation names `{reference}`, which is not a page"),
                )
                .help("check the path, or remove the entry"),
            );
            None
        }
    }
}

fn item(page: &crate::tree::Page) -> Item {
    Item {
        title: page
            .front
            .sidebar_title
            .clone()
            .or_else(|| page.front.title.clone())
            .unwrap_or_else(|| title_of_section(page.route.as_str())),
        route: page.route.as_str().to_owned(),
        icon: page.front.icon.clone(),
        tag: page.front.tag.clone(),
        ..Item::default()
    }
}

/// A route's parent section, which is what an autofilled group is keyed on.
fn section_of(route: &Route) -> String {
    let trimmed = route.as_str().trim_matches('/');
    match trimmed.split_once('/') {
        Some((head, _)) => format!("/{head}"),
        None => "/".to_owned(),
    }
}

/// `"/getting-started"` to `"Getting started"`.
fn title_of_section(section: &str) -> String {
    let last = section.trim_matches('/').rsplit('/').next().unwrap_or("");
    if last.is_empty() {
        return "Overview".to_owned();
    }
    let spaced = last.replace(['-', '_'], " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => spaced,
    }
}

fn normalize(reference: &str) -> String {
    let trimmed = reference.trim_matches('/');
    let without_extension = trimmed
        .strip_suffix(".md")
        .or_else(|| trimmed.strip_suffix(".mdx"))
        .unwrap_or(trimmed);
    let without_index = without_extension
        .strip_suffix("/index")
        .unwrap_or(without_extension);
    match without_index.is_empty() || without_index == "index" {
        true => "/".to_owned(),
        false => format!("/{without_index}"),
    }
}

#[cfg(test)]
mod tests {
    use liyasa_core::frontmatter::FrontmatterFields;
    use liyasa_core::ids::Fingerprint;
    use liyasa_core::vfs::VfsPath;

    use super::*;
    use crate::tree::{Indexing, Page};

    fn page(path: &str, title: &str, hidden: bool) -> Page {
        let front = FrontmatterFields {
            title: Some(title.to_owned()),
            hidden: hidden.then_some(true),
            ..FrontmatterFields::default()
        };
        let route = liyasa_markdown::source::route::route_of(&VfsPath::new(path), Some(&front));
        Page {
            path: VfsPath::new(path),
            base_route: route.clone(),
            route,
            version: None,
            fingerprint: Fingerprint::of(path),
            indexing: Indexing::of(&front, false),
            hidden,
            draft: false,
            front,
        }
    }

    fn tree(pages: Vec<Page>) -> Tree {
        Tree {
            pages,
            ..Tree::default()
        }
    }

    fn config(json: &str) -> Value {
        serde_json::from_str(json).expect("the fixture is JSON")
    }

    fn fixture() -> Tree {
        tree(vec![
            page("index.md", "Home", false),
            page("guides/install.md", "Install", false),
            page("guides/upgrade.md", "Upgrade", false),
            page("reference/api.md", "API", false),
        ])
    }

    #[test]
    fn a_site_with_no_navigation_gets_one_from_its_directories() {
        let resolved = resolve(&config("{}"), &fixture(), &Versions::default(), None);
        let titles: Vec<&str> = resolved
            .navigation
            .order()
            .iter()
            .map(|item| item.title.as_str())
            .collect();
        assert!(titles.contains(&"Install"), "{titles:?}");
        assert!(titles.contains(&"API"), "{titles:?}");
        assert!(resolved.diagnostics.is_empty());
    }

    #[test]
    fn a_declared_group_keeps_its_order() {
        let resolved = resolve(
            &config(
                r#"{"navigation":[{"group":"Guides","pages":["guides/upgrade","guides/install"]}]}"#,
            ),
            &fixture(),
            &Versions::default(),
            None,
        );
        let order: Vec<&str> = resolved
            .navigation
            .order()
            .iter()
            .map(|item| item.route.as_str())
            .collect();
        assert_eq!(order, ["/guides/upgrade", "/guides/install"]);
        assert_eq!(resolved.navigation.tabs[0].groups[0].title, "Guides");
    }

    #[test]
    fn a_page_navigation_names_but_does_not_have_is_e0104() {
        let resolved = resolve(
            &config(r#"{"navigation":[{"group":"Guides","pages":["guides/missing"]}]}"#),
            &fixture(),
            &Versions::default(),
            None,
        );
        assert_eq!(
            resolved
                .diagnostics
                .iter()
                .map(|d| d.code.as_str())
                .collect::<Vec<_>>(),
            ["E0104"]
        );
    }

    #[test]
    fn a_directory_node_pulls_in_what_is_under_it() {
        let resolved = resolve(
            &config(r#"{"navigation":[{"directory":"guides"}]}"#),
            &fixture(),
            &Versions::default(),
            None,
        );
        let routes: Vec<&str> = resolved
            .navigation
            .order()
            .iter()
            .map(|item| item.route.as_str())
            .collect();
        assert_eq!(routes, ["/guides/install", "/guides/upgrade"]);
    }

    #[test]
    fn tabs_become_tabs() {
        let resolved = resolve(
            &config(
                r#"{"navigation":{"tabs":[{"tab":"Guides","pages":["guides/install"]},
                     {"tab":"Reference","pages":["reference/api"]}]}}"#,
            ),
            &fixture(),
            &Versions::default(),
            None,
        );
        let titles: Vec<&str> = resolved
            .navigation
            .tabs
            .iter()
            .map(|tab| tab.title.as_str())
            .collect();
        assert_eq!(titles, ["Guides", "Reference"]);
    }

    #[test]
    fn cm_80_a_hidden_page_is_not_in_navigation() {
        let tree = tree(vec![
            page("index.md", "Home", false),
            page("secret.md", "Secret", true),
        ]);
        let resolved = resolve(&config("{}"), &tree, &Versions::default(), None);
        let routes: Vec<&str> = resolved
            .navigation
            .order()
            .iter()
            .map(|item| item.route.as_str())
            .collect();
        assert!(!routes.contains(&"/secret"), "{routes:?}");
    }

    #[test]
    fn neighbours_and_breadcrumbs_come_from_the_resolved_order() {
        let resolved = resolve(
            &config(
                r#"{"navigation":{"breadcrumbs":"eyebrow",
                     "pages":[{"group":"Guides","pages":["guides/install","guides/upgrade"]}]}}"#,
            ),
            &fixture(),
            &Versions::default(),
            None,
        );
        assert_eq!(resolved.navigation.breadcrumbs, Breadcrumbs::Eyebrow);
        let (previous, next) = resolved.navigation.neighbours("/guides/upgrade");
        assert_eq!(
            previous.map(|link| link.route),
            Some("/guides/install".to_owned())
        );
        assert!(next.is_none());
        let trail = resolved.navigation.trail("/guides/install");
        assert_eq!(
            trail.first().map(|crumb| crumb.title.as_str()),
            Some("Guides")
        );
    }

    #[test]
    fn a_reference_may_be_a_path_or_a_route() {
        let resolved = resolve(
            &config(r#"{"navigation":["guides/install.md","/reference/api"]}"#),
            &fixture(),
            &Versions::default(),
            None,
        );
        assert_eq!(resolved.navigation.order().len(), 2);
        assert!(resolved.diagnostics.is_empty());
    }
}
