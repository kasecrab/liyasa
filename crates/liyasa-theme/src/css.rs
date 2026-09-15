//! The stylesheet compiler (THM-30): nesting, custom media, minification, and
//! the rule selection the critical block is built from.
//!
//! lightningcss does the parsing, lowering, and printing, as §6.2.1 requires;
//! what lives here is the theme's use of it — the browsers the output is
//! pinned to, the feature set the theme authors against, the scheme-scope rule
//! for token overrides, and the filter that builds the critical block.

use lightningcss::printer::PrinterOptions;
use lightningcss::properties::Property;
use lightningcss::properties::custom::CustomPropertyName;
use lightningcss::rules::{CssRule, CssRuleList};
use lightningcss::stylesheet::{MinifyOptions, ParserFlags, ParserOptions, StyleSheet};
use lightningcss::targets::{Browsers, Features, Targets};
use lightningcss::traits::ToCss;
use liyasa_core::diagnostics::{Diagnostic, code};

/// `major << 16 | minor << 8 | patch`, the encoding lightningcss uses.
const fn version(major: u32, minor: u32) -> Option<u32> {
    Some((major & 0xff) << 16 | (minor & 0xff) << 8)
}

/// What the theme's output has to run in.
///
/// NFR-40 asks for the last two versions of each browser; this floor is years
/// below that on purpose, because one cached stylesheet serves every reader
/// and a docs site outlives a release cycle. Naming it also pins the output:
/// without a browser list lightningcss prints whatever syntax is newest, and
/// the emitted CSS would drift with the crate rather than with this list.
fn browsers() -> Browsers {
    Browsers {
        chrome: version(111, 0),
        edge: version(111, 0),
        firefox: version(113, 0),
        safari: version(16, 4),
        ios_saf: version(16, 4),
        opera: version(97, 0),
        samsung: version(22, 0),
        android: version(111, 0),
        ie: None,
    }
}

/// Nesting and custom media are lowered whatever the targets say: the theme
/// authors in them, and neither is worth a browser check on every build.
fn targets() -> Targets {
    Targets {
        browsers: Some(browsers()),
        include: Features::Nesting | Features::CustomMediaQueries,
        exclude: Features::empty(),
    }
}

