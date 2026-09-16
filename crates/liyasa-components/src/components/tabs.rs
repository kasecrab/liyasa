//! Tab groups (CMP-12).
//!
//! The group renders its own tab list, so the buttons and the panels are built
//! from one pass over the children and their `aria-controls` cannot disagree.
//! §25: an agent sees the tabs flattened, so RX-61 turns each one into an H3
//! titled `Group: Tab` — a tab called "npm" is meaningless on its own.

use liyasa_core::components::{ComponentInst, PropType, RenderError};

use crate::nodes;
use crate::props::Reader;
use crate::render::{HtmlCtx, MarkdownCtx, Render};
use crate::{anchor, declare, text};

declare! {
    /// A group of tabs (CMP-12).
    pub struct Tabs;
    name = "tabs";
    aliases = ["Tabs", "tab-group", "TabGroup"];
    kind = Container;
    editor = ("folder-tree", "Disclosure");
    props = [
        // TODO(rfc-0032): RX-61's "Install: npm" needs a name for the group;
        // the requirement row does not give the prop it comes from.
        ("title", PropType::Str, Optional, "Names the group, e.g. `Install`; used to prefix tab titles in the agent output."),
        ("sync", PropType::Str, Optional, "Synchronizes every tab group with the same key site-wide and remembers the reader's choice."),
    ];
}

/// The tabs of a group, in order, with the title each one shows.
fn tabs_of(inst: &ComponentInst) -> Vec<(ComponentInst, String)> {
    nodes::component_children(&inst.children, &["tab", "Tab"])
        .enumerate()
        .map(|(at, tab)| {
            let props = Reader::of(&tab, Tab::schema_of());
            let title = props
                .str("title")
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Tab {}", at + 1));
            (tab, title)
        })
        .collect()
}

impl Render for Tabs {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let group = format!("t-{}", inst.id.to_hex());
        let tabs = tabs_of(inst);

        ctx.out
            .open("div")
            .attr("class", "ly-tabs")
            .attr("data-liyasa", "tabs")
            .flag("data-ly-tabs")
            .attr_if("data-sync", props.str("sync"));

        ctx.out
            .open("div")
            .attr("class", "ly-tablist")
            .attr("role", "tablist")
            .attr_if("aria-label", props.str("title"));
        for (at, (tab, title)) in tabs.iter().enumerate() {
            let tab_props = Reader::of(tab, Tab::schema_of());
            let selected = at == 0;
            ctx.out
                .open("button")
                .attr("class", "ly-tab")
                .attr("type", "button")
                .attr("role", "tab")
                .attr("id", &format!("{group}-tab-{at}"))
                .attr("aria-controls", &format!("{group}-panel-{at}"))
                .attr("aria-selected", if selected { "true" } else { "false" })
                // Roving tabindex: only the selected tab is in the tab order.
                .attr("tabindex", if selected { "0" } else { "-1" })
                .attr_if("data-sync-value", tab_props.str("sync"));
            if let Some(icon) = tab_props.str("icon") {
                ctx.out
                    .open("span")
                    .attr("class", "ly-icon")
                    .attr("data-icon", icon)
                    .attr("aria-hidden", "true")
                    .close();
            }
            ctx.out.text(title).close();
        }
        ctx.out.close();

        for (at, (tab, _)) in tabs.iter().enumerate() {
            ctx.out
                .open("div")
                .attr("class", "ly-tabpanel")
                .attr("role", "tabpanel")
                .attr("id", &format!("{group}-panel-{at}"))
                .attr("aria-labelledby", &format!("{group}-tab-{at}"))
                .attr("tabindex", "0")
                .flag_if("hidden", at > 0);
            ctx.children(&tab.children)?;
            ctx.out.close();
        }
        ctx.out.close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        let group = props.str("title").or_else(|| props.str("sync"));
        for (tab, title) in tabs_of(inst) {
            let heading = match group {
                Some(group) => format!("{group}: {title}"),
                None => title,
            };
            ctx.out.heading(3, &heading);
            ctx.children(&tab.children)?;
        }
        Ok(())
    }

    fn text(&self, inst: &ComponentInst) -> String {
        // The group's own name only: walking the children already picks up each
        // tab's title, and naming them here indexed every label twice.
        let props = Reader::of(inst, Self::schema_of());
        text::with_titles(&[props.str_or("title", "")], &inst.children)
    }
}

declare! {
    /// One tab of a [`Tabs`] group (CMP-12).
    pub struct Tab;
    name = "tab";
    aliases = ["Tab"];
    kind = Container;
    editor = ("square", "Disclosure");
    props = [
        ("title", PropType::Str, Optional, "Tab label. Must say what the tab holds: agents read it flattened."),
        ("icon", PropType::Icon, Optional, "Icon shown before the label."),
        ("sync", PropType::Str, Optional, "Value this tab represents for its group's `sync` key, e.g. `npm`."),
    ];
}

impl Render for Tab {
    fn html(&self, inst: &ComponentInst, ctx: &mut HtmlCtx<'_>) -> Result<(), RenderError> {
        // Reached only when a tab is written outside a group, where there is no
        // tab list to belong to.
        let props = Reader::of(inst, Self::schema_of());
        let title = props.str_or("title", "Tab");
        ctx.out
            .open("section")
            .attr("class", "ly-tab-standalone")
            .attr("data-liyasa", "tab")
            .attr("aria-label", title);
        ctx.out
            .open("p")
            .attr("class", "ly-tab-title")
            .attr("id", &anchor::slug(title))
            .text(title)
            .close();
        ctx.children(&inst.children)?;
        ctx.out.close();
        Ok(())
    }

    fn markdown(&self, inst: &ComponentInst, ctx: &mut MarkdownCtx<'_>) -> Result<(), RenderError> {
        let props = Reader::of(inst, Self::schema_of());
        ctx.out.heading(3, props.str_or("title", "Tab"));
        ctx.children(&inst.children)
    }

    fn text(&self, inst: &ComponentInst) -> String {
        let props = Reader::of(inst, Self::schema_of());
        text::with_titles(&[props.str_or("title", "")], &inst.children)
    }
}
