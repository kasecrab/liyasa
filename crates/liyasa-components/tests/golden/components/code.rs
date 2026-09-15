//! CMP-30 to CMP-34 and CMP-60: fences, code groups, inline code, terminals,
//! snippets, and Mermaid.

use liyasa_components::{inst, nodes};
use liyasa_core::document::{FenceAttrs, Inline, Node, PropValue};

use crate::support::Gallery;

fn str(value: &str) -> PropValue {
    PropValue::Str(value.to_owned())
}

fn fenced(info: &str, body: &str) -> Node {
    let (parsed, _) = liyasa_components::fence::parse_info(info);
    nodes::code_block_with(parsed.lang.as_deref(), body, parsed.attrs)
}

#[test]
fn fenced_code_attributes() {
    Gallery::new("code-block")
        .case(
            "plain",
            inst::new("terminal")
                .prop("title", str("plain"))
                .child(fenced("rust", "fn main() {}\n"))
                .build(),
        )
        .case(
            "every attribute",
            inst::new("terminal")
                .prop("title", str("every attribute"))
                .child(fenced(
                    r#"rust title="main.rs" icon="rust" {1,3} focus={2} lines start=10 wrap expandable maxLines=20"#,
                    "let a = 1;\nlet b = 2;\nlet c = 3;\n",
                ))
                .build(),
        )
        .case(
            "diff",
            inst::new("terminal")
                .prop("title", str("diff"))
                .child(fenced("diff lines", "-let a = 1;\n+let a = 2;\n let b = 3;\n"))
                .build(),
        )
        .case(
            "prompt and no copy",
            inst::new("terminal")
                .prop("title", str("prompt"))
                .child(fenced(r#"sh prompt="$ " copy=false"#, "$ liyasa build\n"))
                .build(),
        )
        .case(
            "unknown attribute",
            inst::new("terminal")
                .prop("title", str("unknown"))
                .child(fenced("rust wat=1", "fn main() {}\n"))
                .build(),
        )
        .check();
}

#[test]
fn mermaid() {
    Gallery::new("mermaid")
        .case(
            "default",
            inst::new("terminal")
                .prop("title", str("default"))
                .child(fenced("mermaid", "flowchart LR\n  a --> b\n"))
                .build(),
        )
        .case(
            "every attribute",
            inst::new("terminal")
                .prop("title", str("every attribute"))
                .child(fenced(
                    "mermaid layout=elk theme=dark zoom pan fullscreen",
                    "sequenceDiagram\n  A->>B: hi\n",
                ))
                .build(),
        )
        .check();
}

#[test]
fn code_group() {
    Gallery::new("code-group")
        .case(
            "default",
            inst::new("code-group")
                .child(fenced(r#"sh title="npm""#, "npm i liyasa\n"))
                .child(fenced(r#"sh title="cargo""#, "cargo install liyasa\n"))
                .build(),
        )
        .case(
            "every prop",
            inst::new("code-group")
                .prop("sync", str("pm"))
                .prop("dropdown", PropValue::Bool(true))
                .child(fenced(r#"sh title="npm""#, "npm i liyasa\n"))
                .build(),
        )
        .case("empty body", inst::new("code-group").build())
        .case(
            "untitled fence",
            inst::new("code-group")
                .child(fenced("rust", "fn main() {}\n"))
                .build(),
        )
        .check();
}

#[test]
fn inline_code() {
    Gallery::new("code")
        .case(
            "default",
            inst::new("code")
                .child(Node::Inline(Inline::Text("Vec<u8>".into())))
                .build(),
        )
        .case(
            "every prop",
            inst::new("code")
                .prop("lang", str("rust"))
                .child(Node::Inline(Inline::Text("Vec<u8>".into())))
                .build(),
        )
        .case(
            "backticks in the body",
            inst::new("code")
                .child(Node::Inline(Inline::Text("a ` b".into())))
                .build(),
        )
        .check();
}

#[test]
fn terminal() {
    Gallery::new("terminal")
        .case(
            "default",
            inst::new("terminal")
                .child(nodes::code_block_with(
                    Some("sh"),
                    "$ liyasa build\n",
                    FenceAttrs::default(),
                ))
                .build(),
        )
        .case(
            "every prop",
            inst::new("terminal")
                .prop("title", str("bash"))
                .prop("prompt", str("> "))
                .child(nodes::paragraph("> liyasa dev"))
                .build(),
        )
        .check();
}

#[test]
fn snippet_from() {
    Gallery::new("snippet-from")
        .case(
            "default",
            inst::new("snippet-from")
                .prop("file", str("src/lib.rs"))
                .build(),
        )
        .case(
            "every prop",
            inst::new("snippet-from")
                .prop("file", str("src/lib.rs"))
                .prop("lines", str("10-25"))
                .prop("symbol", str("parse"))
                .prop("repo", str("kasecrab/liyasa"))
                .prop("ref", str("v0.4.0"))
                .prop("lang", str("rust"))
                .prop("title", str("The parser"))
                .build(),
        )
        .case("missing required prop", inst::new("snippet-from").build())
        .case(
            "lines and symbol",
            inst::new("snippet-from")
                .prop("file", str("src/lib.rs"))
                .prop("lines", str("10-25"))
                .prop("symbol", str("parse"))
                .build(),
        )
        .check();
}
