//! Directives: the info string, leaf and inline forms, the tag form, and the
//! checks over the parsed tree (PRD §7.5.1, `plan/rfcs/0003-parser-spike.md`).
//!
//! comrak 0.55 owns container segmentation, so this module parses what comrak
//! hands over and scans the two forms comrak does not know about.

pub mod info;
pub mod inline;
pub mod mask;
pub mod props;
pub mod rewrite;
pub mod slots;
pub mod tag;
#[cfg(test)]
pub mod testing;
pub mod validate;

use liyasa_core::document::PropValue;

/// A prop value written the way a directive would carry it. Shared by the
/// formatter, the Markdown serialization, and the tag round-trip.
pub fn render_value(value: &PropValue) -> String {
    match value {
        PropValue::Str(text) => format!("\"{}\"", text.replace('"', "&quot;")),
        PropValue::Num(number) => {
            if number.fract() == 0.0 && number.abs() < 1e15 {
                format!("{number:.0}")
            } else {
                number.to_string()
            }
        }
        PropValue::Bool(value) => value.to_string(),
        PropValue::List(items) => format!(
            "[{}]",
            items.iter().map(render_value).collect::<Vec<_>>().join(",")
        ),
        PropValue::Expr(expr) => format!("{{{{ {expr} }}}}"),
    }
}
