//! The `style` attribute under `security.styleAttribute: "allowlist"` (CM-32).
//!
//! CSS is not decoration only: `position: fixed` builds an overlay over someone
//! else's content and `url()` is a network request that reports who read the
//! page. The allow list is the set of properties that cannot do either.

/// Properties an author may set inline.
pub const PROPERTIES: &[&str] = &[
    "background-color",
    "color",
    "display",
    "float",
    "height",
    "margin",
    "margin-bottom",
    "margin-left",
    "margin-right",
    "margin-top",
    "max-width",
    "padding",
    "padding-bottom",
    "padding-left",
    "padding-right",
    "padding-top",
    "text-align",
    "width",
];

/// Substrings that disqualify a declaration whatever property it sets.
const REJECTED: &[&str] = &["url(", "expression(", "@import", "position:", "/*"];

/// The declarations of a `style` attribute that survive, re-joined. `None` when
/// nothing does.
pub fn filter(style: &str) -> Option<String> {
    let kept: Vec<String> = style
        .split(';')
        .filter_map(|declaration| {
            let declaration = declaration.trim();
            let (property, value) = declaration.split_once(':')?;
            let property = property.trim().to_ascii_lowercase();
            if !PROPERTIES.contains(&property.as_str()) {
                return None;
            }
            let flattened = declaration.to_ascii_lowercase().replace([' ', '\t'], "");
            if REJECTED.iter().any(|bad| flattened.contains(bad)) {
                return None;
            }
            Some(format!("{property}: {}", value.trim()))
        })
        .collect();
    (!kept.is_empty()).then(|| kept.join("; "))
}

#[cfg(test)]
mod tests;
