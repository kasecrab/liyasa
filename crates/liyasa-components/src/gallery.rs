//! The browser fixture behind `web/e2e/components/` (§34.10).
//!
//! The golden files under `tests/golden/expected/` pin the bytes; a browser
//! cannot read them, so the same instances are rendered into real pages here
//! for the axe and keyboard runs. The cases are the interactive and P0 set the
//! acceptance criteria name, not every golden case: a violation is a property
//! of the markup, and rendering the same element forty times finds nothing the
//! first one does not.

use liyasa_core::components::ComponentInst;
use liyasa_core::document::{Inline, Node, PropValue};

use crate::registry::Registry;
use crate::render::HtmlCtx;
use crate::{Reference, html, inst, nodes};

/// One instance a spec can address by its `id`.
pub struct Case {
    pub label: &'static str,
    pub inst: ComponentInst,
}

fn case(label: &'static str, builder: inst::Builder) -> Case {
    Case {
        label,
        inst: builder.build(),
    }
}

fn str(value: &str) -> PropValue {
    PropValue::Str(value.to_owned())
}

/// A slug a spec can use as a selector, derived from the case label.
pub fn slug(label: &str) -> String {
    crate::anchor::slug(label)
}

/// Every component the fixture has a page for.
pub fn names() -> Vec<&'static str> {
    vec![
        "accordion",
        "badge",
        "callout",
        "card",
        "cards",
        "code-block",
        "code-group",
        "columns",
        "expandable",
        "file",
        "frame",
        "icon",
        "iframe",
        "image",
        "kbd",
        "note",
        "param",
        "request-example",
        "response-field",
        "steps",
        "tabs",
        "video",
    ]
}

