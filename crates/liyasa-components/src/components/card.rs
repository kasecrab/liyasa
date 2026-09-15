//! Cards and the grids that hold them (CMP-01, CMP-02).
//!
//! RX-61: a card serializes as a link in a list with its body as the
//! description, which is what a reader with no renderer can still follow.

use liyasa_core::components::{ComponentInst, PropType, RenderError};
use liyasa_core::diagnostics::{Diagnostic, code};

use crate::props::Reader;
use crate::render::{HtmlCtx, MarkdownCtx, Render};
use crate::schema::{num, one_of, text as default_text};
use crate::{declare, nodes, text};

declare! {
    /// A card: a titled block that is clickable when it has an `href` (CMP-01).
    pub struct Card;
    name = "card";
    aliases = ["Card"];
    kind = Container;
    editor = ("square", "Layout");
    props = [
        ("title", PropType::Str, Optional, "Card heading."),
        ("icon", PropType::Icon, Optional, "Icon shown above or beside the title."),
        ("href", PropType::Route, Optional, "Makes the whole card a link to this route or URL."),
        ("img", PropType::Asset, Optional, "Image shown on top, or on the left when `horizontal`."),
        ("horizontal", PropType::Bool, Optional, "Lays the image beside the body instead of above it."),
        ("cta", PropType::Str, Optional, "Call-to-action text shown at the foot of the card."),
        ("color", PropType::Color, Optional, "Accent colour: a theme token name or a hex value."),
        ("arrow", PropType::Bool, Optional, "Shows an arrow beside the call to action."),
    ];
}

impl Render for Card {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let href = props.url("href");
        let title = props.str("title");

        ctx.out
            .open(if href.is_some() { "a" } else { "div" })
            .attr("class", "ly-card")
            .attr("data-liyasa", "card")
            .attr_if("href", href)
            .attr_if("data-color", props.str("color"))
            .flag_if("data-horizontal", props.bool("horizontal"));

        if let Some(img) = props.url("img") {
            ctx.out
                .open("img")
                .attr("class", "ly-card-image")
                .attr("src", img)
                // The card's title is the accessible name; a decorative image
                // beside it must not repeat it.
                .attr("alt", "")
                .attr("loading", "lazy");
        }

        ctx.out.open("div").attr("class", "ly-card-body");
        if let Some(icon) = props.str("icon") {
            ctx.out
                .open("span")
                .attr("class", "ly-icon ly-card-icon")
                .attr("data-icon", icon)
                .attr("aria-hidden", "true")
                .close();
        }
        if let Some(title) = title {
            ctx.out
                .open("p")
                .attr("class", "ly-card-title")
                .text(title)
                .close();
        }
        if !nodes::is_blank(&inst.children) {
            ctx.out.open("div").attr("class", "ly-card-content");
            ctx.children(&inst.children)?;
            ctx.out.close();
        }
        if let Some(cta) = props.str("cta") {
            ctx.out.open("span").attr("class", "ly-card-cta").text(cta);
            if props.bool("arrow") {
                ctx.out
                    .open("span")
                    .attr("class", "ly-card-arrow")
                    .attr("aria-hidden", "true")
                    .close();
            }
            ctx.out.close();
        }
        ctx.out.close().close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let title = props.str("title").unwrap_or("Card").to_owned();
        let href = props.url("href").map(|href| ctx.absolute(href));
        let heading = match &href {
            Some(href) => crate::md::link(&title, href),
            None => format!("**{}**", crate::md::escape_inline(&title)),
        };
        let children = ctx.renderer();
        let body = inst.children.clone();
        let mut error = Ok(());
        ctx.out.item("- ", |md| {
            md.write(&heading);
            md.end_line();
            error = children.markdown(&body, md);
        });
        error
    }

    fn text(&self, inst: &ComponentInst) -> String {
        let props = Reader::of(inst, Self::schema_of());
        text::with_titles(
            &[props.str_or("title", ""), props.str_or("cta", "")],
            &inst.children,
        )
    }
}

declare! {
    /// A grid of cards (CMP-02).
    pub struct Cards;
    name = "cards";
    aliases = ["card-group", "CardGroup", "Cards"];
    kind = Container;
    editor = ("grid", "Layout");
    props = [
        ("cols", PropType::Num, Default(num(2)), "Columns in the grid, 1 to 4."),
        ("gap", PropType::Str, Optional, "Space between cards: a theme spacing token or a CSS length."),
    ];
}

impl Render for Cards {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        grid_html(inst, ctx, Self::schema_of(), "cards", &["card"], &[])
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        // The children are cards, and each one writes a list item, so the group
        // is the list RX-61 asks for with no extra framing.
        ctx.out.block();
        ctx.children(&inst.children)
    }
}

