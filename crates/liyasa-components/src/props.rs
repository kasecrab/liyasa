//! Reading and validating a component instance's props against its schema.
//!
//! Validation runs once, before rendering, and reports everything it finds; a
//! component's own render path then reads through [`Reader`], which falls back
//! to the schema's declared default and never panics on a wrong type.

use liyasa_core::components::{ComponentInst, PropDef, PropSchema, PropType, Value};
use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::document::{PropValue, Props};

/// Schemes a prop typed [`PropType::Route`] or [`PropType::Asset`] may carry.
const SAFE_SCHEMES: &[&str] = &["http", "https", "mailto", "tel"];

pub struct Reader<'a> {
    props: &'a Props,
    schema: &'a PropSchema,
}

impl<'a> Reader<'a> {
    pub fn new(props: &'a Props, schema: &'a PropSchema) -> Self {
        Self { props, schema }
    }

    pub fn of(inst: &'a ComponentInst, schema: &'a PropSchema) -> Self {
        Self::new(&inst.props, schema)
    }

    /// Whether the author wrote the prop, ignoring any default.
    pub fn given(&self, name: &str) -> bool {
        self.props.get(name).is_some()
    }

    pub fn str(&self, name: &str) -> Option<&'a str> {
        match self.props.get(name) {
            Some(PropValue::Str(text)) => Some(text.as_str()),
            // An expression the expander left alone still has to render as
            // something; its literal text is the honest choice.
            Some(PropValue::Expr(text)) => Some(text.as_str()),
            Some(_) => None,
            None => self.default(name).and_then(Value::as_str),
        }
    }

    pub fn str_or(&self, name: &str, fallback: &'a str) -> &'a str {
        self.str(name).unwrap_or(fallback)
    }

    pub fn num(&self, name: &str) -> Option<f64> {
        match self.props.get(name) {
            Some(PropValue::Num(number)) => Some(*number),
            Some(PropValue::Str(text)) => text.parse().ok(),
            Some(_) => None,
            None => self.default(name).and_then(Value::as_f64),
        }
    }

    /// A whole number, rounded toward zero and clamped into `range`.
    pub fn int_in(&self, name: &str, range: std::ops::RangeInclusive<i64>) -> Option<i64> {
        self.num(name)
            .map(|n| (n as i64).clamp(*range.start(), *range.end()))
    }

    pub fn int(&self, name: &str) -> Option<i64> {
        self.num(name).map(|n| n as i64)
    }

    /// A flag. A bare `{open}` with no value parses as `Bool(true)`; a missing
    /// prop falls back to the schema default, then to false.
    pub fn bool(&self, name: &str) -> bool {
        match self.props.get(name) {
            Some(PropValue::Bool(flag)) => *flag,
            Some(PropValue::Str(text)) => !matches!(text.as_str(), "false" | "0" | ""),
            Some(PropValue::Num(number)) => *number != 0.0,
            Some(_) => true,
            None => self
                .default(name)
                .and_then(Value::as_bool)
                .unwrap_or_default(),
        }
    }

    /// A flag that defaults to true when the schema says so and the author is
    /// silent, e.g. `copy` on a code block.
    pub fn bool_or(&self, name: &str, fallback: bool) -> bool {
        if self.given(name) || self.default(name).is_some() {
            self.bool(name)
        } else {
            fallback
        }
    }

    pub fn list(&self, name: &str) -> Vec<String> {
        match self.props.get(name) {
            Some(PropValue::List(items)) => items.iter().filter_map(scalar_text).collect(),
            Some(value) => scalar_text(value).into_iter().collect(),
            None => match self.default(name) {
                Some(Value::Array(items)) => items
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect(),
                Some(Value::String(text)) => vec![text.clone()],
                _ => Vec::new(),
            },
        }
    }

    /// A URL prop that passed the scheme check; `None` when it did not, so a
    /// component emits no `href` rather than an unsafe one.
    pub fn url(&self, name: &str) -> Option<&'a str> {
        self.str(name).filter(|url| is_safe_url(url))
    }

    fn default(&self, name: &str) -> Option<&'a Value> {
        self.schema.prop(name)?.default.as_ref()
    }
}

fn scalar_text(value: &PropValue) -> Option<String> {
    match value {
        PropValue::Str(text) | PropValue::Expr(text) => Some(text.clone()),
        PropValue::Num(number) => Some(format_num(*number)),
        PropValue::Bool(flag) => Some(flag.to_string()),
        PropValue::List(_) => None,
    }
}

/// `2` rather than `2.0`, so a prop round-trips through serialization.
pub fn format_num(number: f64) -> String {
    if number.fract() == 0.0 && number.abs() < 1e15 {
        format!("{}", number as i64)
    } else {
        format!("{number}")
    }
}

/// Whether a URL may be emitted into `href` or `src`.
///
/// A value with no scheme is relative and always allowed. A value with one is
/// allowed only from [`SAFE_SCHEMES`], which is why `javascript:` and `data:`
/// cannot reach the output through a prop.
pub fn is_safe_url(url: &str) -> bool {
    match scheme_of(url) {
        Some(scheme) => SAFE_SCHEMES.contains(&scheme.as_str()),
        None => true,
    }
}

/// The scheme of a URL, lowercased, or `None` when it is relative.
///
/// Leading whitespace and C0 controls are stripped first: `java\0script:` and
/// ` javascript:` are the classic ways past a naive `starts_with`.
fn scheme_of(url: &str) -> Option<String> {
    let cleaned: String = url
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .collect();
    let colon = cleaned.find(':')?;
    let scheme = &cleaned[..colon];
    if scheme.is_empty() {
        return None;
    }
    // A `:` after a path separator or a fragment is part of the path, not a
    // scheme: `/a/b:c` and `#x:y` are relative.
    if cleaned[..colon].contains(['/', '?', '#']) {
        return None;
    }
    let mut chars = scheme.chars();
    let first = chars.next()?;
    if !first.is_ascii_alphabetic() {
        return None;
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')) {
        return None;
    }
    Some(scheme.to_ascii_lowercase())
}

