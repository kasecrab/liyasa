//! Code groups, inline code, terminals, and file-backed snippets
//! (CMP-31 to CMP-34).

use liyasa_core::components::{ComponentInst, PropType, RenderError};
use liyasa_core::document::{Block, BlockKind, Dep, DepTarget, Node};

use crate::props::Reader;
use crate::render::{HtmlCtx, MarkdownCtx, Render};
use crate::{declare, deps, fence, text};

declare! {
    /// Fenced blocks keyed by title, shown as tabs or a dropdown (CMP-31).
    pub struct CodeGroup;
    name = "code-group";
    aliases = ["CodeGroup", "codegroup"];
    kind = Container;
    editor = ("code", "Code");
    props = [
        ("sync", PropType::Str, Optional, "Synchronizes every group with the same key site-wide and remembers the reader's choice."),
        ("dropdown", PropType::Bool, Optional, "Shows a select instead of a row of tabs."),
    ];
}

/// The fences in a group, with the title each one is keyed by.
fn blocks_of(inst: &ComponentInst) -> Vec<(&Block, String, fence::CodeOptions)> {
    inst.children
        .iter()
        .filter_map(|node| match node {
            Node::Block(block) => match &block.kind {
                BlockKind::CodeBlock { lang, attrs, .. } => {
                    let options = fence::CodeOptions::read(attrs);
                    let title = options
                        .title
                        .clone()
                        .or_else(|| lang.clone())
                        .unwrap_or_else(|| "Code".to_owned());
                    Some((block, title, options))
                }
                _ => None,
            },
            Node::Inline(_) => None,
        })
        .collect()
}

fn code_body(block: &Block) -> String {
    text::of(&block.children)
}

fn lang_of(block: &Block) -> Option<&str> {
    match &block.kind {
        BlockKind::CodeBlock { lang, .. } => lang.as_deref(),
        _ => None,
    }
}

impl Render for CodeGroup {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let group = format!("cg-{}", inst.id.to_hex());
        let dropdown = props.bool("dropdown");
        let blocks = blocks_of(inst);

        ctx.out
            .open("div")
            .attr("class", "ly-code-group")
            .attr("data-liyasa", "code-group")
            .attr_if("data-sync", props.str("sync"))
            .flag_if("data-dropdown", dropdown);

        if dropdown {
            ctx.out
                .open("label")
                .attr("class", "ly-visually-hidden")
                .attr("for", &format!("{group}-select"))
                .text("Choose a variant")
                .close();
            ctx.out
                .open("select")
                .attr("class", "ly-code-group-select")
                .attr("id", &format!("{group}-select"));
            for (at, (_, title, _)) in blocks.iter().enumerate() {
                ctx.out
                    .open("option")
                    .attr("value", &at.to_string())
                    .flag_if("selected", at == 0)
                    .text(title)
                    .close();
            }
            ctx.out.close();
        } else {
            ctx.out
                .open("div")
                .attr("class", "ly-tablist")
                .attr("role", "tablist");
            for (at, (_, title, _)) in blocks.iter().enumerate() {
                let selected = at == 0;
                ctx.out
                    .open("button")
                    .attr("class", "ly-tab")
                    .attr("type", "button")
                    .attr("role", "tab")
                    .attr("id", &format!("{group}-tab-{at}"))
                    .attr("aria-controls", &format!("{group}-panel-{at}"))
                    .attr("aria-selected", if selected { "true" } else { "false" })
                    .attr("tabindex", if selected { "0" } else { "-1" })
                    .text(title)
                    .close();
            }
            ctx.out.close();
        }

        for (at, (block, _, options)) in blocks.iter().enumerate() {
            ctx.out
                .open("div")
                .attr("class", "ly-tabpanel")
                .attr("role", "tabpanel")
                .attr("id", &format!("{group}-panel-{at}"))
                .attr("aria-labelledby", &format!("{group}-tab-{at}"))
                .attr("tabindex", "0")
                .flag_if("hidden", at > 0);
            let highlighted = match &block.kind {
                BlockKind::CodeBlock { highlighted, .. } => highlighted.clone(),
                _ => None,
            };
            fence::render_html(
                &mut ctx.out,
                lang_of(block),
                &code_body(block),
                options,
                highlighted.as_deref(),
            );
            ctx.out.close();
        }
        ctx.out.close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        // RX-61: consecutive fences, each carrying its title.
        // TODO(rfc-0006): RX-61 says "title comments"; the title rides in the
        // info string instead, which needs no per-language comment syntax.
        for (block, title, options) in blocks_of(inst) {
            let mut options = options;
            options.title = Some(title);
            fence::render_markdown(&mut ctx.out, lang_of(block), &code_body(block), &options);
        }
        Ok(())
    }
}

declare! {
    /// A highlighted inline code span (CMP-32).
    pub struct Code;
    name = "code";
    aliases = ["Code"];
    kind = Inline;
    editor = ("code", "Code");
    props = [
        ("lang", PropType::Str, Optional, "Language the span is highlighted as."),
    ];
}

