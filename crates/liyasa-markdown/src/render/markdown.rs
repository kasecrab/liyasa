//! The Markdown serialization (CM-55, §11.7).
//!
//! What an agent is served instead of HTML, and what the formatter writes back
//! to disk. It contains no HTML: a component round-trips as the directive that
//! produced it, because an agent that reads `:::note` can write `:::note`, and
//! one that reads `<div class="note">` can only guess.
//!
//! The fence a container gets is one colon longer than the longest fence inside
//! it, so nesting never needs an escape.

use std::fmt::Write as _;

use std::collections::BTreeMap;

use liyasa_core::document::{Block, BlockKind, Inline, Node, PropValue, Props};
use liyasa_core::markdown::Audience;

use crate::directives::render_value;

pub fn render(root: &Block) -> String {
    let mut out = String::new();
    children(&root.children, &mut out, 0);
    let trimmed = out.trim_end();
    if trimmed.is_empty() {
        return String::new();
    }
    let mut out = format!("{trimmed}\n");
    // CM-40: the definitions are not blocks in the tree, so they are rebuilt
    // from the abbreviations that used them. A page that loses them on a
    // round trip would expand nothing on the next build.
    let definitions = abbreviations(root);
    if !definitions.is_empty() {
        out.push('\n');
        for (abbr, expansion) in definitions {
            let _ = writeln!(out, "*[{abbr}]: {expansion}");
        }
    }
    out
}

/// Every abbreviation the page expanded, by abbreviation.
fn abbreviations(root: &Block) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    collect_abbreviations(&root.children, &mut out);
    out
}

fn collect_abbreviations(nodes: &[Node], out: &mut BTreeMap<String, String>) {
    for node in nodes {
        match node {
            Node::Block(block) => collect_abbreviations(&block.children, out),
            Node::Inline(inline) => collect_abbreviation(inline, out),
        }
    }
}

fn collect_abbreviation(inline: &Inline, out: &mut BTreeMap<String, String>) {
    match inline {
        Inline::InlineComponent {
            name,
            props,
            children,
        } if name == crate::ast::abbr::ABBR => {
            if let Some(PropValue::Str(title)) = props.get("title") {
                out.insert(plain_text(children), title.clone());
            }
        }
        Inline::Emph(children)
        | Inline::Strong(children)
        | Inline::Strike(children)
        | Inline::Link { children, .. }
        | Inline::InlineComponent { children, .. } => {
            for child in children {
                collect_abbreviation(child, out);
            }
        }
        _ => {}
    }
}

fn plain_text(children: &[Inline]) -> String {
    children
        .iter()
        .map(|child| match child {
            Inline::Text(text) => text.clone(),
            _ => String::new(),
        })
        .collect()
}

/// `Audience::Agent` is served the same Markdown; §11.7's extra framing is the
/// serving layer's, not the serializer's, so the bytes an agent reads are the
/// bytes the formatter would write.
pub fn render_for(root: &Block, _audience: Audience) -> String {
    render(root)
}

fn children(nodes: &[Node], out: &mut String, depth: usize) {
    let mut inlines: Vec<&Inline> = Vec::new();
    for node in nodes {
        match node {
            Node::Inline(inline) => inlines.push(inline),
            Node::Block(child) => {
                flush(&mut inlines, out);
                block(child, out, depth);
            }
        }
    }
    flush(&mut inlines, out);
}

fn flush(inlines: &mut Vec<&Inline>, out: &mut String) {
    if inlines.is_empty() {
        return;
    }
    for inline in inlines.drain(..) {
        inline_out(inline, out);
    }
    out.push('\n');
}

