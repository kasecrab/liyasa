//! Accordions and expandables (CMP-10, CMP-11).
//!
//! Both are `<details>`: the disclosure widget the platform already makes
//! keyboard reachable, focusable, and findable by in-page search. RX-61 turns
//! an accordion into an H3 section, because a reader with no renderer must see
//! the folded content, not a summary of it.

use liyasa_core::components::{ComponentInst, PropType, RenderError};

use crate::props::Reader;
use crate::render::{HtmlCtx, MarkdownCtx, Render};
use crate::{anchor, declare, text};

/// The anchor an accordion answers to: its `id`, else its title, else its
/// block ID, which is stable across builds.
fn anchor_of(inst: &ComponentInst, props: &Reader<'_>) -> String {
    if let Some(id) = props.str("id") {
        return id.to_owned();
    }
    match props.str("title").map(anchor::slug) {
        Some(slug) if !slug.is_empty() => slug,
        _ => format!("a-{}", inst.id.to_hex()),
    }
}

declare! {
    /// A titled disclosure that the URL hash can open (CMP-10).
    pub struct Accordion;
    name = "accordion";
    aliases = ["Accordion"];
    kind = Container;
    editor = ("chevron-right", "Disclosure");
    props = [
        ("title", PropType::Str, Optional, "Summary line the reader clicks."),
        ("icon", PropType::Icon, Optional, "Icon shown before the title."),
        ("open", PropType::Bool, Optional, "Starts open."),
        ("id", PropType::Str, Optional, "Anchor for the URL hash; defaults to a slug of the title."),
    ];
}

impl Render for Accordion {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let id = anchor_of(inst, &props);
        ctx.out
            .open("details")
            .attr("class", "ly-accordion")
            .attr("data-liyasa", "accordion")
            .attr("id", &id)
            .flag_if("open", props.bool("open"));
        ctx.out.open("summary").attr("class", "ly-accordion-title");
        if let Some(icon) = props.str("icon") {
            ctx.out
                .open("span")
                .attr("class", "ly-icon")
                .attr("data-icon", icon)
                .attr("aria-hidden", "true")
                .close();
        }
        ctx.out.text(props.str_or("title", "Details")).close();
        ctx.out.open("div").attr("class", "ly-accordion-body");
        ctx.children(&inst.children)?;
        ctx.out.close().close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        ctx.out.heading(3, props.str_or("title", "Details"));
        ctx.children(&inst.children)
    }

    fn text(&self, inst: &ComponentInst) -> String {
        let props = Reader::of(inst, Self::schema_of());
        text::with_titles(&[props.str_or("title", "")], &inst.children)
    }
}

declare! {
    /// A group of accordions, optionally with only one open at a time (CMP-10).
    pub struct Accordions;
    name = "accordions";
    aliases = ["AccordionGroup", "accordion-group"];
    kind = Container;
    editor = ("list-collapse", "Disclosure");
    props = [
        // TODO(rfc-0006): the requirement row names the group without naming
        // the prop that turns one-open mode on.
        ("one", PropType::Bool, Optional, "Opening one accordion closes the others."),
    ];
}

impl Render for Accordions {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        ctx.out
            .open("div")
            .attr("class", "ly-accordions")
            .attr("data-liyasa", "accordions")
            .flag_if("data-one", props.bool("one"));
        ctx.children(&inst.children)?;
        ctx.out.close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        ctx.children(&inst.children)
    }
}

declare! {
    /// A lightweight disclosure for a nested field (CMP-11).
    pub struct Expandable;
    name = "expandable";
    aliases = ["Expandable"];
    kind = Container;
    editor = ("chevron-down", "Disclosure");
    props = [
        ("title", PropType::Str, Optional, "Summary line the reader clicks."),
        ("open", PropType::Bool, Optional, "Starts open."),
    ];
}

impl Render for Expandable {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        ctx.out
            .open("details")
            .attr("class", "ly-expandable")
            .attr("data-liyasa", "expandable")
            .flag_if("open", props.bool("open"));
        ctx.out
            .open("summary")
            .attr("class", "ly-expandable-title")
            .text(props.str_or("title", "Show more"))
            .close();
        ctx.out.open("div").attr("class", "ly-expandable-body");
        ctx.children(&inst.children)?;
        ctx.out.close().close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        // Not a heading: an expandable holds a nested field of the row above
        // it, and a heading there would break the document outline.
        ctx.out.paragraph(&format!(
            "**{}**",
            crate::md::escape_inline(props.str_or("title", "Show more"))
        ));
        ctx.children(&inst.children)
    }

    fn text(&self, inst: &ComponentInst) -> String {
        let props = Reader::of(inst, Self::schema_of());
        text::with_titles(&[props.str_or("title", "")], &inst.children)
    }
}

declare! {
    /// A group of expandables (CMP-11).
    pub struct Expandables;
    name = "expandables";
    aliases = ["ExpandableGroup", "expandable-group"];
    kind = Container;
    editor = ("list-tree", "Disclosure");
    props = [];
}

impl Render for Expandables {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        ctx.out
            .open("div")
            .attr("class", "ly-expandables")
            .attr("data-liyasa", "expandables");
        ctx.children(&inst.children)?;
        ctx.out.close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        ctx.children(&inst.children)
    }
}
