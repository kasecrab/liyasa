//! Framed media, the page rail, the landing hero, and the divider
//! (CMP-05, CMP-06, CMP-08, CMP-09).

use liyasa_core::components::{ComponentInst, PropType, RenderError};

use crate::props::Reader;
use crate::render::{HtmlCtx, MarkdownCtx, Render};
use crate::schema::{one_of, text as default_text};
use crate::{declare, text};

declare! {
    /// A decorated container for an image or a video, with a caption (CMP-05).
    pub struct Frame;
    name = "frame";
    aliases = ["Frame"];
    kind = Container;
    editor = ("frame", "Media");
    props = [
        ("caption", PropType::Str, Optional, "Caption shown under the frame."),
        ("hint", PropType::Str, Optional, "Smaller note under the caption."),
        ("video", PropType::Bool, Optional, "Frames a video rather than an image: no zoom, and the aspect ratio is kept."),
        ("align", one_of(&["left", "center", "right", "full"]), Default(default_text("center")), "How the frame sits in the text column."),
    ];
}

impl Render for Frame {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let video = props.bool("video");
        ctx.out
            .open("figure")
            .attr("class", "ly-frame")
            .attr("data-liyasa", "frame")
            .attr("data-align", props.str_or("align", "center"))
            .flag_if("data-video", video)
            // A still image opens in a lightbox; a video already has controls.
            .flag_if("data-zoom", !video);
        ctx.children(&inst.children)?;

        let caption = props.str("caption");
        let hint = props.str("hint");
        if caption.is_some() || hint.is_some() {
            ctx.out.open("figcaption").attr("class", "ly-frame-caption");
            if let Some(caption) = caption {
                ctx.out.text(caption);
            }
            if let Some(hint) = hint {
                ctx.out
                    .open("span")
                    .attr("class", "ly-frame-hint")
                    .text(hint)
                    .close();
            }
            ctx.out.close();
        }
        ctx.out.close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let caption = props.str("caption").map(str::to_owned);
        let hint = props.str("hint").map(str::to_owned);
        ctx.out.block();
        ctx.children(&inst.children)?;
        if let Some(caption) = caption {
            ctx.out
                .paragraph(&format!("*{}*", crate::md::escape_inline(&caption)));
        }
        if let Some(hint) = hint {
            ctx.out.paragraph(&crate::md::escape_inline(&hint));
        }
        Ok(())
    }

    fn text(&self, inst: &ComponentInst) -> String {
        let props = Reader::of(inst, Self::schema_of());
        text::with_titles(
            &[props.str_or("caption", ""), props.str_or("hint", "")],
            &inst.children,
        )
    }
}

declare! {
    /// Content for the page's right rail, replacing the table of contents
    /// (CMP-06).
    pub struct Panel;
    name = "panel";
    aliases = ["Panel"];
    kind = Container;
    editor = ("panel-right", "Layout");
    props = [];
}

impl Render for Panel {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        // The theme moves this into the rail; in the document flow it is
        // hidden, so a reader never sees it twice.
        ctx.out
            .open("div")
            .attr("class", "ly-panel")
            .attr("data-liyasa", "panel")
            .attr("data-slot", "rail")
            .flag("hidden");
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
    /// The banner at the head of a landing page (CMP-08).
    pub struct Hero;
    name = "hero";
    aliases = ["Hero"];
    kind = Container;
    editor = ("layout-template", "Layout");
    props = [
        ("title", PropType::Str, Optional, "Headline, rendered as the page's H1."),
        ("subtitle", PropType::Str, Optional, "Sentence under the headline."),
        ("image", PropType::Asset, Optional, "Image or illustration beside the text."),
        ("actions", crate::schema::list_of(PropType::Str), Optional, "Buttons, each `Label -> /route`; the first is the primary action."),
    ];
}

/// One `actions` entry: `Label -> /route`, or a label on its own.
// TODO(rfc-0006): the requirement table names `actions` without a shape.
fn action_of(entry: &str) -> (&str, Option<&str>) {
    match entry.split_once("->") {
        Some((label, href)) => (label.trim(), Some(href.trim())),
        None => (entry.trim(), None),
    }
}