fn block(block_: &Block, out: &mut String, depth: usize) {
    separate(out);
    match &block_.kind {
        BlockKind::Document => children(&block_.children, out, depth),
        BlockKind::Heading { level, .. } => {
            let _ = write!(out, "{} ", "#".repeat(*level as usize));
            children(&block_.children, out, depth);
            if let Some(id) = &block_.explicit_id {
                let kept = out.trim_end().len();
                out.truncate(kept);
                let _ = writeln!(out, " {{#{id}}}");
            }
        }
        BlockKind::Paragraph => {
            children(&block_.children, out, depth);
            if let Some(id) = &block_.explicit_id {
                let kept = out.trim_end().len();
                out.truncate(kept);
                let _ = writeln!(out, " {{#{id}}}");
            }
        }
        BlockKind::ThematicBreak => out.push_str("---\n"),
        BlockKind::BlockQuote => {
            let mut inner = String::new();
            children(&block_.children, &mut inner, depth);
            for line in inner.trim_end().lines() {
                let _ = writeln!(out, "> {line}");
            }
        }
        BlockKind::List { ordered, start, .. } => {
            for (at, child) in block_.children.iter().enumerate() {
                let Node::Block(item) = child else { continue };
                let marker = if *ordered {
                    format!("{}. ", *start as usize + at)
                } else {
                    "- ".to_owned()
                };
                list_item(item, &marker, out, depth);
            }
        }
        BlockKind::ListItem { .. } => list_item(block_, "- ", out, depth),
        BlockKind::CodeBlock { lang, attrs, .. } => {
            let mut info = lang.clone().unwrap_or_default();
            for flag in &attrs.flags {
                let _ = write!(info, " {flag}");
            }
            for (key, value) in &attrs.kv {
                let _ = write!(info, " {key}=\"{value}\"");
            }
            if !attrs.highlight.is_empty() {
                let ranges: Vec<String> = attrs
                    .highlight
                    .iter()
                    .map(|(first, last)| {
                        if first == last {
                            first.to_string()
                        } else {
                            format!("{first}-{last}")
                        }
                    })
                    .collect();
                let _ = write!(info, " {{{}}}", ranges.join(","));
            }
            let body = code_of(block_);
            let fence = "`".repeat(3.max(longest_run(&body, '`') + 1));
            let _ = writeln!(out, "{fence}{}", info.trim());
            out.push_str(&body);
            if !body.ends_with('\n') && !body.is_empty() {
                out.push('\n');
            }
            let _ = writeln!(out, "{fence}");
        }
        BlockKind::Math { display, src } => {
            if *display {
                let _ = writeln!(out, "$$\n{}\n$$", src.trim());
            } else {
                let _ = writeln!(out, "${}$", src.trim());
            }
        }
        BlockKind::FootnoteDefinition { label } => {
            let mut inner = String::new();
            children(&block_.children, &mut inner, depth);
            let _ = write!(out, "[^{label}]: ");
            out.push_str(inner.trim_start());
        }
        BlockKind::Table { align } => table(block_, align, out),
        BlockKind::TableRow { .. } | BlockKind::TableCell => {
            children(&block_.children, out, depth);
        }
        // CM-55: a component is served as the directive that made it, never as
        // the HTML a theme would render.
        BlockKind::Component { name, props, slots } => {
            let mut inner = String::new();
            children(&block_.children, &mut inner, depth + 1);
            for (slot, nodes) in &slots.0 {
                separate(&mut inner);
                let mut body = String::new();
                children(nodes, &mut body, depth + 1);
                let fence = fence_for(&body);
                let _ = writeln!(inner, "{fence}slot{{name=\"{slot}\"}}");
                inner.push_str(body.trim_end());
                let _ = writeln!(inner, "\n{fence}");
            }
            if inner.trim().is_empty() {
                let _ = writeln!(out, "::{name}{}", props_of(props));
                return;
            }
            let fence = fence_for(&inner);
            let _ = writeln!(out, "{fence}{name}{}", props_of(props));
            out.push_str(inner.trim_end());
            let _ = writeln!(out, "\n{fence}");
        }
        // Raw HTML has no Markdown form; CM-55 requires there be no HTML in the
        // output, so it is dropped rather than passed through.
        BlockKind::HtmlBlock { .. } | BlockKind::LogicMarker { .. } => {}
    }
}

fn list_item(item: &Block, marker: &str, out: &mut String, depth: usize) {
    let mut inner = String::new();
    children(&item.children, &mut inner, depth + 1);
    let checkbox = match item.kind {
        BlockKind::ListItem {
            checked: Some(true),
        } => "[x] ",
        BlockKind::ListItem {
            checked: Some(false),
        } => "[ ] ",
        _ => "",
    };
    let indent = " ".repeat(marker.len());
    for (at, line) in inner.trim_end().lines().enumerate() {
        if at == 0 {
            let _ = writeln!(out, "{marker}{checkbox}{line}");
        } else if line.is_empty() {
            out.push('\n');
        } else {
            let _ = writeln!(out, "{indent}{line}");
        }
    }
}

fn table(block_: &Block, align: &[liyasa_core::document::Align], out: &mut String) {
    use liyasa_core::document::Align;
    for child in &block_.children {
        let Node::Block(row) = child else { continue };
        let BlockKind::TableRow { header } = row.kind else {
            continue;
        };
        let cells: Vec<String> = row
            .children
            .iter()
            .map(|cell| {
                let mut text = String::new();
                if let Node::Block(cell) = cell {
                    children(&cell.children, &mut text, 0);
                }
                text.trim().to_owned()
            })
            .collect();
        let _ = writeln!(out, "| {} |", cells.join(" | "));
        if header {
            let rules: Vec<&str> = (0..cells.len())
                .map(|at| match align.get(at) {
                    Some(Align::Left) => ":---",
                    Some(Align::Center) => ":---:",
                    Some(Align::Right) => "---:",
                    _ => "---",
                })
                .collect();
            let _ = writeln!(out, "| {} |", rules.join(" | "));
        }
    }
}

