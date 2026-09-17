//! What can be written here.
//!
//! Completion is decided from the text of the line the cursor is on, not from
//! the segment the scanner produced for it. That is deliberate: half a
//! directive is not a directive, `:::no` and `{{ facts.` and `](/gui` are all
//! states the scanner is entitled to classify as ordinary Markdown, and they
//! are exactly the states an author is in when they ask for a completion.

use liyasa_components::Registry;
use liyasa_core::components::{Component, PropType};
use liyasa_core::markdown::ComponentKind;

use crate::protocol::{CompletionItem, CompletionItemKind, PositionEncoding};
use crate::text::Text;
use crate::workspace::Workspace;

/// What the cursor is in the middle of writing, and the byte range the
/// completion replaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Context {
    /// `:::no` — a container, leaf, or inline directive's name.
    Directive {
        kind: ComponentKind,
        prefix: String,
        replace: (u32, u32),
    },
    /// `:::card{ic` — a prop name on a known component.
    Prop {
        component: String,
        given: Vec<String>,
        prefix: String,
        replace: (u32, u32),
    },
    /// `:::card{horizontal=` — a value for a prop whose type constrains it.
    PropValue {
        component: String,
        prop: String,
        prefix: String,
        replace: (u32, u32),
    },
    /// `{{ facts.pri` or `{% if produ` — a name in a template expression.
    Expression { prefix: String, replace: (u32, u32) },
    /// `{% include "legal/te` — a snippet's name.
    Snippet { prefix: String, replace: (u32, u32) },
    /// `[text](/gui` — a route.
    Link { prefix: String, replace: (u32, u32) },
}

impl Context {
    fn replace(&self) -> (u32, u32) {
        match self {
            Self::Directive { replace, .. }
            | Self::Prop { replace, .. }
            | Self::PropValue { replace, .. }
            | Self::Expression { replace, .. }
            | Self::Snippet { replace, .. }
            | Self::Link { replace, .. } => *replace,
        }
    }
}

/// The completions offered at a byte offset, already carrying the edit that
/// applies them.
pub fn at(
    text: &Text,
    workspace: &Workspace,
    offset: u32,
    encoding: PositionEncoding,
) -> Vec<CompletionItem> {
    let Some(context) = context_at(text, offset) else {
        return Vec::new();
    };
    let (start, end) = context.replace();
    let range = text.range_of(start, end, encoding);
    items(&context, workspace)
        .into_iter()
        .map(|(item, new_text)| item.replacing(range, new_text))
        .collect()
}

/// Reads the line up to the cursor and decides what is being written.
pub fn context_at(text: &Text, offset: u32) -> Option<Context> {
    let line_no = text.line_at(offset);
    let line = text.line(line_no);
    let line_start = offset - u32::try_from(prefix_len(text, line_no, offset)).unwrap_or(0);
    let column = (offset - line_start) as usize;
    let before = line.get(..column.min(line.len()))?;

    // A template expression wins over everything else on the line: `{{ … }}`
    // and `{% … %}` are opaque to the directive and link syntax inside them.
    if let Some(open) = last_unclosed_template(before) {
        let inner = &before[open.at..];
        if let Some(quoted) = including(inner) {
            let start = line_start + u32::try_from(before.len() - quoted.len()).unwrap_or(0);
            return Some(Context::Snippet {
                prefix: quoted.to_owned(),
                replace: (start, offset),
            });
        }
        let word = trailing_path(inner);
        let start = line_start + u32::try_from(before.len() - word.len()).unwrap_or(0);
        return Some(Context::Expression {
            prefix: word.to_owned(),
            replace: (start, offset),
        });
    }

    if let Some(context) = directive(before, line_start, offset) {
        return Some(context);
    }

    if let Some(target) = link_target(before) {
        let start = line_start + u32::try_from(before.len() - target.len()).unwrap_or(0);
        return Some(Context::Link {
            prefix: target.to_owned(),
            replace: (start, offset),
        });
    }

    None
}

/// The byte offset of the line's start, derived the same way the position
/// mapping does, so the two cannot disagree.
fn prefix_len(text: &Text, line: u32, offset: u32) -> usize {
    let start = text.as_str()[..offset as usize]
        .rfind('\n')
        .map_or(0, |at| at + 1);
    let _ = line;
    offset as usize - start
}

