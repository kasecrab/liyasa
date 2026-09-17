//! Page-level and audience-gated components
//! (CMP-71, CMP-76 to CMP-80, CMP-82 to CMP-84).

use liyasa_core::build::Variant;
use liyasa_core::components::{ComponentInst, PropType, RenderError};
use liyasa_core::document::{Dep, DepTarget};
use liyasa_core::ids::{Locale, Version};
use liyasa_core::markdown::Audience;

use crate::props::Reader;
use crate::render::{HtmlCtx, MarkdownCtx, Render};
use crate::schema::{list_of, one_of, text as default_text};
use crate::{anchor, declare, deps, gate, text};

declare! {
    /// An in-page announcement the reader can dismiss (CMP-71).
    pub struct Banner;
    name = "banner";
    aliases = ["Banner"];
    kind = Container;
    editor = ("megaphone", "Page");
    props = [
        ("color", PropType::Color, Optional, "Accent colour: a theme token name or a hex value."),
        ("dismissible", PropType::Bool, Optional, "Lets the reader close the banner; `id` is what remembers that."),
        ("id", PropType::Str, Optional, "Identifies the banner so a dismissal is remembered across pages."),
    ];
}

impl Render for Banner {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let dismissible = props.bool("dismissible");
        ctx.out
            .open("div")
            .attr("class", "ly-banner")
            .attr("data-liyasa", "banner")
            .attr("role", "region")
            .attr("aria-label", "Announcement")
            .attr_if("data-color", props.str("color"))
            .attr_if("id", props.str("id"))
            .attr_if("data-ly-banner-id", props.str("id"))
            .flag_if("data-dismissible", dismissible);
        ctx.out.open("div").attr("class", "ly-banner-body");
        ctx.children(&inst.children)?;
        ctx.out.close();
        if dismissible {
            ctx.out
                .open("button")
                .attr("class", "ly-banner-close")
                .attr("type", "button")
                .attr("aria-label", "Dismiss")
                .attr("data-liyasa", "dismiss")
                .close();
        }
        ctx.out.close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        ctx.quote_children(&inst.children)
    }
}

declare! {
    /// One changelog entry (CMP-76).
    pub struct Update;
    name = "update";
    aliases = ["Update"];
    kind = Container;
    editor = ("calendar", "Page");
    props = [
        ("date", PropType::Str, Required, "Release date, `YYYY-MM-DD`."),
        ("version", PropType::Str, Optional, "Version this entry describes."),
        ("labels", list_of(PropType::Str), Optional, "Tags the entry is filtered by, e.g. `breaking` or `api`."),
        ("title", PropType::Str, Optional, "Headline for the entry."),
    ];
}

impl Update {
    fn heading(props: &Reader<'_>) -> String {
        match (props.str("title"), props.str("version")) {
            (Some(title), Some(version)) => format!("{version} — {title}"),
            (Some(title), None) => title.to_owned(),
            (None, Some(version)) => version.to_owned(),
            (None, None) => props.str_or("date", "Update").to_owned(),
        }
    }
}

impl Render for Update {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let date = props.str_or("date", "");
        let heading = Self::heading(&props);
        ctx.out
            .open("article")
            .attr("class", "ly-update")
            .attr("data-liyasa", "update")
            .attr("id", &anchor::slug(&format!("{date} {heading}")))
            .attr("data-date", date)
            .attr_if("data-version", props.str("version"));
        ctx.out
            .open("time")
            .attr("class", "ly-update-date")
            .attr("datetime", date)
            .text(date)
            .close();
        ctx.out
            .open("h3")
            .attr("class", "ly-update-title")
            .text(&heading)
            .close();
        let labels = props.list("labels");
        if !labels.is_empty() {
            ctx.out.open("p").attr("class", "ly-update-labels");
            for label in &labels {
                ctx.out
                    .open("span")
                    .attr("class", "ly-badge")
                    .attr("data-label", label)
                    .text(label)
                    .close();
            }
            ctx.out.close();
        }
        ctx.out.open("div").attr("class", "ly-update-body");
        ctx.children(&inst.children)?;
        ctx.out.close().close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        ctx.out.heading(
            3,
            &format!("{} — {}", props.str_or("date", ""), Self::heading(&props)),
        );
        let labels = props.list("labels");
        if !labels.is_empty() {
            ctx.out.paragraph(&format!("*{}*", labels.join(", ")));
        }
        ctx.children(&inst.children)
    }

    fn text(&self, inst: &ComponentInst) -> String {
        let props = Reader::of(inst, Self::schema_of());
        text::with_titles(
            &[props.str_or("date", ""), &Self::heading(&props)],
            &inst.children,
        )
    }
}

