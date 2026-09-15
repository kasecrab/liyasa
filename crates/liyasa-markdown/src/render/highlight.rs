//! Syntax highlighting at build time (CM-38, PRD §6.2.1).
//!
//! syntect parses with Sublime-syntax definitions and two-face bundles bat's
//! curated set of them, licence-audited upstream, so nothing here decides what
//! a language looks like. What this module decides is that the output carries
//! classes rather than inline styles: the colours then live in one stylesheet
//! the theme can swap between light and dark, the page needs no `style`
//! attribute for the sanitizer to argue with, and the CSP needs no exception.
//!
//! No JavaScript reaches the reader. A language nobody bundles is left plain,
//! which is not an error — a fence is legible without colour.

use liyasa_core::document::{Block, BlockKind, Document, Inline, Node};
use syntect::html::{ClassStyle, ClassedHTMLGenerator, css_for_theme_with_class_style};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;
use two_face::theme::{EmbeddedLazyThemeSet, EmbeddedThemeName};

/// Prefixed, so a highlight class cannot collide with a theme's own.
const CLASS_STYLE: ClassStyle = ClassStyle::SpacedPrefixed { prefix: "ly-" };

/// The theme a site gets when it names one nobody bundles.
pub const DEFAULT_THEME: EmbeddedThemeName = EmbeddedThemeName::OneHalfDark;

pub struct Highlighter {
    syntaxes: SyntaxSet,
    themes: EmbeddedLazyThemeSet,
    theme: EmbeddedThemeName,
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new(DEFAULT_THEME.as_name())
    }
}

impl Highlighter {
    /// `theme` is a Shiki name from config, or a bundled `.tmTheme` name.
    pub fn new(theme: &str) -> Self {
        Self {
            // The newline variant, because the generator is fed whole lines.
            syntaxes: two_face::syntax::extra_newlines(),
            themes: two_face::theme::extra(),
            theme: theme_named(theme).unwrap_or(DEFAULT_THEME),
        }
    }

    /// The stylesheet the classes refer to. The theme emits it once per build.
    pub fn stylesheet(&self) -> String {
        css_for_theme_with_class_style(self.themes.get(self.theme), CLASS_STYLE).unwrap_or_default()
    }

    pub fn theme(&self) -> EmbeddedThemeName {
        self.theme
    }

    /// How many languages are bundled. CM-38 asks for at least sixty.
    pub fn languages(&self) -> usize {
        self.syntaxes.syntaxes().len()
    }

    pub fn supports(&self, lang: &str) -> bool {
        self.syntax_for(lang).is_some()
    }

    /// `body` highlighted into class-carrying spans, or `None` when no bundled
    /// grammar claims the language.
    pub fn code(&self, lang: Option<&str>, body: &str) -> Option<String> {
        let syntax = self.syntax_for(lang?)?;
        let mut generator =
            ClassedHTMLGenerator::new_with_class_style(syntax, &self.syntaxes, CLASS_STYLE);
        for line in LinesWithEndings::from(body) {
            // A grammar that fails mid-file leaves the fence plain rather than
            // half-coloured.
            generator
                .parse_html_for_line_which_includes_newline(line)
                .ok()?;
        }
        Some(generator.finalize())
    }

    /// By token, then by the alias table, then by full name, so `rs`, `rust`,
    /// `csharp`, and `Dockerfile` all resolve.
    fn syntax_for(&self, lang: &str) -> Option<&syntect::parsing::SyntaxReference> {
        let lang = lang.trim();
        if lang.is_empty() {
            return None;
        }
        self.syntaxes
            .find_syntax_by_token(lang)
            .or_else(|| self.syntaxes.find_syntax_by_token(alias(lang)?))
            .or_else(|| self.syntaxes.find_syntax_by_name(lang))
    }
}

/// Fills in every code block the highlighter can claim.
pub fn apply(document: &mut Document, highlighter: &Highlighter) {
    block(&mut document.root, highlighter);
}

fn block(block_: &mut Block, highlighter: &Highlighter) {
    if let BlockKind::CodeBlock {
        lang, highlighted, ..
    } = &mut block_.kind
        && highlighted.is_none()
    {
        *highlighted = highlighter.code(lang.as_deref(), &body_of(&block_.children));
    }
    for child in &mut block_.children {
        if let Node::Block(child) = child {
            block(child, highlighter);
        }
    }
}

fn body_of(children: &[Node]) -> String {
    children
        .iter()
        .filter_map(|child| match child {
            Node::Inline(Inline::Text(text)) => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

/// A Shiki language id that bat's set spells differently (§6.2.1).
///
/// Only the ones that really do differ are listed: `csharp` is `c#` here, and
/// `fsharp` is `f#` rather than `fs`, because `fs` is a GLSL fragment shader in
/// this set and would highlight F# as shader source.
fn alias(lang: &str) -> Option<&'static str> {
    Some(match lang.to_ascii_lowercase().as_str() {
        "c-sharp" | "cs" | "csharp" => "c#",
        "console" | "shell" | "shellscript" | "shellsession" => "sh",
        "docker" | "dockerfile" => "Dockerfile",
        "f-sharp" | "fsharp" => "f#",
        "jsx" => "js",
        "objc" | "objective-c" | "objectivec" => "objective-c",
        "vb" | "vbnet" => "vb.net",
        "yml" => "yaml",
        _ => return None,
    })
}

/// A Shiki theme name from config, mapped onto the bundled set (§6.2.1).
///
/// Only the names that have a real equivalent are mapped; a site that names
/// anything else gets [`DEFAULT_THEME`], because a fence in the wrong colours
/// is better than a build that fails over a colour.
pub fn theme_named(name: &str) -> Option<EmbeddedThemeName> {
    use EmbeddedThemeName as T;
    let wanted = name.trim().to_ascii_lowercase();
    let shiki = match wanted.as_str() {
        "catppuccin-frappe" => Some(T::CatppuccinFrappe),
        "catppuccin-latte" => Some(T::CatppuccinLatte),
        "catppuccin-macchiato" => Some(T::CatppuccinMacchiato),
        "catppuccin-mocha" => Some(T::CatppuccinMocha),
        "dracula" | "dracula-soft" => Some(T::Dracula),
        "github-dark" | "github-dark-default" | "github-dark-dimmed" => Some(T::TwoDark),
        "github-light" | "github-light-default" => Some(T::Github),
        "gruvbox-dark-hard" | "gruvbox-dark-medium" | "gruvbox-dark-soft" => Some(T::GruvboxDark),
        "gruvbox-light-hard" | "gruvbox-light-medium" | "gruvbox-light-soft" => {
            Some(T::GruvboxLight)
        }
        "monokai" => Some(T::MonokaiExtended),
        "nord" => Some(T::Nord),
        "one-dark-pro" => Some(T::OneHalfDark),
        "one-light" => Some(T::OneHalfLight),
        "solarized-dark" => Some(T::SolarizedDark),
        "solarized-light" => Some(T::SolarizedLight),
        _ => None,
    };
    shiki.or_else(|| {
        EmbeddedLazyThemeSet::theme_names()
            .iter()
            .copied()
            .find(|bundled| bundled.as_name().eq_ignore_ascii_case(name.trim()))
    })
}

#[cfg(test)]
mod tests;
