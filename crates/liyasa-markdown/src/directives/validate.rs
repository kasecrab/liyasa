//! Component names, props, and slots checked against the registry
//! (CM-51, CM-52, CM-54).
//!
//! Every message here is read by someone who has just mistyped something, so
//! each one carries the nearest name that would have worked.

use liyasa_core::components::{Component, ComponentRegistry, PropType};
use liyasa_core::diagnostics::code;
use liyasa_core::document::{Block, BlockKind, Inline, Node, PropValue, Props, Slots};
use liyasa_core::markdown::ComponentKind;
use liyasa_core::{Diagnostic, Diagnostics, Span};

use std::collections::BTreeMap;

/// How each component was written, keyed by the span it occupies, so `::card`
/// and `:::card` can be told apart after the tree is built.
pub type Written = BTreeMap<Span, ComponentKind>;

pub fn check(
    root: &mut Block,
    registry: &dyn ComponentRegistry,
    written: &Written,
    diagnostics: &mut Diagnostics,
) {
    canonicalize(root, registry);
    block(root, registry, written, diagnostics);
}

/// Rewrites every component name the registry resolves to its canonical
/// kebab-case form, so `<Card>` and `:::card` are one component from here on
/// and nothing downstream has to know the tag form existed (CM-53).
fn canonicalize(block: &mut Block, registry: &dyn ComponentRegistry) {
    if let BlockKind::Component { name, .. } = &mut block.kind
        && let Some(component) = registry.get(name)
    {
        *name = component.name().to_owned();
    }
    for child in &mut block.children {
        match child {
            Node::Block(child) => canonicalize(child, registry),
            Node::Inline(child) => canonicalize_inline(child, registry),
        }
    }
}

fn canonicalize_inline(inline: &mut Inline, registry: &dyn ComponentRegistry) {
    match inline {
        Inline::InlineComponent { name, children, .. } => {
            if let Some(component) = registry.get(name) {
                *name = component.name().to_owned();
            }
            for child in children {
                canonicalize_inline(child, registry);
            }
        }
        Inline::Emph(children)
        | Inline::Strong(children)
        | Inline::Strike(children)
        | Inline::Link { children, .. } => {
            for child in children {
                canonicalize_inline(child, registry);
            }
        }
        _ => {}
    }
}

fn block(
    block_: &Block,
    registry: &dyn ComponentRegistry,
    written: &Written,
    diagnostics: &mut Diagnostics,
) {
    if let BlockKind::Component { name, props, slots } = &block_.kind {
        let kind = block_
            .origin
            .span
            .and_then(|span| written.get(&span).copied())
            .unwrap_or(if block_.children.is_empty() {
                ComponentKind::Leaf
            } else {
                ComponentKind::Container
            });
        instance(
            name,
            props,
            Some(slots),
            kind,
            block_.origin.span,
            registry,
            diagnostics,
        );
    }
    for child in &block_.children {
        match child {
            Node::Block(child) => block(child, registry, written, diagnostics),
            Node::Inline(child) => inline(child, block_.origin.span, registry, diagnostics),
        }
    }
}

fn inline(
    node: &Inline,
    span: Option<Span>,
    registry: &dyn ComponentRegistry,
    diagnostics: &mut Diagnostics,
) {
    match node {
        Inline::InlineComponent {
            name,
            props,
            children,
        } => {
            instance(
                name,
                props,
                None,
                ComponentKind::Inline,
                span,
                registry,
                diagnostics,
            );
            for child in children {
                inline(child, span, registry, diagnostics);
            }
        }
        Inline::Emph(children)
        | Inline::Strong(children)
        | Inline::Strike(children)
        | Inline::Link { children, .. } => {
            for child in children {
                inline(child, span, registry, diagnostics);
            }
        }
        _ => {}
    }
}

fn instance(
    name: &str,
    props: &Props,
    slots: Option<&Slots>,
    written: ComponentKind,
    span: Option<Span>,
    registry: &dyn ComponentRegistry,
    diagnostics: &mut Diagnostics,
) {
    let at = |diagnostic: Diagnostic| match span {
        Some(span) => diagnostic.at(span),
        None => diagnostic,
    };

    let Some(component) = registry.get(name) else {
        let mut diagnostic = Diagnostic::new(code::E0313, format!("unknown component `{name}`"));
        if let Some(near) = nearest(name, &registry.names()) {
            diagnostic = diagnostic.help(format!("did you mean `{near}`?"));
        }
        diagnostics.push(at(diagnostic));
        return;
    };

    if component.kind() != written {
        diagnostics.push(at(Diagnostic::new(
            code::E0317,
            format!(
                "`{name}` is {}, and it is written here as {}",
                article(component.kind()),
                article(written)
            ),
        )
        .help(form_of(name, component.kind()))));
    }

    let schema = component.schema();
    for required in schema.required_props() {
        if props.get(required.name).is_none() {
            diagnostics.push(at(Diagnostic::new(
                code::E0314,
                format!("`{name}` needs a `{}` prop", required.name),
            )
            .help(required.doc.to_owned())));
        }
    }

    for (key, value) in &props.0 {
        let Some(def) = schema.prop(key) else {
            // `.class` and `#id` are shorthand every component accepts.
            if key == "class" || key == "id" {
                continue;
            }
            let mut diagnostic =
                Diagnostic::new(code::W0316, format!("`{name}` has no `{key}` prop"));
            let known: Vec<&str> = schema.props.iter().map(|p| p.name).collect();
            if let Some(near) = nearest(key, &known) {
                diagnostic = diagnostic.help(format!("did you mean `{near}`?"));
            }
            diagnostics.push(at(diagnostic));
            continue;
        };
        if let Some(found) = mismatch(&def.ty, value) {
            diagnostics.push(at(Diagnostic::new(
                code::E0315,
                format!(
                    "`{name}.{key}` expects {}, and this is {found}",
                    expected(&def.ty)
                ),
            )));
        }
    }

    if let Some(slots) = slots {
        for used in slots.0.keys() {
            if !schema.slots.iter().any(|slot| slot.name == used) {
                let mut diagnostic =
                    Diagnostic::new(code::E0350, format!("`{name}` has no `{used}` slot"));
                let known: Vec<&str> = schema.slots.iter().map(|slot| slot.name).collect();
                if let Some(near) = nearest(used, &known) {
                    diagnostic = diagnostic.help(format!("did you mean `{near}`?"));
                }
                diagnostics.push(at(diagnostic));
            }
        }
    }
}

