//! Images, video, frames, embeds, downloads, and tracked screenshots
//! (CMP-50 to CMP-55).

use liyasa_core::components::{ComponentInst, PropType, RenderError};
use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::document::{Dep, DepTarget};

use crate::inst::located;
use crate::props::Reader;
use crate::render::{HtmlCtx, MarkdownCtx, Render};
use crate::schema::{one_of, text as default_text};
use crate::{declare, deps, provider, text};

declare! {
    /// An image with light and dark variants (CMP-50).
    pub struct Image;
    name = "image";
    aliases = ["Image", "img"];
    kind = Leaf;
    editor = ("image", "Media");
    props = [
        ("src", PropType::Asset, Required, "The image. A path under `assets/`, or an absolute URL."),
        ("alt", PropType::Str, Required, "What the image says, for a reader who cannot see it. Empty only when the image is decorative."),
        ("dark", PropType::Asset, Optional, "Variant shown in dark mode."),
        ("width", PropType::Num, Optional, "Intrinsic width in pixels; prevents layout shift."),
        ("height", PropType::Num, Optional, "Intrinsic height in pixels; prevents layout shift."),
        ("caption", PropType::Str, Optional, "Caption shown under the image."),
        ("zoom", PropType::Bool, Default(crate::schema::yes()), "Opens the image full size when clicked."),
        ("align", one_of(&["left", "center", "right", "full"]), Default(default_text("center")), "How the image sits in the text column."),
        ("border", PropType::Bool, Optional, "Draws a border around the image."),
    ];
}

impl Render for Image {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let Some(src) = props.url("src") else {
            return Ok(());
        };
        let alt = props.str_or("alt", "");
        let caption = props.str("caption");
        let width = props.int("width").map(|n| n.to_string());
        let height = props.int("height").map(|n| n.to_string());

        if caption.is_some() {
            ctx.out
                .open("figure")
                .attr("class", "ly-image-figure")
                .attr("data-align", props.str_or("align", "center"));
        }
        if let Some(dark) = props.url("dark") {
            ctx.out
                .open("picture")
                .open("source")
                .attr("media", "(prefers-color-scheme: dark)")
                .attr("srcset", dark);
        }
        ctx.out
            .open("img")
            .attr("class", "ly-image")
            .attr("data-liyasa", "image")
            .attr("src", src)
            .attr("alt", alt)
            .attr_if("width", width.as_deref())
            .attr_if("height", height.as_deref())
            .attr("loading", "lazy")
            .attr("decoding", "async")
            .attr("data-align", props.str_or("align", "center"))
            .flag_if("data-zoom", props.bool_or("zoom", true))
            .flag_if("data-border", props.bool("border"));
        if props.url("dark").is_some() {
            ctx.out.close();
        }
        if let Some(caption) = caption {
            ctx.out
                .open("figcaption")
                .attr("class", "ly-image-caption")
                .text(caption)
                .close();
            ctx.out.close();
        }
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let src = ctx.absolute(props.str_or("src", ""));
        let alt = props.str_or("alt", "");
        ctx.out.block();
        ctx.out.write(&format!(
            "![{}]({})",
            crate::md::escape_inline(alt),
            crate::md::escape_url(&src)
        ));
        ctx.out.end_line();
        if let Some(caption) = props.str("caption") {
            ctx.out
                .paragraph(&format!("*{}*", crate::md::escape_inline(caption)));
        }
        Ok(())
    }

    fn text(&self, inst: &ComponentInst) -> String {
        let props = Reader::of(inst, Self::schema_of());
        text::collapse(&format!(
            "{} {}",
            props.str_or("alt", ""),
            props.str_or("caption", "")
        ))
    }

    fn validate(&self, inst: &ComponentInst, out: &mut Diagnostics) {
        let props = Reader::of(inst, Self::schema_of());
        // `alt` is required, so a missing one is already `E0314`; an `alt` that
        // is present and empty is the case with its own code.
        if props.given("alt") && props.str_or("alt", "").trim().is_empty() {
            out.push(located(
                Diagnostic::new(code::E0305, "`image.alt` is empty")
                    .help("leave `alt` empty only when the image adds nothing a reader needs"),
                inst,
            ));
        }
        if props.given("width") != props.given("height") {
            out.push(located(
                Diagnostic::new(
                    code::W0714,
                    "`image` has one of `width` and `height`, so its aspect ratio is unknown",
                ),
                inst,
            ));
        }
    }
}