fn parser_options<'i>() -> ParserOptions<'i> {
    ParserOptions {
        flags: ParserFlags::NESTING | ParserFlags::CUSTOM_MEDIA,
        ..ParserOptions::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CssError {
    #[error("stylesheet does not parse: {0}")]
    Parse(String),
    #[error("stylesheet cannot be compiled: {0}")]
    Minify(String),
    #[error("stylesheet cannot be printed: {0}")]
    Print(String),
}

impl CssError {
    /// An operator's `theme.css` reaches this path, so the failure is a
    /// diagnostic rather than a panic.
    pub fn diagnostic(&self) -> Diagnostic {
        Diagnostic::new(code::E0703, self.to_string()).help(
            "the theme compiles CSS with lightningcss; nesting and `@custom-media` are supported",
        )
    }
}

/// Parses, lowers, and minifies one stylesheet.
pub fn compile(source: &str) -> Result<String, CssError> {
    let mut sheet = StyleSheet::parse(source, parser_options())
        .map_err(|error| CssError::Parse(error.to_string()))?;
    sheet
        .minify(MinifyOptions {
            targets: targets(),
            ..MinifyOptions::default()
        })
        .map_err(|error| CssError::Minify(error.to_string()))?;
    print(&sheet)
}

fn print(sheet: &StyleSheet<'_>) -> Result<String, CssError> {
    sheet
        .to_css(PrinterOptions {
            minify: true,
            targets: targets(),
            ..PrinterOptions::default()
        })
        .map(|result| result.code)
        .map_err(|error| CssError::Print(error.to_string()))
}

/// Which scheme a declaration in an override file applies to. One written
/// outside a scheme block applies to both (THM-12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Both,
    Light,
    Dark,
}

/// Every `--*` declaration in a stylesheet, with the scope it was written in.
/// Used to merge `theme/tokens.css` into the generated tokens (THM-10).
pub fn custom_properties(source: &str) -> Result<Vec<(Scope, String, String)>, CssError> {
    let sheet = StyleSheet::parse(source, parser_options())
        .map_err(|error| CssError::Parse(error.to_string()))?;
    let mut out = Vec::new();
    collect(&sheet.rules, Scope::Both, &mut out)?;
    Ok(out)
}

fn collect(
    rules: &CssRuleList<'_>,
    scope: Scope,
    out: &mut Vec<(Scope, String, String)>,
) -> Result<(), CssError> {
    for rule in &rules.0 {
        match rule {
            CssRule::Style(style) => {
                let scope = scope_of(&text(&style.selectors)?, scope);
                for property in &style.declarations.declarations {
                    if let Property::Custom(custom) = property
                        && let CustomPropertyName::Custom(name) = &custom.name
                    {
                        let value = property
                            .value_to_css_string(PrinterOptions::default())
                            .map_err(|error| CssError::Print(error.to_string()))?;
                        out.push((scope, name.0.to_string(), value));
                    }
                }
                collect(&style.rules, scope, out)?;
            }
            CssRule::Media(media) => {
                collect(&media.rules, scope_of(&text(&media.query)?, scope), out)?;
            }
            CssRule::Supports(supports) => collect(&supports.rules, scope, out)?,
            _ => {}
        }
    }
    Ok(())
}

fn text<T: ToCss>(value: &T) -> Result<String, CssError> {
    value
        .to_css_string(PrinterOptions::default())
        .map_err(|error| CssError::Print(error.to_string()))
}

/// A dark scope is the attribute the toggle sets or the query the system
/// preference matches; everything else keeps the scope it inherited.
fn scope_of(prelude: &str, inherited: Scope) -> Scope {
    let lower = prelude.to_ascii_lowercase().replace(['"', '\''], "");
    if lower.contains("prefers-color-scheme: dark")
        || lower.contains("prefers-color-scheme:dark")
        || lower.contains("data-theme=dark")
    {
        return Scope::Dark;
    }
    if lower.contains("prefers-color-scheme: light")
        || lower.contains("prefers-color-scheme:light")
        || lower.contains("data-theme=light")
    {
        return Scope::Light;
    }
    inherited
}

/// The subset of a compiled stylesheet whose selectors `keep` accepts, with the
/// at-rules that wrap them (THM-30's critical block).
///
/// A rule inside `@media print` is never kept: nothing printed is above the
/// fold.
pub fn filter(source: &str, keep: &dyn Fn(&str) -> bool) -> Result<String, CssError> {
    let sheet = StyleSheet::parse(source, parser_options())
        .map_err(|error| CssError::Parse(error.to_string()))?;
    let rules = retain(&sheet.rules, keep)?;
    let filtered = StyleSheet::new(Vec::new(), CssRuleList(rules), parser_options());
    print(&filtered)
}

fn retain<'i>(
    rules: &CssRuleList<'i>,
    keep: &dyn Fn(&str) -> bool,
) -> Result<Vec<CssRule<'i>>, CssError> {
    let mut out = Vec::new();
    for rule in &rules.0 {
        match rule {
            CssRule::Style(style) => {
                if keep(&text(&style.selectors)?) {
                    out.push(rule.clone());
                }
            }
            CssRule::Media(media) => {
                if text(&media.query)?.contains("print") {
                    continue;
                }
                let inner = retain(&media.rules, keep)?;
                if inner.is_empty() {
                    continue;
                }
                let mut media = media.clone();
                media.rules = CssRuleList(inner);
                out.push(CssRule::Media(media));
            }
            CssRule::Supports(supports) => {
                let inner = retain(&supports.rules, keep)?;
                if inner.is_empty() {
                    continue;
                }
                let mut supports = supports.clone();
                supports.rules = CssRuleList(inner);
                out.push(CssRule::Supports(supports));
            }
            _ => {}
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nesting_is_lowered_against_the_parent_selector() {
        let css = compile(".card{color:red;&:hover{color:blue}.title{font-weight:600}}")
            .expect("nesting compiles");
        assert!(!css.contains('&'), "{css}");
        assert!(css.contains(".card:hover"));
        assert!(css.contains(".card .title"));
    }

    #[test]
    fn custom_media_is_expanded_and_its_definition_dropped() {
        let css =
            compile("@custom-media --tablet (min-width: 48em);@media (--tablet){.a{color:red}}")
                .expect("custom media compiles");
        assert!(!css.contains("custom-media"), "{css}");
        assert_eq!(css, "@media (width>=48em){.a{color:red}}");
    }

    #[test]
    fn output_is_minified_and_deterministic() {
        let source = ".a::after { content: \" (\" attr(href) \")\"; color: #ffffff; }";
        let css = compile(source).expect("compiles");
        assert!(!css.contains('\n'));
        assert!(css.contains("attr(href)"));
        assert_eq!(css, compile(source).expect("compiles again"));
    }

    #[test]
    fn a_stylesheet_that_does_not_parse_is_a_diagnostic() {
        let error = compile("} nonsense {").expect_err("a stray block is rejected");
        assert_eq!(error.diagnostic().code.as_str(), "E0703");
        assert!(error.diagnostic().message.contains("does not parse"));
    }

    #[test]
    fn the_output_is_pinned_to_the_declared_browsers() {
        // Range syntax: every browser in `browsers()` has had it since 2023,
        // and it is shorter. The point of the assertion is that the output
        // follows that list rather than whatever lightningcss prefers today.
        let css = compile("@media (min-width: 64em){.a{gap:1rem}}").expect("compiles");
        assert_eq!(css, "@media (width>=64em){.a{gap:1rem}}");
    }

    #[test]
    fn custom_properties_carry_the_scheme_they_were_written_in() {
        let properties = custom_properties(
            r#"
            :root { --ly-color-primary: #111111; }
            [data-theme="dark"] { --ly-color-primary: #eeeeee; }
            @media (prefers-color-scheme: dark) { :root { --ly-color-bg: #000000; } }
            .card { color: red; }
            "#,
        )
        .expect("the override parses");
        assert_eq!(properties.len(), 3);
        assert_eq!(properties[0].0, Scope::Both);
        assert_eq!(properties[0].1, "--ly-color-primary");
        assert_eq!(properties[1].0, Scope::Dark);
        assert_eq!(properties[2].0, Scope::Dark);
        assert_eq!(properties[2].1, "--ly-color-bg");
        assert_eq!(
            properties[2].2, "#000",
            "values are minified with the sheet"
        );
    }

    #[test]
    fn a_nested_override_keeps_its_scope() {
        let properties =
            custom_properties("[data-theme=\"dark\"]{ .brand { --ly-color-primary: #eeeeee; } }")
                .expect("the override parses");
        assert_eq!(properties.len(), 1);
        assert_eq!(properties[0].0, Scope::Dark);
    }

    #[test]
    fn filtering_keeps_the_at_rules_that_wrap_a_kept_rule() {
        let source = ".ly-shell{display:grid}.ly-callout{color:red}\
                      @media (width>=64em){.ly-shell{gap:1rem}.ly-callout{gap:0}}\
                      @media print{.ly-shell{display:block}}";
        let css =
            filter(source, &|selector| selector.starts_with(".ly-shell")).expect("the filter runs");
        assert!(css.contains(".ly-shell{display:grid}"));
        assert!(!css.contains(".ly-callout"));
        assert!(
            css.contains("@media (width>=64em){.ly-shell{gap:1rem}}"),
            "{css}"
        );
        assert!(!css.contains("print"), "nothing printed is above the fold");
    }

    #[test]
    fn filtering_drops_an_at_rule_whose_body_is_all_dropped() {
        let css = filter("@media (min-width:64em){.ly-callout{gap:0}}", &|selector| {
            selector.starts_with(".ly-shell")
        })
        .expect("the filter runs");
        assert!(css.is_empty(), "{css}");
    }
}