struct Open {
    at: usize,
}

/// The last `{{` or `{%` on the line that nothing has closed. Both forms are
/// one line in practice — CM-21 requires a block statement to be a whole line —
/// and a completion inside a multi-line expression is not a state an author
/// reaches by typing.
fn last_unclosed_template(before: &str) -> Option<Open> {
    let bytes = before.as_bytes();
    let mut open = None;
    let mut at = 0;
    while at + 1 < bytes.len() {
        match (bytes[at], bytes[at + 1]) {
            (b'{', b'{') | (b'{', b'%') => {
                open = Some(Open { at: at + 2 });
                at += 2;
            }
            (b'}', b'}') | (b'%', b'}') => {
                open = None;
                at += 2;
            }
            _ => at += 1,
        }
    }
    open
}

/// `include "legal/te` and `import "x` name a snippet; the completion replaces
/// what is inside the quotes.
fn including(inner: &str) -> Option<&str> {
    let trimmed = inner.trim_start();
    let rest = ["include", "import", "from", "extends"]
        .iter()
        .find_map(|keyword| trimmed.strip_prefix(keyword))?;
    let rest = rest.strip_prefix(' ').or_else(|| rest.strip_prefix('"'))?;
    let after_quote = rest.rsplit_once(['"', '\'']).map_or(rest, |(_, tail)| tail);
    (!after_quote.contains(['"', '\''])).then_some(after_quote)
}

/// The dotted name being typed: `facts.pricing.pro` from `{{ facts.pricing.pro`.
fn trailing_path(inner: &str) -> &str {
    let at = inner
        .rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
        .map_or(0, |at| at + 1);
    &inner[at..]
}

/// A directive is the first thing on its line, after any container or list
/// indentation. Inside its `{…}` the cursor is on a prop rather than a name.
fn directive(before: &str, line_start: u32, offset: u32) -> Option<Context> {
    let indent = before.len() - before.trim_start_matches([' ', '\t', '>']).len();
    let body = &before[indent..];
    let colons = body.bytes().take_while(|b| *b == b':').count();
    if colons == 0 {
        return None;
    }
    let kind = match colons {
        1 => ComponentKind::Inline,
        2 => ComponentKind::Leaf,
        _ => ComponentKind::Container,
    };
    let rest = &body[colons..];

    let Some(brace) = rest.rfind('{') else {
        // Still in the name. A name run stops at the first character a name
        // cannot contain, so `:::note ` is past it and offers nothing.
        if rest.contains([' ', '\t', '[', '}']) {
            return None;
        }
        let start = line_start + u32::try_from(indent + colons).unwrap_or(0);
        return Some(Context::Directive {
            kind,
            prefix: rest.to_owned(),
            replace: (start, offset),
        });
    };

    let name = rest[..brace].split(['[', ' ']).next().unwrap_or_default();
    if name.is_empty() {
        return None;
    }
    let props = &rest[brace + 1..];
    if props.contains('}') {
        return None;
    }

    let (given, last) = props_so_far(props);
    let start = line_start + u32::try_from(before.len() - last.len()).unwrap_or(0);
    match last.split_once('=') {
        Some((prop, value)) => {
            let value = value.trim_start_matches(['"', '\'']);
            let start = start + u32::try_from(last.len() - value.len()).unwrap_or(0);
            Some(Context::PropValue {
                component: name.to_owned(),
                prop: prop.to_owned(),
                prefix: value.to_owned(),
                replace: (start, offset),
            })
        }
        None => Some(Context::Prop {
            component: name.to_owned(),
            given,
            prefix: last.to_owned(),
            replace: (start, offset),
        }),
    }
}

/// The props already written, and the one being written now. Quoted values may
/// contain spaces, so the split tracks quoting rather than splitting on
/// whitespace.
fn props_so_far(props: &str) -> (Vec<String>, &str) {
    let mut given = Vec::new();
    let mut quote: Option<char> = None;
    let mut token_start = 0;
    for (at, ch) in props.char_indices() {
        match quote {
            Some(open) if ch == open => quote = None,
            Some(_) => {}
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None if ch == ' ' || ch == '\t' => {
                let token = &props[token_start..at];
                if let Some((name, _)) = token.split_once('=') {
                    given.push(name.to_owned());
                } else if !token.is_empty() {
                    given.push(token.to_owned());
                }
                token_start = at + ch.len_utf8();
            }
            None => {}
        }
    }
    (given, &props[token_start..])
}