declare! {
    /// A video, local or from an allow-listed provider (CMP-51).
    pub struct Video;
    name = "video";
    aliases = ["Video"];
    kind = Leaf;
    editor = ("play", "Media");
    props = [
        ("src", PropType::Asset, Required, "The video file, or a YouTube, Vimeo, or Loom URL."),
        ("poster", PropType::Asset, Optional, "Still shown before the video plays."),
        ("autoplay", PropType::Bool, Optional, "Plays as soon as it is visible. Requires `muted`."),
        ("loop", PropType::Bool, Optional, "Restarts when it ends."),
        ("muted", PropType::Bool, Optional, "Starts with no sound."),
        ("controls", PropType::Bool, Default(crate::schema::yes()), "Shows the player's controls."),
        ("caption", PropType::Str, Optional, "Caption shown under the video."),
        ("title", PropType::Str, Optional, "Accessible name for an embedded player."),
    ];
}

impl Render for Video {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let Some(src) = props.url("src") else {
            return Ok(());
        };
        let caption = props.str("caption");
        if caption.is_some() {
            ctx.out.open("figure").attr("class", "ly-video-figure");
        }
        match provider::resolve(src) {
            Some((hosted, embed)) => {
                ctx.out
                    .open("iframe")
                    .attr("class", "ly-video ly-video-hosted")
                    .attr("data-liyasa", "video")
                    .attr("data-provider", hosted.name)
                    .attr("src", &embed)
                    .attr("title", props.str_or("title", "Video"))
                    .attr("loading", "lazy")
                    .attr(
                        "allow",
                        "accelerometer; encrypted-media; picture-in-picture",
                    )
                    .attr("referrerpolicy", "strict-origin-when-cross-origin")
                    .flag("allowfullscreen")
                    .close();
            }
            None => {
                let autoplay = props.bool("autoplay");
                ctx.out
                    .open("video")
                    .attr("class", "ly-video")
                    .attr("data-liyasa", "video")
                    .attr("src", src)
                    .attr_if("poster", props.url("poster"))
                    .attr("preload", "metadata")
                    .flag_if("controls", props.bool_or("controls", true))
                    .flag_if("autoplay", autoplay)
                    .flag_if("loop", props.bool("loop"))
                    // A browser refuses to autoplay with sound, so an autoplay
                    // video that is not muted simply never starts.
                    .flag_if("muted", props.bool("muted") || autoplay)
                    .flag_if("playsinline", autoplay)
                    .close();
            }
        }
        if let Some(caption) = caption {
            ctx.out
                .open("figcaption")
                .attr("class", "ly-video-caption")
                .text(caption)
                .close();
            ctx.out.close();
        }
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let src = ctx.absolute(props.str_or("src", ""));
        let label = props
            .str("title")
            .or_else(|| props.str("caption"))
            .unwrap_or("Video");
        ctx.out.paragraph(&crate::md::link(label, &src));
        Ok(())
    }

    fn text(&self, inst: &ComponentInst) -> String {
        let props = Reader::of(inst, Self::schema_of());
        text::collapse(&format!(
            "{} {}",
            props.str_or("title", ""),
            props.str_or("caption", "")
        ))
    }

    fn validate(&self, inst: &ComponentInst, out: &mut Diagnostics) {
        let props = Reader::of(inst, Self::schema_of());
        if props.bool("autoplay") && props.given("muted") && !props.bool("muted") {
            out.push(located(
                Diagnostic::new(
                    code::W0355,
                    "`video.muted=false` is ignored because `autoplay` is set",
                )
                .help("browsers refuse to autoplay a video with sound"),
                inst,
            ));
        }
    }
}

declare! {
    /// A sandboxed iframe (CMP-52).
    pub struct IFrame;
    name = "iframe";
    aliases = ["Iframe", "IFrame"];
    kind = Leaf;
    editor = ("square-code", "Media");
    props = [
        ("src", PropType::Route, Required, "The page to frame."),
        ("title", PropType::Str, Required, "What the frame holds. A screen reader announces this instead of the frame."),
        ("height", PropType::Str, Optional, "CSS height, e.g. `480px`."),
        ("allow", PropType::Str, Optional, "Permissions policy for the frame, e.g. `clipboard-write`."),
    ];
}

