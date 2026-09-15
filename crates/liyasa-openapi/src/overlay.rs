//! OpenAPI Overlay 1.0 (API-06).
//!
//! An overlay is how an operator fixes someone else's spec without editing it:
//! rename a path, add a description, hide an operation, correct an example.
//! Actions are applied to the document tree before it is read, so everything
//! downstream sees the corrected spec and nothing has to know an overlay
//! happened.
//!
//! The target is a JSONPath expression. The subset here is the one overlays
//! actually use — a walk down named keys, wildcards, indices, recursive
//! descent, and an equality filter — and anything else is `E0507` naming what
//! it could not read rather than a silent no-op.

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};

use crate::tree::{Map, Pointer, Value, as_map, as_seq, as_str};

/// One `actions` entry.
#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub target: String,
    pub description: Option<String>,
    pub update: Option<Value>,
    pub remove: bool,
}

/// A parsed overlay document.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Overlay {
    pub version: String,
    pub title: Option<String>,
    pub extends: Option<String>,
    pub actions: Vec<Action>,
}

impl Overlay {
    /// Reads an overlay document. A document that is not one is `E0507`
    /// rather than an overlay with no actions, because silently doing nothing
    /// is the failure an operator cannot see.
    pub fn read(root: &Value, origin: &str) -> Result<Self, crate::SpecError> {
        let Some(version) = crate::tree::field_str(root, "overlay") else {
            return Err(Box::new(
                Diagnostic::new(
                    code::E0507,
                    format!("`{origin}` does not declare `overlay`"),
                )
                .help("an overlay document starts with `overlay: 1.0.0`"),
            ));
        };
        if !version.starts_with("1.") {
            return Err(Box::new(Diagnostic::new(
                code::E0507,
                format!("`{origin}` is Overlay {version}; Liyasa applies 1.x"),
            )));
        }
        let mut actions = Vec::new();
        for item in crate::tree::field(root, "actions")
            .and_then(as_seq)
            .unwrap_or_default()
        {
            let Some(target) = crate::tree::field_str(item, "target") else {
                return Err(Box::new(Diagnostic::new(
                    code::E0507,
                    format!("`{origin}`: an action has no `target`"),
                )));
            };
            actions.push(Action {
                target: target.to_owned(),
                description: crate::tree::field_str(item, "description").map(str::to_owned),
                update: crate::tree::field(item, "update").cloned(),
                remove: crate::tree::field(item, "remove")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            });
        }
        Ok(Self {
            version: version.to_owned(),
            title: crate::tree::field(root, "info")
                .and_then(|info| crate::tree::field_str(info, "title"))
                .map(str::to_owned),
            extends: crate::tree::field_str(root, "extends").map(str::to_owned),
            actions,
        })
    }

    /// Applies every action in order, reporting the ones that could not be.
    pub fn apply(&self, spec: &mut Value) -> Diagnostics {
        let mut diagnostics = Diagnostics::new();
        for (index, action) in self.actions.iter().enumerate() {
            let matched = match select(spec, &action.target) {
                Ok(matched) => matched,
                Err(message) => {
                    diagnostics.push(
                        Diagnostic::new(code::E0507, format!("action {index}: {message}"))
                            .help(format!("target: `{}`", action.target)),
                    );
                    continue;
                }
            };
            if matched.is_empty() {
                diagnostics.push(
                    Diagnostic::new(code::W0512, format!("action {index} matched nothing"))
                        .help(format!("target: `{}`", action.target)),
                );
                continue;
            }
            // Deepest first, so removing one does not move the next.
            let mut matched = matched;
            matched.sort_by_key(|pointer| std::cmp::Reverse(pointer.as_str().len()));
            for pointer in matched {
                if action.remove {
                    crate::tree::remove(spec, &pointer);
                    continue;
                }
                let Some(update) = &action.update else {
                    continue;
                };
                let Some(target) = crate::tree::get_mut(spec, &pointer) else {
                    continue;
                };
                merge(target, update);
            }
        }
        diagnostics
    }
}

