//! Open Graph thumbnails (CFG-73).
//!
//! The theme draws the card as SVG, deterministically: the same page and the
//! same configuration produce the same bytes (§6.6.2). Rasterizing it to PNG
//! belongs to the build's lazy image tier (§6.6), which owns resvg and the
//! pinned fonts; nothing here loads a font or touches the file system.

use serde::{Deserialize, Serialize};

use crate::color::Color;
use crate::tokens::{Scheme, Tokens};

/// `social.thumbnails` (CFG-73).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThumbnailConfig {
    /// `light` or `dark`; the card follows the theme's own tokens either way.
    pub appearance: Option<String>,
    pub background: Option<String>,
    pub font: Option<String>,
    /// A logo to place in the corner, as an already-resolved data URI or path.
    pub logo: Option<String>,
}

/// What one card says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Card {
    pub title: String,
    pub description: String,
    /// The site name, shown as the eyebrow.
    pub site: String,
}

/// Open Graph's documented size; every consumer crops to it.
pub const WIDTH: u32 = 1200;
pub const HEIGHT: u32 = 630;

/// The card as SVG. Deterministic, self-contained, and free of any request that
/// would leave the origin (THM-32).
pub fn svg(card: &Card, config: &ThumbnailConfig, tokens: &Tokens) -> String {
    let scheme = match config.appearance.as_deref() {
        Some("dark") => Scheme::Dark,
        _ => Scheme::Light,
    };
    let color = |token: &str, fallback: &str| {
        tokens
            .color(token, scheme)
            .map_or_else(|| fallback.to_owned(), |color| color.to_string())
    };
    let background = config
        .background
        .as_deref()
        .and_then(|value| Color::parse(value).ok())
        .map_or_else(
            || color("--ly-color-bg", "#ffffff"),
            |color| color.to_string(),
        );
    let text = color("--ly-color-text", "#111111");
    let muted = color("--ly-color-text-muted", "#555555");
    let primary = color("--ly-color-primary", "#1e34c8");
    let font = config
        .font
        .clone()
        .unwrap_or_else(|| "Inter, system-ui, sans-serif".to_owned());

    let title = wrap(&card.title, 26, 3);
    let title_lines = title
        .iter()
        .enumerate()
        .map(|(line, text)| {
            format!(
                "<tspan x=\"80\" y=\"{}\">{}</tspan>",
                260 + line as u32 * 84,
                escape(text)
            )
        })
        .collect::<String>();
    let description = wrap(&card.description, 62, 2)
        .iter()
        .enumerate()
        .map(|(line, text)| {
            format!(
                "<tspan x=\"80\" y=\"{}\">{}</tspan>",
                260 + title.len() as u32 * 84 + 28 + line as u32 * 44,
                escape(text)
            )
        })
        .collect::<String>();

    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{WIDTH}\" height=\"{HEIGHT}\" \
         viewBox=\"0 0 {WIDTH} {HEIGHT}\" role=\"img\" aria-label=\"{}\">\
         <rect width=\"{WIDTH}\" height=\"{HEIGHT}\" fill=\"{background}\"/>\
         <rect width=\"{WIDTH}\" height=\"12\" fill=\"{primary}\"/>\
         <text x=\"80\" y=\"150\" font-family=\"{font}\" font-size=\"32\" font-weight=\"600\" \
         letter-spacing=\"2\" fill=\"{primary}\">{}</text>\
         <text font-family=\"{font}\" font-size=\"72\" font-weight=\"700\" fill=\"{text}\">{title_lines}</text>\
         <text font-family=\"{font}\" font-size=\"34\" fill=\"{muted}\">{description}</text>\
         </svg>",
        escape(&card.title),
        escape(&card.site.to_uppercase()),
    )
}