impl Render for IFrame {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let Some(src) = props.url("src") else {
            return Ok(());
        };
        ctx.out
            .open("iframe")
            .attr("class", "ly-iframe")
            .attr("data-liyasa", "iframe")
            .attr("src", src)
            .attr("title", props.str_or("title", ""))
            .attr_if(
                "style",
                props
                    .str("height")
                    .map(|h| format!("height: {h}"))
                    .as_deref(),
            )
            .attr_if("allow", props.str("allow"))
            .attr("loading", "lazy")
            .attr("referrerpolicy", "strict-origin-when-cross-origin")
            // Scripts yes, same-origin no: a framed page must not reach back
            // into the site that framed it.
            .attr("sandbox", "allow-scripts allow-popups allow-forms")
            .close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let src = ctx.absolute(props.str_or("src", ""));
        ctx.out
            .paragraph(&crate::md::link(props.str_or("title", "Frame"), &src));
        Ok(())
    }

    fn text(&self, inst: &ComponentInst) -> String {
        Reader::of(inst, Self::schema_of())
            .str_or("title", "")
            .to_owned()
    }
}

declare! {
    /// An embed from an allow-listed provider (CMP-53).
    pub struct Embed;
    name = "embed";
    aliases = ["Embed"];
    kind = Leaf;
    editor = ("link", "Media");
    props = [
        ("url", PropType::Route, Required, "The page to embed. Must be from an allow-listed provider."),
        ("title", PropType::Str, Optional, "Accessible name for the frame; defaults to the provider's name."),
        ("height", PropType::Str, Optional, "CSS height, e.g. `480px`."),
    ];
    deps = Embed::embed_deps;
}

impl Embed {
    fn embed_deps(inst: &ComponentInst) -> Vec<Dep> {
        let props = Reader::of(inst, Self::schema_of());
        let mut edges = deps::from_schema(inst, Self::schema_of());
        if let Some(url) = props.str("url") {
            edges.push(deps::embeds(inst, DepTarget::ExternalUrl(url.to_owned())));
        }
        edges
    }
}

impl Render for Embed {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let Some((hosted, src)) = props.url("url").and_then(provider::resolve) else {
            return Ok(());
        };
        ctx.out
            .open("iframe")
            .attr("class", "ly-embed")
            .attr("data-liyasa", "embed")
            .attr("data-provider", hosted.name)
            // The build reads this to add the provider's `frame-src` entry.
            .attr("data-frame-src", hosted.frame_src)
            .attr("src", &src)
            .attr("title", props.str_or("title", hosted.name))
            .attr_if(
                "style",
                props
                    .str("height")
                    .map(|h| format!("height: {h}"))
                    .as_deref(),
            )
            .flag_if("data-aspect", hosted.video)
            .attr("loading", "lazy")
            .attr("referrerpolicy", "strict-origin-when-cross-origin")
            .flag("allowfullscreen")
            .close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let url = props.str_or("url", "");
        ctx.out
            .paragraph(&crate::md::link(props.str_or("title", url), url));
        Ok(())
    }

    fn text(&self, inst: &ComponentInst) -> String {
        let props = Reader::of(inst, Self::schema_of());
        props.str_or("title", props.str_or("url", "")).to_owned()
    }

    fn validate(&self, inst: &ComponentInst, out: &mut Diagnostics) {
        let props = Reader::of(inst, Self::schema_of());
        let Some(url) = props.str("url") else { return };
        if provider::resolve(url).is_none() {
            out.push(located(
                Diagnostic::new(
                    code::E0357,
                    format!("`embed.url` is `{url}`, which is not an allow-listed provider"),
                )
                .help(format!(
                    "allowed: {}",
                    provider::ALLOWED
                        .iter()
                        .map(|p| p.name)
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
                inst,
            ));
        }
    }
}

declare! {
    /// A download card (CMP-54).
    pub struct File;
    name = "file";
    aliases = ["File"];
    kind = Leaf;
    editor = ("file-down", "Media");
    props = [
        ("src", PropType::Asset, Required, "The file to download."),
        ("name", PropType::Str, Optional, "Name shown on the card; defaults to the file name."),
        ("size", PropType::Str, Optional, "Size shown on the card, e.g. `2.4 MB`."),
        ("type", PropType::Str, Optional, "File type shown on the card; defaults to the extension."),
    ];
}

/// The last path segment, which is the file's name.
fn file_name(src: &str) -> &str {
    src.rsplit('/').next().unwrap_or(src)
}

fn file_type(src: &str) -> String {
    file_name(src)
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_uppercase())
        .unwrap_or_default()
}