fn inline_out(inline: &Inline, out: &mut String) {
    match inline {
        Inline::Text(text) => out.push_str(text),
        Inline::Emph(children) => wrap("*", children, out),
        Inline::Strong(children) => wrap("**", children, out),
        Inline::Strike(children) => wrap("~~", children, out),
        Inline::Code(text) => {
            let fence = "`".repeat(longest_run(text, '`') + 1);
            let pad = if text.starts_with('`') || text.ends_with('`') {
                " "
            } else {
                ""
            };
            let _ = write!(out, "{fence}{pad}{text}{pad}{fence}");
        }
        Inline::Link {
            href,
            title,
            children,
            ..
        } => {
            out.push('[');
            for child in children {
                inline_out(child, out);
            }
            match title {
                Some(title) => {
                    let _ = write!(out, "]({href} \"{title}\")");
                }
                None => {
                    let _ = write!(out, "]({href})");
                }
            }
        }
        Inline::Image {
            src, alt, title, ..
        } => match title {
            Some(title) => {
                let _ = write!(out, "![{alt}]({src} \"{title}\")");
            }
            None => {
                let _ = write!(out, "![{alt}]({src})");
            }
        },
        Inline::FootnoteRef(label) => {
            let _ = write!(out, "[^{label}]");
        }
        Inline::SoftBreak => out.push('\n'),
        Inline::HardBreak => out.push_str("\\\n"),
        Inline::Math(src) => {
            let _ = write!(out, "${src}$");
        }
        // CM-40: an expanded abbreviation is written as the word the author
        // typed; the definition is appended once at the end of the page.
        Inline::InlineComponent { name, children, .. } if name == crate::ast::abbr::ABBR => {
            for child in children {
                inline_out(child, out);
            }
        }
        Inline::InlineComponent {
            name,
            props,
            children,
        } => {
            let _ = write!(out, ":{name}[");
            for child in children {
                inline_out(child, out);
            }
            let _ = write!(out, "]{}", props_of(props));
        }
        // No HTML reaches an agent, and a template fragment is not content.
        Inline::HtmlInline(_) | Inline::TemplateInline { .. } => {}
    }
}

fn wrap(marker: &str, children: &[Inline], out: &mut String) {
    out.push_str(marker);
    for child in children {
        inline_out(child, out);
    }
    out.push_str(marker);
}

fn props_of(props: &Props) -> String {
    if props.is_empty() {
        return String::new();
    }
    let written: Vec<String> = props
        .0
        .iter()
        .map(|(name, value)| match (name.as_str(), value) {
            ("class", liyasa_core::document::PropValue::Str(classes)) => classes
                .split_whitespace()
                .map(|class| format!(".{class}"))
                .collect::<Vec<_>>()
                .join(" "),
            ("id", liyasa_core::document::PropValue::Str(id)) => format!("#{id}"),
            _ => format!("{name}={}", render_value(value)),
        })
        .collect();
    format!("{{{}}}", written.join(" "))
}

/// One colon longer than the longest run already inside, so nesting never has
/// to be escaped.
fn fence_for(body: &str) -> String {
    ":".repeat(3.max(longest_line_run(body, ':') + 1))
}

fn longest_run(text: &str, ch: char) -> usize {
    let mut best = 0;
    let mut run = 0;
    for seen in text.chars() {
        run = if seen == ch { run + 1 } else { 0 };
        best = best.max(run);
    }
    best
}

/// The longest run of `ch` that opens a line, which is what a fence has to beat.
fn longest_line_run(text: &str, ch: char) -> usize {
    text.lines()
        .map(|line| line.trim_start().chars().take_while(|c| *c == ch).count())
        .max()
        .unwrap_or(0)
}

fn code_of(block: &Block) -> String {
    let mut out = String::new();
    for child in &block.children {
        match child {
            Node::Inline(Inline::Text(text) | Inline::Code(text)) => out.push_str(text),
            Node::Inline(Inline::SoftBreak | Inline::HardBreak) => out.push('\n'),
            _ => {}
        }
    }
    out
}

fn separate(out: &mut String) {
    if out.is_empty() {
        return;
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    if !out.ends_with("\n\n") {
        out.push('\n');
    }
}

#[cfg(test)]
mod tests;
