//! Generates the Liyasa half of the conformance corpus (PRD §30.9, §31.7 item 4).
//!
//! The matrix below is the specification's edge cases written out: directives
//! at every list-marker width, inside blockquotes at every depth, after lazy
//! continuations, in table cells (where they must stay literal), the props
//! grammar, the inline-code masking rules, and the forged-marker class.
//!
//! Expected output is filled in from an engine and marked `generated`, because
//! the alternative — a human typing 300 HTML fragments — produces worse
//! expectations, not better ones. The packet's fixture reviewer clears a
//! random sample against the reference implementations before the corpus is
//! trusted; `xtask corpus review-sample` picks it deterministically.

use std::path::Path;

use crate::corpus::{Case, CaseHeader, ExpectedDiagnostic};
use crate::spike::engines::Engine;

struct Seed {
    id: String,
    requirement: &'static str,
    tags: Vec<String>,
    source: String,
    diagnostics: Option<Vec<ExpectedDiagnostic>>,
    options: Option<(&'static str, serde_json::Value)>,
}

fn seed(id: String, requirement: &'static str, tags: &[&str], source: String) -> Seed {
    Seed {
        id,
        requirement,
        tags: tags.iter().map(|t| (*t).to_owned()).collect(),
        source,
        diagnostics: None,
        options: None,
    }
}

fn expecting(mut s: Seed, codes: &[&str]) -> Seed {
    s.diagnostics = Some(
        codes
            .iter()
            .map(|code| ExpectedDiagnostic {
                code: (*code).to_owned(),
                line: None,
                col: None,
                message: None,
            })
            .collect(),
    );
    s
}

/// Every generated case, in a stable order.
fn seeds() -> Vec<Seed> {
    let mut out = Vec::new();
    out.extend(containers());
    out.extend(leaves());
    out.extend(props());
    out.extend(masking());
    out.extend(forged_markers());
    out.extend(inline_directives());
    out.extend(tag_form());
    out.extend(front_matter());
    out.extend(templating());
    out.extend(markdown_features());
    out.extend(block_identity());
    out.extend(slots_and_unknown());
    out.extend(html_modes());
    out.extend(span_composition());
    out.extend(links_and_media());
    out
}

/// The blocking container subset (§7.5.1 item 7).
fn containers() -> Vec<Seed> {
    let mut out = Vec::new();

    // A directive inside a list item at every marker width. The content column
    // depends on the marker, which is exactly what a scanner must not guess.
    for (slug, marker) in [
        ("dash", "-"),
        ("star", "*"),
        ("plus", "+"),
        ("ordered-1", "1."),
        ("ordered-10", "10."),
        ("ordered-100", "100."),
        ("ordered-paren", "1)"),
    ] {
        let indent = " ".repeat(marker.len() + 1);
        out.push(seed(
            format!("cm-50/containers/list-item-{slug}"),
            "CM-50",
            &["directive", "container", "list"],
            format!("{marker} item\n{indent}:::note\n{indent}body\n{indent}:::\n"),
        ));
    }

    for depth in 1..=3usize {
        let indent = "  ".repeat(depth);
        let mut source = String::new();
        for level in 0..depth {
            source.push_str(&format!("{}- level {}\n", "  ".repeat(level), level + 1));
        }
        source.push_str(&format!("{indent}:::note\n{indent}body\n{indent}:::\n"));
        out.push(seed(
            format!("cm-50/containers/list-depth-{depth}"),
            "CM-50",
            &["directive", "container", "list"],
            source,
        ));
    }

    for depth in 1..=3usize {
        let prefix = "> ".repeat(depth);
        out.push(seed(
            format!("cm-50/containers/blockquote-depth-{depth}"),
            "CM-50",
            &["directive", "container", "blockquote"],
            format!("{prefix}:::note\n{prefix}body\n{prefix}:::\n"),
        ));
    }
    out.push(seed(
        "cm-50/containers/list-in-blockquote".to_owned(),
        "CM-50",
        &["directive", "container", "blockquote", "list"],
        "> - item\n>   :::note\n>   body\n>   :::\n".to_owned(),
    ));
    out.push(seed(
        "cm-50/containers/blockquote-in-list".to_owned(),
        "CM-50",
        &["directive", "container", "blockquote", "list"],
        "- item\n  > :::note\n  > body\n  > :::\n".to_owned(),
    ));

    // Nesting uses longer fences.
    out.push(seed(
        "cm-50/containers/fence-3".to_owned(),
        "CM-50",
        &["directive", "container"],
        ":::note\nbody\n:::\n".to_owned(),
    ));
    for colons in 4..=6usize {
        let outer = ":".repeat(colons);
        let inner = ":".repeat(colons - 1);
        out.push(seed(
            format!("cm-50/containers/fence-{colons}-nested"),
            "CM-50",
            &["directive", "container", "nesting"],
            format!("{outer}tabs\n{inner}tab\nbody\n{inner}\n{outer}\n"),
        ));
    }

    out.push(seed(
        "cm-50/containers/after-lazy-paragraph".to_owned(),
        "CM-50",
        &["directive", "container", "lazy"],
        "paragraph text\ncontinued lazily\n:::note\nbody\n:::\n".to_owned(),
    ));
    out.push(seed(
        "cm-50/containers/blank-lines-in-body".to_owned(),
        "CM-50",
        &["directive", "container"],
        ":::note\n\nfirst\n\nsecond\n\n:::\n".to_owned(),
    ));
    out.push(seed(
        "cm-50/containers/trailing-whitespace".to_owned(),
        "CM-50",
        &["directive", "container"],
        ":::note  \nbody\n:::   \n".to_owned(),
    ));
    out.push(seed(
        "cm-50/containers/empty-body".to_owned(),
        "CM-50",
        &["directive", "container"],
        ":::note\n:::\n".to_owned(),
    ));
    out.push(seed(
        "cm-50/containers/adjacent".to_owned(),
        "CM-50",
        &["directive", "container"],
        ":::a\nx\n:::\n:::b\ny\n:::\n".to_owned(),
    ));
    out.push(seed(
        "cm-50/containers/heading-inside".to_owned(),
        "CM-50",
        &["directive", "container"],
        ":::note\n## Heading\nbody\n:::\n".to_owned(),
    ));
    out.push(seed(
        "cm-50/containers/table-inside".to_owned(),
        "CM-50",
        &["directive", "container", "table"],
        ":::note\n| a | b |\n|---|---|\n| 1 | 2 |\n:::\n".to_owned(),
    ));
    out.push(seed(
        "cm-50/containers/list-inside".to_owned(),
        "CM-50",
        &["directive", "container", "list"],
        ":::note\n- a\n- b\n:::\n".to_owned(),
    ));

    for extra in 1..=3usize {
        out.push(seed(
            format!("cm-50/containers/mixed-indent-{extra}"),
            "CM-50",
            &["directive", "container", "list", "indent"],
            format!("- item\n  {}:::note\n  body\n  :::\n", " ".repeat(extra)),
        ));
    }

    // Contexts where a directive must stay literal.
    out.push(seed(
        "cm-50/containers/table-cell-is-literal".to_owned(),
        "CM-50",
        &["directive", "container", "table"],
        "| a | b |\n|---|---|\n| :::note | y |\n".to_owned(),
    ));
    out.push(seed(
        "cm-50/containers/inside-fenced-code".to_owned(),
        "CM-50",
        &["directive", "container", "code"],
        "```\n:::note\nbody\n:::\n```\n".to_owned(),
    ));
    out.push(seed(
        "cm-50/containers/inside-indented-code".to_owned(),
        "CM-50",
        &["directive", "container", "code"],
        "    :::note\n    body\n    :::\n".to_owned(),
    ));
    out.push(seed(
        "cm-50/containers/inside-tilde-fence".to_owned(),
        "CM-50",
        &["directive", "container", "code"],
        "~~~\n:::note\n~~~\n".to_owned(),
    ));

    out.push(expecting(
        seed(
            "cm-50/containers/unclosed".to_owned(),
            "CM-50",
            &["directive", "container", "error"],
            ":::note\nbody\n".to_owned(),
        ),
        &["E0310"],
    ));
    out.push(expecting(
        seed(
            "cm-50/containers/close-without-open".to_owned(),
            "CM-50",
            &["directive", "container", "error"],
            "body\n:::\n".to_owned(),
        ),
        &["E0311"],
    ));
    out.push(expecting(
        seed(
            "cm-50/containers/close-at-wrong-depth".to_owned(),
            "CM-50",
            &["directive", "container", "error"],
            "- item\n  :::note\n  body\n:::\n".to_owned(),
        ),
        &["E0311"],
    ));
    out.push(expecting(
        seed(
            "cm-50/containers/inner-unclosed".to_owned(),
            "CM-50",
            &["directive", "container", "error"],
            "::::tabs\n:::tab\nbody\n::::\n".to_owned(),
        ),
        &["E0310"],
    ));

    out
}

fn leaves() -> Vec<Seed> {
    let mut out = vec![seed(
        "cm-50/leaf/plain".to_owned(),
        "CM-50",
        &["directive", "leaf"],
        "::image{src=\"/flow.png\" alt=\"Request flow\"}\n".to_owned(),
    )];
    for (slug, wrapper) in [
        ("in-list", "- item\n  ::image{src=\"/a.png\"}\n"),
        ("in-blockquote", "> ::image{src=\"/a.png\"}\n"),
        ("in-container", ":::note\n::image{src=\"/a.png\"}\n:::\n"),
        (
            "between-paragraphs",
            "before\n\n::image{src=\"/a.png\"}\n\nafter\n",
        ),
        (
            "adjacent",
            "::image{src=\"/a.png\"}\n::image{src=\"/b.png\"}\n",
        ),
        ("in-fence", "```\n::image{src=\"/a.png\"}\n```\n"),
        (
            "after-lazy-paragraph",
            "para\ncontinued\n::image{src=\"/a.png\"}\n",
        ),
    ] {
        out.push(seed(
            format!("cm-50/leaf/{slug}"),
            "CM-50",
            &["directive", "leaf"],
            wrapper.to_owned(),
        ));
    }
    out.push(seed(
        "cm-50/leaf/no-props".to_owned(),
        "CM-50",
        &["directive", "leaf"],
        "::divider\n".to_owned(),
    ));
    out.push(seed(
        "cm-50/leaf/not-a-directive".to_owned(),
        "CM-50",
        &["directive", "leaf"],
        "::\n".to_owned(),
    ));
    out
}

/// The props grammar of CM-51.
fn props() -> Vec<Seed> {
    let cases: &[(&str, &str)] = &[
        ("string", "{title=\"Install the CLI\"}"),
        ("number", "{columns=3}"),
        ("negative-number", "{offset=-2}"),
        ("float", "{ratio=1.5}"),
        ("bool-true", "{open=true}"),
        ("bool-false", "{open=false}"),
        ("list", "{tags=[alpha,beta]}"),
        ("list-quoted", "{tags=[\"a b\",\"c\"]}"),
        ("expr", "{href={{ page.url }}}"),
        ("class-shorthand", "{.wide}"),
        ("id-shorthand", "{#install}"),
        ("class-and-id", "{.wide #install}"),
        ("many", "{title=\"A\" columns=2 open=true .wide #x}"),
        ("empty-string", "{title=\"\"}"),
        ("arrow-in-value", "{title=\"a --> b\"}"),
        ("brace-in-value", "{title=\"a } b\"}"),
        ("backtick-in-value", "{title=\"a `code` b\"}"),
        ("unicode", "{title=\"安装 · Установка\"}"),
        ("dashed-key", "{data-test=\"x\"}"),
        ("underscore-key", "{data_test=\"x\"}"),
        ("url-value", "{href=\"https://example.com\"}"),
        ("spaces-around", "{ title = \"A\" }"),
        ("colons-in-value", "{title=\"a:b:c\"}"),
        ("no-props", ""),
    ];
    let mut out = Vec::new();
    for (slug, props) in cases {
        out.push(seed(
            format!("cm-51/props/{slug}"),
            "CM-51",
            &["directive", "props"],
            format!(":::card{props}\nbody\n:::\n"),
        ));
        out.push(seed(
            format!("cm-51/props/leaf-{slug}"),
            "CM-51",
            &["directive", "props", "leaf"],
            format!("::image{props}\n"),
        ));
    }
    out
}

/// Fence-aware masking: code spans and fences are opaque to the scanner and to
/// template expansion (CM-11).
fn masking() -> Vec<Seed> {
    let bodies: &[(&str, &str)] = &[
        ("inline-code-directive", "Use `:::note` to open a note.\n"),
        (
            "inline-code-template",
            "Write `{{ page.title }}` to print the title.\n",
        ),
        (
            "inline-code-statement",
            "Write `{% for x in y %}` to loop.\n",
        ),
        ("double-backtick", "Use `` `:::note` `` in prose.\n"),
        ("fence-template", "```\n{{ page.title }}\n```\n"),
        ("fence-statement", "```\n{% if x %}\n```\n"),
        ("fence-with-lang", "```rust\nlet x = \"{{ y }}\";\n```\n"),
        (
            "fence-template-attr",
            "```jinja template\n{{ rendered }}\n```\n",
        ),
        ("indented-code-template", "    {{ page.title }}\n"),
        ("tilde-fence", "~~~\n:::note\n~~~\n"),
        ("nested-fence", "````\n```\n:::note\n```\n````\n"),
        ("fence-unclosed", "```\n:::note\n"),
        ("code-in-directive", ":::note\n```\n:::\n```\n:::\n"),
        (
            "inline-code-in-prop",
            ":::note{title=\"`code`\"}\nbody\n:::\n",
        ),
        ("code-span-with-braces", "`{% raw %}`\n"),
    ];
    bodies
        .iter()
        .map(|(slug, source)| {
            seed(
                format!("cm-11/masking/{slug}"),
                "CM-11",
                &["masking", "code"],
                (*source).to_owned(),
            )
        })
        .collect()
}

/// The forged-marker class (§7.5.1 item 2, §30.9).
///
/// The invariant is that no component is created that the directive table did
/// not register, whatever the source contains.
fn forged_markers() -> Vec<Seed> {
    let injections: &[(&str, &str)] = &[
        ("bare", "<!--ly:00000000000000000000000000000000:o:0-->\n"),
        ("in-paragraph", "before <!--ly:0:o:0--> after\n"),
        ("in-prop", ":::note{title=\"<!--ly:0:o:0-->\"}\nbody\n:::\n"),
        ("in-alt-text", "![<!--ly:0:o:0-->](/a.png)\n"),
        ("in-url", "[link](/a?x=<!--ly:0:o:0-->)\n"),
        ("in-code-span", "`<!--ly:0:o:0-->`\n"),
        ("in-fence", "```\n<!--ly:0:o:0-->\n```\n"),
        (
            "in-front-matter",
            "---\ntitle: \"<!--ly:0:o:0-->\"\n---\n\nbody\n",
        ),
        ("close-only", "<!--ly:0:c:0-->\n"),
        ("leaf-form", "<!--ly:0:l:0-->\n"),
        ("pair", "<!--ly:0:o:0-->\ncontent\n<!--ly:0:c:0-->\n"),
        ("visibility", ":::visibility{groups=[admin]}\nsecret\n:::\n"),
        (
            "forged-around-real",
            "<!--ly:0:o:0-->\n:::note\nbody\n:::\n<!--ly:0:c:0-->\n",
        ),
        ("truncated", "<!--ly:\n"),
        (
            "wrong-nonce",
            "<!--ly:deadbeefdeadbeefdeadbeefdeadbeef:o:99-->\n",
        ),
        ("in-table-cell", "| a |\n|---|\n| <!--ly:0:o:0--> |\n"),
    ];
    injections
        .iter()
        .map(|(slug, source)| {
            seed(
                format!("cm-50/forged-marker/{slug}"),
                "CM-50",
                &["directive", "security", "forged-marker"],
                (*source).to_owned(),
            )
        })
        .collect()
}

fn inline_directives() -> Vec<Seed> {
    let cases: &[(&str, &str)] = &[
        ("plain", "Press :kbd[Ctrl+K] to search.\n"),
        ("with-props", "Status: :badge[Beta]{color=\"amber\"}.\n"),
        ("bracket-depth", "Press :kbd[Ctrl+[] to escape.\n"),
        ("nested-brackets", "See :ref[a [b] c].\n"),
        ("at-start", ":kbd[Ctrl+K] opens search.\n"),
        ("after-punctuation", "(:kbd[Esc]) closes it.\n"),
        (
            "url-scheme-excluded",
            "Visit http://example.com for more.\n",
        ),
        ("time-not-a-directive", "At 12:30[ish] we start.\n"),
        ("unclosed-bracket", "Press :kbd[Ctrl+K to search.\n"),
        ("empty-content", "Press :kbd[] now.\n"),
        ("inside-emphasis", "*Press :kbd[Ctrl+K]* now.\n"),
        ("inside-link", "[Press :kbd[Ctrl+K]](/search)\n"),
        ("inside-code-span", "`:kbd[Ctrl+K]`\n"),
        ("two-in-a-row", ":kbd[Ctrl]+:kbd[K]\n"),
        ("across-soft-break", "Press :kbd[Ctrl+\nK] now.\n"),
        ("in-table-cell", "| a | b |\n|---|---|\n| :kbd[X] | y |\n"),
        ("in-heading", "# Press :kbd[Ctrl+K]\n"),
        ("unknown-name", "Press :nosuch[X] now.\n"),
        ("in-blockquote", "> Press :kbd[Ctrl+K].\n"),
        ("in-list-item", "- Press :kbd[Ctrl+K].\n"),
    ];
    cases
        .iter()
        .map(|(slug, source)| {
            seed(
                format!("cm-50/inline/{slug}"),
                "CM-50",
                &["directive", "inline"],
                (*source).to_owned(),
            )
        })
        .collect()
}

fn tag_form() -> Vec<Seed> {
    let cases: &[(&str, &str)] = &[
        ("block", "<Card title=\"Install\">\nbody\n</Card>\n"),
        ("self-closing", "<Image src=\"/a.png\" />\n"),
        (
            "nested",
            "<Tabs>\n<Tab title=\"npm\">\nnpm i\n</Tab>\n</Tabs>\n",
        ),
        (
            "in-list",
            "- item\n  <Card title=\"x\">\n  body\n  </Card>\n",
        ),
        ("lowercase-stays-html", "<div class=\"x\">\nbody\n</div>\n"),
        ("inline", "Press <Kbd>Ctrl+K</Kbd> now.\n"),
        (
            "mixed-with-directive",
            "<Card title=\"x\">\n:::note\nbody\n:::\n</Card>\n",
        ),
        (
            "attribute-with-arrow",
            "<Card title=\"a --> b\">\nbody\n</Card>\n",
        ),
        ("unclosed", "<Card title=\"x\">\nbody\n"),
        ("close-without-open", "body\n</Card>\n"),
        ("in-fence", "```\n<Card title=\"x\">\n```\n"),
        (
            "attribute-expr",
            "<Card href={{ page.url }}>\nbody\n</Card>\n",
        ),
    ];
    cases
        .iter()
        .map(|(slug, source)| {
            seed(
                format!("cm-53/tag-form/{slug}"),
                "CM-53",
                &["directive", "tag-form"],
                (*source).to_owned(),
            )
        })
        .collect()
}

fn front_matter() -> Vec<Seed> {
    let cases: &[(&str, &str, &[&str])] = &[
        ("title-only", "---\ntitle: Install\n---\n\nbody\n", &[]),
        (
            "every-common-key",
            "---\ntitle: Install\ndescription: How to install\nsidebarTitle: Setup\nicon: terminal\ntag: New\nmode: wide\nkeywords: [cli, setup]\n---\n\nbody\n",
            &[],
        ),
        ("no-front-matter", "# Install\n\nbody\n", &[]),
        ("empty", "---\n---\n\nbody\n", &[]),
        (
            "nested-object",
            "---\nog:\n  title: A\n  image: /a.png\n---\n\nbody\n",
            &[],
        ),
        (
            "boolean-and-list",
            "---\ndraft: true\nlocales: [en, de]\n---\n\nbody\n",
            &[],
        ),
        (
            "personalized",
            "---\npersonalized: true\n---\n\nHello {{ reader.name }}.\n",
            &[],
        ),
        ("unknown-key", "---\nnosuchkey: 1\n---\n\nbody\n", &[]),
        (
            "dashes-in-body",
            "---\ntitle: A\n---\n\nbody\n\n---\n\nmore\n",
            &[],
        ),
        ("not-at-start", "\n---\ntitle: A\n---\n\nbody\n", &[]),
        (
            "invalid-yaml",
            "---\ntitle: [unclosed\n---\n\nbody\n",
            &["E0101"],
        ),
        (
            "windows-newlines",
            "---\r\ntitle: A\r\n---\r\n\r\nbody\r\n",
            &[],
        ),
        (
            "page-id",
            "---\nid: 01J8ZZZZZZZZZZZZZZZZZZZZZZ\n---\n\nbody\n",
            &[],
        ),
        (
            "regions",
            "---\nregions:\n  only: [us, eu]\n---\n\nbody\n",
            &[],
        ),
        (
            "directive-right-after",
            "---\ntitle: A\n---\n:::note\nbody\n:::\n",
            &[],
        ),
    ];
    cases
        .iter()
        .map(|(slug, source, codes)| {
            let case = seed(
                format!("fm/{slug}"),
                "CM-40",
                &["front-matter"],
                (*source).to_owned(),
            );
            if codes.is_empty() {
                case
            } else {
                expecting(case, codes)
            }
        })
        .collect()
}

fn templating() -> Vec<Seed> {
    let cases: &[(&str, &str, &[&str])] = &[
        ("output", "Version {{ version }} is out.\n", &[]),
        ("statement-if", "{% if x %}\nyes\n{% endif %}\n", &[]),
        (
            "statement-for",
            "{% for item in items %}\n- {{ item }}\n{% endfor %}\n",
            &[],
        ),
        ("comment", "{# not rendered #}\nbody\n", &[]),
        (
            "whitespace-control",
            "{%- if x -%}\nyes\n{%- endif -%}\n",
            &[],
        ),
        (
            "nested-blocks",
            "{% for a in b %}{% if a %}{{ a }}{% endif %}{% endfor %}\n",
            &[],
        ),
        ("in-prop", ":::card{href={{ page.url }}}\nbody\n:::\n", &[]),
        ("in-heading", "# {{ page.title }}\n", &[]),
        ("in-link-target", "[text]({{ page.url }})\n", &[]),
        (
            "directive-from-loop",
            "{% for t in tabs %}\n:::tab{title={{ t }}}\nbody\n:::\n{% endfor %}\n",
            &[],
        ),
        ("snippet", "{% snippet \"plans\" %}\n", &[]),
        ("include", "{% include \"partial.md\" %}\n", &[]),
        (
            "split-block-not-well-formed",
            "{% if x %}\n- a\n{% endif %}\n- b\n",
            &["E0210"],
        ),
        ("private-use-character", "body \u{E000} more\n", &["E0212"]),
        ("statement-inside-code", "```\n{% if x %}\n```\n", &[]),
        ("output-inside-code-span", "`{{ x }}`\n", &[]),
        (
            "raw-block",
            "{% raw %}{{ not_expanded }}{% endraw %}\n",
            &[],
        ),
        ("unclosed-statement", "{% if x %}\nbody\n", &["E0202"]),
        (
            "fact-reference",
            "The price is {{ fact(\"plan.pro.price\") }}.\n",
            &[],
        ),
        ("env-reference", "Built in {{ env(\"CI\") }}.\n", &[]),
        ("reader-field", "Hello {{ reader.name }}.\n", &["E0208"]),
        (
            "loop-index",
            "{% for x in y %}{{ loop.index0 }}{% endfor %}\n",
            &[],
        ),
    ];
    cases
        .iter()
        .map(|(slug, source, codes)| {
            let case = seed(
                format!("tmpl/{slug}"),
                "CM-20",
                &["templating"],
                (*source).to_owned(),
            );
            if codes.is_empty() {
                case
            } else {
                expecting(case, codes)
            }
        })
        .collect()
}

fn markdown_features() -> Vec<Seed> {
    let cases: &[(&str, &str)] = &[
        ("table", "| a | b |\n|---|---|\n| 1 | 2 |\n"),
        (
            "table-alignment",
            "| a | b | c |\n|:--|:-:|--:|\n| 1 | 2 | 3 |\n",
        ),
        ("task-list", "- [ ] todo\n- [x] done\n"),
        ("strikethrough", "~~gone~~\n"),
        ("footnote", "text[^1]\n\n[^1]: the note\n"),
        ("autolink", "See https://example.com for more.\n"),
        ("description-list", "term\n: definition\n"),
        ("alert", "> [!NOTE]\n> body\n"),
        ("multiline-blockquote", ">>>\nbody\n>>>\n"),
        ("superscript", "x^2^\n"),
        ("heading-anchor", "## Install the CLI {#install}\n"),
        ("hard-break", "one  \ntwo\n"),
        ("reference-link", "[text][ref]\n\n[ref]: /target\n"),
        ("image-with-title", "![alt](/a.png \"title\")\n"),
        ("html-block", "<div>\nbody\n</div>\n"),
        ("setext-heading", "Title\n=====\n"),
        ("thematic-break", "a\n\n---\n\nb\n"),
        ("nested-emphasis", "*a **b** c*\n"),
        (
            "code-fence-attributes",
            "```rust {1,3-4} showLineNumbers\nlet x = 1;\n```\n",
        ),
        ("image-without-alt", "![](/a.png)\n"),
        ("deep-heading", "###### Six\n"),
        ("entity", "&amp; &#65; &nbsp;\n"),
    ];
    cases
        .iter()
        .map(|(slug, source)| {
            seed(
                format!("cm-30/markdown/{slug}"),
                "CM-30",
                &["markdown"],
                (*source).to_owned(),
            )
        })
        .collect()
}
/// Explicit and implicit block IDs (§7.16).
fn block_identity() -> Vec<Seed> {
    let cases: &[(&str, &str)] = &[
        ("heading-attribute", "## Install the CLI {#install}\n"),
        (
            "paragraph-trailing-attribute",
            "A sentence about pricing. {#pricing}\n",
        ),
        (
            "comment-before-block",
            "<!-- #pricing -->\nA sentence about pricing.\n",
        ),
        ("directive-attribute", ":::note{#warning-1}\nbody\n:::\n"),
        ("duplicate-explicit-id", "## A {#x}\n\n## B {#x}\n"),
        ("id-on-list-item", "- item {#first}\n- item\n"),
        ("id-on-table-row", "| a |\n|---|\n| x |\n"),
        ("identical-siblings", "para\n\npara\n\npara\n"),
        (
            "identical-under-different-headings",
            "# A\n\npara\n\n# B\n\npara\n",
        ),
        ("whitespace-only-difference", "a  b\n\na b\n"),
        ("id-with-unicode", "## 安装 {#install}\n"),
        ("id-looks-like-class", "## A {.wide}\n"),
    ];
    cases
        .iter()
        .map(|(slug, source)| {
            let case = seed(
                format!("cm-56/block-id/{slug}"),
                "CM-56",
                &["block-id"],
                (*source).to_owned(),
            );
            if *slug == "duplicate-explicit-id" {
                expecting(case, &["E0318"])
            } else {
                case
            }
        })
        .collect()
}

/// Slots (CM-52) and unknown components (CM-54).
fn slots_and_unknown() -> Vec<Seed> {
    let cases: &[(&str, &str, &[&str])] = &[
        (
            "named-slot",
            ":::card\nbody\n:::slot{name=\"footer\"}\nfooter\n:::\n:::\n",
            &[],
        ),
        (
            "two-slots",
            ":::card\n:::slot{name=\"header\"}\nh\n:::\n:::slot{name=\"footer\"}\nf\n:::\n:::\n",
            &[],
        ),
        (
            "slot-outside-component",
            ":::slot{name=\"footer\"}\nf\n:::\n",
            &["E0350"],
        ),
        (
            "unknown-slot-name",
            ":::card\n:::slot{name=\"nope\"}\nx\n:::\n:::\n",
            &["E0350"],
        ),
        ("unknown-component", ":::callou\nbody\n:::\n", &["E0313"]),
        (
            "unknown-leaf-component",
            "::imag{src=\"/a.png\"}\n",
            &["E0313"],
        ),
        ("unknown-inline-component", "Press :kbdd[X].\n", &["E0313"]),
        ("missing-required-prop", "::image{alt=\"x\"}\n", &["E0314"]),
        (
            "prop-type-mismatch",
            ":::card{columns=\"three\"}\nbody\n:::\n",
            &["E0315"],
        ),
        ("unknown-prop", ":::card{nosuch=1}\nbody\n:::\n", &["W0316"]),
        ("container-used-as-leaf", "::card\n", &["E0317"]),
        ("inline-used-as-block", ":::kbd\nbody\n:::\n", &["E0317"]),
    ];
    cases
        .iter()
        .map(|(slug, source, codes)| {
            let case = seed(
                format!("cm-52/slots/{slug}"),
                "CM-52",
                &["directive", "slots"],
                (*source).to_owned(),
            );
            if codes.is_empty() {
                case
            } else {
                expecting(case, codes)
            }
        })
        .collect()
}

/// `content.html` is enforced by the sanitizer over the Rendered AST, never by
/// the parser (§7.5.1 item 3), so the same source runs under all three modes.
fn html_modes() -> Vec<Seed> {
    let sources: &[(&str, &str)] = &[
        ("block", "<div class=\"x\">\nbody\n</div>\n"),
        ("inline", "text <span>x</span> more\n"),
        ("script", "<script>alert(1)</script>\n"),
        ("iframe", "<iframe src=\"https://example.com\"></iframe>\n"),
        ("event-handler", "<a href=\"/x\" onclick=\"evil()\">x</a>\n"),
        ("javascript-url", "[x](javascript:alert(1))\n"),
        ("style-attribute", "<p style=\"color:red\">x</p>\n"),
        ("svg", "<svg><use href=\"#x\"/></svg>\n"),
        ("comment", "<!-- a comment -->\n"),
        (
            "data-uri-image",
            "![x](data:image/svg+xml;base64,PHN2Zz48L3N2Zz4=)\n",
        ),
    ];
    let mut out = Vec::new();
    for (slug, source) in sources {
        for mode in ["allow", "sanitize", "off"] {
            let mut case = seed(
                format!("cm-31/html/{mode}-{slug}"),
                "CM-31",
                &["html", "sanitizer"],
                (*source).to_owned(),
            );
            case.options = Some(("content.html", serde_json::Value::from(mode)));
            out.push(case);
        }
    }
    out
}

/// The property §7.5.1 item 2 promises: a position reported on a rewritten line
/// composes back to its source span. These sources mix directives with content
/// that changes line lengths, which is what the composition has to survive.
fn span_composition() -> Vec<Seed> {
    let cases: &[(&str, &str)] = &[
        ("short-directive", ":::a\nx\n:::\n"),
        (
            "long-directive",
            ":::verylongcomponentname{title=\"a rather long value here\"}\nx\n:::\n",
        ),
        (
            "many-directives",
            ":::a\nx\n:::\n:::b\ny\n:::\n:::c\nz\n:::\n",
        ),
        (
            "directive-after-long-paragraph",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n\n:::a\nx\n:::\n",
        ),
        (
            "directive-between-fences",
            "```\ncode\n```\n:::a\nx\n:::\n```\ncode\n```\n",
        ),
        (
            "unicode-before-directive",
            "안녕하세요 세계\n\n:::a\nx\n:::\n",
        ),
        ("tabs-before-directive", "\tindented\n\n:::a\nx\n:::\n"),
        ("crlf", ":::a\r\nx\r\n:::\r\n"),
        ("no-trailing-newline", ":::a\nx\n:::"),
        (
            "leaf-and-container",
            "::img{src=\"/a.png\"}\n:::a\nx\n:::\n",
        ),
    ];
    cases
        .iter()
        .map(|(slug, source)| {
            seed(
                format!("cm-50/span-composition/{slug}"),
                "CM-50",
                &["directive", "span-composition"],
                (*source).to_owned(),
            )
        })
        .collect()
}

/// Links, images, math, and wikilinks (§7.4), plus the Markdown serialization
/// every component owes an agent (CM-55).
fn links_and_media() -> Vec<Seed> {
    type Row = (&'static str, &'static str, &'static [(&'static str, bool)]);
    let cases: &[Row] = &[
        (
            "internal-link",
            "[Install](/getting-started/install)\n",
            &[],
        ),
        ("relative-link", "[Install](./install.md)\n", &[]),
        ("anchor-link", "[Section](#install)\n", &[]),
        ("external-link", "[Example](https://example.com)\n", &[]),
        (
            "link-with-title",
            "[Example](https://example.com \"Title\")\n",
            &[],
        ),
        (
            "image-dark-variant",
            "::image{src=\"/a.png\" dark=\"/a.dark.png\" alt=\"A\"}\n",
            &[],
        ),
        ("image-in-link", "[![alt](/a.png)](/target)\n", &[]),
        (
            "wikilink",
            "See [[install]] for more.\n",
            &[("content.wikilinks", true)],
        ),
        (
            "wikilink-with-label",
            "See [[install|the guide]].\n",
            &[("content.wikilinks", true)],
        ),
        (
            "math-inline",
            "The value is $x^2$ here.\n",
            &[("content.math", true)],
        ),
        (
            "math-display",
            "$$\nx^2 + y^2 = z^2\n$$\n",
            &[("content.math", true)],
        ),
        ("math-in-code-span", "`$x^2$`\n", &[("content.math", true)]),
        ("math-off", "The value is $x^2$ here.\n", &[]),
        ("bare-url", "https://example.com\n", &[]),
        ("email-autolink", "<a@example.com>\n", &[]),
        (
            "link-in-directive",
            ":::note\n[Install](/install)\n:::\n",
            &[],
        ),
        ("image-in-directive", ":::note\n![alt](/a.png)\n:::\n", &[]),
        ("empty-link-text", "[](/target)\n", &[]),
        ("link-with-parens", "[x](/a(b))\n", &[]),
        ("link-with-spaces", "[x](</a b>)\n", &[]),
    ];
    cases
        .iter()
        .map(|(slug, source, options)| {
            let mut case = seed(
                format!("cm-32/links/{slug}"),
                "CM-32",
                &["links", "media"],
                (*source).to_owned(),
            );
            if let Some((key, value)) = options.first() {
                case.options = Some((key, serde_json::Value::from(*value)));
            }
            case
        })
        .collect()
}