/// The shared body of `cards`, `columns`, and `tiles`.
///
/// `allowed` names the children the grid accepts; an empty list accepts
/// anything. A child of the wrong kind is reported as `E0354` and still
/// rendered, because dropping content is worse than showing it in the wrong
/// box.
pub(crate) fn grid_html(
    inst: &ComponentInst,
    ctx: &mut HtmlCtx<'_>,
    schema: &crate::PropSchema,
    kind: &str,
    allowed: &[&str],
    extra: &[(&str, &str)],
) -> Result<(), RenderError> {
    let props = Reader::of(inst, schema);
    let cols = props.int_in("cols", 1..=4).unwrap_or(2);
    let gap = props.str("gap");

    if !allowed.is_empty() {
        for child in &inst.children {
            let Some(child_inst) = nodes::as_component(child) else {
                continue;
            };
            if !allowed.contains(&child_inst.name.as_str()) {
                let diagnostic = Diagnostic::new(
                    code::E0354,
                    format!("`{}` holds `{}`", inst.name, child_inst.name),
                )
                .help(format!(
                    "`{}` accepts only: {}",
                    inst.name,
                    allowed.join(", ")
                ));
                ctx.report(match child_inst.origin.span {
                    Some(span) => diagnostic.at(span),
                    None => diagnostic,
                });
            }
        }
    }

    ctx.out
        .open("div")
        .attr("class", &format!("ly-{kind}"))
        .attr("data-liyasa", kind)
        .attr("data-cols", &cols.to_string())
        .attr(
            "style",
            &match gap {
                Some(gap) => format!("--ly-cols: {cols}; --ly-gap: {gap}"),
                None => format!("--ly-cols: {cols}"),
            },
        );
    for (name, value) in extra {
        ctx.out.attr(name, value);
    }
    ctx.children(&inst.children)?;
    ctx.out.close();
    Ok(())
}

declare! {
    /// A generic grid whose children are `column` blocks (CMP-03).
    pub struct Columns;
    name = "columns";
    aliases = ["Columns"];
    kind = Container;
    editor = ("columns", "Layout");
    props = [
        ("cols", PropType::Num, Default(num(2)), "Columns in the grid, 1 to 4."),
        ("gap", PropType::Str, Optional, "Space between columns: a theme spacing token or a CSS length."),
        ("align", one_of(&["start", "center", "end", "stretch"]), Default(default_text("stretch")), "How columns line up against each other vertically."),
    ];
}

impl Render for Columns {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let align = props.str_or("align", "stretch").to_owned();
        grid_html(
            inst,
            ctx,
            Self::schema_of(),
            "columns",
            &["column"],
            &[("data-align", &align)],
        )
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        ctx.out.block();
        ctx.children(&inst.children)
    }
}

declare! {
    /// One column of a [`Columns`] grid (CMP-03).
    pub struct Column;
    name = "column";
    aliases = ["Column"];
    kind = Container;
    editor = ("column", "Layout");
    props = [
        ("span", PropType::Num, Optional, "Columns this one spans, 1 to 4."),
    ];
}

impl Render for Column {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        ctx.out
            .open("div")
            .attr("class", "ly-column")
            .attr("data-liyasa", "column")
            .attr_if(
                "data-span",
                props
                    .int_in("span", 1..=4)
                    .map(|n| n.to_string())
                    .as_deref(),
            );
        ctx.children(&inst.children)?;
        ctx.out.close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        ctx.out.block();
        ctx.children(&inst.children)
    }
}

declare! {
    /// Compact icon tiles for a hub page (CMP-04).
    pub struct Tiles;
    name = "tiles";
    aliases = ["Tiles"];
    kind = Container;
    editor = ("layout-grid", "Layout");
    props = [
        ("cols", PropType::Num, Default(num(3)), "Columns in the grid, 1 to 4."),
    ];
}

impl Render for Tiles {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        grid_html(inst, ctx, Self::schema_of(), "tiles", &["tile"], &[])
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        ctx.out.block();
        ctx.children(&inst.children)
    }
}

declare! {
    /// One tile of a [`Tiles`] hub (CMP-04).
    pub struct Tile;
    name = "tile";
    aliases = ["Tile"];
    kind = Container;
    editor = ("square", "Layout");
    props = [
        ("title", PropType::Str, Optional, "Tile label."),
        ("icon", PropType::Icon, Optional, "Icon shown above the label."),
        ("href", PropType::Route, Optional, "Makes the tile a link to this route or URL."),
    ];
}

impl Render for Tile {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let href = props.url("href");
        ctx.out
            .open(if href.is_some() { "a" } else { "div" })
            .attr("class", "ly-tile")
            .attr("data-liyasa", "tile")
            .attr_if("href", href);
        if let Some(icon) = props.str("icon") {
            ctx.out
                .open("span")
                .attr("class", "ly-icon")
                .attr("data-icon", icon)
                .attr("aria-hidden", "true")
                .close();
        }
        if let Some(title) = props.str("title") {
            ctx.out
                .open("span")
                .attr("class", "ly-tile-title")
                .text(title)
                .close();
        }
        ctx.children(&inst.children)?;
        ctx.out.close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let title = props.str("title").unwrap_or("Tile").to_owned();
        let heading = match props.url("href").map(|href| ctx.absolute(href)) {
            Some(href) => crate::md::link(&title, &href),
            None => format!("**{}**", crate::md::escape_inline(&title)),
        };
        let children = ctx.renderer();
        let body = inst.children.clone();
        let mut error = Ok(());
        ctx.out.item("- ", |md| {
            md.write(&heading);
            md.end_line();
            error = children.markdown(&body, md);
        });
        error
    }

    fn text(&self, inst: &ComponentInst) -> String {
        let props = Reader::of(inst, Self::schema_of());
        text::with_titles(&[props.str_or("title", "")], &inst.children)
    }
}