impl Render for Hero {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        ctx.out
            .open("section")
            .attr("class", "ly-hero")
            .attr("data-liyasa", "hero");
        ctx.out.open("div").attr("class", "ly-hero-text");
        if let Some(title) = props.str("title") {
            ctx.out
                .open("h1")
                .attr("class", "ly-hero-title")
                .attr("id", &crate::anchor::slug(title))
                .text(title)
                .close();
        }
        if let Some(subtitle) = props.str("subtitle") {
            ctx.out
                .open("p")
                .attr("class", "ly-hero-subtitle")
                .text(subtitle)
                .close();
        }
        ctx.children(&inst.children)?;

        let actions = props.list("actions");
        if !actions.is_empty() {
            ctx.out.open("div").attr("class", "ly-hero-actions");
            for (at, entry) in actions.iter().enumerate() {
                let (label, href) = action_of(entry);
                let primary = if at == 0 { "primary" } else { "secondary" };
                ctx.out
                    .open(if href.is_some() { "a" } else { "span" })
                    .attr("class", &format!("ly-button ly-button-{primary}"))
                    .attr_if("href", href.filter(|h| crate::props::is_safe_url(h)))
                    .text(label)
                    .close();
            }
            ctx.out.close();
        }
        ctx.out.close();

        if let Some(image) = props.url("image") {
            ctx.out
                .open("img")
                .attr("class", "ly-hero-image")
                .attr("src", image)
                .attr("alt", "");
        }
        ctx.out.close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        if let Some(title) = props.str("title") {
            ctx.out.heading(1, title);
        }
        if let Some(subtitle) = props.str("subtitle") {
            ctx.out.paragraph(&crate::md::escape_inline(subtitle));
        }
        ctx.children(&inst.children)?;
        let actions: Vec<String> = props.list("actions");
        if !actions.is_empty() {
            ctx.out.block();
        }
        for entry in &actions {
            let (label, href) = action_of(entry);
            let rendered = match href {
                Some(href) => crate::md::link(label, &ctx.absolute(href)),
                None => crate::md::escape_inline(label),
            };
            ctx.out.item("- ", |md| {
                md.write(&rendered);
            });
        }
        Ok(())
    }

    fn text(&self, inst: &ComponentInst) -> String {
        let props = Reader::of(inst, Self::schema_of());
        let actions = props.list("actions");
        let labels: Vec<&str> = actions.iter().map(|entry| action_of(entry).0).collect();
        let mut titles = vec![props.str_or("title", ""), props.str_or("subtitle", "")];
        titles.extend(labels);
        text::with_titles(&titles, &inst.children)
    }
}

declare! {
    /// A horizontal rule, optionally labelled (CMP-09).
    pub struct Divider;
    name = "divider";
    aliases = ["Divider"];
    kind = Leaf;
    editor = ("minus", "Layout");
    props = [
        ("label", PropType::Str, Optional, "Text shown in the middle of the rule."),
    ];
}

impl Render for Divider {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        match props.str("label") {
            Some(label) => {
                ctx.out
                    .open("div")
                    .attr("class", "ly-divider")
                    .attr("data-liyasa", "divider")
                    .attr("role", "separator")
                    .attr("aria-label", label)
                    .open("span")
                    .attr("class", "ly-divider-label")
                    .attr("aria-hidden", "true")
                    .text(label)
                    .close()
                    .close();
            }
            None => {
                ctx.out
                    .open("hr")
                    .attr("class", "ly-divider")
                    .attr("data-liyasa", "divider");
            }
        }
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        ctx.out.thematic_break();
        // The label would be lost in a bare rule, so it becomes the paragraph
        // that follows it.
        if let Some(label) = props.str("label") {
            ctx.out
                .paragraph(&format!("**{}**", crate::md::escape_inline(label)));
        }
        Ok(())
    }

    fn text(&self, inst: &ComponentInst) -> String {
        Reader::of(inst, Self::schema_of())
            .str_or("label", "")
            .to_owned()
    }
}
