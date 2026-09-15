//! The HTML sanitizer (PRD §7.5.1 item 3, CM-32).
//!
//! One pass over the Rendered AST, so every syntax that can produce a URL or a
//! tag — raw HTML, a Markdown link, an image, a component prop — is filtered by
//! the same rules. Putting it after the parser rather than inside it is what
//! makes that possible: by this point there is no syntax left, only nodes.
//!
//! The pass is a deny-by-default allow list. Anything the list does not name is
//! removed and reported, so a new HTML feature is inert until someone adds it.

pub mod allowlist;
pub mod html;
pub mod style;
pub mod url;

use liyasa_core::diagnostics::code;
use liyasa_core::document::{Block, BlockKind, Inline, Node};
use liyasa_core::markdown::HtmlMode;
use liyasa_core::{Diagnostic, Diagnostics, Span};

/// Elements whose content is not markup and must go with them.
const DROP_CONTENT: &[&str] = &["script", "style", "iframe", "object", "embed", "template"];

pub fn run(root: &mut Block, mode: HtmlMode, diagnostics: &mut Diagnostics) {
    let mut pass = Pass {
        mode,
        diagnostics,
        span: None,
    };
    pass.block(root);
}

struct Pass<'a> {
    mode: HtmlMode,
    diagnostics: &'a mut Diagnostics,
    span: Option<Span>,
}

impl Pass<'_> {
    fn block(&mut self, block: &mut Block) {
        let inner = block.origin.span.or(self.span);
        let outer = std::mem::replace(&mut self.span, inner);
        if let BlockKind::HtmlBlock { html } = &mut block.kind {
            *html = self.filter(html);
        }
        for child in &mut block.children {
            match child {
                Node::Block(child) => self.block(child),
                Node::Inline(child) => self.inline(child),
            }
        }
        self.span = outer;
    }

    fn inline(&mut self, inline: &mut Inline) {
        match inline {
            Inline::HtmlInline(html) => *html = self.filter(html),
            Inline::Link { href, children, .. } => {
                if !url::allowed(href, false) {
                    self.rejected(code::E0304, format!("link to `{href}` was removed"));
                    href.clear();
                }
                for child in children {
                    self.inline(child);
                }
            }
            Inline::Image { src, .. } => {
                if !url::allowed(src, true) {
                    self.rejected(code::E0304, format!("image at `{src}` was removed"));
                    src.clear();
                }
            }
            Inline::Emph(children)
            | Inline::Strong(children)
            | Inline::Strike(children)
            | Inline::InlineComponent { children, .. } => {
                for child in children {
                    self.inline(child);
                }
            }
            _ => {}
        }
    }

    /// Raw HTML, filtered or removed.
    fn filter(&mut self, raw: &str) -> String {
        if self.mode == HtmlMode::Off {
            if !raw.trim().is_empty() {
                self.rejected(code::E0303, "raw HTML is off for this site");
            }
            return String::new();
        }

        let mut out = String::with_capacity(raw.len());
        let mut dropping: Option<String> = None;
        for token in html::tokenize(raw) {
            match token {
                Token::Text(text) if dropping.is_none() => out.push_str(text),
                Token::Bogus(text) if dropping.is_none() => {
                    // A comment is inert, but it is also where a `<!--[if IE]>`
                    // conditional hides markup, so it does not survive.
                    if text.starts_with("<!--") {
                        continue;
                    }
                    self.rejected(code::E0304, format!("`{}` was removed", first_line(text)));
                }
                Token::Tag(tag) => self.tag(&tag, &mut out, &mut dropping),
                _ => {}
            }
        }
        out
    }

    fn tag(&mut self, tag: &html::Tag<'_>, out: &mut String, dropping: &mut Option<String>) {
        if let Some(open) = dropping.clone() {
            if tag.closing && tag.name == open {
                *dropping = None;
            }
            return;
        }
        if DROP_CONTENT.contains(&tag.name.as_str()) {
            self.rejected(
                code::E0304,
                format!("`<{}>` is not allowed and was removed", tag.name),
            );
            if !tag.closing && !tag.self_closing {
                *dropping = Some(tag.name.clone());
            }
            return;
        }
        if !allowlist::element_allowed(&tag.name) {
            self.rejected(
                code::E0304,
                format!("`<{}>` is not allowed and was removed", tag.name),
            );
            return;
        }
        if tag.closing {
            out.push_str(&format!("</{}>", tag.name));
            return;
        }

        let mut rendered = format!("<{}", tag.name);
        for (name, value) in &tag.attributes {
            let Some(kept) = self.attribute(&tag.name, name, *value) else {
                continue;
            };
            match kept {
                Some(value) => {
                    rendered.push_str(&format!(" {name}=\"{}\"", html::escape_attribute(&value)));
                }
                None => rendered.push_str(&format!(" {name}")),
            }
        }
        if tag.self_closing {
            rendered.push_str(" /");
        }
        rendered.push('>');
        out.push_str(&rendered);
    }

    /// `None` to drop the attribute, `Some(value)` to keep it.
    fn attribute(
        &mut self,
        element: &str,
        name: &str,
        value: Option<&str>,
    ) -> Option<Option<String>> {
        if allowlist::is_event_handler(name) {
            self.rejected(
                code::E0304,
                format!("`{name}` is an inline event handler and was removed"),
            );
            return None;
        }
        if name == "style" {
            // TODO(CM-32): `security.styleAttribute: "off"` is a config key
            // WP-01 owns; until `ParseOptions` carries it the default
            // `"allowlist"` is what runs.
            return match value.and_then(style::filter) {
                Some(kept) => Some(Some(kept)),
                None => {
                    self.rejected(code::E0304, "the `style` attribute was removed");
                    None
                }
            };
        }
        if !allowlist::attribute_allowed(element, name) {
            self.rejected(
                code::E0304,
                format!("`{name}` is not allowed on `<{element}>` and was removed"),
            );
            return None;
        }
        if allowlist::URL_ATTRIBUTES.contains(&name)
            && let Some(value) = value
            && !url::allowed(value, element == "img" || element == "source")
        {
            self.rejected(
                code::E0304,
                format!("`{name}=\"{value}\"` uses a scheme that was removed"),
            );
            return None;
        }
        Some(value.map(str::to_owned))
    }

    fn rejected(&mut self, code: liyasa_core::Code, message: impl Into<String>) {
        let diagnostic = Diagnostic::new(code, message);
        self.diagnostics.push(match self.span {
            Some(span) => diagnostic.at(span),
            None => diagnostic,
        });
    }
}

use html::Token;

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or(text)
}

#[cfg(test)]
mod tests;
