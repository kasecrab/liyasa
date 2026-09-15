//! The HTML sink every component writes into.
//!
//! Escaping is not optional: `text` and `attr` are the only ways to put
//! author-controlled bytes into the output, and both escape. `raw` exists for
//! fragments a component has already produced (a child's rendered HTML) and is
//! the one place a reviewer has to look.

use std::fmt::Write as _;

/// Elements with no closing tag.
const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];

#[derive(Debug, Default)]
pub struct Html {
    buf: String,
    stack: Vec<&'static str>,
    /// Inside a start tag, between the name and the `>`.
    in_tag: bool,
}

impl Html {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(bytes: usize) -> Self {
        Self {
            buf: String::with_capacity(bytes),
            ..Self::default()
        }
    }

    /// Opens a start tag and enters attribute position.
    pub fn open(&mut self, tag: &'static str) -> &mut Self {
        self.finish_tag();
        self.buf.push('<');
        self.buf.push_str(tag);
        self.in_tag = true;
        if !VOID.contains(&tag) {
            self.stack.push(tag);
        }
        self
    }

    /// Writes `name="value"`, escaped. Ignored outside attribute position so a
    /// misordered call cannot inject markup into text.
    pub fn attr(&mut self, name: &str, value: &str) -> &mut Self {
        if self.in_tag {
            self.buf.push(' ');
            self.buf.push_str(name);
            self.buf.push_str("=\"");
            escape_attr(value, &mut self.buf);
            self.buf.push('"');
        }
        self
    }

    pub fn attr_if(&mut self, name: &str, value: Option<&str>) -> &mut Self {
        match value {
            Some(value) => self.attr(name, value),
            None => self,
        }
    }

    /// A valueless attribute such as `open` or `muted`.
    pub fn flag(&mut self, name: &str) -> &mut Self {
        if self.in_tag {
            self.buf.push(' ');
            self.buf.push_str(name);
        }
        self
    }

    pub fn flag_if(&mut self, name: &str, present: bool) -> &mut Self {
        if present { self.flag(name) } else { self }
    }

    pub fn text(&mut self, text: &str) -> &mut Self {
        self.finish_tag();
        escape_text(text, &mut self.buf);
        self
    }

    /// Already-escaped markup: a child's rendered HTML, or a highlighted code
    /// body. Never author input.
    pub fn raw(&mut self, markup: &str) -> &mut Self {
        self.finish_tag();
        self.buf.push_str(markup);
        self
    }

    pub fn newline(&mut self) -> &mut Self {
        self.finish_tag();
        if !self.buf.ends_with('\n') && !self.buf.is_empty() {
            self.buf.push('\n');
        }
        self
    }

    /// Closes the innermost open element.
    pub fn close(&mut self) -> &mut Self {
        self.finish_tag();
        if let Some(tag) = self.stack.pop() {
            let _ = write!(self.buf, "</{tag}>");
        }
        self
    }

    /// Closes every element opened since `depth`, innermost first.
    pub fn close_to(&mut self, depth: usize) -> &mut Self {
        while self.stack.len() > depth {
            self.close();
        }
        self
    }

    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// `<tag …>text</tag>` in one call.
    pub fn element(&mut self, tag: &'static str, text: &str) -> &mut Self {
        self.open(tag).text(text).close()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn as_str(&mut self) -> &str {
        self.finish_tag();
        &self.buf
    }

    /// Closes anything still open and returns the markup.
    pub fn finish(mut self) -> String {
        self.close_to(0);
        self.finish_tag();
        self.buf
    }

    fn finish_tag(&mut self) {
        if self.in_tag {
            self.buf.push('>');
            self.in_tag = false;
        }
    }
}

pub fn escape_text(text: &str, out: &mut String) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
}

pub fn escape_attr(value: &str, out: &mut String) {
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
}

pub fn escaped_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    escape_text(text, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attributes_close_before_content() {
        let mut html = Html::new();
        html.open("div").attr("class", "ly-note").text("hi").close();
        assert_eq!(html.finish(), r#"<div class="ly-note">hi</div>"#);
    }

    #[test]
    fn text_cannot_open_an_element() {
        let mut html = Html::new();
        html.open("p").text("<script>alert(1)</script>").close();
        assert_eq!(
            html.finish(),
            "<p>&lt;script&gt;alert(1)&lt;/script&gt;</p>"
        );
    }

    #[test]
    fn attribute_values_cannot_escape_their_quotes() {
        let mut html = Html::new();
        html.open("a").attr("title", r#"" onclick="x"#).close();
        assert_eq!(html.finish(), r#"<a title="&quot; onclick=&quot;x"></a>"#);
    }

    #[test]
    fn single_quotes_are_escaped_too() {
        let mut html = Html::new();
        html.open("a").attr("title", "it's").close();
        assert_eq!(html.finish(), r#"<a title="it&#39;s"></a>"#);
    }

    #[test]
    fn void_elements_are_never_closed() {
        let mut html = Html::new();
        html.open("img").attr("src", "a.png").attr("alt", "");
        assert_eq!(html.finish(), r#"<img src="a.png" alt="">"#);
    }

    #[test]
    fn an_attribute_after_content_is_dropped() {
        let mut html = Html::new();
        html.open("div").text("x").attr("class", "late").close();
        assert_eq!(html.finish(), "<div>x</div>");
    }

    #[test]
    fn finish_closes_what_is_still_open() {
        let mut html = Html::new();
        html.open("section").open("div").text("x");
        assert_eq!(html.finish(), "<section><div>x</div></section>");
    }

    #[test]
    fn close_to_unwinds_to_a_depth() {
        let mut html = Html::new();
        html.open("ul");
        let depth = html.depth();
        html.open("li").open("span").text("x");
        html.close_to(depth);
        html.open("li").text("y");
        assert_eq!(html.finish(), "<ul><li><span>x</span></li><li>y</li></ul>");
    }

    #[test]
    fn flags_have_no_value() {
        let mut html = Html::new();
        html.open("details").flag("open").flag_if("hidden", false);
        assert_eq!(html.finish(), "<details open></details>");
    }

    #[test]
    fn raw_is_not_escaped() {
        let mut html = Html::new();
        html.open("div").raw("<em>child</em>").close();
        assert_eq!(html.finish(), "<div><em>child</em></div>");
    }

    #[test]
    fn newline_never_doubles() {
        let mut html = Html::new();
        html.open("div").close().newline().newline();
        assert_eq!(html.finish(), "<div></div>\n");
    }
}