declare! {
    /// A copyable prompt with "open in" actions (CMP-77).
    pub struct Prompt;
    name = "prompt";
    aliases = ["Prompt"];
    kind = Container;
    editor = ("sparkle", "Page");
    props = [
        ("title", PropType::Str, Optional, "Headline above the prompt."),
        ("open", list_of(one_of(&["cursor", "claude", "chatgpt"])), Optional, "Assistants to offer an `open in` button for."),
    ];
}

impl Render for Prompt {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        ctx.out
            .open("div")
            .attr("class", "ly-prompt")
            .attr("data-liyasa", "prompt");
        if let Some(title) = props.str("title") {
            ctx.out
                .open("p")
                .attr("class", "ly-prompt-title")
                .text(title)
                .close();
        }
        ctx.out.open("div").attr("class", "ly-prompt-body");
        ctx.children(&inst.children)?;
        ctx.out.close();
        ctx.out.open("div").attr("class", "ly-prompt-actions");
        ctx.out
            .open("button")
            .attr("class", "ly-button")
            .attr("type", "button")
            .attr("data-liyasa", "copy")
            .text("Copy prompt")
            .close();
        for target in props.list("open") {
            ctx.out
                .open("button")
                .attr("class", "ly-button ly-button-secondary")
                .attr("type", "button")
                .attr("data-open-in", &target)
                .text(&format!("Open in {target}"))
                .close();
        }
        ctx.out.close().close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        if let Some(title) = props.str("title") {
            ctx.out
                .paragraph(&format!("**{}**", crate::md::escape_inline(title)));
        }
        ctx.out
            .fence("text", &format!("{}\n", text::of(&inst.children)));
        Ok(())
    }

    fn text(&self, inst: &ComponentInst) -> String {
        let props = Reader::of(inst, Self::schema_of());
        text::with_titles(&[props.str_or("title", "")], &inst.children)
    }
}

declare! {
    /// A repository card, filled at build time (CMP-78).
    pub struct Github;
    name = "github";
    aliases = ["GitHub", "Github"];
    kind = Leaf;
    editor = ("github", "Page");
    props = [
        ("repo", PropType::Str, Required, "Repository as `owner/name`."),
    ];
    deps = Github::repo_deps;
}

impl Github {
    fn repo_deps(inst: &ComponentInst) -> Vec<Dep> {
        let props = Reader::of(inst, Self::schema_of());
        let mut edges = deps::from_schema(inst, Self::schema_of());
        if let Some(repo) = props.str("repo") {
            // Fetched through `liyasa-net` at build time and cached; never at
            // runtime (THM-32).
            edges.push(deps::embeds(
                inst,
                DepTarget::ExternalUrl(format!("https://api.github.com/repos/{repo}")),
            ));
        }
        edges
    }
}

impl Render for Github {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let repo = props.str_or("repo", "");
        // Stars, description, and language are substituted by the build; what
        // is here is the placeholder shown when the fetch fails.
        ctx.out
            .open("a")
            .attr("class", "ly-github")
            .attr("data-liyasa", "github")
            .attr("data-repo", repo)
            .attr("href", &format!("https://github.com/{repo}"))
            .open("span")
            .attr("class", "ly-github-name")
            .text(repo)
            .close()
            .close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let repo = props.str_or("repo", "");
        ctx.out.paragraph(&crate::md::link(
            repo,
            &format!("https://github.com/{repo}"),
        ));
        Ok(())
    }

    fn text(&self, inst: &ComponentInst) -> String {
        Reader::of(inst, Self::schema_of())
            .str_or("repo", "")
            .to_owned()
    }
}

declare! {
    /// Forces Markdown parsing inside a raw HTML block (CMP-79).
    pub struct Md;
    name = "md";
    aliases = ["Md", "markdown"];
    kind = Container;
    editor = ("file-text", "Page");
    props = [];
}

