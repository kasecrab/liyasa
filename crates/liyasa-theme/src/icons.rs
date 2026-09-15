//! The theme's own icons (CFG-07).
//!
//! Drawn here as inline SVG rather than taken from an icon set: the chrome
//! needs eight shapes, the site's configured library is for content, and a
//! glyph from a font renders differently on every platform. Each is a 16-unit
//! square on `currentColor`, so it inherits the colour and the size of the text
//! beside it and costs no request (THM-32).

/// The icons the chrome uses, by name.
pub const NAMES: &[&str] = &[
    "menu",
    "search",
    "appearance",
    "more",
    "close",
    "chevron",
    "copy",
    "external",
];

/// One icon as an inline `<svg>`, or `None` for a name the theme has no shape
/// for — a content icon, which the configured library resolves instead.
pub fn svg(name: &str) -> Option<String> {
    let path = match name {
        "menu" => "M2 4h12M2 8h12M2 12h12",
        "search" => "M11.5 11.5 14 14M7 12a5 5 0 1 1 0-10 5 5 0 0 1 0 10Z",
        "appearance" => "M8 1.5v13M8 1.5a6.5 6.5 0 0 0 0 13 6.5 6.5 0 0 0 0-13Z",
        "more" => "M3.25 8h.01M8 8h.01M12.75 8h.01",
        "close" => "M4 4l8 8M12 4l-8 8",
        "chevron" => "M4 6l4 4 4-4",
        "copy" => {
            "M5.5 5.5V3.25A1.25 1.25 0 0 1 6.75 2h6A1.25 1.25 0 0 1 14 3.25v6A1.25 1.25 0 0 1 12.75 10.5H10.5M3.25 5.5h6A1.25 1.25 0 0 1 10.5 6.75v6A1.25 1.25 0 0 1 9.25 14h-6A1.25 1.25 0 0 1 2 12.75v-6A1.25 1.25 0 0 1 3.25 5.5Z"
        }
        "external" => {
            "M6.5 3H3.25A1.25 1.25 0 0 0 2 4.25v8.5A1.25 1.25 0 0 0 3.25 14h8.5A1.25 1.25 0 0 0 13 12.75V9.5M9.5 2.5H14V7M7 9l7-7"
        }
        _ => return None,
    };
    Some(format!(
        "<svg class=\"ly-icon\" width=\"16\" height=\"16\" viewBox=\"0 0 16 16\" fill=\"none\" \
         stroke=\"currentColor\" stroke-width=\"1.5\" stroke-linecap=\"round\" \
         stroke-linejoin=\"round\" aria-hidden=\"true\" focusable=\"false\"><path d=\"{path}\"/></svg>"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_named_icon_is_a_square_inline_svg() {
        for name in NAMES {
            let svg = svg(name).unwrap_or_else(|| panic!("`{name}` has a shape"));
            assert!(svg.starts_with("<svg class=\"ly-icon\""));
            assert!(svg.contains("viewBox=\"0 0 16 16\""));
            assert!(
                svg.contains("stroke=\"currentColor\""),
                "`{name}` is not themeable"
            );
            assert!(
                svg.contains("aria-hidden=\"true\""),
                "`{name}` is decorative"
            );
            assert!(svg.ends_with("</svg>"));
        }
    }

    #[test]
    fn an_unknown_name_is_not_an_icon() {
        assert!(svg("rocket").is_none());
        assert!(svg("").is_none());
    }

    #[test]
    fn nothing_an_icon_draws_leaves_the_origin() {
        for name in NAMES {
            let svg = svg(name).unwrap_or_default();
            assert!(crate::runtime::external_requests(&[&svg]).is_empty());
        }
    }
}