/// `[text](/gui` — the target of a link that has not been closed.
fn link_target(before: &str) -> Option<&str> {
    let open = before.rfind("](")? + 2;
    let target = &before[open..];
    (!target.contains(')')).then_some(target)
}

fn items(context: &Context, workspace: &Workspace) -> Vec<(CompletionItem, String)> {
    match context {
        Context::Directive { kind, prefix, .. } => directives(&workspace.registry, *kind, prefix),
        Context::Prop {
            component,
            given,
            prefix,
            ..
        } => props(&workspace.registry, component, given, prefix),
        Context::PropValue {
            component,
            prop,
            prefix,
            ..
        } => prop_values(workspace, component, prop, prefix),
        Context::Expression { prefix, .. } => expressions(workspace, prefix),
        Context::Snippet { prefix, .. } => snippets(workspace, prefix),
        Context::Link { prefix, .. } => links(workspace, prefix),
    }
}

fn directives(
    registry: &Registry,
    kind: ComponentKind,
    prefix: &str,
) -> Vec<(CompletionItem, String)> {
    let mut out = Vec::new();
    for name in registry.all_names() {
        if !name.starts_with(prefix) {
            continue;
        }
        let Some(component) = registry.resolve(name) else {
            continue;
        };
        if component.kind() != kind {
            continue;
        }
        let canonical = component.name();
        let schema = component.schema();
        let required: Vec<&str> = schema.required_props().map(|p| p.name).collect();
        let item = CompletionItem::new(name, CompletionItemKind::Struct)
            .detail(form(kind))
            .documentation(describe(component))
            // An alias sorts after the canonical name it points at, so the
            // name the formatter would write is the one offered first.
            .sort(format!("{}{name}", u8::from(name != canonical)));
        let insert = if required.is_empty() {
            name.to_owned()
        } else {
            format!("{name}{{{}=\"\"}}", required[0])
        };
        out.push((item, insert));
    }
    out
}

fn props(
    registry: &Registry,
    component: &str,
    given: &[String],
    prefix: &str,
) -> Vec<(CompletionItem, String)> {
    let Some(component) = registry.resolve(component) else {
        return Vec::new();
    };
    component
        .schema()
        .props
        .iter()
        .filter(|def| def.name.starts_with(prefix))
        .filter(|def| !given.iter().any(|name| name == def.name))
        .map(|def| {
            let item = CompletionItem::new(def.name, CompletionItemKind::Property)
                .detail(type_name(&def.ty))
                .documentation(def.doc.to_owned())
                // Required first, so the prop the author cannot omit is the
                // one under the cursor.
                .sort(format!("{}{}", u8::from(!def.required), def.name));
            let insert = match def.ty {
                PropType::Bool => format!("{}=true", def.name),
                _ => format!("{}=\"\"", def.name),
            };
            (item, insert)
        })
        .collect()
}

fn prop_values(
    workspace: &Workspace,
    component: &str,
    prop: &str,
    prefix: &str,
) -> Vec<(CompletionItem, String)> {
    let Some(def) = workspace
        .registry
        .resolve(component)
        .and_then(|component| component.schema().prop(prop).cloned())
    else {
        return Vec::new();
    };
    match &def.ty {
        PropType::Enum(values) => values
            .iter()
            .filter(|value| value.starts_with(prefix))
            .map(|value| {
                (
                    CompletionItem::new(value, CompletionItemKind::Value),
                    value.clone(),
                )
            })
            .collect(),
        PropType::Bool => ["true", "false"]
            .iter()
            .filter(|value| value.starts_with(prefix))
            .map(|value| {
                (
                    CompletionItem::new(*value, CompletionItemKind::Value),
                    (*value).to_owned(),
                )
            })
            .collect(),
        PropType::Route => links(workspace, prefix),
        _ => Vec::new(),
    }
}

