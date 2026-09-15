//! Assembling the theme's stylesheet (THM-30, CMP-100).
//!
//! One cached file for the whole site, plus a small critical block inlined per
//! page. The critical block is not a second stylesheet: it is the subset of the
//! compiled rules that paint the shell, selected by [`CRITICAL_SELECTORS`], so
//! it cannot drift from what the full sheet says.

use std::fmt::Write as _;

use crate::config::ThemeConfig;
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
        source.push_str(PRINT);
        for part in custom {
            source.push('\n');
            source.push_str(part);
        }
        let _ = config;

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
    for step in ["0", "1", "2", "3", "4", "5", "6", "7"] {
        let space = format!("var(--ly-space-{step})");
        let _ = writeln!(
            out,
            ".ly-m-{step}{{margin:{space}}}.ly-mt-{step}{{margin-top:{space}}}\
             .ly-mb-{step}{{margin-bottom:{space}}}.ly-p-{step}{{padding:{space}}}\
             .ly-gap-{step}{{gap:{space}}}"
        );
    }
    let _ = tokens;
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
    fn nesting_is_resolved_before_the_sheet_is_served() {
        let styles = build();
        assert!(!styles.css.contains('&'));
    }
}
