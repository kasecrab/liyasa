//! RX-91's static half: the accessibility checks that run in the binary.
//!
//! Three of the four the requirement names are already diagnostics the build
//! produces — alt text (E0305) and heading order (W0306) from the Markdown, and
//! token contrast from the theme. The fourth, label presence, is about the
//! rendered HTML and is checked here.
//!
//! axe-core in a real browser is the other half and needs the companion
//! runtime; a run without it says so rather than reporting a pass it did not
//! earn.

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};

/// The codes the build emits that belong to this check set.
pub const BUILD_CODES: &[u16] = &[
    305, // image without alt text
    306, // heading level skipped
];

pub fn is_a11y_code(number: u16) -> bool {
    BUILD_CODES.contains(&number)
}

/// Token contrast, from the theme the project configures.
pub fn contrast(config: &serde_json::Value) -> Diagnostics {
    let theme: liyasa_theme::config::ThemeConfig = config
        .get("theme")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default();
    let (tokens, _) = liyasa_theme::tokens::Tokens::from_config(&theme);
    tokens.contrast_diagnostics()
}

/// A form control with no accessible name, in one page's HTML.
///
/// A string scan rather than a parse: the check is "is there a name at all",
/// and the four ways to give one are all attributes or a `<label for>` pointing
/// at the control's id. Anything this misses, axe-core catches when the
/// companion runtime is present.
pub fn unlabelled_controls(route: &str, html: &str) -> Diagnostics {
    let mut out = Diagnostics::new();
    let labelled = labelled_ids(html);

    for (tag, at) in controls(html) {
        let element = &html[at..];
        let end = element.find('>').unwrap_or(element.len());
        let open = &element[..end];

        if open.contains("aria-label")
            || open.contains("aria-labelledby")
            || open.contains("title=")
            || is_hidden(open)
        {
            continue;
        }
        // A submit or button input names itself with `value`.
        if tag == "input"
            && type_of(open).is_some_and(|kind| {
                matches!(kind, "submit" | "button" | "reset" | "hidden" | "image")
            })
        {
            continue;
        }
        if id_of(open).is_some_and(|id| labelled.contains(&id)) {
            continue;
        }

        out.push(
            // W0019 is "a check class could not run"; this is a finding the
            // check made. Sharing a code would send a reader following the
            // link to a page about the other condition entirely.
            Diagnostic::new(
                code::W0020,
                format!("`<{tag}>` on `{route}` has no accessible name"),
            )
            .help("Add `aria-label`, or a `<label for>` naming its `id`."),
        );
    }
    out
}

fn controls(html: &str) -> Vec<(&'static str, usize)> {
    let mut found = Vec::new();
    for tag in ["input", "select", "textarea"] {
        let needle = format!("<{tag}");
        let mut from = 0;
        while let Some(at) = html[from..].find(&needle) {
            let absolute = from + at;
            // `<inputs>` is not `<input>`; the next character has to end the
            // tag name.
            let after = html[absolute + needle.len()..].chars().next();
            if matches!(after, Some(c) if c.is_whitespace() || c == '>' || c == '/') {
                found.push((tag, absolute));
            }
            from = absolute + needle.len();
        }
    }
    found
}

fn labelled_ids(html: &str) -> Vec<&str> {
    let mut ids = Vec::new();
    let mut from = 0;
    while let Some(at) = html[from..].find("<label") {
        let absolute = from + at;
        let end = html[absolute..]
            .find('>')
            .map_or(html.len(), |e| absolute + e);
        if let Some(value) = attribute(&html[absolute..end], "for") {
            ids.push(value);
        }
        from = end.max(absolute + 6);
    }
    ids
}

fn id_of(open: &str) -> Option<&str> {
    attribute(open, "id")
}

fn type_of(open: &str) -> Option<&str> {
    attribute(open, "type")
}

fn is_hidden(open: &str) -> bool {
    open.contains("aria-hidden=\"true\"") || open.contains("hidden")
}

/// The value of `name="…"` or `name='…'` inside one open tag.
fn attribute<'a>(open: &'a str, name: &str) -> Option<&'a str> {
    for quote in ['"', '\''] {
        let needle = format!("{name}={quote}");
        if let Some(at) = open.find(&needle) {
            let rest = &open[at + needle.len()..];
            if let Some(end) = rest.find(quote) {
                return Some(&rest[..end]);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_input_has_no_name() {
        let found = unlabelled_controls("/search", "<form><input type=\"text\"></form>");
        assert_eq!(found.len(), 1, "{found:?}");
    }

    #[test]
    fn every_way_of_naming_a_control_counts() {
        for html in [
            "<input type=\"text\" aria-label=\"Search\">",
            "<input type=\"text\" aria-labelledby=\"heading\">",
            "<input type=\"text\" title=\"Search\">",
            "<label for=\"q\">Search</label><input type=\"text\" id=\"q\">",
            "<input type=\"submit\" value=\"Go\">",
            "<input type=\"hidden\" name=\"csrf\">",
        ] {
            assert!(
                unlabelled_controls("/x", html).is_empty(),
                "reported a name-less control in: {html}"
            );
        }
    }

    #[test]
    fn select_and_textarea_are_controls_too() {
        assert_eq!(unlabelled_controls("/x", "<select></select>").len(), 1);
        assert_eq!(unlabelled_controls("/x", "<textarea></textarea>").len(), 1);
    }

    /// A tag whose name merely starts the same way is not a control.
    #[test]
    fn a_longer_tag_name_is_not_a_control() {
        assert!(unlabelled_controls("/x", "<inputs></inputs>").is_empty());
    }

    /// W0019 says a check class could not run. This is a finding the check
    /// made, which is the opposite claim, and a reader who follows the help
    /// link has to arrive at the right page.
    #[test]
    fn a_finding_does_not_share_the_could_not_run_code() {
        let found = unlabelled_controls("/x", "<input type=\"text\">");
        let first = found.iter().next().expect("a finding");
        assert_eq!(first.code.as_str(), "W0020");
    }

    #[test]
    fn the_default_theme_has_no_contrast_failure() {
        let found = contrast(&serde_json::json!({}));
        assert!(!found.has_errors(), "{found:?}");
    }
}
