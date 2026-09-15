//! The navigation model the sidebar, breadcrumbs, pagination, and table of
//! contents are rendered from (RX-20, RX-21, RX-22).
//!
//! The theme does not read `liyasa.json`: resolving the configured tree against
//! the content is the build engine's job (§8.4), and what arrives here is
//! already resolved — every entry has a title and a route, and hidden pages are
//! gone. That keeps the sidebar a pure function of a tree, which is what makes
//! previous and next links, ancestor highlighting, and breadcrumbs testable.
// TODO(rfc-0005): `NavCtx` in liyasa-core is an empty frozen stub; these are
// the types the partials actually receive.

use liyasa_core::document::{Block, BlockKind, Inline, Node};
use serde::{Deserialize, Serialize};

/// A resolved navigation tree.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Navigation {
    /// Tabs shown above the sidebar; a site without tabs has one unnamed tab.
    pub tabs: Vec<Tab>,
    pub versions: Vec<Choice>,
    pub locales: Vec<Choice>,
    /// `path`, `eyebrow`, or `none` (CFG-33).
    pub breadcrumbs: Breadcrumbs,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tab {
    pub title: String,
    pub href: Option<String>,
    pub icon: Option<String>,
    pub groups: Vec<Group>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Group {
    pub title: String,
    pub icon: Option<String>,
    /// Collapsed groups still render their items; the sidebar module hides them.
    pub expanded: bool,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Item {
    pub title: String,
    pub route: String,
    pub icon: Option<String>,
    pub tag: Option<String>,
    pub children: Vec<Item>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Choice {
    pub label: String,
    pub value: String,
    pub href: String,
    pub current: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Breadcrumbs {
    #[default]
    Path,
    Eyebrow,
    None,
}

/// One step of the trail above a page title (RX-22).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Crumb {
    pub title: String,
    pub href: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub title: String,
    pub route: String,
}

impl Navigation {
    pub fn with_tab(tab: Tab) -> Self {
        Self {
            tabs: vec![tab],
            ..Self::default()
        }
    }

    /// Every page in navigation order: tabs in order, groups in order, items
    /// depth-first. Previous and next read straight off this (RX-22).
    pub fn order(&self) -> Vec<&Item> {
        let mut out = Vec::new();
        for tab in &self.tabs {
            for group in &tab.groups {
                for item in &group.items {
                    walk(item, &mut out);
                }
            }
        }
        out
    }

    pub fn neighbours(&self, route: &str) -> (Option<Link>, Option<Link>) {
        let order = self.order();
        let Some(at) = order.iter().position(|item| item.route == route) else {
            return (None, None);
        };
        let link = |item: &&Item| Link {
            title: item.title.clone(),
            route: item.route.clone(),
        };
        (
            at.checked_sub(1).and_then(|at| order.get(at)).map(link),
            order.get(at + 1).map(link),
        )
    }

    /// The trail to a page: tab, group, ancestors, page. The page itself is the
    /// last crumb and carries no link.
    pub fn trail(&self, route: &str) -> Vec<Crumb> {
        for tab in &self.tabs {
            for group in &tab.groups {
                for item in &group.items {
                    let mut path = Vec::new();
                    if find(item, route, &mut path) {
                        let mut out = Vec::new();
                        if !tab.title.is_empty() {
                            out.push(Crumb {
                                title: tab.title.clone(),
                                href: tab.href.clone(),
                            });
                        }
                        if !group.title.is_empty() {
                            out.push(Crumb {
                                title: group.title.clone(),
                                href: None,
                            });
                        }
                        let last = path.len() - 1;
                        for (index, step) in path.iter().enumerate() {
                            out.push(Crumb {
                                title: step.title.clone(),
                                href: (index != last).then(|| step.route.clone()),
                            });
                        }
                        return out;
                    }
                }
            }
        }
        Vec::new()
    }

    /// Whether an item is the active page or an ancestor of it (RX-20).
    pub fn is_ancestor(item: &Item, route: &str) -> bool {
        item.children
            .iter()
            .any(|child| child.route == route || Self::is_ancestor(child, route))
    }

    pub fn contains(&self, route: &str) -> bool {
        self.order().iter().any(|item| item.route == route)
    }
}

fn walk<'a>(item: &'a Item, out: &mut Vec<&'a Item>) {
    if !item.route.is_empty() {
        out.push(item);
    }
    for child in &item.children {
        walk(child, out);
    }
}

fn find<'a>(item: &'a Item, route: &str, path: &mut Vec<&'a Item>) -> bool {
    path.push(item);
    if item.route == route {
        return true;
    }
    for child in &item.children {
        if find(child, route, path) {
            return true;
        }
    }
    path.pop();
    false
}

/// One heading in the on-page table of contents (RX-21).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TocEntry {
    pub level: u8,
    pub text: String,
    pub anchor: String,
    pub children: Vec<TocEntry>,
}

