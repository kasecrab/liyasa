//! Semantic callouts and the custom one (CMP-20, CMP-21).
//!
//! The six semantic callouts differ only in their name, their default title,
//! and their icon; GitHub's alert syntax maps onto them (§7.4). RX-61 says a
//! callout serializes as a block quote with a bold label, which is the one
//! shape every Markdown reader renders as an aside.

use liyasa_core::components::{ComponentInst, PropType, RenderError};

use crate::props::Reader;
use crate::render::{HtmlCtx, MarkdownCtx, Render};
use crate::schema::one_of;
use crate::{declare, text};

/// What a callout needs beyond its props.
struct Style {
    /// `note`, `tip`, …; the value of `data-variant`.
    variant: &'static str,
    title: &'static str,
    icon: &'static str,
}

fn render_html(
    inst: &ComponentInst,
    ctx: &mut HtmlCtx<'_>,
    style: &Style,
    schema: &crate::PropSchema,
    color: Option<&str>,
) -> Result<(), RenderError> {
    let props = Reader::of(inst, schema);
    let title = props.str_or("title", style.title);
    let icon = props.str_or("icon", style.icon);
    let collapsible = props.bool("collapsible");
    let open = props.bool("open");

    let tag = if collapsible { "details" } else { "aside" };
    ctx.out
        .open(tag)
        .attr("class", &format!("ly-callout ly-callout-{}", style.variant))
        .attr("data-liyasa", "callout")
        // Only a collapsible callout is a `<details>`, and only a `<details>`
        // has a toggle for `accordion.js` to hook.
        .flag_if("data-ly-accordion", collapsible)
        .attr("data-variant", style.variant)
        .attr_if("data-color", color)
        .attr_if(
            "data-callout-variant",
            props.str("variant").filter(|_| color.is_some()),
        );
    if collapsible {
        ctx.out.flag_if("open", open);
    } else {
        ctx.out.attr("aria-label", title);
    }

    ctx.out
        .open(if collapsible { "summary" } else { "p" })
        .attr("class", "ly-callout-title");
    if !icon.is_empty() {
        ctx.out
            .open("span")
            .attr("class", "ly-icon")
            .attr("data-icon", icon)
            .attr("aria-hidden", "true")
            .close();
    }
    ctx.out.text(title).close();

    ctx.out.open("div").attr("class", "ly-callout-body");
    ctx.children(&inst.children)?;
    ctx.out.close().close();
    Ok(())
}

fn render_markdown(
    inst: &ComponentInst,
    ctx: &mut MarkdownCtx<'_>,
    style: &Style,
    schema: &crate::PropSchema,
) -> Result<(), RenderError> {
    let props = Reader::of(inst, schema);
    let title = props.str_or("title", style.title).to_owned();
    let quote = ctx.out.push_quote();
    ctx.out
        .paragraph(&format!("**{}**", crate::md::escape_inline(&title)));
    let result = ctx.children(&inst.children);
    ctx.out.pop(quote);
    result
}

macro_rules! callout {
    ($type:ident, $name:literal, $title:literal, $icon:literal) => {
        declare! {
            #[doc = concat!("The `", $name, "` callout (CMP-20).")]
            pub struct $type;
            name = $name;
            kind = Container;
            editor = ($icon, "Callouts");
            props = [
                ("title", PropType::Str, Optional, "Heading shown above the body; defaults to the callout's name."),
                ("icon", PropType::Icon, Optional, "Overrides the default icon. An empty value removes it."),
                ("collapsible", PropType::Bool, Optional, "Renders the callout as a disclosure the reader can fold away."),
                ("open", PropType::Bool, Optional, "Starts a collapsible callout open."),
            ];
        }

        impl $type {
            const STYLE: Style = Style {
                variant: $name,
                title: $title,
                icon: $icon,
            };
        }

        impl Render for $type {
            fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
                render_html(inst, ctx, &Self::STYLE, Self::schema_of(), None)
            }

            fn markdown(
                &self,
                inst: &ComponentInst,
                ctx: &mut MarkdownCtx<'_>,
            ) -> Result<(), RenderError> {
                render_markdown(inst, ctx, &Self::STYLE, Self::schema_of())
            }

            fn text(&self, inst: &ComponentInst) -> String {
                let props = Reader::of(inst, Self::schema_of());
                text::with_titles(&[props.str_or("title", $title)], &inst.children)
            }
        }
    };
}

callout!(Note, "note", "Note", "info");
callout!(Tip, "tip", "Tip", "lightbulb");
callout!(Warning, "warning", "Warning", "triangle-alert");
callout!(Info, "info", "Info", "circle-info");
callout!(Check, "check", "Check", "circle-check");
callout!(Danger, "danger", "Danger", "octagon-alert");

declare! {
    /// A callout whose colour and shape the author chooses (CMP-21).
    pub struct Callout;
    name = "callout";
    kind = Container;
    editor = ("megaphone", "Callouts");
    props = [
        ("title", PropType::Str, Optional, "Heading shown above the body."),
        ("icon", PropType::Icon, Optional, "Icon shown beside the title."),
        ("color", PropType::Color, Optional, "Accent colour: a theme token name or a hex value."),
        ("variant", one_of(&["soft", "outline", "solid"]), Default(crate::schema::text("soft")), "How strongly the colour is applied."),
        ("collapsible", PropType::Bool, Optional, "Renders the callout as a disclosure the reader can fold away."),
        ("open", PropType::Bool, Optional, "Starts a collapsible callout open."),
    ];
}

impl Callout {
    const STYLE: Style = Style {
        variant: "callout",
        title: "",
        icon: "",
    };
}

impl Render for Callout {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let color = props.str("color").unwrap_or("");
        render_html(
            inst,
            ctx,
            &Self::STYLE,
            Self::schema_of(),
            Some(if color.is_empty() { "default" } else { color }),
        )
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        // A custom callout with no title has no label to bold, so it serializes
        // as a plain quote rather than an empty one.
        match props.str("title") {
            Some(title) if !title.is_empty() => {
                render_markdown(inst, ctx, &Self::STYLE, Self::schema_of())
            }
            _ => ctx.quote_children(&inst.children),
        }
    }

    fn text(&self, inst: &ComponentInst) -> String {
        let props = Reader::of(inst, Self::schema_of());
        text::with_titles(&[props.str_or("title", "")], &inst.children)
    }
}
