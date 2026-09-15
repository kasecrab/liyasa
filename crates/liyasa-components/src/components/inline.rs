//! The inline components (CMP-70, CMP-72 to CMP-75, CMP-81).
//!
//! An inline component writes into the middle of a sentence, so none of these
//! open a block or touch the line prefix.

use liyasa_core::components::{ComponentInst, PropType, RenderError};
use liyasa_core::document::Dep;

use crate::props::Reader;
use crate::render::{HtmlCtx, MarkdownCtx, Render};
use crate::schema::{one_of, text as default_text};
use crate::{declare, deps, text};

declare! {
    /// A status label (CMP-70).
    pub struct Badge;
    name = "badge";
    aliases = ["Badge"];
    kind = Inline;
    editor = ("tag", "Inline");
    props = [
        ("color", PropType::Color, Optional, "Accent colour: a theme token name or a hex value."),
        ("variant", one_of(&["soft", "outline", "solid"]), Default(default_text("soft")), "How strongly the colour is applied."),
        ("icon", PropType::Icon, Optional, "Icon shown before the label."),
    ];
}

impl Render for Badge {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        ctx.out
            .open("span")
            .attr("class", "ly-badge")
            .attr("data-liyasa", "badge")
            .attr("data-variant", props.str_or("variant", "soft"))
            .attr_if("data-color", props.str("color"));
        if let Some(icon) = props.str("icon") {
            ctx.out
                .open("span")
                .attr("class", "ly-icon")
                .attr("data-icon", icon)
                .attr("aria-hidden", "true")
                .close();
        }
        ctx.children(&inst.children)?;
        ctx.out.close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let label = text::of(&inst.children);
        if !label.is_empty() {
            ctx.out
                .write(&format!("**{}**", crate::md::escape_inline(&label)));
        }
        Ok(())
    }
}

declare! {
    /// A colour swatch the reader can copy (CMP-72).
    pub struct Color;
    name = "color";
    aliases = ["Color", "Colour", "colour"];
    kind = Inline;
    editor = ("palette", "Inline");
    props = [
        ("value", PropType::Color, Required, "The colour, as a CSS value."),
        ("name", PropType::Str, Optional, "What the colour is called; shown beside the swatch."),
    ];
}

/// Whether a value is safe to put in a `style` attribute.
///
/// A CSS value reaches a stylesheet, where `url(...)` and `expression(...)`
/// are the ways out, so only the shapes a colour actually takes are allowed.
fn is_color_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '#' | '(' | ')' | ',' | '.' | '%' | ' ' | '-')
        })
        && !value.to_ascii_lowercase().contains("url")
        && !value.to_ascii_lowercase().contains("expression")
}

impl Render for Color {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let value = props.str_or("value", "");
        let label = props.str_or("name", value);
        ctx.out
            .open("button")
            .attr("class", "ly-color")
            .attr("type", "button")
            .attr("data-liyasa", "color")
            .attr("data-value", value)
            .attr("title", &format!("Copy {value}"));
        ctx.out
            .open("span")
            .attr("class", "ly-color-swatch")
            .attr("aria-hidden", "true");
        if is_color_value(value) {
            ctx.out.attr("style", &format!("background: {value}"));
        }
        ctx.out.close();
        ctx.out
            .open("span")
            .attr("class", "ly-color-label")
            .text(label)
            .close();
        ctx.out.close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let value = props.str_or("value", "");
        ctx.out.write(&match props.str("name") {
            Some(name) => format!("{name} ({})", crate::md::code_span(value)),
            None => crate::md::code_span(value),
        });
        Ok(())
    }

    fn text(&self, inst: &ComponentInst) -> String {
        let props = Reader::of(inst, Self::schema_of());
        text::collapse(&format!(
            "{} {}",
            props.str_or("name", ""),
            props.str_or("value", "")
        ))
    }
}

declare! {
    /// An inline icon (CMP-73).
    pub struct Icon;
    name = "icon";
    aliases = ["Icon"];
    kind = Inline;
    editor = ("sparkles", "Inline");
    props = [
        ("name", PropType::Icon, Required, "Icon name in the chosen set."),
        ("type", PropType::Str, Optional, "Icon set the name comes from."),
        ("size", PropType::Num, Optional, "Size in pixels; defaults to the surrounding text's size."),
        ("color", PropType::Color, Optional, "Colour: a theme token name or a hex value."),
        ("label", PropType::Str, Optional, "Accessible name. Without it the icon is decorative and screen readers skip it."),
    ];
}

