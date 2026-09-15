//! Assembling the theme's stylesheet (THM-30, CMP-100).
//!
//! One cached file for the whole site, plus a small critical block inlined per
//! page. The critical block is not a second stylesheet: it is the subset of the
//! compiled rules that paint the shell, selected by [`CRITICAL_SELECTORS`], so
//! it cannot drift from what the full sheet says.

use std::fmt::Write as _;

use crate::config::{Decoration, ThemeConfig};
use crate::css::{Block, Rule, Stylesheet};
use crate::tokens::Tokens;

/// THM-30: the served stylesheet, compressed.
pub const STYLESHEET_BUDGET: usize = 60 * 1024;
/// THM-30: the inlined critical block, uncompressed.
pub const CRITICAL_BUDGET: usize = 8 * 1024;

const BASE: &str = include_str!("../assets/css/base.css");
const SHELL: &str = include_str!("../assets/css/shell.css");
const COMPONENTS: &str = include_str!("../assets/css/components.css");
const OVERLAYS: &str = include_str!("../assets/css/overlays.css");
const PRINT: &str = include_str!("../assets/css/print.css");

/// What paints before the reader scrolls: the page skeleton, the navbar, and
/// the first screen of the sidebar and content column.
/// A trailing `*` matches a prefix; every other entry matches the selector
/// part exactly.
pub const CRITICAL_SELECTORS: &[&str] = &[
    "html",
    "body",
    "*",
    "*::before",
    "*::after",
    ".ly-page",
    ".ly-banner",
    ".ly-navbar",
    ".ly-navbar-logo",
    ".ly-navbar-links",
    ".ly-navbar-actions",
    ".ly-shell",
    ".ly-main",
    ".ly-sidebar",
    ".ly-sidebar-inner",
    ".ly-rail",
    ".ly-rail-inner",
    ".ly-skip-link",
    ".ly-visually-hidden",
    ".ly-mode-*",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Styles {
    /// The whole theme, minified: served once and cached.
    pub css: String,
    /// Inlined in `<head>`; counts against the 100 KB HTML budget (RX-12).
    pub critical: String,
}

impl Styles {
    /// Assembles and compiles the theme.
    ///
    /// `custom` carries the contents of `theme.css` (CFG-10) in order; they are
    /// appended after the theme so an operator's selector of equal specificity
    /// wins, which is what CMP-100 promises, and they are compiled through the
    /// same pipeline so an operator may use nesting too.
    pub fn build(config: &ThemeConfig, tokens: &Tokens, custom: &[&str]) -> Self {
        let mut source = String::with_capacity(64 * 1024);
        source.push_str(&tokens.to_css());
        for part in [BASE, SHELL, COMPONENTS, OVERLAYS] {
            source.push_str(part);
            source.push('\n');
        }
        source.push_str(&utilities(tokens));
        source.push_str(&background(config));
        source.push_str(PRINT);
        for part in custom {
            source.push('\n');
            source.push_str(part);
        }

        let mut sheet = Stylesheet::parse(&source);
        sheet.expand_custom_media();
        let flat = Stylesheet {
            rules: sheet.flatten(),
        };
        Self {
            critical: format!("{}{}", tokens.critical_css(), critical(&flat)),
            css: flat.minify(),
        }
    }

    /// Prepends the `@font-face` block for the faces the build has files for
    /// (THM-03). Faces are not part of `build` because whether a file exists is
    /// the build's knowledge, not the theme's.
    #[must_use]
    pub fn with_fonts(mut self, faces: &[crate::fonts::FaceFile], base_path: &str) -> Self {
        let block = crate::fonts::css(faces, base_path);
        if block.is_empty() {
            return self;
        }
        self.css.insert_str(0, &block);
        self
    }

    pub fn over_budget(&self, compressed: usize) -> Vec<String> {
        let mut out = Vec::new();
        if compressed > STYLESHEET_BUDGET {
            out.push(format!(
                "the stylesheet is {compressed} bytes compressed, over the {STYLESHEET_BUDGET} byte budget"
            ));
        }
        if self.critical.len() > CRITICAL_BUDGET {
            out.push(format!(
                "the critical block is {} bytes, over the {CRITICAL_BUDGET} byte budget",
                self.critical.len()
            ));
        }
        out
    }
}

/// `theme.appearance.background` (CFG-08): a colour, an image, or one of the
/// four decorations. A decoration is drawn from tokens, so it follows the
/// scheme without a second asset, and it sits behind the content rather than
/// over it.
fn background(config: &ThemeConfig) -> String {
    let background = &config.appearance.background;
    let mut declarations = String::new();
    if let Some(color) = &background.color {
        let _ = write!(declarations, "background-color:{color};");
    }
    if let Some(image) = &background.image {
        // Self-hosted: the build copies the file and rewrites the path (THM-32).
        let _ = write!(
            declarations,
            "background-image:url(\"{image}\");background-size:cover;background-attachment:fixed;"
        );
    } else {
        let decoration = match config.appearance.background.decoration {
            Decoration::None => "",
            Decoration::Grid => concat!(
                "background-image:",
                "linear-gradient(to right, var(--ly-color-border-subtle) 1px, transparent 1px),",
                "linear-gradient(to bottom, var(--ly-color-border-subtle) 1px, transparent 1px);",
                "background-size:var(--ly-space-7) var(--ly-space-7);",
                "background-position:center top;"
            ),
            Decoration::Gradient => concat!(
                "background-image:",
                "radial-gradient(60rem 30rem at 50% -8rem, var(--ly-color-primary-subtle), transparent 70%);",
                "background-repeat:no-repeat;"
            ),
            Decoration::Windows => concat!(
                "background-image:",
                "radial-gradient(28rem 18rem at 12% -4rem, var(--ly-color-primary-subtle), transparent 70%),",
                "radial-gradient(24rem 16rem at 88% -2rem, var(--ly-color-accent-subtle), transparent 70%);",
                "background-repeat:no-repeat;"
            ),
        };
        declarations.push_str(decoration);
    }
    if declarations.is_empty() {
        return String::new();
    }
    format!("\nbody {{ {declarations} }}\n")
}

fn critical(sheet: &Stylesheet) -> String {
    let mut out = String::new();
    for rule in &sheet.rules {
        if let Some(kept) = keep(rule) {
            let filtered = Stylesheet { rules: vec![kept] };
            out.push_str(&filtered.minify());
        }
    }
    out
}

fn keep(rule: &Rule) -> Option<Rule> {
    let block = rule.block.as_ref()?;
    if rule.prelude.starts_with('@') {
        // Print and dark-scheme rules are not above the fold; the media queries
        // that lay the shell out are.
        if rule.prelude.contains("print") {
            return None;
        }
        let rules: Vec<Rule> = block.rules.iter().filter_map(keep).collect();
        let declarations = block.declarations.clone();
        if rules.is_empty() && declarations.is_empty() {
            return None;
        }
        return Some(Rule {
            prelude: rule.prelude.clone(),
            block: Some(Block {
                declarations,
                rules,
            }),
        });
    }
    is_critical(&rule.prelude).then(|| rule.clone())
}

fn is_critical(selector: &str) -> bool {
    selector.split(',').any(|part| {
        let part = part.trim();
        CRITICAL_SELECTORS
            .iter()
            .any(|pattern| match pattern.strip_suffix('*') {
                // `*` alone is the universal selector, not a prefix.
                Some(prefix) if !prefix.is_empty() => part.starts_with(prefix),
                _ => part == *pattern,
            })
    })
}

/// The utility classes CMP-100 offers on directives through `.class`, generated
/// from the spacing scale so the two cannot disagree.
fn utilities(tokens: &Tokens) -> String {
    let mut out = String::from("\n/* utilities */\n");
    for name in tokens.names() {
        // One set per step of the live spacing scale, whatever the scale is:
        // a step added to the token table is a utility without a second edit.
        let Some(step) = name.strip_prefix("--ly-space-") else {
            continue;
        };
        let space = format!("var({name})");
        let _ = writeln!(
            out,
            ".ly-m-{step}{{margin:{space}}}.ly-mt-{step}{{margin-top:{space}}}\
             .ly-mb-{step}{{margin-bottom:{space}}}.ly-p-{step}{{padding:{space}}}\
             .ly-gap-{step}{{gap:{space}}}"
        );
    }
    out.push_str(
        ".ly-text-left{text-align:left}.ly-text-center{text-align:center}\
         .ly-text-right{text-align:right}\n\
         .ly-muted{color:var(--ly-color-text-muted)}\
         .ly-subtle{color:var(--ly-color-text-subtle)}\
         .ly-accent{color:var(--ly-color-accent-text)}\n\
         .ly-flex{display:flex}.ly-grid{display:grid}.ly-inline{display:inline-flex}\
         .ly-wrap{flex-wrap:wrap}.ly-items-center{align-items:center}\
         .ly-justify-between{justify-content:space-between}\n\
         .ly-cols-2{grid-template-columns:repeat(2,minmax(0,1fr))}\
         .ly-cols-3{grid-template-columns:repeat(3,minmax(0,1fr))}\
         .ly-cols-4{grid-template-columns:repeat(4,minmax(0,1fr))}\n\
         .ly-rounded{border-radius:var(--ly-radius-md)}\
         .ly-bordered{border:var(--ly-border)}\
         .ly-surface{background:var(--ly-color-surface)}\n\
         .ly-full{width:100%}.ly-truncate{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build() -> Styles {
        Styles::build(&ThemeConfig::default(), &Tokens::aurora(), &[])
    }

    #[test]
    fn the_sheet_carries_the_tokens_and_every_layer() {
        let styles = build();
        assert!(styles.css.contains("--ly-color-primary:"));
        assert!(styles.css.contains(".ly-sidebar-link"));
        assert!(styles.css.contains(".ly-callout"));
        assert!(styles.css.contains(".ly-dialog"));
        assert!(styles.css.contains("@media print"));
        assert!(
            styles
                .css
                .contains(".ly-mt-4{margin-top:var(--ly-space-4)}")
        );
    }

    #[test]
    fn custom_stylesheets_come_after_the_theme() {
        let styles = Styles::build(
            &ThemeConfig::default(),
            &Tokens::aurora(),
            &[".ly-navbar { background: rebeccapurple; }"],
        );
        let theme = styles
            .css
            .find(".ly-navbar{")
            .expect("the theme styles the navbar");
        let custom = styles
            .css
            .rfind("background:rebeccapurple")
            .expect("the custom sheet is present");
        assert!(custom > theme, "a custom rule must win by cascade order");
    }

    #[test]
    fn custom_media_never_reaches_the_output() {
        let styles = build();
        assert!(!styles.css.contains("@custom-media"));
        assert!(!styles.css.contains("(--md)"));
        assert!(styles.css.contains("@media (min-width:48em)"));
    }

    #[test]
    fn the_critical_block_is_the_shell_and_nothing_else() {
        let styles = build();
        assert!(styles.critical.contains(".ly-shell"));
        assert!(styles.critical.contains(":root{"));
        assert!(
            !styles.critical.contains(".ly-callout"),
            "components are not above the fold"
        );
        assert!(!styles.critical.contains("@media print"));
        assert!(
            styles.critical.len() <= CRITICAL_BUDGET,
            "the critical block is {} bytes",
            styles.critical.len()
        );
    }

    #[test]
    fn a_configured_background_reaches_the_page() {
        use crate::config::{Appearance, AppearanceBackground};

        let plain = build();
        assert!(!plain.css.contains("background-image:radial-gradient"));

        let decorated = Styles::build(
            &ThemeConfig {
                appearance: Appearance {
                    background: AppearanceBackground {
                        decoration: Decoration::Windows,
                        color: Some("#fafafa".to_owned()),
                        ..AppearanceBackground::default()
                    },
                    ..Appearance::default()
                },
                ..ThemeConfig::default()
            },
            &Tokens::aurora(),
            &[],
        );
        assert!(decorated.css.contains("background-color:#fafafa"));
        assert!(decorated.css.contains("var(--ly-color-accent-subtle)"));

        let with_image = Styles::build(
            &ThemeConfig {
                appearance: Appearance {
                    background: AppearanceBackground {
                        image: Some("/hero.avif".to_owned()),
                        decoration: Decoration::Grid,
                        ..AppearanceBackground::default()
                    },
                    ..Appearance::default()
                },
                ..ThemeConfig::default()
            },
            &Tokens::aurora(),
            &[],
        );
        assert!(with_image.css.contains("url(\"/hero.avif\")"));
        assert!(
            !with_image.css.contains("linear-gradient(to right"),
            "an image replaces the decoration rather than layering under it"
        );
    }

    #[test]
    fn font_faces_are_prepended_when_the_build_has_the_files() {
        let styles = build().with_fonts(&crate::fonts::bundled(), "");
        assert!(styles.css.starts_with("@font-face{"));
        assert!(styles.css.contains("inter-variable.woff2"));
        assert_eq!(
            build().with_fonts(&[], "").css,
            build().css,
            "no files means no font-face block"
        );
    }

    #[test]
    fn nesting_is_resolved_before_the_sheet_is_served() {
        let styles = build();
        assert!(!styles.css.contains('&'));
    }
}
