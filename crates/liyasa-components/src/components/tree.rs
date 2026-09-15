//! The file tree and the inline table of contents (CMP-14, CMP-15).

use liyasa_core::components::{ComponentInst, PropType, RenderError};
use liyasa_core::document::{BlockKind, Node};

use crate::props::Reader;
use crate::render::{HtmlCtx, MarkdownCtx, Render};
use crate::schema::num;
use crate::{declare, text};

declare! {
    /// A file tree written as an indented list (CMP-14).
    pub struct Tree;
    name = "tree";
    aliases = ["Tree", "file-tree", "FileTree"];
    kind = Container;
    editor = ("folder-tree", "Layout");
    props = [
        ("root", PropType::Str, Optional, "Label for the top of the tree, e.g. the repository name."),
        ("active", PropType::Str, Optional, "Path highlighted as the file being described."),
        ("expanded", PropType::Bool, Optional, "Opens every folder."),
    ];
}

/// An entry's own text, without its nested list.
fn entry_label(item: &liyasa_core::document::Block) -> String {
    let own: Vec<Node> = item
        .children
        .iter()
        .filter(|child| !matches!(child, Node::Block(block) if matches!(block.kind, BlockKind::List { .. })))
        .cloned()
        .collect();
    text::of(&own)
}

/// The nested list under an entry, if it has one.
fn entry_children(item: &liyasa_core::document::Block) -> Option<&liyasa_core::document::Block> {
    item.children.iter().find_map(|child| match child {
        Node::Block(block) if matches!(block.kind, BlockKind::List { .. }) => Some(block),
        _ => None,
    })
}

/// A label that names a folder: a trailing `/`, or an entry with children.
fn is_folder(label: &str, has_children: bool) -> bool {
    has_children || label.ends_with('/')
}

impl Tree {
    fn list_html(
        nodes: &[Node],
        ctx: &mut HtmlCtx<'_>,
        active: &str,
        expanded: bool,
        depth: usize,
    ) -> Result<(), RenderError> {
        for node in nodes {
            let Node::Block(list) = node else { continue };
            if !matches!(list.kind, BlockKind::List { .. }) {
                continue;
            }
            ctx.out
                .open("ul")
                .attr("class", "ly-tree-list")
                .attr("role", if depth == 0 { "tree" } else { "group" });
            for child in &list.children {
                let Node::Block(item) = child else { continue };
                if !matches!(item.kind, BlockKind::ListItem { .. }) {
                    continue;
                }
                let label = entry_label(item);
                let nested = entry_children(item);
                let folder = is_folder(&label, nested.is_some());
                ctx.out
                    .open("li")
                    .attr(
                        "class",
                        if folder {
                            "ly-tree-folder"
                        } else {
                            "ly-tree-file"
                        },
                    )
                    .attr("role", "treeitem")
                    .attr("data-path", &label);
                if folder {
                    ctx.out
                        .attr("aria-expanded", if expanded { "true" } else { "false" });
                }
                if !active.is_empty() && label.trim_end_matches('/') == active.trim_end_matches('/')
                {
                    ctx.out.attr("aria-current", "true").attr("data-active", "");
                }
                // Only the entry the reader lands on is in the tab order; the
                // rest are reached with the arrow keys.
                ctx.out
                    .attr("tabindex", if depth == 0 { "0" } else { "-1" })
                    .open("span")
                    .attr("class", "ly-tree-label")
                    .text(label.trim_end_matches('/'))
                    .close();
                if let Some(nested) = nested {
                    Self::list_html(
                        &[Node::Block(nested.clone())],
                        ctx,
                        active,
                        expanded,
                        depth + 1,
                    )?;
                }
                ctx.out.close();
            }
            ctx.out.close();
        }
        Ok(())
    }
}

impl Render for Tree {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let active = props.str_or("active", "").to_owned();
        let expanded = props.bool("expanded");
        ctx.out
            .open("div")
            .attr("class", "ly-tree")
            .attr("data-liyasa", "tree")
            .attr("aria-label", props.str_or("root", "Files"));
        if let Some(root) = props.str("root") {
            ctx.out
                .open("p")
                .attr("class", "ly-tree-root")
                .text(root)
                .close();
        }
        Self::list_html(&inst.children, ctx, &active, expanded, 0)?;
        ctx.out.close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        if let Some(root) = props.str("root") {
            ctx.out
                .paragraph(&format!("**{}**", crate::md::escape_inline(root)));
        }
        // The source was an indented list and a list is what a reader wants.
        ctx.children(&inst.children)
    }
}

declare! {
    /// A table of contents for the page or a subtree of it (CMP-15).
    pub struct Toc;
    name = "toc";
    aliases = ["Toc", "TableOfContents"];
    kind = Leaf;
    editor = ("list", "Layout");
    props = [
        ("depth", PropType::Num, Default(num(3)), "Deepest heading level listed, 1 to 6."),
        ("from", PropType::Route, Optional, "Lists the pages under this route instead of the headings on this page."),
    ];
}

impl Render for Toc {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        // The build fills this in: the headings are not known until the whole
        // page has been parsed.
        ctx.out
            .open("nav")
            .attr("class", "ly-toc")
            .attr("data-liyasa", "toc")
            .attr("aria-label", "Table of contents")
            .attr(
                "data-depth",
                &props.int_in("depth", 1..=6).unwrap_or(3).to_string(),
            )
            .attr_if("data-from", props.url("from"))
            .close();
        Ok(())
    }

    fn markdown(
        &self,
        _inst: &ComponentInst,
        _ctx: &mut MarkdownCtx<'_>,
    ) -> Result<(), RenderError> {
        // An agent has the whole page; a list of links into it adds nothing.
        Ok(())
    }

    fn text(&self, _inst: &ComponentInst) -> String {
        String::new()
    }
}