impl Render for Icon {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let label = props.str("label");
        ctx.out
            .open("span")
            .attr("class", "ly-icon")
            .attr("data-liyasa", "icon")
            .attr("data-icon", props.str_or("name", ""))
            .attr_if("data-set", props.str("type"))
            .attr_if("data-color", props.str("color"))
            .attr_if(
                "style",
                props
                    .int("size")
                    .map(|size| format!("--ly-icon-size: {size}px"))
                    .as_deref(),
            );
        match label {
            Some(label) => {
                ctx.out.attr("role", "img").attr("aria-label", label);
            }
            None => {
                ctx.out.attr("aria-hidden", "true");
            }
        }
        ctx.out.close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        // A decorative icon is noise in the agent output; a labelled one is
        // the only kind that carries meaning.
        let props = Reader::of(inst, Self::schema_of());
        if let Some(label) = props.str("label") {
            ctx.out.write(&crate::md::escape_inline(label));
        }
        Ok(())
    }

    fn text(&self, inst: &ComponentInst) -> String {
        Reader::of(inst, Self::schema_of())
            .str_or("label", "")
            .to_owned()
    }
}

declare! {
    /// An accessible tooltip (CMP-74).
    pub struct Tooltip;
    name = "tooltip";
    aliases = ["Tooltip"];
    kind = Inline;
    editor = ("message-square", "Inline");
    props = [
        ("text", PropType::Str, Required, "What the tooltip says."),
        ("href", PropType::Route, Optional, "Makes the anchor a link as well as a tooltip."),
    ];
}

impl Render for Tooltip {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let id = format!("tt-{}", inst.id.to_hex());
        let href = props.url("href");
        // A `button` when there is nothing to link to: a tooltip has to be
        // reachable with the keyboard, and a bare `span` is not.
        ctx.out
            .open(if href.is_some() { "a" } else { "button" })
            .attr("class", "ly-tooltip")
            .attr("data-liyasa", "tooltip")
            .attr("aria-describedby", &id)
            .attr_if("href", href);
        if href.is_none() {
            ctx.out.attr("type", "button");
        }
        ctx.children(&inst.children)?;
        ctx.out.close();
        ctx.out
            .open("span")
            .attr("class", "ly-tooltip-text")
            .attr("role", "tooltip")
            .attr("id", &id)
            .text(props.str_or("text", ""))
            .close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let anchor = text::of(&inst.children);
        let rendered = match props.url("href") {
            Some(href) => crate::md::link(&anchor, &ctx.absolute(href)),
            None => crate::md::escape_inline(&anchor),
        };
        // The tooltip's own text is the part a reader would otherwise never
        // get, so it goes inline rather than being dropped.
        ctx.out.write(&format!(
            "{rendered} ({})",
            crate::md::escape_inline(props.str_or("text", ""))
        ));
        Ok(())
    }

    fn text(&self, inst: &ComponentInst) -> String {
        let props = Reader::of(inst, Self::schema_of());
        text::with_titles(&[props.str_or("text", "")], &inst.children)
    }
}

declare! {
    /// A keyboard key (CMP-75).
    pub struct Kbd;
    name = "kbd";
    aliases = ["Kbd"];
    kind = Inline;
    editor = ("keyboard", "Inline");
    props = [];
}

impl Render for Kbd {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        ctx.out
            .open("kbd")
            .attr("class", "ly-kbd")
            .attr("data-liyasa", "kbd");
        ctx.children(&inst.children)?;
        ctx.out.close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        ctx.out
            .write(&crate::md::code_span(&text::of(&inst.children)));
        Ok(())
    }
}

declare! {
    /// A verified fact value, linked to its source (CMP-81).
    pub struct Fact;
    name = "fact";
    aliases = ["Fact"];
    kind = Inline;
    editor = ("badge-check", "Inline");
    props = [
        ("id", PropType::Str, Required, "The fact's ID, as declared under `facts/`."),
        ("format", PropType::Str, Optional, "How to render the value, e.g. `currency` or `date`."),
    ];
    deps = Fact::fact_deps;
}

impl Fact {
    fn fact_deps(inst: &ComponentInst) -> Vec<Dep> {
        let props = Reader::of(inst, Self::schema_of());
        let mut edges = deps::from_schema(inst, Self::schema_of());
        if let Some(id) = props.str("id") {
            edges.push(deps::fact(inst, id));
        }
        edges
    }
}

impl Render for Fact {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        // The value is substituted by the build, which has the fact store; the
        // children are what it substituted last, or empty on a first render.
        ctx.out
            .open("span")
            .attr("class", "ly-fact")
            .attr("data-liyasa", "fact")
            .attr("data-fact", props.str_or("id", ""))
            .attr_if("data-format", props.str("format"));
        ctx.children(&inst.children)?;
        ctx.out.close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let value = text::of(&inst.children);
        ctx.out.write(&if value.is_empty() {
            crate::md::code_span(props.str_or("id", ""))
        } else {
            crate::md::escape_inline(&value)
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_colour_value_cannot_carry_a_url() {
        assert!(is_color_value("#ff8800"));
        assert!(is_color_value("rgb(255, 136, 0)"));
        assert!(is_color_value("var--ly-accent"));
        assert!(!is_color_value("url(//evil.example.com)"));
        assert!(!is_color_value("red; background: url(x)"));
        assert!(!is_color_value(""));
    }
}
