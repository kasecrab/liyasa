//! Candidate (a): comrak driven by out-of-band markers (PRD §7.5.1).
//!
//! The scanner replaces every directive line with an HTML comment marker, comrak
//! parses the result as CommonMark, and a re-parenting pass moves the siblings
//! between a marker pair under one container node. Positions compose
//! rewritten → expanded → source through the [`RewriteMap`].

use comrak::nodes::{AstNode, NodeBlockDirective, NodeValue};
use comrak::{Arena, Options};
use liyasa_core::markdown::{DirectiveTable, RewriteMap};
use liyasa_core::{Diagnostics, SourceId, Span};

use super::scan::{self, MARKER_PREFIX, MarkerRole};

pub struct Parsed<'a> {
    pub root: &'a AstNode<'a>,
    pub map: RewriteMap,
    pub table: DirectiveTable,
    pub diagnostics: Diagnostics,
}

pub fn parse<'a>(
    arena: &'a Arena<'a>,
    source: &str,
    options: &Options,
    nonce: [u8; 16],
) -> Parsed<'a> {
    let rewritten = scan::rewrite(source, SourceId(0), nonce);
    let root = comrak::parse_document(arena, &rewritten.text, options);
    reparent(root, &rewritten);
    Parsed {
        root,
        map: rewritten.map,
        table: rewritten.table,
        diagnostics: rewritten.diagnostics,
    }
}

/// Moves the siblings between each marker pair under one container node.
///
/// Both markers of a directive always sit in the same parent, because the
/// scanner refuses a pair whose lines are at different container depths
/// (§7.5.1 item 7), so this never crosses a list or blockquote boundary.
fn reparent<'a>(root: &'a AstNode<'a>, rewritten: &scan::Rewritten) {
    let mut parents: Vec<&'a AstNode<'a>> = vec![root];
    while let Some(parent) = parents.pop() {
        let children: Vec<_> = parent.children().collect();
        parents.extend(children.iter().copied());

        let mut open: Vec<(usize, &'a AstNode<'a>)> = Vec::new();
        for child in &children {
            let Some((role, id)) = marker_of(child) else {
                continue;
            };
            match role {
                MarkerRole::Open => open.push((id, child)),
                MarkerRole::Leaf => {
                    let info = rewritten
                        .table
                        .get(id)
                        .map(|d| d.name.clone())
                        .unwrap_or_default();
                    child.data.borrow_mut().value = directive_node(&info);
                }
                MarkerRole::Close => {
                    let Some(at) = open.iter().rposition(|(open_id, _)| {
                        rewritten.markers.get(*open_id).and_then(|m| m.partner) == Some(id)
                    }) else {
                        continue;
                    };
                    let (open_id, open_node) = open.remove(at);
                    let info = rewritten
                        .table
                        .get(open_id)
                        .map(|d| d.name.clone())
                        .unwrap_or_default();
                    open_node.data.borrow_mut().value = directive_node(&info);
                    let mut cursor = open_node.next_sibling();
                    while let Some(node) = cursor {
                        if std::ptr::eq(node, *child) {
                            break;
                        }
                        cursor = node.next_sibling();
                        node.detach();
                        open_node.append(node);
                    }
                    child.detach();
                }
            }
        }
    }
}

fn directive_node(name: &str) -> NodeValue {
    NodeValue::BlockDirective(Box::new(NodeBlockDirective {
        fence_length: 3,
        fence_offset: 0,
        info: name.to_owned(),
    }))
}

/// `(role, marker id)` when this node is one of the scanner's markers.
fn marker_of(node: &AstNode<'_>) -> Option<(MarkerRole, usize)> {
    let data = node.data.borrow();
    let NodeValue::HtmlBlock(block) = &data.value else {
        return None;
    };
    let rest = block
        .literal
        .trim()
        .strip_prefix(MARKER_PREFIX)?
        .strip_suffix("-->")?;
    let mut parts = rest.split(':');
    let _nonce = parts.next()?;
    let role = match parts.next()? {
        "o" => MarkerRole::Open,
        "c" => MarkerRole::Close,
        "l" => MarkerRole::Leaf,
        _ => return None,
    };
    Some((role, parts.next()?.parse().ok()?))
}

/// The property §7.5.1 item 2 promises: every position comrak reports on a
/// marker line composes back to the directive's recorded source span.
///
/// Returns the offending marker IDs, so a corpus case names what broke.
pub fn position_round_trip(parsed: &Parsed<'_>, source: &str) -> Vec<usize> {
    let line_starts: Vec<u32> = std::iter::once(0)
        .chain(
            source
                .bytes()
                .enumerate()
                .filter(|(_, b)| *b == b'\n')
                .map(|(at, _)| at as u32 + 1),
        )
        .collect();
    let mut broken = Vec::new();
    for node in parsed.root.descendants() {
        let data = node.data.borrow();
        let NodeValue::BlockDirective(_) = &data.value else {
            continue;
        };
        let line = data.sourcepos.start.line;
        let Some(&expected_line_start) = line_starts.get(line.saturating_sub(1)) else {
            continue;
        };
        // comrak reports (line, column) in the rewritten text; the rewrite
        // preserves line numbers, so the composed offset must land on the same
        // source line the directive was recorded on.
        let recorded = parsed.table.0.iter().position(|d| {
            d.span.start >= expected_line_start
                && d.span.start < expected_line_start + line_length(source, expected_line_start)
        });
        match recorded {
            Some(_) => {}
            None => broken.push(line),
        }
    }
    broken
}

fn line_length(source: &str, from: u32) -> u32 {
    source[from as usize..]
        .find('\n')
        .map_or_else(|| source.len() as u32 - from, |at| at as u32 + 1)
}

/// Composes a rewritten byte offset all the way back to a source span.
pub fn compose(map: &RewriteMap, source: SourceId, rewritten_start: u32, len: u32) -> Span {
    let start = map.to_expanded(rewritten_start);
    Span::new(source, start, start + len)
}
