//! Turning a spec into navigation (API-03).
//!
//! A navigation node `{ openapi: "api" }` expands to one page per operation,
//! grouped and ordered; `"api:GET /users/{id}"` places one operation wherever
//! the author wants it. Both produce the same [`Entry`], so a page reached
//! either way has one route.

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use serde::{Deserialize, Serialize};

use crate::config::{GroupBy, Operations, SpecConfig};
use crate::model::{Method, OperationRef, Spec, slug};

/// Where generated pages live when a site does not say otherwise.
pub const DEFAULT_BASE: &str = "api-reference";

/// The group an untagged operation falls into.
const UNGROUPED: &str = "Other";

/// A navigation node that expands to a spec's operations.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Node {
    /// The spec's `id`.
    pub openapi: String,
    pub group_by: Option<GroupBy>,
    /// Only these operations, by selector.
    pub operations: Option<Operations>,
    /// Display names, order, and collapsed state per group.
    pub groups: Vec<GroupConfig>,
    /// The route prefix; `api-reference` by default.
    pub base: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GroupConfig {
    /// The tag or path prefix this is about.
    pub name: String,
    pub title: Option<String>,
    /// Lower sorts first; groups with no order keep their spec order, after
    /// the ordered ones.
    pub order: Option<i64>,
    pub collapsed: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reference {
    pub spec: String,
    pub base: String,
    pub groups: Vec<Group>,
}

impl Reference {
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.groups.iter().flat_map(|group| group.pages.iter())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    /// The tag or prefix, as written in the spec.
    pub name: String,
    pub title: String,
    pub description: Option<String>,
    pub collapsed: bool,
    pub pages: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// `GET /users/{id}`.
    pub selector: String,
    pub method: Method,
    pub path: String,
    pub title: String,
    pub route: String,
    pub deprecated: bool,
    /// `x-liyasa.href`: the entry points here instead of at the generated
    /// page.
    pub href: Option<String>,
    pub webhook: bool,
}

/// Splits `api:GET /users/{id}` into the spec id and the selector.
///
/// The colon is the separator, and a path may contain one, so only the first
/// counts and the spec id may not contain one.
pub fn parse_selector(text: &str) -> Option<(&str, &str)> {
    let (id, rest) = text.split_once(':')?;
    let rest = rest.trim_start();
    if id.is_empty() || rest.is_empty() {
        return None;
    }
    let (method, _) = rest.split_once(' ')?;
    Method::parse(method)?;
    Some((id, rest))
}

/// Expands a node into the pages it generates.
pub fn generate(spec: &Spec, config: &SpecConfig, node: &Node) -> (Reference, Diagnostics) {
    let mut diagnostics = Diagnostics::new();
    let base = node.base.clone().unwrap_or_else(|| DEFAULT_BASE.to_owned());
    let group_by = node.group_by.unwrap_or(config.group_by);
    let wanted = node.operations.clone().unwrap_or(config.operations.clone());

    let mut groups: Vec<Group> = Vec::new();
    // Declared tags first, in the order the spec declares them, so a spec that
    // has thought about its order keeps it.
    for tag in &spec.tags {
        if tag.name.is_empty() {
            continue;
        }
        groups.push(Group {
            name: tag.name.clone(),
            title: tag
                .extensions
                .get(crate::model::ext::NAMESPACE)
                .map(|_| crate::model::XLiyasa::read(&tag.extensions))
                .and_then(|hints| hints.title)
                .unwrap_or_else(|| tag.name.clone()),
            description: tag.description.clone(),
            collapsed: false,
            pages: Vec::new(),
        });
    }

    let mut taken: Vec<String> = Vec::new();
    for operation in spec.operations() {
        if !wanted.includes(&operation.selector()) {
            continue;
        }
        if operation.operation.liyasa.hidden {
            continue;
        }
        let entry = entry(&operation, &base, &mut taken);
        for name in group_names(&operation, group_by) {
            match groups.iter_mut().find(|group| group.name == name) {
                Some(group) => group.pages.push(entry.clone()),
                None => groups.push(Group {
                    name: name.clone(),
                    title: name.clone(),
                    description: None,
                    collapsed: false,
                    pages: vec![entry.clone()],
                }),
            }
        }
    }

    if let Operations::Listed(listed) = &wanted {
        for selector in listed {
            if spec
                .operations()
                .all(|operation| operation.selector() != *selector)
            {
                diagnostics.push(missing(&config.id, selector));
            }
        }
    }

    groups.retain(|group| !group.pages.is_empty());
    for group in &mut groups {
        order_pages(spec, group);
        apply(group, &node.groups);
    }
    order_groups(&mut groups, &node.groups);

    (
        Reference {
            spec: config.id.clone(),
            base,
            groups,
        },
        diagnostics,
    )
}

/// Resolves one individually placed operation (`"api:GET /users/{id}"`).
pub fn place(
    spec: &Spec,
    config: &SpecConfig,
    selector: &str,
    base: &str,
) -> Result<Entry, crate::SpecError> {
    let mut taken = Vec::new();
    spec.operations()
        .find(|operation| operation.selector() == selector)
        .map(|operation| entry(&operation, base, &mut taken))
        .ok_or_else(|| Box::new(missing(&config.id, selector)))
}

fn missing(id: &str, selector: &str) -> Diagnostic {
    Diagnostic::new(
        code::E0506,
        format!("navigation names `{id}:{selector}`, which the spec does not declare"),
    )
    .help("the selector is the method and the path exactly as the spec writes them")
}

/// Which groups an operation belongs to. Tags may put it in several; a path
/// prefix puts it in exactly one.
fn group_names(operation: &OperationRef<'_>, group_by: GroupBy) -> Vec<String> {
    if let Some(forced) = &operation.operation.liyasa.group {
        return vec![forced.clone()];
    }
    match group_by {
        GroupBy::None => vec![UNGROUPED.to_owned()],
        GroupBy::PathPrefix => vec![prefix(operation.path)],
        GroupBy::Tag => {
            if operation.operation.tags.is_empty() {
                vec![UNGROUPED.to_owned()]
            } else {
                operation.operation.tags.clone()
            }
        }
    }
}

/// The first path segment that is not a template variable: `/users/{id}/notes`
/// groups under `users`.
fn prefix(path: &str) -> String {
    path.split('/')
        .find(|segment| !segment.is_empty() && !segment.starts_with('{'))
        .unwrap_or(UNGROUPED)
        .to_owned()
}

fn entry(operation: &OperationRef<'_>, base: &str, taken: &mut Vec<String>) -> Entry {
    let hints = &operation.operation.liyasa;
    let title = hints
        .title
        .clone()
        .or_else(|| operation.operation.summary.clone())
        .unwrap_or_else(|| operation.selector());
    Entry {
        selector: operation.selector(),
        method: operation.method,
        path: operation.path.to_owned(),
        title,
        route: route(operation, base, taken),
        deprecated: operation.operation.deprecated,
        href: hints.href.clone(),
        webhook: operation.webhook,
    }
}

/// `/api-reference/get-users-id`, from the operation id when there is one and
/// from the method and path when there is not. A collision gets a suffix
/// rather than two pages at one route.
fn route(operation: &OperationRef<'_>, base: &str, taken: &mut Vec<String>) -> String {
    let stem = match &operation.operation.operation_id {
        Some(id) => slug(id),
        None => slug(&format!(
            "{} {}",
            operation.method.lowercase(),
            operation.path
        )),
    };
    let mut candidate = stem.clone();
    let mut next = 2;
    while taken.contains(&candidate) {
        candidate = format!("{stem}-{next}");
        next += 1;
    }
    taken.push(candidate.clone());
    format!("/{}/{candidate}", base.trim_matches('/'))
}

/// `x-liyasa.order` first, then the order the spec declares the operations in.
fn order_pages(spec: &Spec, group: &mut Group) {
    let order = |entry: &Entry| {
        spec.operation(entry.method, &entry.path)
            .and_then(|operation| operation.operation.liyasa.order)
    };
    let mut ordered: Vec<(usize, Entry)> = group.pages.drain(..).enumerate().collect();
    ordered.sort_by_key(|(index, entry)| (order(entry).unwrap_or(i64::MAX), *index));
    group.pages = ordered.into_iter().map(|(_, entry)| entry).collect();
}

fn apply(group: &mut Group, configured: &[GroupConfig]) {
    let Some(config) = configured.iter().find(|c| c.name == group.name) else {
        return;
    };
    if let Some(title) = &config.title {
        group.title = title.clone();
    }
    if let Some(collapsed) = config.collapsed {
        group.collapsed = collapsed;
    }
}

fn order_groups(groups: &mut [Group], configured: &[GroupConfig]) {
    let order = |group: &Group| {
        configured
            .iter()
            .find(|c| c.name == group.name)
            .and_then(|c| c.order)
    };
    let keys: Vec<(i64, usize)> = groups
        .iter()
        .enumerate()
        .map(|(index, group)| (order(group).unwrap_or(i64::MAX), index))
        .collect();
    let mut sorted: Vec<usize> = (0..groups.len()).collect();
    sorted.sort_by_key(|index| keys[*index]);
    let reordered: Vec<Group> = sorted
        .into_iter()
        .map(|index| groups[index].clone())
        .collect();
    groups.clone_from_slice(&reordered);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load;

    const SPEC: &str = r##"
openapi: 3.1.0
info: { title: T, version: "1" }
tags:
  - { name: Users, description: People }
  - { name: Notes }
paths:
  /users:
    get:
      operationId: listUsers
      summary: List users
      tags: [Users]
      responses: { "200": { description: ok } }
    post:
      operationId: createUser
      tags: [Users]
      x-liyasa: { order: -1 }
      responses: { "201": { description: made } }
  /users/{id}:
    get:
      operationId: getUser
      tags: [Users]
      responses: { "200": { description: ok } }
  /notes:
    get:
      operationId: listNotes
      tags: [Notes]
      responses: { "200": { description: ok } }
  /health:
    get:
      operationId: health
      responses: { "200": { description: ok } }
"##;

    fn spec() -> Spec {
        load::from_bytes("api", "api.yaml", SPEC.as_bytes())
            .expect("the spec loads")
            .spec
    }

    fn config() -> SpecConfig {
        SpecConfig::parse(&serde_json::json!("openapi/api.yaml")).expect("it reads")
    }

    fn reference(node: &Node) -> Reference {
        let spec = spec();
        let (reference, diagnostics) = generate(&spec, &config(), node);
        assert!(!diagnostics.has_errors(), "{:?}", diagnostics.as_slice());
        reference
    }

    #[test]
    fn grouping_by_tag_keeps_the_specs_tag_order_and_collects_the_untagged() {
        let found = reference(&Node::default());
        assert_eq!(
            found
                .groups
                .iter()
                .map(|g| g.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Users", "Notes", "Other"]
        );
        assert_eq!(found.groups[0].description.as_deref(), Some("People"));
        assert_eq!(found.groups[2].pages.len(), 1, "the untagged operation");
    }

    #[test]
    fn an_order_hint_moves_a_page_within_its_group() {
        let found = reference(&Node::default());
        assert_eq!(
            found.groups[0]
                .pages
                .iter()
                .map(|p| p.selector.as_str())
                .collect::<Vec<_>>(),
            vec!["POST /users", "GET /users", "GET /users/{id}"],
            "`order: -1` sorts before the two with no order, which keep spec order"
        );
    }

    #[test]
    fn grouping_by_path_prefix_ignores_the_tags() {
        let found = reference(&Node {
            group_by: Some(GroupBy::PathPrefix),
            ..Node::default()
        });
        assert_eq!(
            found
                .groups
                .iter()
                .map(|g| g.name.as_str())
                .collect::<Vec<_>>(),
            vec!["users", "notes", "health"]
        );
    }

    #[test]
    fn a_listed_subset_generates_only_what_it_lists() {
        let found = reference(&Node {
            operations: Some(Operations::Listed(vec!["GET /users/{id}".to_owned()])),
            ..Node::default()
        });
        assert_eq!(found.entries().count(), 1);
        assert_eq!(
            found.entries().next().map(|e| e.selector.as_str()),
            Some("GET /users/{id}")
        );
    }

    #[test]
    fn a_listed_operation_the_spec_does_not_have_is_e0506() {
        let spec = spec();
        let (_, diagnostics) = generate(
            &spec,
            &config(),
            &Node {
                operations: Some(Operations::Listed(vec!["DELETE /users".to_owned()])),
                ..Node::default()
            },
        );
        let missing: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == code::E0506)
            .collect();
        assert_eq!(missing.len(), 1, "{:?}", diagnostics.as_slice());
        assert!(
            missing[0].message.contains("DELETE /users"),
            "{}",
            missing[0].message
        );
    }

    #[test]
    fn a_group_gets_the_display_name_and_order_the_site_configures() {
        let found = reference(&Node {
            groups: vec![
                GroupConfig {
                    name: "Notes".to_owned(),
                    title: Some("Note taking".to_owned()),
                    order: Some(1),
                    collapsed: Some(true),
                },
                GroupConfig {
                    name: "Users".to_owned(),
                    order: Some(2),
                    ..GroupConfig::default()
                },
            ],
            ..Node::default()
        });
        assert_eq!(found.groups[0].name, "Notes");
        assert_eq!(found.groups[0].title, "Note taking");
        assert!(found.groups[0].collapsed);
        assert_eq!(found.groups[1].name, "Users");
    }

    #[test]
    fn a_route_comes_from_the_operation_id() {
        let found = reference(&Node::default());
        let entry = found
            .entries()
            .find(|e| e.selector == "GET /users/{id}")
            .expect("the page is there");
        assert_eq!(entry.route, "/api-reference/getuser");
        assert_eq!(
            entry.title, "GET /users/{id}",
            "no summary, so the selector titles it"
        );
    }

    #[test]
    fn a_summary_titles_the_page_when_there_is_one() {
        let found = reference(&Node::default());
        let entry = found
            .entries()
            .find(|e| e.selector == "GET /users")
            .expect("the page is there");
        assert_eq!(entry.title, "List users");
    }

    #[test]
    fn a_selector_splits_on_the_first_colon_so_a_path_may_contain_one() {
        assert_eq!(
            parse_selector("api:GET /users/{id}"),
            Some(("api", "GET /users/{id}"))
        );
        assert_eq!(
            parse_selector("api:GET /a:b"),
            Some(("api", "GET /a:b")),
            "a colon inside the path is part of it"
        );
        assert_eq!(parse_selector("api-reference/users"), None);
        assert_eq!(
            parse_selector("api:WAT /users"),
            None,
            "the method must be one"
        );
    }

    #[test]
    fn placing_one_operation_gives_the_same_entry_as_generating_it() {
        let spec = spec();
        let placed = place(&spec, &config(), "GET /users/{id}", DEFAULT_BASE).expect("it resolves");
        assert_eq!(placed.route, "/api-reference/getuser");
        assert_eq!(placed.method, Method::Get);
    }

    #[test]
    fn placing_an_operation_the_spec_does_not_have_is_e0506() {
        let spec = spec();
        let error = place(&spec, &config(), "PATCH /users", DEFAULT_BASE)
            .expect_err("the spec has no such operation");
        assert_eq!(error.code, code::E0506);
    }

    #[test]
    fn a_hidden_operation_generates_no_page() {
        let loaded = load::from_bytes(
            "api",
            "api.yaml",
            br##"
openapi: 3.1.0
info: { title: T, version: "1" }
paths:
  /secret:
    get:
      operationId: secret
      x-liyasa: { hidden: true }
      responses: { "200": { description: ok } }
  /open:
    get:
      operationId: open
      responses: { "200": { description: ok } }
"##,
        )
        .expect("the spec loads");
        let (found, _) = generate(&loaded.spec, &config(), &Node::default());
        assert_eq!(found.entries().count(), 1);
        assert_eq!(
            found.entries().next().map(|e| e.selector.as_str()),
            Some("GET /open")
        );
    }

    #[test]
    fn two_operations_that_would_share_a_route_do_not() {
        let loaded = load::from_bytes(
            "api",
            "api.yaml",
            br##"
openapi: 3.1.0
info: { title: T, version: "1" }
paths:
  /a:
    get:
      responses: { "200": { description: ok } }
  /a/:
    get:
      responses: { "200": { description: ok } }
"##,
        )
        .expect("the spec loads");
        let (found, _) = generate(&loaded.spec, &config(), &Node::default());
        let routes: Vec<&str> = found.entries().map(|e| e.route.as_str()).collect();
        assert_eq!(routes.len(), 2);
        assert_ne!(routes[0], routes[1], "{routes:?}");
    }
}