impl Render for Md {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        // No wrapper: the point of `md` is that the parser treated the body as
        // Markdown, and by the time it is rendered that has already happened.
        ctx.children(&inst.children)
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        ctx.children(&inst.children)
    }
}

declare! {
    /// Content for one audience only (CMP-80).
    pub struct Visibility;
    name = "visibility";
    aliases = ["Visibility"];
    kind = Container;
    editor = ("eye", "Page");
    props = [
        ("humans", PropType::Bool, Optional, "Include in the HTML output."),
        ("agents", PropType::Bool, Optional, "Include in the Markdown output."),
        ("groups", list_of(PropType::Str), Optional, "Authenticated groups that may see it."),
        ("regions", list_of(PropType::Str), Optional, "Regions it is shown in."),
        ("locales", list_of(PropType::Str), Optional, "Locales it is shown in."),
        ("versions", list_of(PropType::Str), Optional, "Versions it is shown in."),
    ];
}

impl Visibility {
    /// Whether this block is for `audience`.
    ///
    /// Naming neither audience means both, which is what `:::visibility{groups=…}`
    /// alone has to mean.
    fn shows(props: &Reader<'_>, audience: Audience) -> bool {
        let humans = props.bool("humans");
        let agents = props.bool("agents");
        if !humans && !agents {
            return true;
        }
        match audience {
            Audience::Human => humans,
            Audience::Agent => agents,
        }
    }

    fn gates(props: &Reader<'_>) -> Vec<(&'static str, String)> {
        ["groups", "regions", "locales", "versions"]
            .into_iter()
            .filter_map(|name| {
                let values = props.list(name);
                (!values.is_empty()).then(|| (name, values.join(" ")))
            })
            .collect()
    }

    /// Whether `variant` satisfies every gate the block declares.
    ///
    /// TODO(rfc-0401): a gate admits only what the variant positively
    /// satisfies, so a build that does not know the reader's groups, region,
    /// locale or version withholds the block rather than serving it. The four
    /// props used to reach the output as `data-` attributes and nothing else,
    /// which put `groups="admin"` content in front of anonymous readers.
    fn admits(props: &Reader<'_>, variant: &Variant) -> bool {
        gate::any_of(&props.list("groups"), &variant.groups)
            && gate::is_one_of(&props.list("regions"), variant.region.as_deref())
            && gate::is_one_of(
                &props.list("locales"),
                variant.locale.as_ref().map(Locale::as_str),
            )
            && gate::is_one_of(
                &props.list("versions"),
                variant.version.as_ref().map(Version::as_str),
            )
    }
}

impl Render for Visibility {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        if !Self::shows(&props, Audience::Human) || !Self::admits(&props, ctx.variant()) {
            return Ok(());
        }
        let gates = Self::gates(&props);
        if gates.is_empty() {
            return ctx.children(&inst.children);
        }
        ctx.out
            .open("div")
            .attr("class", "ly-visibility")
            .attr("data-liyasa", "visibility");
        for (name, value) in gates {
            ctx.out.attr(&format!("data-{name}"), &value);
        }
        ctx.children(&inst.children)?;
        ctx.out.close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        if !Self::shows(&props, ctx.audience()) || !Self::admits(&props, ctx.variant()) {
            return Ok(());
        }
        ctx.children(&inst.children)
    }

    fn text_for(&self, inst: &ComponentInst, variant: &Variant) -> String {
        let props = Reader::of(inst, Self::schema_of());
        if Self::shows(&props, Audience::Human) && Self::admits(&props, variant) {
            text::of_for(&inst.children, variant)
        } else {
            String::new()
        }
    }
}

declare! {
    /// A region-gated block (CMP-82).
    pub struct Region;
    name = "region";
    aliases = ["Region"];
    kind = Container;
    editor = ("globe", "Page");
    props = [
        ("only", list_of(PropType::Str), Optional, "Regions this block is shown in."),
        ("except", list_of(PropType::Str), Optional, "Regions this block is hidden in."),
    ];
}