/// The written form a component of this kind takes, for the help line.
fn form_of(name: &str, kind: ComponentKind) -> String {
    match kind {
        ComponentKind::Container => format!(":::{name}\nbody\n:::"),
        ComponentKind::Leaf => format!("::{name}"),
        ComponentKind::Inline => format!(":{name}[content]"),
    }
}

fn article(kind: ComponentKind) -> &'static str {
    match kind {
        ComponentKind::Container => "a container component",
        ComponentKind::Leaf => "a leaf component",
        ComponentKind::Inline => "an inline component",
    }
}

/// `None` when the value fits the declared type. An expression is unevaluated
/// until expansion, so it fits everything.
fn mismatch(ty: &PropType, value: &PropValue) -> Option<&'static str> {
    if matches!(value, PropValue::Expr(_)) {
        return None;
    }
    let ok = match ty {
        PropType::Str | PropType::Route | PropType::Asset | PropType::Icon | PropType::Color => {
            matches!(value, PropValue::Str(_))
        }
        PropType::Num => matches!(value, PropValue::Num(_)),
        PropType::Bool => matches!(value, PropValue::Bool(_)),
        PropType::Enum(allowed) => {
            matches!(value, PropValue::Str(text) if allowed.iter().any(|a| a == text))
        }
        PropType::List(inner) => match value {
            PropValue::List(items) => items.iter().all(|item| mismatch(inner, item).is_none()),
            _ => false,
        },
        PropType::Expr => true,
        _ => true,
    };
    (!ok).then(|| found(value))
}

fn found(value: &PropValue) -> &'static str {
    match value {
        PropValue::Str(_) => "a string",
        PropValue::Num(_) => "a number",
        PropValue::Bool(_) => "a boolean",
        PropValue::List(_) => "a list",
        PropValue::Expr(_) => "an expression",
    }
}

fn expected(ty: &PropType) -> String {
    match ty {
        PropType::Str => "a string".to_owned(),
        PropType::Num => "a number".to_owned(),
        PropType::Bool => "a boolean".to_owned(),
        PropType::Enum(allowed) => format!("one of {}", allowed.join(", ")),
        PropType::List(inner) => format!("a list of {}", expected(inner)),
        PropType::Route => "a route".to_owned(),
        PropType::Asset => "an asset path".to_owned(),
        PropType::Icon => "an icon name".to_owned(),
        PropType::Color => "a colour".to_owned(),
        PropType::Expr => "an expression".to_owned(),
        _ => "a different type".to_owned(),
    }
}

/// The closest name worth suggesting, by edit distance. A suggestion that is
/// more than a third wrong is not a suggestion, it is a guess.
pub fn nearest<'a>(typo: &str, names: &[&'a str]) -> Option<&'a str> {
    let budget = (typo.chars().count() / 3).max(1);
    names
        .iter()
        .map(|name| (distance(typo, name), *name))
        .filter(|(seen, _)| *seen <= budget)
        .min_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)))
        .map(|(_, name)| name)
}

/// Damerau-Levenshtein distance over characters, three rows at a time.
///
/// Transposition costs one, not two: `crad` for `card` is one slip of the
/// fingers, and a plain Levenshtein budget would refuse to suggest anything.
fn distance(left: &str, right: &str) -> usize {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    let mut rows = vec![vec![0usize; right.len() + 1]; left.len() + 1];
    for (column, cell) in rows[0].iter_mut().enumerate() {
        *cell = column;
    }
    for row in 1..=left.len() {
        rows[row][0] = row;
        for column in 1..=right.len() {
            let cost = usize::from(left[row - 1] != right[column - 1]);
            let mut best = (rows[row - 1][column - 1] + cost)
                .min(rows[row - 1][column] + 1)
                .min(rows[row][column - 1] + 1);
            if row > 1
                && column > 1
                && left[row - 1] == right[column - 2]
                && left[row - 2] == right[column - 1]
            {
                best = best.min(rows[row - 2][column - 2] + 1);
            }
            rows[row][column] = best;
        }
    }
    rows[left.len()][right.len()]
}

/// Held so the trait object's lifetime does not leak into every signature.
pub type Registered<'a> = &'a dyn Component;

#[cfg(test)]
mod tests;