impl Render for Code {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let lang = props.str("lang");
        ctx.out
            .open("code")
            .attr(
                "class",
                &match lang {
                    Some(lang) => format!("ly-code-inline language-{lang}"),
                    None => "ly-code-inline".to_owned(),
                },
            )
            .attr("data-liyasa", "code")
            .attr_if("data-lang", lang);
        ctx.children(&inst.children)?;
        ctx.out.close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        ctx.out
            .write(&crate::md::code_span(&text::of(&inst.children)));
        Ok(())
    }

    fn text(&self, inst: &ComponentInst) -> String {
        text::of(&inst.children)
    }
}

declare! {
    /// A shell session, styled and with the prompt stripped on copy (CMP-33).
    pub struct Terminal;
    name = "terminal";
    aliases = ["Terminal"];
    kind = Container;
    editor = ("terminal", "Code");
    props = [
        ("title", PropType::Str, Optional, "Window title shown above the session."),
        ("prompt", PropType::Str, Default(crate::schema::text("$ ")), "Prompt prefix; the copy button removes it."),
    ];
}

impl Render for Terminal {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        ctx.out
            .open("div")
            .attr("class", "ly-terminal")
            .attr("data-liyasa", "terminal")
            .attr("data-prompt", props.str_or("prompt", "$ "));
        ctx.out
            .open("div")
            .attr("class", "ly-terminal-bar")
            .attr("aria-hidden", "true")
            .close();
        if let Some(title) = props.str("title") {
            ctx.out
                .open("p")
                .attr("class", "ly-terminal-title")
                .text(title)
                .close();
        }
        ctx.out.open("div").attr("class", "ly-terminal-body");
        ctx.children(&inst.children)?;
        ctx.out.close().close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        ctx.out.block();
        ctx.children(&inst.children)
    }

    fn text(&self, inst: &ComponentInst) -> String {
        let props = Reader::of(inst, Self::schema_of());
        text::with_titles(&[props.str_or("title", "")], &inst.children)
    }
}

declare! {
    /// A code region pulled from a file so the sample cannot drift (CMP-34).
    pub struct SnippetFrom;
    name = "snippet-from";
    aliases = ["SnippetFrom", "code-from"];
    kind = Leaf;
    editor = ("file-code", "Code");
    props = [
        ("file", PropType::Str, Required, "Path to the file, relative to the repository root."),
        ("lines", PropType::Str, Optional, "Line range, e.g. `10-25`. Mutually exclusive with `symbol`."),
        ("symbol", PropType::Str, Optional, "Name of a `// [liyasa:start name]` marker region, or of a symbol the language server can find."),
        ("repo", PropType::Str, Optional, "Connected repository the file lives in; defaults to this one."),
        ("ref", PropType::Str, Optional, "Branch, tag, or commit to read the file at."),
        ("lang", PropType::Str, Optional, "Language to highlight as; defaults to the file's extension."),
        ("title", PropType::Str, Optional, "Title shown above the block; defaults to the file path."),
    ];
    deps = SnippetFrom::snippet_deps;
}

impl Render for SnippetFrom {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let file = props.str_or("file", "");
        // The body arrives from the build, which has the file; the element
        // carries everything needed to fetch and to verify it.
        ctx.out
            .open("div")
            .attr("class", "ly-snippet")
            .attr("data-liyasa", "snippet-from")
            .attr("data-file", file)
            .attr_if("data-lines", props.str("lines"))
            .attr_if("data-symbol", props.str("symbol"))
            .attr_if("data-repo", props.str("repo"))
            .attr_if("data-ref", props.str("ref"))
            .attr_if("data-lang", props.str("lang"))
            .attr_if("data-title", props.str("title"))
            .close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let file = props.str_or("file", "");
        let region = match (props.str("lines"), props.str("symbol")) {
            (Some(lines), _) => format!("{file}:{lines}"),
            (None, Some(symbol)) => format!("{file}#{symbol}"),
            (None, None) => file.to_owned(),
        };
        ctx.out.fence(
            props.str("lang").unwrap_or(""),
            &format!("// from {region}\n"),
        );
        Ok(())
    }

    fn text(&self, inst: &ComponentInst) -> String {
        Reader::of(inst, Self::schema_of())
            .str_or("file", "")
            .to_owned()
    }
}

impl SnippetFrom {
    /// The snippet's own edge, which the schema cannot express: a file region
    /// is `Includes`, not a link.
    pub fn snippet_deps(inst: &ComponentInst) -> Vec<Dep> {
        let props = Reader::of(inst, Self::schema_of());
        let mut edges = deps::from_schema(inst, Self::schema_of());
        if let Some(file) = props.str("file") {
            edges.push(deps::includes(
                inst,
                DepTarget::Source(match props.str("repo") {
                    Some(repo) => format!("{repo}:{file}"),
                    None => file.to_owned(),
                }),
            ));
        }
        edges
    }
}