/// Variables and facts, offered by the dotted path a template writes.
fn expressions(workspace: &Workspace, prefix: &str) -> Vec<(CompletionItem, String)> {
    let mut out = Vec::new();
    for (path, value) in &workspace.variables {
        if !path.starts_with(prefix) {
            continue;
        }
        out.push((
            CompletionItem::new(path, CompletionItemKind::Variable)
                .detail(preview(value))
                .sort(format!("0{path}")),
            path.clone(),
        ));
    }
    for (path, fact) in &workspace.facts {
        let qualified = format!("facts.{path}");
        if !qualified.starts_with(prefix) {
            continue;
        }
        out.push((
            CompletionItem::new(&qualified, CompletionItemKind::Field)
                .detail(preview(&fact.value))
                .documentation(format!("From `{}`.", fact.file))
                .sort(format!("1{qualified}")),
            qualified,
        ));
    }
    out
}

fn snippets(workspace: &Workspace, prefix: &str) -> Vec<(CompletionItem, String)> {
    workspace
        .snippets
        .values()
        .filter(|snippet| snippet.name.starts_with(prefix))
        .map(|snippet| {
            (
                CompletionItem::new(&snippet.name, CompletionItemKind::Folder)
                    .detail(snippet.file.as_str().to_owned()),
                snippet.name.clone(),
            )
        })
        .collect()
}

fn links(workspace: &Workspace, prefix: &str) -> Vec<(CompletionItem, String)> {
    workspace
        .pages
        .values()
        .filter(|page| page.route.starts_with(prefix))
        .map(|page| {
            let mut item = CompletionItem::new(&page.route, CompletionItemKind::Reference)
                .detail(page.file.as_str().to_owned());
            if let Some(title) = &page.title {
                item = item.documentation(title.clone());
            }
            (item, page.route.clone())
        })
        .collect()
}

/// The hover and completion card for a component: what it is written as, and
/// the props it takes.
pub fn describe(component: &dyn Component) -> String {
    let schema = component.schema();
    let mut out = format!("**`{}`** — {}\n", component.name(), form(component.kind()));
    if !component.aliases().is_empty() {
        out.push_str(&format!(
            "\nAlso written `{}`.\n",
            component.aliases().join("`, `")
        ));
    }
    if !schema.props.is_empty() {
        out.push_str("\n| Prop | Type | |\n|---|---|---|\n");
        for def in &schema.props {
            out.push_str(&format!(
                "| `{}` | {} | {} |\n",
                def.name,
                type_name(&def.ty),
                if def.required { "required" } else { "" }
            ));
        }
    }
    if !schema.slots.is_empty() {
        out.push_str("\nSlots: ");
        out.push_str(
            &schema
                .slots
                .iter()
                .map(|slot| format!("`{}`", slot.name))
                .collect::<Vec<_>>()
                .join(", "),
        );
        out.push('\n');
    }
    out
}

pub fn form(kind: ComponentKind) -> &'static str {
    match kind {
        ComponentKind::Container => "container directive, `:::name … :::`",
        ComponentKind::Leaf => "leaf directive, `::name{…}`",
        ComponentKind::Inline => "inline directive, `:name[…]`",
    }
}

pub fn type_name(ty: &PropType) -> String {
    match ty {
        PropType::Str => "string".to_owned(),
        PropType::Num => "number".to_owned(),
        PropType::Bool => "boolean".to_owned(),
        PropType::Enum(values) => values.join(" \\| "),
        PropType::List(of) => format!("list of {}", type_name(of)),
        PropType::Route => "route".to_owned(),
        PropType::Asset => "asset".to_owned(),
        PropType::Icon => "icon".to_owned(),
        PropType::Color => "colour".to_owned(),
        PropType::Expr => "expression".to_owned(),
        _ => "value".to_owned(),
    }
}

/// A one-line rendering of a value, for the card beside a completion.
fn preview(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) if text.len() <= 40 => format!("\"{text}\""),
        serde_json::Value::String(_) => "string".to_owned(),
        serde_json::Value::Object(_) => "object".to_owned(),
        serde_json::Value::Array(items) => format!("{} items", items.len()),
        other => other.to_string(),
    }
}