/// The instances the fixture page for `component` shows.
pub fn cases(component: &str) -> Vec<Case> {
    match component {
        "card" => vec![
            case(
                "default",
                inst::new("card")
                    .prop("title", str("Quickstart"))
                    .child(nodes::paragraph("Get running in five minutes.")),
            ),
            case(
                "linked",
                inst::new("card")
                    .prop("title", str("Configuration"))
                    .prop("href", str("/guide/configuration"))
                    .prop("icon", str("settings"))
                    .prop("cta", str("Read the guide"))
                    .prop("arrow", PropValue::Bool(true))
                    .child(nodes::paragraph("One file configures a site.")),
            ),
        ],
        "cards" => vec![case(
            "grid",
            inst::new("cards")
                .prop("cols", PropValue::Num(2.0))
                .child(inst::nested(
                    inst::new("card")
                        .prop("title", str("Install"))
                        .prop("href", str("/guide/install"))
                        .child(nodes::paragraph("Get the binary.")),
                ))
                .child(inst::nested(
                    inst::new("card")
                        .prop("title", str("Configure"))
                        .prop("href", str("/guide/configuration"))
                        .child(nodes::paragraph("Write liyasa.json.")),
                )),
        )],
        "columns" => vec![case(
            "two",
            inst::new("columns")
                .prop("cols", PropValue::Num(2.0))
                .child(inst::nested(
                    inst::new("column").child(nodes::paragraph("On the left.")),
                ))
                .child(inst::nested(
                    inst::new("column").child(nodes::paragraph("On the right.")),
                )),
        )],
        "frame" => vec![case(
            "captioned",
            inst::new("frame")
                .prop("caption", str("The build output"))
                .child(image_node("/favicon.svg", "A square logo")),
        )],
        "accordion" => vec![
            case(
                "closed",
                inst::new("accordion")
                    .prop("title", str("What is a fact?"))
                    .child(nodes::paragraph("A value with a source.")),
            ),
            case(
                "open",
                inst::new("accordion")
                    .prop("title", str("What is a snippet?"))
                    .prop("open", PropValue::Bool(true))
                    .child(nodes::paragraph("A code region read from a file.")),
            ),
            case(
                "grouped",
                inst::new("accordions")
                    .prop("one", PropValue::Bool(true))
                    .child(inst::nested(
                        inst::new("accordion")
                            .prop("title", str("First"))
                            .child(nodes::paragraph("One.")),
                    ))
                    .child(inst::nested(
                        inst::new("accordion")
                            .prop("title", str("Second"))
                            .child(nodes::paragraph("Two.")),
                    )),
            ),
        ],
        "expandable" => vec![case(
            "nested fields",
            inst::new("expandable")
                .prop("title", str("filter properties"))
                .child(inst::nested(
                    inst::new("response-field")
                        .prop("name", str("status"))
                        .prop("type", str("string"))
                        .child(nodes::paragraph("Only items in this status.")),
                )),
        )],
        "tabs" => vec![
            case(
                "install",
                inst::new("tabs")
                    .prop("title", str("Install"))
                    .child(inst::nested(
                        inst::new("tab")
                            .prop("title", str("npm"))
                            .child(nodes::code_block(Some("sh"), "npm i liyasa\n")),
                    ))
                    .child(inst::nested(
                        inst::new("tab")
                            .prop("title", str("cargo"))
                            .child(nodes::code_block(Some("sh"), "cargo install liyasa\n")),
                    ))
                    .child(inst::nested(
                        inst::new("tab")
                            .prop("title", str("brew"))
                            .child(nodes::code_block(Some("sh"), "brew install liyasa\n")),
                    )),
            ),
            case(
                "synced",
                inst::new("tabs")
                    .prop("sync", str("lang"))
                    .child(inst::nested(
                        inst::new("tab")
                            .prop("title", str("Rust"))
                            .prop("sync", str("rust"))
                            .child(nodes::paragraph("The Rust client.")),
                    ))
                    .child(inst::nested(
                        inst::new("tab")
                            .prop("title", str("Python"))
                            .prop("sync", str("python"))
                            .child(nodes::paragraph("The Python client.")),
                    )),
            ),
        ],
        "steps" => vec![case(
            "three",
            inst::new("steps")
                .child(inst::nested(
                    inst::new("step")
                        .prop("title", str("Install"))
                        .child(nodes::paragraph("Run the installer.")),
                ))
                .child(inst::nested(
                    inst::new("step")
                        .prop("title", str("Configure"))
                        .child(nodes::paragraph("Write liyasa.json.")),
                ))
                .child(inst::nested(
                    inst::new("step")
                        .prop("title", str("Build"))
                        .child(nodes::paragraph("Run liyasa build.")),
                )),
        )],
        "note" => ["note", "tip", "warning", "info", "check", "danger"]
            .iter()
            .map(|name| Case {
                label: name,
                inst: inst::new(name)
                    .prop("title", str("Heads up"))
                    .child(nodes::paragraph("Something worth reading."))
                    .build(),
            })
            .collect(),
        "callout" => vec![
            case(
                "custom",
                inst::new("callout")
                    .prop("title", str("Preview"))
                    .prop("variant", str("outline"))
                    .child(nodes::paragraph("This feature is in preview.")),
            ),
            case(
                "collapsible",
                inst::new("callout")
                    .prop("title", str("Why this matters"))
                    .prop("collapsible", PropValue::Bool(true))
                    .child(nodes::paragraph("The long version.")),
            ),
        ],
        "code-block" => vec![case(
            "titled",
            inst::new("terminal")
                .prop("title", str("Build the site"))
                .prop("prompt", str("$ "))
                .child(nodes::code_block(Some("sh"), "$ liyasa build\n")),
        )],
        "code-group" => vec![case(
            "install",
            inst::new("code-group")
                .child(titled_code("sh", "npm", "npm i liyasa\n"))
                .child(titled_code("sh", "cargo", "cargo install liyasa\n")),
        )],
        "param" => vec![case(
            "query",
            inst::new("param")
                .prop("name", str("limit"))
                .prop("in", str("query"))
                .prop("type", str("integer"))
                .prop("required", PropValue::Bool(true))
                .prop("default", str("20"))
                .child(nodes::paragraph("How many items to return.")),
        )],
        "response-field" => vec![case(
            "id",
            inst::new("response-field")
                .prop("name", str("id"))
                .prop("type", str("string"))
                .prop("required", PropValue::Bool(true))
                .child(nodes::paragraph("The item's identifier.")),
        )],
        "request-example" => vec![
            case(
                "request",
                inst::new("request-example")
                    .prop("lang", str("curl"))
                    .prop("title", str("cURL"))
                    .child(nodes::code_block(
                        Some("sh"),
                        "curl https://api.example.com/v1/items\n",
                    )),
            ),
            case(
                "response",
                inst::new("response-example")
                    .prop("lang", str("json"))
                    .prop("status", str("200"))
                    .child(nodes::code_block(Some("json"), "{\n  \"ok\": true\n}\n")),
            ),
        ],
        "image" => vec![case(
            "captioned",
            inst::new("image")
                .prop("src", str("/favicon.svg"))
                .prop("alt", str("The Liyasa mark"))
                .prop("caption", str("The mark at its smallest size")),
        )],
        "video" => vec![case(
            "local",
            inst::new("video")
                .prop("src", str("/media/build.mp4"))
                .prop("controls", PropValue::Bool(true))
                .prop("caption", str("A build from a cold cache")),
        )],
        "iframe" => vec![case(
            "sandboxed",
            inst::new("iframe")
                .prop("src", str("/guide/install"))
                .prop("title", str("The install guide"))
                .prop("height", PropValue::Num(240.0)),
        )],
        "file" => vec![case(
            "download",
            inst::new("file")
                .prop("src", str("/favicon.svg"))
                .prop("name", str("favicon.svg"))
                .prop("size", str("312 B")),
        )],
        "badge" => vec![case(
            "status",
            inst::new("badge")
                .prop("color", str("green"))
                .children(text("Stable")),
        )],
        "icon" => vec![case(
            "labelled",
            inst::new("icon")
                .prop("name", str("rocket"))
                .prop("label", str("Ship it")),
        )],
        "kbd" => vec![case("shortcut", inst::new("kbd").children(text("Ctrl+K")))],
        _ => Vec::new(),
    }
}

