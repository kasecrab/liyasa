//! Authoring checks over the rendered AST (RX-92).
//!
//! These are the three the specification names: an image with no alternative
//! text, a heading level that skips, and link text that says nothing. They run
//! over the Rendered AST rather than the source, so a heading emitted by a
//! snippet or a loop is checked the same way an authored one is.

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, Severity, code};
use liyasa_core::document::{Block, BlockKind, Inline, Node};

/// Link text that names no destination (W0405). RX-92 names the first two; the
/// rest are the same failure with different words.
pub const NON_DESCRIPTIVE: &[&str] = &[
    "here",
    "click here",
    "this link",
    "link",
    "read more",
    "learn more",
    "more",
    "this page",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Checks {
    /// RX-92: an image without alt text is a build error, configurably a
    /// warning for a site migrating a large corpus.
    pub missing_alt: Severity,
    pub heading_skip: bool,
    pub link_text: bool,
}

impl Default for Checks {
    fn default() -> Self {
        Self {
            missing_alt: Severity::Error,
            heading_skip: true,
            link_text: true,
        }
    }
}

/// Every authoring problem in one page.
pub fn check(root: &Block, checks: &Checks) -> Diagnostics {
    let mut diagnostics = Diagnostics::new();
    let mut previous_heading = 0u8;
    walk(root, checks, &mut previous_heading, &mut diagnostics);
    diagnostics
}

fn walk(block: &Block, checks: &Checks, previous: &mut u8, diagnostics: &mut Diagnostics) {
    if let BlockKind::Heading { level, .. } = block.kind
        && checks.heading_skip
    {
        // The first heading sets the level; after that a jump of more than one
        // leaves a gap in the outline.
        if *previous != 0 && level > *previous + 1 {
            diagnostics.push(located(
                block,
                Diagnostic::new(
                    code::W0306,
                    format!("heading level jumps from H{previous} to H{level}"),
                )
                .help(format!(
                    "use an H{} here, or restructure the section",
                    *previous + 1
                )),
            ));
        }
        *previous = level;
    }

    for node in &block.children {
        match node {
            Node::Block(child) => walk(child, checks, previous, diagnostics),
            Node::Inline(inline) => inline_checks(inline, block, checks, diagnostics),
        }
    }
}

fn inline_checks(inline: &Inline, block: &Block, checks: &Checks, diagnostics: &mut Diagnostics) {
    match inline {
        Inline::Image { alt, src, .. } if alt.trim().is_empty() => {
            diagnostics.push(located(
                block,
                Diagnostic::new(code::E0305, format!("`{src}` has no alt text"))
                    .with_severity(checks.missing_alt)
                    .help("describe the image, or use `alt=\"\"` on a decorative one once `content.decorativeImages` is set"),
            ));
        }
        Inline::Link { children, href, .. } if checks.link_text => {
            let text = crate::nav::text_of(
                &children
                    .iter()
                    .cloned()
                    .map(Node::Inline)
                    .collect::<Vec<_>>(),
            );
            let normalized = text
                .trim()
                .trim_end_matches(['.', '!', '›', '→'])
                .trim()
                .to_lowercase();
            if NON_DESCRIPTIVE.contains(&normalized.as_str()) {
                diagnostics.push(located(
                    block,
                    Diagnostic::new(
                        code::W0405,
                        format!("link text `{text}` does not say where it goes"),
                    )
                    .help(format!(
                        "name the destination, for example the title of `{href}`"
                    )),
                ));
            }
            for child in children {
                inline_checks(child, block, checks, diagnostics);
            }
        }
        Inline::Emph(children) | Inline::Strong(children) | Inline::Strike(children) => {
            for child in children {
                inline_checks(child, block, checks, diagnostics);
            }
        }
        Inline::Link { children, .. } | Inline::InlineComponent { children, .. } => {
            for child in children {
                inline_checks(child, block, checks, diagnostics);
            }
        }
        _ => {}
    }
}

fn located(block: &Block, diagnostic: Diagnostic) -> Diagnostic {
    match block.origin.span {
        Some(span) => diagnostic.at(span),
        None => diagnostic,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liyasa_core::document::Origin;
    use liyasa_core::ids::BlockId;

    fn block(kind: BlockKind, children: Vec<Node>) -> Block {
        Block {
            id: BlockId::explicit("x"),
            explicit_id: None,
            kind,
            origin: Origin::default(),
            children,
        }
    }

    fn document(children: Vec<Node>) -> Block {
        block(BlockKind::Document, children)
    }

    fn heading(level: u8) -> Node {
        Node::Block(block(
            BlockKind::Heading {
                level,
                anchor: format!("h{level}"),
            },
            vec![Node::Inline(Inline::Text(format!("H{level}")))],
        ))
    }

    fn paragraph(inline: Inline) -> Node {
        Node::Block(block(BlockKind::Paragraph, vec![Node::Inline(inline)]))
    }

    fn codes(diagnostics: &Diagnostics) -> Vec<&str> {
        diagnostics.iter().map(|d| d.code.as_str()).collect()
    }

    #[test]
    fn an_image_without_alt_text_is_an_error() {
        let root = document(vec![paragraph(Inline::Image {
            src: "/diagram.png".to_owned(),
            alt: String::new(),
            title: None,
            dark: None,
        })]);
        let diagnostics = check(&root, &Checks::default());
        assert_eq!(codes(&diagnostics), vec!["E0305"]);
        assert!(diagnostics.has_errors());
        assert!(diagnostics.as_slice()[0].message.contains("/diagram.png"));
    }

    #[test]
    fn the_alt_check_is_configurable() {
        let root = document(vec![paragraph(Inline::Image {
            src: "/a.png".to_owned(),
            alt: "   ".to_owned(),
            title: None,
            dark: None,
        })]);
        let checks = Checks {
            missing_alt: Severity::Warning,
            ..Checks::default()
        };
        let diagnostics = check(&root, &checks);
        assert_eq!(codes(&diagnostics), vec!["E0305"]);
        assert!(!diagnostics.has_errors(), "demoted to a warning");
    }

    #[test]
    fn an_image_with_alt_text_passes() {
        let root = document(vec![paragraph(Inline::Image {
            src: "/a.png".to_owned(),
            alt: "The build pipeline".to_owned(),
            title: None,
            dark: None,
        })]);
        assert!(check(&root, &Checks::default()).is_empty());
    }

    #[test]
    fn a_skipped_heading_level_is_a_warning() {
        let root = document(vec![heading(2), heading(4)]);
        let diagnostics = check(&root, &Checks::default());
        assert_eq!(codes(&diagnostics), vec!["W0306"]);
        assert!(diagnostics.as_slice()[0].message.contains("H2 to H4"));
    }

    #[test]
    fn descending_and_repeating_levels_are_fine() {
        let root = document(vec![heading(2), heading(3), heading(3), heading(2)]);
        assert!(check(&root, &Checks::default()).is_empty());
    }

    #[test]
    fn the_first_heading_sets_the_level_whatever_it_is() {
        let root = document(vec![heading(3), heading(4)]);
        assert!(check(&root, &Checks::default()).is_empty());
    }

    #[test]
    fn non_descriptive_link_text_is_a_warning() {
        for text in ["here", "Click here", "read more.", "  LINK  "] {
            let root = document(vec![paragraph(Inline::Link {
                href: "/guide".to_owned(),
                title: None,
                children: vec![Inline::Text(text.to_owned())],
                resolved: None,
            })]);
            let diagnostics = check(&root, &Checks::default());
            assert_eq!(codes(&diagnostics), vec!["W0405"], "`{text}` should warn");
        }
    }

    #[test]
    fn descriptive_link_text_passes() {
        let root = document(vec![paragraph(Inline::Link {
            href: "/guide".to_owned(),
            title: None,
            children: vec![Inline::Text("the installation guide".to_owned())],
            resolved: None,
        })]);
        assert!(check(&root, &Checks::default()).is_empty());
    }

    #[test]
    fn a_check_that_is_off_reports_nothing() {
        let root = document(vec![
            heading(2),
            heading(5),
            paragraph(Inline::Link {
                href: "/x".to_owned(),
                title: None,
                children: vec![Inline::Text("here".to_owned())],
                resolved: None,
            }),
        ]);
        let checks = Checks {
            heading_skip: false,
            link_text: false,
            ..Checks::default()
        };
        assert!(check(&root, &checks).is_empty());
    }

    #[test]
    fn a_page_is_checked_through_nested_blocks() {
        let root = document(vec![
            heading(2),
            Node::Block(block(
                BlockKind::BlockQuote,
                vec![
                    heading(4),
                    paragraph(Inline::Strong(vec![Inline::Link {
                        href: "/x".to_owned(),
                        title: None,
                        children: vec![Inline::Text("here".to_owned())],
                        resolved: None,
                    }])),
                ],
            )),
        ]);
        let diagnostics = check(&root, &Checks::default());
        assert_eq!(codes(&diagnostics), vec!["W0306", "W0405"]);
    }
}