/// Merges `update` into `target`: objects recursively, everything else by
/// replacement, and an array grows by what the update adds.
///
/// Overlay 1.0 leaves `null` alone rather than treating it as a deletion the
/// way JSON Merge Patch does; `remove: true` is how an overlay deletes.
fn merge(target: &mut Value, update: &Value) {
    match (&mut *target, update) {
        (Value::Mapping(into), Value::Mapping(from)) => {
            for (key, value) in from.iter() {
                match into.get_mut(key) {
                    Some(existing) => merge(existing, value),
                    None => {
                        into.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (Value::Sequence(into), Value::Sequence(from)) => into.extend(from.iter().cloned()),
        (Value::Sequence(into), other) => into.push(other.clone()),
        _ => *target = update.clone(),
    }
}

/// One step of a target expression.
#[derive(Debug, Clone, PartialEq)]
enum Step {
    Key(String),
    Index(usize),
    Wildcard,
    /// `..name`, or `..*` with `None`.
    Descend(Option<String>),
    /// `[?(@.field=='value')]`.
    Filter {
        field: String,
        value: Value,
    },
}

/// Resolves a target expression to the pointers it matches.
pub fn select(root: &Value, target: &str) -> Result<Vec<Pointer>, String> {
    let steps = parse(target)?;
    let mut found = vec![Pointer::root()];
    for step in &steps {
        let mut next = Vec::new();
        for pointer in &found {
            let Some(node) = crate::tree::get(root, pointer) else {
                continue;
            };
            walk(node, pointer, step, &mut next);
        }
        found = next;
    }
    Ok(found)
}

fn walk(node: &Value, at: &Pointer, step: &Step, out: &mut Vec<Pointer>) {
    match step {
        Step::Key(name) => {
            if let Some(map) = as_map(node)
                && map.contains_key(name.as_str())
            {
                out.push(at.push(name));
            }
        }
        Step::Index(index) => {
            if let Some(items) = as_seq(node)
                && *index < items.len()
            {
                out.push(at.index(*index));
            }
        }
        Step::Wildcard => children(node, at, out),
        Step::Descend(name) => descend(node, at, name.as_deref(), out),
        Step::Filter { field, value } => {
            let mut candidates = Vec::new();
            children(node, at, &mut candidates);
            for candidate in candidates {
                // `children` appends one segment, so the candidate is one step
                // below `node` and is resolved against it, not against a root
                // this function does not have.
                let Some(segment) = candidate.segments().last() else {
                    continue;
                };
                if let Some(item) = step_into(node, &segment)
                    && crate::tree::field(item, field) == Some(value)
                {
                    out.push(candidate);
                }
            }
        }
    }
}

fn children(node: &Value, at: &Pointer, out: &mut Vec<Pointer>) {
    match node {
        Value::Mapping(map) => {
            for (key, _) in map.iter() {
                if let Some(key) = as_str(key) {
                    out.push(at.push(key));
                }
            }
        }
        Value::Sequence(items) => {
            for index in 0..items.len() {
                out.push(at.index(index));
            }
        }
        _ => {}
    }
}

fn descend(node: &Value, at: &Pointer, name: Option<&str>, out: &mut Vec<Pointer>) {
    match name {
        Some(name) => {
            if let Some(map) = as_map(node)
                && map.contains_key(name)
            {
                out.push(at.push(name));
            }
        }
        None => out.push(at.clone()),
    }
    let mut below = Vec::new();
    children(node, at, &mut below);
    for child in below {
        // `children` appends exactly one segment, so the child is one step
        // down from `node`.
        let Some(segment) = child.segments().last() else {
            continue;
        };
        let Some(item) = step_into(node, &segment) else {
            continue;
        };
        descend(item, &child, name, out);
    }
}

fn step_into<'a>(node: &'a Value, segment: &str) -> Option<&'a Value> {
    match node {
        Value::Mapping(map) => map.get(segment),
        Value::Sequence(items) => items.get(segment.parse::<usize>().ok()?),
        _ => None,
    }
}

/// Reads the JSONPath subset overlays use.
fn parse(target: &str) -> Result<Vec<Step>, String> {
    let target = target.trim();
    let mut rest = match target.strip_prefix('$') {
        Some(rest) => rest,
        None => return Err(format!("`{target}` does not start at `$`")),
    };
    let mut steps = Vec::new();
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("..") {
            let (name, tail) = name_of(after);
            steps.push(Step::Descend(if name == "*" {
                None
            } else {
                Some(name.to_owned())
            }));
            rest = tail;
            continue;
        }
        if let Some(after) = rest.strip_prefix('.') {
            let (name, tail) = name_of(after);
            if name.is_empty() {
                return Err(format!("`{target}` has an empty step"));
            }
            steps.push(if name == "*" {
                Step::Wildcard
            } else {
                Step::Key(name.to_owned())
            });
            rest = tail;
            continue;
        }
        if let Some(after) = rest.strip_prefix('[') {
            let Some(end) = after.find(']') else {
                return Err(format!("`{target}` has an unclosed `[`"));
            };
            let inside = &after[..end];
            steps.push(bracket(inside, target)?);
            rest = &after[end + 1..];
            continue;
        }
        return Err(format!("`{target}` is not a path Liyasa reads"));
    }
    Ok(steps)
}

fn bracket(inside: &str, target: &str) -> Result<Step, String> {
    let inside = inside.trim();
    if inside == "*" {
        return Ok(Step::Wildcard);
    }
    if let Some(quoted) = unquote(inside) {
        return Ok(Step::Key(quoted));
    }
    if let Ok(index) = inside.parse::<usize>() {
        return Ok(Step::Index(index));
    }
    if let Some(filter) = inside.strip_prefix("?(").and_then(|s| s.strip_suffix(')')) {
        let (field, value) = filter
            .split_once("==")
            .ok_or_else(|| format!("`{target}`: only `==` filters are read"))?;
        let field = field
            .trim()
            .strip_prefix("@.")
            .ok_or_else(|| format!("`{target}`: a filter compares `@.field`"))?;
        return Ok(Step::Filter {
            field: field.trim().to_owned(),
            value: literal(value.trim()),
        });
    }
    Err(format!(
        "`{target}`: `[{inside}]` is not a step Liyasa reads"
    ))
}

fn unquote(text: &str) -> Option<String> {
    for quote in ['\'', '"'] {
        if text.starts_with(quote) && text.ends_with(quote) && text.len() >= 2 {
            return Some(text[1..text.len() - 1].to_owned());
        }
    }
    None
}

fn literal(text: &str) -> Value {
    if let Some(quoted) = unquote(text) {
        return Value::String(quoted);
    }
    match text {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        "null" => Value::Null,
        other => match other.parse::<i64>() {
            Ok(number) => Value::Number(number.into()),
            Err(_) => Value::String(other.to_owned()),
        },
    }
}

/// A dotted step's name, which runs to the next `.` or `[`.
fn name_of(text: &str) -> (&str, &str) {
    let end = text.find(['.', '[']).unwrap_or(text.len());
    (&text[..end], &text[end..])
}

/// Reads and applies one overlay document in one call.
pub fn apply(
    spec: &mut Value,
    overlay: &Value,
    origin: &str,
) -> Result<Diagnostics, crate::SpecError> {
    Ok(Overlay::read(overlay, origin)?.apply(spec))
}

/// The empty mapping an action's `update` falls back to, kept here so callers
/// do not build one each time.
pub fn nothing() -> Value {
    Value::Mapping(Map::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::parse as parse_tree;

    const SPEC: &str = r##"
openapi: 3.1.0
info: { title: Original, version: "1" }
paths:
  /users:
    get:
      summary: List
      parameters:
        - { name: limit, in: query, schema: { type: integer } }
        - { name: cursor, in: query, schema: { type: string } }
      responses:
        "200": { description: ok }
    post:
      summary: Create
      responses:
        "201": { description: made }
  /internal:
    get:
      summary: Internal
      responses:
        "200": { description: ok }
"##;

    fn spec() -> Value {
        parse_tree(SPEC.as_bytes(), "api.yaml").expect("the fixture parses")
    }

    fn overlay(actions: &str) -> Value {
        parse_tree(
            format!("overlay: 1.0.0\ninfo: {{ title: T, version: \"1\" }}\nactions:\n{actions}")
                .as_bytes(),
            "api.overlay.yaml",
        )
        .expect("the overlay parses")
    }

    fn at(root: &Value, pointer: &str) -> Value {
        crate::tree::get(root, &Pointer::parse(pointer))
            .cloned()
            .unwrap_or(Value::Null)
    }

    #[test]
    fn an_update_merges_into_one_named_target() {
        let mut spec = spec();
        let diagnostics = apply(
            &mut spec,
            &overlay("  - target: $.info\n    update: { title: Renamed, contact: { name: Us } }\n"),
            "api.overlay.yaml",
        )
        .expect("the overlay reads");
        assert!(diagnostics.is_empty(), "{:?}", diagnostics.as_slice());
        assert_eq!(at(&spec, "/info/title"), Value::from("Renamed"));
        assert_eq!(
            at(&spec, "/info/version"),
            Value::from("1"),
            "untouched keys stay"
        );
        assert_eq!(at(&spec, "/info/contact/name"), Value::from("Us"));
    }

    #[test]
    fn a_quoted_key_reaches_a_path_with_slashes_in_it() {
        let mut spec = spec();
        apply(
            &mut spec,
            &overlay(
                "  - target: $.paths['/users'].get\n    update: { description: Lists users }\n",
            ),
            "api.overlay.yaml",
        )
        .expect("the overlay reads");
        assert_eq!(
            at(&spec, "/paths/~1users/get/description"),
            Value::from("Lists users")
        );
    }

    #[test]
    fn remove_deletes_the_node_and_leaves_its_siblings() {
        let mut spec = spec();
        apply(
            &mut spec,
            &overlay("  - target: $.paths['/internal']\n    remove: true\n"),
            "api.overlay.yaml",
        )
        .expect("the overlay reads");
        assert_eq!(at(&spec, "/paths/~1internal"), Value::Null);
        assert!(at(&spec, "/paths/~1users/get").is_mapping());
    }

    #[test]
    fn a_wildcard_updates_every_operation_of_a_path() {
        let mut spec = spec();
        apply(
            &mut spec,
            &overlay(
                "  - target: $.paths['/users'].*\n    update: { x-liyasa: { group: Users } }\n",
            ),
            "api.overlay.yaml",
        )
        .expect("the overlay reads");
        assert_eq!(
            at(&spec, "/paths/~1users/get/x-liyasa/group"),
            Value::from("Users")
        );
        assert_eq!(
            at(&spec, "/paths/~1users/post/x-liyasa/group"),
            Value::from("Users")
        );
    }

    #[test]
    fn a_filter_reaches_the_one_parameter_it_names() {
        let mut spec = spec();
        apply(
            &mut spec,
            &overlay(
                "  - target: $.paths['/users'].get.parameters[?(@.name=='limit')]\n    update: { description: How many }\n",
            ),
            "api.overlay.yaml",
        )
        .expect("the overlay reads");
        assert_eq!(
            at(&spec, "/paths/~1users/get/parameters/0/description"),
            Value::from("How many")
        );
        assert_eq!(
            at(&spec, "/paths/~1users/get/parameters/1/description"),
            Value::Null
        );
    }

    #[test]
    fn recursive_descent_reaches_every_node_with_that_key() {
        let mut spec = spec();
        apply(
            &mut spec,
            &overlay(
                "  - target: $..responses\n    update: { \"500\": { description: Broken } }\n",
            ),
            "api.overlay.yaml",
        )
        .expect("the overlay reads");
        assert_eq!(
            at(&spec, "/paths/~1users/get/responses/500/description"),
            Value::from("Broken")
        );
        assert_eq!(
            at(&spec, "/paths/~1internal/get/responses/500/description"),
            Value::from("Broken")
        );
    }

    #[test]
    fn an_update_on_an_array_appends_rather_than_replacing() {
        let mut spec = spec();
        apply(
            &mut spec,
            &overlay(
                "  - target: $.paths['/users'].get.parameters\n    update: [{ name: fields, in: query }]\n",
            ),
            "api.overlay.yaml",
        )
        .expect("the overlay reads");
        assert_eq!(
            at(&spec, "/paths/~1users/get/parameters/2/name"),
            Value::from("fields")
        );
        assert_eq!(
            at(&spec, "/paths/~1users/get/parameters/0/name"),
            Value::from("limit")
        );
    }

    #[test]
    fn a_target_that_matches_nothing_is_w0512_rather_than_silence() {
        let mut spec = spec();
        let diagnostics = apply(
            &mut spec,
            &overlay("  - target: $.paths['/nope'].get\n    update: { summary: x }\n"),
            "api.overlay.yaml",
        )
        .expect("the overlay reads");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics.iter().next().map(|d| d.code), Some(code::W0512));
    }

    #[test]
    fn a_target_liyasa_cannot_read_is_e0507_naming_it() {
        let mut spec = spec();
        let diagnostics = apply(
            &mut spec,
            &overlay("  - target: $.paths[?(@.get.summary =~ /List/)]\n    update: { x: 1 }\n"),
            "api.overlay.yaml",
        )
        .expect("the overlay reads");
        assert_eq!(diagnostics.iter().next().map(|d| d.code), Some(code::E0507));
    }

    #[test]
    fn a_document_that_is_not_an_overlay_is_e0507() {
        let document = spec();
        let mut spec = spec();
        let error = apply(&mut spec, &document, "api.yaml").expect_err("a spec is not an overlay");
        assert_eq!(error.code, code::E0507);
    }

    #[test]
    fn a_later_overlay_version_is_refused() {
        let mut spec = spec();
        let document = parse_tree(b"overlay: 2.0.0\nactions: []\n", "o.yaml").expect("parses");
        let error = apply(&mut spec, &document, "o.yaml").expect_err("2.0 is not applied");
        assert_eq!(error.code, code::E0507);
    }
}