/// Checks one instance's props against its schema.
///
/// Reports every problem rather than the first: an author fixing a component
/// call should see all of it in one build.
pub fn validate(inst: &ComponentInst, schema: &PropSchema, out: &mut Diagnostics) {
    let name = &inst.name;
    for def in schema.required_props() {
        if inst.props.get(def.name).is_none() {
            out.push(located(
                Diagnostic::new(
                    code::E0314,
                    format!("`{name}` requires the `{}` prop", def.name),
                )
                .help(def.doc.to_owned()),
                inst,
            ));
        }
    }

    for (prop, value) in &inst.props.0 {
        let Some(def) = schema.prop(prop) else {
            out.push(located(
                Diagnostic::new(code::W0316, format!("`{name}` has no prop `{prop}`"))
                    .help(suggest(prop, schema)),
                inst,
            ));
            continue;
        };
        if let Some(problem) = type_error(def, value) {
            out.push(located(
                Diagnostic::new(code::E0315, format!("`{name}.{prop}` {problem}")),
                inst,
            ));
        }
        if matches!(def.ty, PropType::Route | PropType::Asset)
            && let PropValue::Str(url) = value
            && !is_safe_url(url)
        {
            out.push(located(
                Diagnostic::new(
                    code::E0353,
                    format!("`{name}.{prop}` uses a URL scheme that is not allowed"),
                )
                .help(format!("allowed schemes: {}", SAFE_SCHEMES.join(", "))),
                inst,
            ));
        }
    }
}

fn located(diagnostic: Diagnostic, inst: &ComponentInst) -> Diagnostic {
    match inst.origin.span {
        Some(span) => diagnostic.at(span),
        None => diagnostic,
    }
}

/// `None` when the value fits the declared type.
fn type_error(def: &PropDef, value: &PropValue) -> Option<String> {
    // An unevaluated expression is checked after expansion, not here.
    if matches!(value, PropValue::Expr(_)) {
        return None;
    }
    match (&def.ty, value) {
        (
            PropType::Str | PropType::Route | PropType::Asset | PropType::Icon | PropType::Color,
            PropValue::Str(_),
        )
        | (PropType::Expr, _)
        | (PropType::Bool, PropValue::Bool(_))
        | (PropType::Num, PropValue::Num(_)) => None,
        // `cols=2` written without quotes, and `{open}` written bare, are the
        // shapes authors actually write; accept them and coerce on read.
        (PropType::Num, PropValue::Str(text)) if text.parse::<f64>().is_ok() => None,
        (PropType::Bool, PropValue::Str(text)) if matches!(text.as_str(), "true" | "false") => None,
        (PropType::Enum(allowed), PropValue::Str(text)) => {
            if allowed.iter().any(|a| a == text) {
                None
            } else {
                Some(format!(
                    "is `{text}`, which is not one of: {}",
                    allowed.join(", ")
                ))
            }
        }
        (PropType::List(item), PropValue::List(items)) => items
            .iter()
            .find_map(|value| type_error(&as_def(def, item), value)),
        // A one-item list may be written without brackets.
        (PropType::List(item), value) => type_error(&as_def(def, item), value),
        (expected, found) => Some(format!(
            "expects {}, found {}",
            type_name(expected),
            value_name(found)
        )),
    }
}

fn as_def(def: &PropDef, ty: &PropType) -> PropDef {
    PropDef {
        name: def.name,
        ty: ty.clone(),
        required: false,
        default: None,
        doc: def.doc,
    }
}

fn type_name(ty: &PropType) -> String {
    match ty {
        PropType::Str => "a string".into(),
        PropType::Num => "a number".into(),
        PropType::Bool => "true or false".into(),
        PropType::Enum(allowed) => format!("one of: {}", allowed.join(", ")),
        PropType::List(item) => format!("a list of {}", type_name(item)),
        PropType::Route => "a route or URL".into(),
        PropType::Asset => "an asset path or URL".into(),
        PropType::Icon => "an icon name".into(),
        PropType::Color => "a colour".into(),
        PropType::Expr => "an expression".into(),
        _ => "a value".into(),
    }
}

fn value_name(value: &PropValue) -> &'static str {
    match value {
        PropValue::Str(_) => "a string",
        PropValue::Num(_) => "a number",
        PropValue::Bool(_) => "true or false",
        PropValue::List(_) => "a list",
        PropValue::Expr(_) => "an expression",
    }
}

/// The closest prop name by edit distance, for the `did you mean` line.
fn suggest(typo: &str, schema: &PropSchema) -> String {
    let best = schema
        .props
        .iter()
        .map(|def| (distance(typo, def.name), def.name))
        .filter(|(d, _)| *d * 3 <= typo.len().max(1))
        .min();
    match best {
        Some((_, name)) => format!("did you mean `{name}`?"),
        None if schema.props.is_empty() => "this component takes no props".into(),
        None => format!(
            "props: {}",
            schema
                .props
                .iter()
                .map(|d| d.name)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn distance(a: &str, b: &str) -> usize {
    let b_chars: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b_chars.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b_chars.iter().enumerate() {
            let cost = usize::from(ca != *cb);
            let next = (row[j] + 1).min(row[j + 1] + 1).min(diagonal + cost);
            diagonal = row[j + 1];
            row[j + 1] = next;
        }
    }
    row[b_chars.len()]
}
