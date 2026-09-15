//! The HTML a theme is handed for everything it does not render itself.

use liyasa_core::document::BlockKind;

use super::*;
use crate::directives::testing::*;

/// A theme that renders every component the same way, so the assertions are
/// about the renderer rather than about a theme.
struct Plain {
    fail: bool,
    seen: Vec<String>,
}

impl Blocks for Plain {
    fn component(&mut self, inst: &ComponentInst, children: &str) -> Result<String, RenderError> {
        self.seen.push(inst.name.clone());
        if self.fail {
            return Err(RenderError::Component(inst.name.clone()));
        }
        Ok(format!("<x-{0}>{children}</x-{0}>", inst.name))
    }

    fn code(&mut self, block: &Block, body: &str) -> Result<String, RenderError> {
        let BlockKind::CodeBlock { lang, .. } = &block.kind else {
            return Err(RenderError::Component("code".to_owned()));
        };
        if self.fail {
            return Err(RenderError::Component("code".to_owned()));
        }
        Ok(format!(
            "<x-code lang=\"{}\">{body}</x-code>",
            lang.as_deref().unwrap_or("")
        ))
    }
}

fn html(source: &str) -> String {
    let mut theme = Plain {
        fail: false,
        seen: Vec::new(),
    };
    render(&document(source).root, &mut theme)
}

fn html_without_theme(source: &str) -> String {
    let mut theme = Plain {
        fail: true,
        seen: Vec::new(),
    };
    render(&document(source).root, &mut theme)
}

#[test]
fn prose_and_emphasis() {
    assert_eq!(
        html("A *b* **c** ~~d~~ `e`\n"),
        "<p>A <em>b</em> <strong>c</strong> <del>d</del> <code>e</code></p>\n"
    );
}

#[test]
fn a_heading_carries_its_anchor() {
    assert_eq!(
        html("## Install the CLI\n"),
        "<h2 id=\"install-the-cli\">Install the CLI</h2>\n"
    );
    assert_eq!(
        html("## Install {#custom}\n"),
        "<h2 id=\"custom\">Install</h2>\n"
    );
}

#[test]
fn a_block_with_an_explicit_id_carries_it() {
    assert_eq!(html("para {#here}\n"), "<p id=\"here\">para</p>\n");
}

#[test]
fn lists_and_task_lists() {
    assert_eq!(html("- a\n- b\n"), "<ul>\n<li>a</li>\n<li>b</li>\n</ul>\n");
    assert_eq!(html("3. a\n"), "<ol start=\"3\">\n<li>a</li>\n</ol>\n");
    assert!(html("- [x] done\n").contains("checked"));
}

#[test]
fn a_table_carries_its_alignment() {
    let rendered = html("| a | b |\n|:--|--:|\n| 1 | 2 |\n");
    assert!(rendered.contains("<th align=\"left\">a</th>"), "{rendered}");
    assert!(
        rendered.contains("<th align=\"right\">b</th>"),
        "{rendered}"
    );
    assert!(rendered.contains("<thead>"), "{rendered}");
    assert!(rendered.contains("<tbody>"), "{rendered}");
}

#[test]
fn the_theme_renders_components_and_code() {
    assert_eq!(
        html(":::note\nbody\n:::\n"),
        "<x-note><p>body</p>\n</x-note>"
    );
    assert_eq!(
        html("```rust\nfn main() {}\n```\n"),
        "<x-code lang=\"rust\">fn main() {}\n</x-code>"
    );
    assert_eq!(html("a :kbd[K] b\n"), "<p>a <x-kbd>K</x-kbd> b</p>\n");
}

/// A theme that fails must not take the content with it.
#[test]
fn a_failing_theme_degrades_rather_than_truncates() {
    let rendered = html_without_theme(":::note\nbody\n:::\n");
    assert!(rendered.contains("body"), "{rendered}");
    assert!(rendered.contains("class=\"note\""), "{rendered}");

    let rendered = html_without_theme("```rust\nfn main() {}\n```\n");
    assert!(rendered.contains("fn main()"), "{rendered}");
}

/// The sanitizer ran over the tree already, so what is left is emitted as is.
#[test]
fn sanitized_raw_html_is_emitted_verbatim() {
    assert!(html("<details>\n<summary>s</summary>\n</details>\n").contains("<details>"));
    assert!(!html("<div onclick=\"x\">y</div>\n").contains("onclick"));
}

/// Everything else is escaped, or a page could write its own markup.
#[test]
fn text_is_escaped() {
    assert_eq!(html("a < b & c\n"), "<p>a &lt; b &amp; c</p>\n");
    assert_eq!(html("`<script>`\n"), "<p><code>&lt;script&gt;</code></p>\n");
    assert!(html("## a < b\n").contains("id=\"a-b\""));
}

#[test]
fn a_link_and_an_image_escape_their_attributes() {
    assert_eq!(
        html("[a\"b](/c\"d)\n"),
        "<p><a href=\"/c&quot;d\">a&quot;b</a></p>\n"
    );
    assert_eq!(
        html("![a\"b](/c.png)\n"),
        "<p><img src=\"/c.png\" alt=\"a&quot;b\" /></p>\n"
    );
}

#[test]
fn an_empty_page_renders_to_nothing() {
    assert_eq!(html(""), "");
}