impl Region {
    /// TODO(rfc-0401): `except` withholds what it cannot check. With no region
    /// in the variant the build cannot show the reader is outside the excluded
    /// set, and a gate that cannot be checked is a gate that did not hold.
    fn admits(props: &Reader<'_>, variant: &Variant) -> bool {
        let region = variant.region.as_deref();
        let except = props.list("except");
        gate::is_one_of(&props.list("only"), region)
            && (except.is_empty() || region.is_some_and(|r| !except.iter().any(|e| e == r)))
    }
}

impl Render for Region {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        if !Self::admits(&props, ctx.variant()) {
            return Ok(());
        }
        let only = props.list("only");
        let except = props.list("except");
        ctx.out
            .open("div")
            .attr("class", "ly-region")
            .attr("data-liyasa", "region");
        if !only.is_empty() {
            ctx.out.attr("data-only", &only.join(" "));
        }
        if !except.is_empty() {
            ctx.out.attr("data-except", &except.join(" "));
        }
        ctx.children(&inst.children)?;
        ctx.out.close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        if !Self::admits(&props, ctx.variant()) {
            return Ok(());
        }
        ctx.children(&inst.children)
    }

    fn text_for(&self, inst: &ComponentInst, variant: &Variant) -> String {
        let props = Reader::of(inst, Self::schema_of());
        if Self::admits(&props, variant) {
            text::of_for(&inst.children, variant)
        } else {
            String::new()
        }
    }
}

declare! {
    /// An inline feedback widget (CMP-83).
    pub struct Feedback;
    name = "feedback";
    aliases = ["Feedback"];
    kind = Leaf;
    editor = ("thumbs-up", "Page");
    props = [
        ("question", PropType::Str, Default(default_text("Was this helpful?")), "What the reader is asked."),
    ];
}

impl Render for Feedback {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let question = props.str_or("question", "Was this helpful?");
        let id = format!("fb-{}", inst.id.to_hex());
        ctx.out
            .open("form")
            .attr("class", "ly-feedback")
            .attr("data-liyasa", "feedback")
            .flag("data-ly-feedback")
            .attr("method", "post")
            .attr("action", "/_liyasa/feedback")
            .attr("aria-labelledby", &id);
        ctx.out
            .open("p")
            .attr("class", "ly-feedback-question")
            .attr("id", &id)
            .text(question)
            .close();
        for (value, label) in [("yes", "Yes"), ("no", "No")] {
            ctx.out
                .open("button")
                .attr("class", "ly-button ly-button-secondary")
                .attr("type", "submit")
                .attr("name", "answer")
                .attr("value", value)
                .attr("data-ly-feedback-value", value)
                .text(label)
                .close();
        }
        ctx.out.close();
        Ok(())
    }

    fn markdown(
        &self,
        _inst: &ComponentInst,
        _ctx: &mut MarkdownCtx<'_>,
    ) -> Result<(), RenderError> {
        // A form is not something an agent can answer.
        Ok(())
    }

    fn text(&self, _inst: &ComponentInst) -> String {
        String::new()
    }
}

declare! {
    /// A button that opens the assistant with a question ready (CMP-84).
    pub struct Assistant;
    name = "assistant";
    aliases = ["Assistant"];
    kind = Leaf;
    editor = ("bot", "Page");
    props = [
        ("prompt", PropType::Str, Required, "The question the assistant opens with."),
        ("label", PropType::Str, Optional, "Text on the button; defaults to the prompt."),
    ];
}

impl Render for Assistant {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let prompt = props.str_or("prompt", "");
        ctx.out
            .open("button")
            .attr("class", "ly-assistant")
            .attr("type", "button")
            .attr("data-liyasa", "assistant")
            .flag("data-ly-assistant-trigger")
            .attr("data-prompt", prompt)
            .text(props.str_or("label", prompt))
            .close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        // An agent can answer the question itself; the button is the part that
        // does not travel.
        let props = Reader::of(inst, Self::schema_of());
        ctx.out
            .paragraph(&crate::md::escape_inline(props.str_or("prompt", "")));
        Ok(())
    }

    fn text(&self, inst: &ComponentInst) -> String {
        let props = Reader::of(inst, Self::schema_of());
        text::collapse(&format!(
            "{} {}",
            props.str_or("label", ""),
            props.str_or("prompt", "")
        ))
    }
}
