//! Documentation for the name under the cursor.
//!
//! Everything shown here is read from the thing itself — a component's own prop
//! schema, a fact's own value, a page's own title. Nothing is a second copy of
//! documentation that lives somewhere else and can fall behind it.

use crate::locate::{self, Target};
use crate::protocol::{Hover, MarkupContent, PositionEncoding};
use crate::text::Text;
use crate::workspace::Workspace;

pub fn at(
    text: &Text,
    workspace: &Workspace,
    offset: u32,
    encoding: PositionEncoding,
) -> Option<Hover> {
    let target = locate::at(text, offset)?;
    let (start, end) = target.span();
    let value = describe(&target, workspace)?;
    Some(Hover {
        contents: MarkupContent::markdown(value),
        range: Some(text.range_of(start, end, encoding)),
    })
}

fn describe(target: &Target, workspace: &Workspace) -> Option<String> {
    match target {
        Target::Component { name, .. } => {
            let component = workspace.registry.resolve(name)?;
            Some(crate::completion::describe(component))
        }
        Target::Prop {
            component, name, ..
        } => {
            let def = workspace
                .registry
                .resolve(component)?
                .schema()
                .prop(name)?
                .clone();
            let mut out = format!(
                "**`{name}`** on `{component}` — {}{}\n",
                crate::completion::type_name(&def.ty),
                if def.required { ", required" } else { "" }
            );
            if !def.doc.is_empty() {
                out.push_str(&format!("\n{}\n", def.doc));
            }
            if let Some(default) = &def.default {
                out.push_str(&format!("\nDefaults to `{default}`.\n"));
            }
            Some(out)
        }
        Target::Path { path, .. } => {
            if let Some(fact) = path
                .strip_prefix("facts.")
                .and_then(|path| workspace.facts.get(path))
            {
                return Some(format!(
                    "**`{path}`** — fact\n\n```json\n{}\n```\n\nFrom `{}`.\n",
                    serde_json::to_string_pretty(&fact.value).unwrap_or_default(),
                    fact.file
                ));
            }
            let value = workspace.variables.get(path)?;
            Some(format!(
                "**`{path}`** — variable\n\n```json\n{}\n```\n",
                serde_json::to_string_pretty(value).unwrap_or_default()
            ))
        }
        Target::Snippet { name, .. } => {
            let snippet = workspace.snippets.get(name)?;
            Some(format!(
                "**`{name}`** — snippet\n\nIn `{}`.\n",
                snippet.file
            ))
        }
        Target::Route { route, .. } => {
            let page = workspace
                .pages
                .get(route.trim_end_matches('/'))
                .or_else(|| workspace.pages.get(route))?;
            let mut out = format!("**`{}`**\n", page.route);
            if let Some(title) = &page.title {
                out.push_str(&format!("\n{title}\n"));
            }
            out.push_str(&format!("\nIn `{}`.\n", page.file));
            Some(out)
        }
    }
}
