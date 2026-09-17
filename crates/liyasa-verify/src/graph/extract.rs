//! Every edge one page contributes to the truth graph (§14.12).
//!
//! `DependencyExtractor::extract` is handed a document and an expansion record
//! and no page, but `EdgeOrigin` names one, so the page is a field: build one
//! extractor per page. `liyasa-components` has the same problem in
//! `Component::deps` and solves it by leaving a nil `PageId` behind for "the
//! build" to rewrite; this is where that rewrite happens.

use liyasa_core::document::{
    Block, BlockKind, DepTarget, Document, Edge, EdgeKind, EdgeOrigin, Frame, Inline, Node,
};
use liyasa_core::ids::{PageId, Route};
use liyasa_core::markdown::ExpansionRecord;
use liyasa_core::verify::DependencyExtractor;

pub struct PageExtractor {
    page: PageId,
}

impl PageExtractor {
    pub fn for_page(page: PageId) -> Self {
        Self { page }
    }
}

impl DependencyExtractor for PageExtractor {
    fn extract(&self, doc: &Document, expansion: &ExpansionRecord) -> Vec<Edge> {
        let page = EdgeOrigin::Page(self.page);
        let mut edges = Vec::new();

        // The record is per page, not per block: a fact read inside a template
        // expression has no AST node to hang an edge on, so the page reads it.
        for fact in &expansion.facts {
            edges.push(Edge {
                from: page.clone(),
                to: DepTarget::Fact(fact.clone()),
                kind: EdgeKind::Reads,
            });
        }
        for include in &expansion.includes {
            edges.push(Edge {
                from: page.clone(),
                to: DepTarget::Snippet(*include),
                kind: EdgeKind::Includes,
            });
        }

        self.block(&doc.root, &mut edges);
        edges.extend(doc.deps.0.iter().map(|edge| self.rebase(edge)));
        normalize(edges)
    }
}

impl PageExtractor {
    fn block(&self, block: &Block, out: &mut Vec<Edge>) {
        let from = EdgeOrigin::Block(self.page, block.id);
        for frame in &block.origin.frames {
            if let Frame::Include { file, .. } = frame {
                out.push(Edge {
                    from: from.clone(),
                    to: DepTarget::Snippet(*file),
                    kind: EdgeKind::Includes,
                });
            }
        }
        if let BlockKind::Component { name, slots, .. } = &block.kind {
            out.push(Edge {
                from: from.clone(),
                to: DepTarget::Component(name.clone()),
                kind: EdgeKind::Documents,
            });
            for node in slots.0.values().flatten() {
                self.node(node, &from, out);
            }
        }
        for child in &block.children {
            self.node(child, &from, out);
        }
    }

    fn node(&self, node: &Node, from: &EdgeOrigin, out: &mut Vec<Edge>) {
        match node {
            Node::Block(block) => self.block(block, out),
            Node::Inline(inline) => self.inline(inline, from, out),
        }
    }

    fn inline(&self, inline: &Inline, from: &EdgeOrigin, out: &mut Vec<Edge>) {
        match inline {
            Inline::Link {
                href,
                children,
                resolved,
                ..
            } => {
                if let Some(to) = link_target(href, resolved.as_ref()) {
                    out.push(Edge {
                        from: from.clone(),
                        to,
                        kind: EdgeKind::Links,
                    });
                }
                self.inlines(children, from, out);
            }
            Inline::Image { src, dark, .. } => {
                for value in [Some(src), dark.as_ref()].into_iter().flatten() {
                    if let Some(to) = asset_target(value) {
                        out.push(Edge {
                            from: from.clone(),
                            to,
                            kind: EdgeKind::Embeds,
                        });
                    }
                }
            }
            Inline::InlineComponent { name, children, .. } => {
                out.push(Edge {
                    from: from.clone(),
                    to: DepTarget::Component(name.clone()),
                    kind: EdgeKind::Documents,
                });
                self.inlines(children, from, out);
            }
            Inline::Emph(children) | Inline::Strong(children) | Inline::Strike(children) => {
                self.inlines(children, from, out);
            }
            _ => {}
        }
    }

    fn inlines(&self, inlines: &[Inline], from: &EdgeOrigin, out: &mut Vec<Edge>) {
        for inline in inlines {
            self.inline(inline, from, out);
        }
    }

    /// `liyasa-components` mints its edges before it knows which page it is on
    /// and leaves a nil `PageId` in the origin. Anything else is left alone, so
    /// running this twice changes nothing.
    fn rebase(&self, edge: &Edge) -> Edge {
        let nil = PageId(ulid::Ulid::nil());
        let from = match &edge.from {
            EdgeOrigin::Block(page, block) if *page == nil => EdgeOrigin::Block(self.page, *block),
            EdgeOrigin::Page(page) if *page == nil => EdgeOrigin::Page(self.page),
            other => other.clone(),
        };
        Edge {
            from,
            to: edge.to.clone(),
            kind: edge.kind,
        }
    }
}

/// The scheme of a URL-ish string, by RFC 3986's rule: letter first, then
/// letters, digits, `+`, `-`, and `.`, up to the first colon. `/a:b` and
/// `./x.md` have none.
fn scheme_of(value: &str) -> Option<&str> {
    let end = value.find(':')?;
    let scheme = &value[..end];
    let mut chars = scheme.chars();
    let first = chars.next()?;
    (first.is_ascii_alphabetic()
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')))
    .then_some(scheme)
}

fn link_target(href: &str, resolved: Option<&Route>) -> Option<DepTarget> {
    if let Some(route) = resolved {
        return Some(DepTarget::Page(route.clone()));
    }
    match scheme_of(href) {
        Some("http" | "https") => Some(DepTarget::ExternalUrl(href.to_owned())),
        // `mailto:`, `tel:`, `data:`: nothing the graph can invalidate on.
        Some(_) => None,
        None => Some(DepTarget::Page(Route::new(route_part(href)?))),
    }
}

fn asset_target(value: &str) -> Option<DepTarget> {
    match scheme_of(value) {
        Some("http" | "https") => Some(DepTarget::ExternalUrl(value.to_owned())),
        Some(_) => None,
        None => (!value.is_empty()).then(|| DepTarget::Asset(value.to_owned())),
    }
}

/// The page half of an unresolved internal link. A fragment or a query names a
/// position on a page, not a page, and a link that is nothing but one points at
/// the page it is written on, which is not a dependency.
fn route_part(href: &str) -> Option<&str> {
    let end = href.find(['#', '?']).unwrap_or(href.len());
    (end > 0).then_some(&href[..end])
}

fn normalize(mut edges: Vec<Edge>) -> Vec<Edge> {
    edges.sort_by(|a, b| (&a.from, &a.to, &a.kind).cmp(&(&b.from, &b.to, &b.kind)));
    edges.dedup();
    edges
}

#[cfg(test)]
mod tests;