/// H2 and H3 by default (RX-21); `depth` is the inclusive level range.
pub fn toc(root: &Block, depth: (u8, u8)) -> Vec<TocEntry> {
    let mut flat = Vec::new();
    collect_headings(root, depth, &mut flat);
    nest(&flat)
}

fn collect_headings(block: &Block, depth: (u8, u8), out: &mut Vec<TocEntry>) {
    if let BlockKind::Heading { level, anchor } = &block.kind
        && (depth.0..=depth.1).contains(level)
    {
        out.push(TocEntry {
            level: *level,
            text: text_of(&block.children),
            anchor: anchor.clone(),
            children: Vec::new(),
        });
    }
    for node in &block.children {
        if let Node::Block(child) = node {
            collect_headings(child, depth, out);
        }
    }
}

/// Nests each entry under the closest preceding entry of a lower level, so a
/// document that jumps from H2 to H4 still produces a tree.
fn nest(flat: &[TocEntry]) -> Vec<TocEntry> {
    fn build(flat: &[TocEntry], at: &mut usize, level: u8) -> Vec<TocEntry> {
        let mut out = Vec::new();
        while let Some(entry) = flat.get(*at) {
            if entry.level <= level {
                break;
            }
            *at += 1;
            let mut node = entry.clone();
            node.children = build(flat, at, entry.level);
            out.push(node);
        }
        out
    }

    build(flat, &mut 0, 0)
}

/// The plain text of a heading, which is what a table of contents shows.
pub fn text_of(nodes: &[Node]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            Node::Inline(inline) => push_inline(inline, &mut out),
            Node::Block(block) => out.push_str(&text_of(&block.children)),
        }
    }
    out.trim().to_owned()
}