impl Render for File {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let Some(src) = props.url("src") else {
            return Ok(());
        };
        let name = props.str_or("name", file_name(src));
        let kind = props
            .str("type")
            .map(str::to_owned)
            .unwrap_or_else(|| file_type(src));
        ctx.out
            .open("a")
            .attr("class", "ly-file")
            .attr("data-liyasa", "file")
            .attr("href", src)
            .attr("download", name);
        ctx.out
            .open("span")
            .attr("class", "ly-file-name")
            .text(name)
            .close();
        if !kind.is_empty() {
            ctx.out
                .open("span")
                .attr("class", "ly-file-type")
                .text(&kind)
                .close();
        }
        if let Some(size) = props.str("size") {
            ctx.out
                .open("span")
                .attr("class", "ly-file-size")
                .text(size)
                .close();
        }
        ctx.out.close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let src = props.str_or("src", "");
        let name = props.str_or("name", file_name(src)).to_owned();
        let label = match props.str("size") {
            Some(size) => format!("{name} ({size})"),
            None => name,
        };
        let href = ctx.absolute(src);
        ctx.out.item("- ", |md| {
            md.write(&crate::md::link(&label, &href));
        });
        Ok(())
    }

    fn text(&self, inst: &ComponentInst) -> String {
        let props = Reader::of(inst, Self::schema_of());
        props
            .str_or("name", file_name(props.str_or("src", "")))
            .to_owned()
    }
}

declare! {
    /// A group of download cards (CMP-54).
    pub struct Files;
    name = "files";
    aliases = ["Files"];
    kind = Container;
    editor = ("folder", "Media");
    props = [];
}

impl Render for Files {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        ctx.out
            .open("div")
            .attr("class", "ly-files")
            .attr("data-liyasa", "files");
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
    /// An image whose freshness the verifier tracks (CMP-55).
    pub struct Screenshot;
    name = "screenshot";
    aliases = ["Screenshot"];
    kind = Leaf;
    editor = ("camera", "Media");
    props = [
        ("src", PropType::Asset, Required, "Where the capture is stored; the automation writes it."),
        ("alt", PropType::Str, Required, "What the screenshot shows."),
        ("app", PropType::Str, Optional, "Which application to capture, as named in the verification config."),
        ("route", PropType::Route, Optional, "Route within that application."),
        ("selector", PropType::Str, Optional, "CSS selector to crop to."),
        ("viewport", PropType::Str, Optional, "Viewport to capture at, e.g. `1280x800`."),
        ("caption", PropType::Str, Optional, "Caption shown under the screenshot."),
    ];
    deps = Screenshot::screenshot_deps;
}

impl Screenshot {
    fn screenshot_deps(inst: &ComponentInst) -> Vec<Dep> {
        let props = Reader::of(inst, Self::schema_of());
        let mut edges = deps::from_schema(inst, Self::schema_of());
        if let Some(src) = props.str("src") {
            // The verifier keys drift on this edge: when the product UI moves,
            // the screenshot behind it is what gets flagged.
            edges.push(deps::embeds(inst, DepTarget::Screenshot(src.to_owned())));
        }
        edges
    }
}

impl Render for Screenshot {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let Some(src) = props.url("src") else {
            return Ok(());
        };
        let caption = props.str("caption");
        if caption.is_some() {
            ctx.out.open("figure").attr("class", "ly-screenshot-figure");
        }
        ctx.out
            .open("img")
            .attr("class", "ly-image ly-screenshot")
            .attr("data-liyasa", "screenshot")
            .attr("src", src)
            .attr("alt", props.str_or("alt", ""))
            .attr_if("data-app", props.str("app"))
            .attr_if("data-route", props.str("route"))
            .attr_if("data-selector", props.str("selector"))
            .attr_if("data-viewport", props.str("viewport"))
            .attr("loading", "lazy")
            .flag("data-zoom");
        if let Some(caption) = caption {
            ctx.out
                .open("figcaption")
                .attr("class", "ly-image-caption")
                .text(caption)
                .close();
            ctx.out.close();
        }
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let src = ctx.absolute(props.str_or("src", ""));
        ctx.out.block();
        ctx.out.write(&format!(
            "![{}]({})",
            crate::md::escape_inline(props.str_or("alt", "")),
            crate::md::escape_url(&src)
        ));
        ctx.out.end_line();
        Ok(())
    }

    fn text(&self, inst: &ComponentInst) -> String {
        let props = Reader::of(inst, Self::schema_of());
        text::collapse(&format!(
            "{} {}",
            props.str_or("alt", ""),
            props.str_or("caption", "")
        ))
    }
}
