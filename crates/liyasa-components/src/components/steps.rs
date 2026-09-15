//! Numbered steps (CMP-13).
//!
//! An `<ol>`, so a screen reader announces "step 3 of 7" without being told.
//! RX-61 serializes it as an ordered list whose items begin with the step
//! title.

use liyasa_core::components::{ComponentInst, PropType, RenderError};

use crate::nodes;
use crate::props::Reader;
use crate::render::{HtmlCtx, MarkdownCtx, Render};
use crate::schema::{one_of, text as default_text};
use crate::{anchor, declare, text};

declare! {
    /// A sequence of steps (CMP-13).
    pub struct Steps;
    name = "steps";
    aliases = ["Steps"];
    kind = Container;
    editor = ("list-ordered", "Disclosure");
    props = [
        ("style", one_of(&["numbered", "icon"]), Default(default_text("numbered")), "Whether each step shows its number or its icon."),
        ("start", PropType::Num, Optional, "Number the first step carries."),
    ];
}

fn steps_of(inst: &ComponentInst) -> Vec<ComponentInst> {
    nodes::component_children(&inst.children, &["step", "Step"]).collect()
}

impl Render for Steps {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let style = props.str_or("style", "numbered");
        let start = props.int("start").unwrap_or(1);
        ctx.out
            .open("ol")
            .attr("class", "ly-steps")
            .attr("data-liyasa", "steps")
            .attr("data-style", style);
        if start != 1 {
            ctx.out.attr("start", &start.to_string());
        }

        let steps = steps_of(inst);
        for (at, step) in steps.iter().enumerate() {
            let step_props = Reader::of(step, Step::schema_of());
            let title = step_props.str("title");
            let number = step_props.int("number").unwrap_or(start + at as i64);
            let id = match title.map(anchor::slug) {
                Some(slug) if !slug.is_empty() => slug,
                _ => format!("step-{number}"),
            };
            ctx.out
                .open("li")
                .attr("class", "ly-step")
                .attr("id", &id)
                .attr("data-step", &number.to_string());
            ctx.out
                .open("div")
                .attr("class", "ly-step-marker")
                .attr("aria-hidden", "true");
            match (style, step_props.str("icon")) {
                ("icon", Some(icon)) => {
                    ctx.out
                        .open("span")
                        .attr("class", "ly-icon")
                        .attr("data-icon", icon)
                        .close();
                }
                _ => {
                    ctx.out.text(&number.to_string());
                }
            }
            ctx.out.close();
            ctx.out.open("div").attr("class", "ly-step-body");
            if let Some(title) = title {
                ctx.out
                    .open("p")
                    .attr("class", "ly-step-title")
                    .text(title)
                    .close();
            }
            ctx.children(&step.children)?;
            ctx.out.close().close();
        }
        ctx.out.close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let start = props.int("start").unwrap_or(1);
        ctx.out.block();
        for (at, step) in steps_of(inst).into_iter().enumerate() {
            let step_props = Reader::of(&step, Step::schema_of());
            let number = step_props.int("number").unwrap_or(start + at as i64);
            let title = step_props.str("title").map(str::to_owned);
            let item = ctx.out.push_item(&format!("{number}. "));
            if let Some(title) = &title {
                ctx.out
                    .line(&format!("**{}**", crate::md::escape_inline(title)));
            }
            let result = ctx.children(&step.children);
            ctx.out.pop(item);
            result?;
        }
        Ok(())
    }

    fn text(&self, inst: &ComponentInst) -> String {
        text::of(&inst.children)
    }
}

declare! {
    /// One step of a [`Steps`] sequence (CMP-13).
    pub struct Step;
    name = "step";
    aliases = ["Step"];
    kind = Container;
    editor = ("circle-dot", "Disclosure");
    props = [
        ("title", PropType::Str, Optional, "What this step does; becomes the step's anchor."),
        ("icon", PropType::Icon, Optional, "Icon shown in the marker when the group's style is `icon`."),
        ("number", PropType::Num, Optional, "Overrides the number this step would otherwise get."),
    ];
}

impl Render for Step {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        // Reached only outside a `steps` group, which owns the `<ol>`.
        let props = Reader::of(inst, Self::schema_of());
        let title = props.str_or("title", "Step");
        ctx.out
            .open("section")
            .attr("class", "ly-step ly-step-standalone")
            .attr("data-liyasa", "step")
            .attr("id", &anchor::slug(title));
        ctx.out
            .open("p")
            .attr("class", "ly-step-title")
            .text(title)
            .close();
        ctx.children(&inst.children)?;
        ctx.out.close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let title = props.str_or("title", "Step").to_owned();
        let item = ctx.out.push_item("1. ");
        ctx.out
            .line(&format!("**{}**", crate::md::escape_inline(&title)));
        let result = ctx.children(&inst.children);
        ctx.out.pop(item);
        result
    }

    fn text(&self, inst: &ComponentInst) -> String {
        let props = Reader::of(inst, Self::schema_of());
        text::with_titles(&[props.str_or("title", "")], &inst.children)
    }
}