fn push_inline(inline: &Inline, out: &mut String) {
    match inline {
        Inline::Text(text) | Inline::Code(text) | Inline::Math(text) => out.push_str(text),
        Inline::Emph(children) | Inline::Strong(children) | Inline::Strike(children) => {
            for child in children {
                push_inline(child, out);
            }
        }
        Inline::Link { children, .. } => {
            for child in children {
                push_inline(child, out);
            }
        }
        Inline::InlineComponent { children, .. } => {
            for child in children {
                push_inline(child, out);
            }
        }
        Inline::Image { alt, .. } => out.push_str(alt),
        Inline::SoftBreak | Inline::HardBreak => out.push(' '),
        Inline::HtmlInline(_) | Inline::FootnoteRef(_) | Inline::TemplateInline { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liyasa_core::document::{Block, BlockKind, Origin};
    use liyasa_core::ids::BlockId;

    fn item(title: &str, route: &str, children: Vec<Item>) -> Item {
        Item {
            title: title.to_owned(),
            route: route.to_owned(),
            children,
            ..Item::default()
        }
    }

    fn navigation() -> Navigation {
        Navigation {
            tabs: vec![
                Tab {
                    title: "Guides".to_owned(),
                    groups: vec![Group {
                        title: "Get started".to_owned(),
                        expanded: true,
                        items: vec![
                            item("Introduction", "/introduction", Vec::new()),
                            item(
                                "Install",
                                "/install",
                                vec![item("Docker", "/install/docker", Vec::new())],
                            ),
                        ],
                        ..Group::default()
                    }],
                    ..Tab::default()
                },
                Tab {
                    title: "API".to_owned(),
                    groups: vec![Group {
                        title: "Reference".to_owned(),
                        items: vec![item("Pages", "/api/pages", Vec::new())],
                        ..Group::default()
                    }],
                    ..Tab::default()
                },
            ],
            ..Navigation::default()
        }
    }

    #[test]
    fn order_runs_depth_first_across_groups_and_tabs() {
        let navigation = navigation();
        let routes: Vec<&str> = navigation
            .order()
            .iter()
            .map(|item| item.route.as_str())
            .collect();
        assert_eq!(
            routes,
            vec!["/introduction", "/install", "/install/docker", "/api/pages"]
        );
    }

    #[test]
    fn previous_and_next_cross_a_tab_boundary() {
        let navigation = navigation();
        let (previous, next) = navigation.neighbours("/install/docker");
        assert_eq!(previous.map(|link| link.route), Some("/install".to_owned()));
        assert_eq!(next.map(|link| link.route), Some("/api/pages".to_owned()));

        let (first_previous, _) = navigation.neighbours("/introduction");
        assert!(first_previous.is_none());
        let (_, last_next) = navigation.neighbours("/api/pages");
        assert!(last_next.is_none());
    }

    #[test]
    fn an_unknown_route_has_no_neighbours() {
        assert_eq!(navigation().neighbours("/nowhere"), (None, None));
    }

    #[test]
    fn the_trail_names_tab_group_and_ancestors() {
        let trail = navigation().trail("/install/docker");
        let titles: Vec<&str> = trail.iter().map(|crumb| crumb.title.as_str()).collect();
        assert_eq!(titles, vec!["Guides", "Get started", "Install", "Docker"]);
        assert_eq!(trail[2].href.as_deref(), Some("/install"));
        assert!(
            trail[3].href.is_none(),
            "the page itself is not a link to itself"
        );
    }

    #[test]
    fn an_ancestor_of_the_active_page_is_marked() {
        let navigation = navigation();
        let install = navigation.order()[1];
        assert!(Navigation::is_ancestor(install, "/install/docker"));
        assert!(!Navigation::is_ancestor(install, "/api/pages"));
    }

    fn heading(level: u8, anchor: &str, text: &str) -> Node {
        Node::Block(Block {
            id: BlockId::explicit(anchor),
            explicit_id: None,
            kind: BlockKind::Heading {
                level,
                anchor: anchor.to_owned(),
            },
            origin: Origin::default(),
            children: vec![Node::Inline(Inline::Text(text.to_owned()))],
        })
    }

    #[test]
    fn the_table_of_contents_takes_h2_and_h3_and_nests_them() {
        let root = Block {
            id: BlockId::explicit("root"),
            explicit_id: None,
            kind: BlockKind::Document,
            origin: Origin::default(),
            children: vec![
                heading(1, "title", "Title"),
                heading(2, "first", "First"),
                heading(3, "detail", "Detail"),
                heading(2, "second", "Second"),
                heading(4, "deep", "Deep"),
            ],
        };
        let toc = toc(&root, (2, 3));
        assert_eq!(toc.len(), 2);
        assert_eq!(toc[0].anchor, "first");
        assert_eq!(toc[0].children.len(), 1);
        assert_eq!(toc[0].children[0].text, "Detail");
        assert_eq!(toc[1].text, "Second");
        assert!(toc[1].children.is_empty(), "H4 is outside the depth range");
    }

    #[test]
    fn heading_text_drops_markup_but_keeps_words() {
        let nodes = vec![
            Node::Inline(Inline::Text("Install ".to_owned())),
            Node::Inline(Inline::Code("liyasa".to_owned())),
            Node::Inline(Inline::Emph(vec![Inline::Text(" now".to_owned())])),
        ];
        assert_eq!(text_of(&nodes), "Install liyasa now");
    }
}