/// Writes every generated case, filling expectations from `engine`.
///
/// An existing case is left alone unless `overwrite` is set, so a reviewed or
/// hand-corrected expectation is never silently replaced.
pub fn run(out: &Path, engine: &dyn Engine, overwrite: bool) -> Result<(usize, usize), String> {
    let mut written = 0;
    let mut kept = 0;
    for seed in seeds() {
        let path = out.join(format!("{}.md", seed.id));
        if path.exists() && !overwrite {
            kept += 1;
            continue;
        }
        let mut case = Case {
            path,
            header: CaseHeader {
                id: seed.id,
                requirement: Some(seed.requirement.to_owned()),
                tags: seed.tags,
                options: seed
                    .options
                    .into_iter()
                    .map(|(key, value)| (key.to_owned(), value))
                    .collect(),
                generated: Some(format!("{} (comrak {})", engine.name(), comrak::version())),
                ..CaseHeader::default()
            },
            source: seed.source,
            html: None,
            markdown: None,
            ast: None,
            source_document: None,
            diagnostics: seed.diagnostics,
        };
        if engine.produces().contains(&"html") {
            case.html = engine.run(&case)?.html;
        }
        crate::corpus_import::write(&case)?;
        written += 1;
    }
    Ok((written, kept))
}

/// The deterministic 20% sample the fixture reviewer checks by hand.
///
/// Deterministic so the sample can be recorded, re-derived, and audited; the
/// key mixes the corpus size in so a grown corpus does not reshuffle it.
pub fn review_sample(cases: &[Case], percent: u32) -> Vec<&Case> {
    let threshold = u64::from(percent).saturating_mul(u64::MAX / 100);
    cases
        .iter()
        .filter(|case| {
            let digest = liyasa_core::Fingerprint::of(case.header.id.as_bytes());
            let mut key = [0u8; 8];
            key.copy_from_slice(&digest.0[..8]);
            u64::from_le_bytes(key) < threshold
        })
        .collect()
}