fn text(value: &str) -> Vec<Node> {
    vec![Node::Inline(Inline::Text(value.to_owned()))]
}

/// A fence with a title, which is what a code group keys its tabs by.
fn titled_code(lang: &str, title: &str, body: &str) -> Node {
    let mut attrs = liyasa_core::document::FenceAttrs::default();
    attrs.kv.insert("title".to_owned(), title.to_owned());
    nodes::code_block_with(Some(lang), body, attrs)
}

fn image_node(src: &str, alt: &str) -> Node {
    nodes::paragraph_of(vec![Inline::Image {
        src: src.to_owned(),
        alt: alt.to_owned(),
        title: None,
        dark: None,
    }])
}

/// A whole HTML document for one component, ready to serve.
///
/// The shell is deliberately thin — a landmark, one `h1`, one `h2` per case —
/// so an axe violation is a property of the component and not of a page
/// template the components do not own.
pub fn page(component: &str, stylesheet: &str, script: &str) -> String {
    let registry = Registry::builtins();
    let reference = Reference::with(&registry);

    let mut body = html::Html::new();
    for case in cases(component) {
        let Some(rendered) = registry.resolve(&case.inst.name) else {
            continue;
        };
        body.open("section")
            .attr("class", "ly-gallery-case")
            .attr("data-case", case.label)
            .attr("id", &format!("case-{}", slug(case.label)));
        body.open("h2").text(case.label).close();

        let mut ctx = HtmlCtx::new(&reference);
        match rendered.html(&case.inst, &mut ctx) {
            Ok(()) => body.raw(&ctx.finish()),
            Err(error) => body.open("p").text(&error.to_string()).close(),
        };
        body.close();
    }

    let mut out = html::Html::with_capacity(4096);
    out.raw("<!doctype html>\n");
    out.open("html").attr("lang", "en");
    out.open("head");
    // `meta` and `link` are void: `Html` never puts them on the stack, so a
    // `close()` here would close the `head` instead.
    out.open("meta").attr("charset", "utf-8");
    out.open("meta")
        .attr("name", "viewport")
        .attr("content", "width=device-width, initial-scale=1");
    out.open("title")
        .text(&format!("{component} — component gallery"))
        .close();
    out.open("link")
        .attr("rel", "stylesheet")
        .attr("href", stylesheet);
    // Without an icon the browser asks for /favicon.ico and the 404 is a
    // console error the audit counts (RX-10).
    out.open("link")
        .attr("rel", "icon")
        .attr("href", "/favicon.svg");
    out.close();
    out.open("body").attr("class", "ly-body");
    out.open("main").attr("class", "ly-main").attr("id", "main");
    out.open("h1").text(&format!("{component} gallery")).close();
    out.raw(&body.finish());
    out.close();
    out.open("script").attr("src", script).flag("defer").close();
    out.close();
    out.close();
    out.finish()
}