/// Greedy wrap to `columns` characters, at most `limit` lines; the last line is
/// elided rather than dropped.
fn wrap(text: &str, columns: usize, limit: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(line) if line.chars().count() + 1 + word.chars().count() <= columns => {
                line.push(' ');
                line.push_str(word);
            }
            _ => {
                if lines.len() == limit {
                    if let Some(last) = lines.last_mut() {
                        truncate(last, columns);
                    }
                    return lines;
                }
                lines.push(word.to_owned());
            }
        }
    }
    lines
}

fn truncate(line: &mut String, columns: usize) {
    while line.chars().count() > columns.saturating_sub(1) {
        line.pop();
    }
    line.push('…');
}

fn escape(text: &str) -> String {
    text.chars()
        .map(|character| match character {
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '>' => "&gt;".to_owned(),
            '"' => "&quot;".to_owned(),
            '\'' => "&#39;".to_owned(),
            other => other.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card() -> Card {
        Card {
            title: "Install Liyasa and build the first site".to_owned(),
            description: "A supported platform and a terminal is everything the build needs."
                .to_owned(),
            site: "Acme docs".to_owned(),
        }
    }

    #[test]
    fn the_card_is_an_open_graph_sized_svg() {
        let svg = svg(&card(), &ThumbnailConfig::default(), &Tokens::aurora());
        assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
        assert!(svg.contains("width=\"1200\" height=\"630\""));
        assert!(svg.contains("Install Liyasa"));
        assert!(svg.contains("ACME DOCS"), "the site name is the eyebrow");
    }

    #[test]
    fn the_same_inputs_draw_the_same_bytes() {
        let tokens = Tokens::aurora();
        assert_eq!(
            svg(&card(), &ThumbnailConfig::default(), &tokens),
            svg(&card(), &ThumbnailConfig::default(), &tokens)
        );
    }

    #[test]
    fn the_card_follows_the_configured_appearance_and_background() {
        let tokens = Tokens::aurora();
        let light = svg(&card(), &ThumbnailConfig::default(), &tokens);
        let dark = svg(
            &card(),
            &ThumbnailConfig {
                appearance: Some("dark".to_owned()),
                ..ThumbnailConfig::default()
            },
            &tokens,
        );
        assert_ne!(light, dark);

        let branded = svg(
            &card(),
            &ThumbnailConfig {
                background: Some("#101010".to_owned()),
                ..ThumbnailConfig::default()
            },
            &tokens,
        );
        assert!(branded.contains("fill=\"#101010\""));
    }

    #[test]
    fn a_long_title_is_wrapped_and_elided_rather_than_clipped() {
        let long = Card {
            title: "A title so long that it could not possibly fit on three lines of a card at \
                    this size, however hard the wrap tries to make it"
                .to_owned(),
            ..card()
        };
        let svg = svg(&long, &ThumbnailConfig::default(), &Tokens::aurora());
        assert_eq!(
            svg.matches("<tspan").count(),
            5,
            "three title, two description"
        );
        assert!(svg.contains('…'));
    }

    #[test]
    fn nothing_in_the_card_can_close_a_tag() {
        let hostile = Card {
            title: "</text><script>alert(1)</script>".to_owned(),
            description: "a & b".to_owned(),
            site: "acme".to_owned(),
        };
        let svg = svg(&hostile, &ThumbnailConfig::default(), &Tokens::aurora());
        assert!(!svg.contains("<script>"));
        assert!(
            svg.contains("&lt;/text&gt;&lt;script&gt;"),
            "the title is escaped where it is drawn"
        );
        assert!(svg.contains("a &amp; b"));
    }

    #[test]
    fn the_card_requests_nothing_from_another_origin() {
        let svg = svg(&card(), &ThumbnailConfig::default(), &Tokens::aurora());
        // The SVG namespace is a name, not a fetch; everything else is inline.
        let found = crate::runtime::external_requests(&[&svg]);
        assert_eq!(found.len(), 1, "only the namespace declaration");
        assert!(found[0].starts_with("http://www.w3.org/2000/svg"));
    }
}
