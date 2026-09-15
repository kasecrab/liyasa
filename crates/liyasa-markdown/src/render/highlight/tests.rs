//! CM-38: sixty languages highlight, an unknown one falls back to plain, and
//! nothing the reader loads is JavaScript.

use super::*;
use crate::directives::testing::*;

/// The languages CM-38 names, plus the ones a documentation site actually uses.
const LANGUAGES: &[&str] = &[
    "bash",
    "c",
    "clojure",
    "cmake",
    "cpp",
    "crystal",
    "csharp",
    "css",
    "d",
    "dart",
    "diff",
    "docker",
    "elixir",
    "elm",
    "erlang",
    "fsharp",
    "go",
    "graphql",
    "groovy",
    "haskell",
    "html",
    "ini",
    "java",
    "javascript",
    "json",
    "jsx",
    "julia",
    "kotlin",
    "latex",
    "less",
    "lisp",
    "lua",
    "make",
    "markdown",
    "matlab",
    "nix",
    "objc",
    "ocaml",
    "perl",
    "php",
    "protobuf",
    "puppet",
    "python",
    "r",
    "ruby",
    "rust",
    "scala",
    "scss",
    "sh",
    "shell",
    "sql",
    "svelte",
    "swift",
    "terraform",
    "toml",
    "tsx",
    "typescript",
    "vue",
    "xml",
    "yaml",
    "zig",
];

/// Languages bat's set does not carry. CM-38 allows this: a grammar outside the
/// bundled set is loadable from config, and an unclaimed fence renders plain.
const NOT_BUNDLED: &[&str] = &["powershell", "prisma", "astro"];

#[test]
fn at_least_sixty_languages_are_bundled() {
    let highlighter = Highlighter::default();
    assert!(
        highlighter.languages() >= 60,
        "only {} bundled",
        highlighter.languages()
    );
}

#[test]
fn every_language_a_documentation_site_uses_is_bundled() {
    let highlighter = Highlighter::default();
    let missing: Vec<&str> = LANGUAGES
        .iter()
        .copied()
        .filter(|lang| !highlighter.supports(lang))
        .collect();
    assert!(missing.is_empty(), "not bundled: {missing:?}");
    assert!(LANGUAGES.len() >= 60, "the list itself is short");
}

#[test]
fn a_language_bat_does_not_carry_falls_back_rather_than_failing() {
    let highlighter = Highlighter::default();
    for lang in NOT_BUNDLED {
        assert!(!highlighter.supports(lang), "{lang} is bundled after all");
        assert_eq!(highlighter.code(Some(lang), "x\n"), None);
    }
}

/// Shiki spells several languages differently from bat's set.
#[test]
fn shiki_language_ids_map_onto_the_bundled_set() {
    let highlighter = Highlighter::default();
    for lang in [
        "csharp",
        "cs",
        "fsharp",
        "docker",
        "dockerfile",
        "jsx",
        "objc",
        "objective-c",
        "console",
        "shellscript",
        "yml",
    ] {
        assert!(highlighter.supports(lang), "{lang}");
    }
}

/// `fs` is a GLSL fragment shader in this set, so `fsharp` must not take it.
#[test]
fn fsharp_does_not_resolve_to_a_shader() {
    let highlighter = Highlighter::default();
    assert!(
        highlighter
            .code(Some("fsharp"), "let x = 1\n")
            .is_some_and(|html| !html.is_empty())
    );
    assert_eq!(alias("fsharp"), Some("f#"));
}

#[test]
fn a_fence_gets_classed_spans() {
    let highlighter = Highlighter::default();
    let html = highlighter
        .code(Some("rust"), "fn main() {}\n")
        .expect("rust is bundled");
    assert!(html.contains("<span class=\"ly-"), "{html}");
    assert!(html.contains("main"), "{html}");
}

/// The colours live in one stylesheet, so a page carries no inline style and
/// the sanitizer and the CSP have nothing to argue with.
#[test]
fn the_output_carries_classes_and_no_inline_style() {
    let highlighter = Highlighter::default();
    let html = highlighter
        .code(Some("python"), "def f():\n    return 1\n")
        .expect("python is bundled");
    assert!(!html.contains("style="), "{html}");
    assert!(!html.contains("<script"), "{html}");
    assert!(highlighter.stylesheet().contains(".ly-"));
}

/// CM-38: an unknown language falls back to plain with no error.
#[test]
fn an_unknown_language_falls_back_to_plain() {
    let highlighter = Highlighter::default();
    assert_eq!(highlighter.code(Some("nosuchlang"), "x\n"), None);
    assert_eq!(highlighter.code(None, "x\n"), None);
    assert_eq!(highlighter.code(Some(""), "x\n"), None);
    assert_eq!(highlighter.code(Some("   "), "x\n"), None);
}

#[test]
fn a_language_resolves_by_name_or_by_extension() {
    let highlighter = Highlighter::default();
    for lang in [
        "rust",
        "rs",
        "python",
        "py",
        "javascript",
        "js",
        "yaml",
        "yml",
    ] {
        assert!(highlighter.supports(lang), "{lang}");
    }
}

/// §6.2.1: Shiki theme names in config map onto the bundled set.
#[test]
fn shiki_theme_names_map_onto_the_bundled_set() {
    for (shiki, bundled) in [
        ("github-dark", EmbeddedThemeName::TwoDark),
        ("github-light", EmbeddedThemeName::Github),
        ("dracula", EmbeddedThemeName::Dracula),
        ("nord", EmbeddedThemeName::Nord),
        ("one-dark-pro", EmbeddedThemeName::OneHalfDark),
        ("solarized-light", EmbeddedThemeName::SolarizedLight),
        ("catppuccin-mocha", EmbeddedThemeName::CatppuccinMocha),
        ("gruvbox-dark-medium", EmbeddedThemeName::GruvboxDark),
    ] {
        assert_eq!(theme_named(shiki), Some(bundled), "{shiki}");
        assert_eq!(Highlighter::new(shiki).theme(), bundled, "{shiki}");
    }
}

#[test]
fn a_bundled_theme_name_is_accepted_as_written() {
    assert_eq!(theme_named("Dracula"), Some(EmbeddedThemeName::Dracula));
    assert_eq!(theme_named("nord"), Some(EmbeddedThemeName::Nord));
    assert_eq!(
        theme_named("Solarized (dark)"),
        Some(EmbeddedThemeName::SolarizedDark)
    );
}

/// A colour is never worth failing a build over.
#[test]
fn an_unknown_theme_falls_back_to_the_default() {
    assert_eq!(theme_named("no-such-theme"), None);
    assert_eq!(Highlighter::new("no-such-theme").theme(), DEFAULT_THEME);
    assert_eq!(Highlighter::new("").theme(), DEFAULT_THEME);
}

#[test]
fn the_pass_fills_in_every_fence_it_can_claim() {
    let mut parsed = document("```rust\nfn main() {}\n```\n\n```nosuchlang\nx\n```\n");
    apply(&mut parsed, &Highlighter::default());
    let highlighted: Vec<bool> = blocks(&parsed.root)
        .into_iter()
        .filter_map(|b| match &b.kind {
            liyasa_core::document::BlockKind::CodeBlock { highlighted, .. } => {
                Some(highlighted.is_some())
            }
            _ => None,
        })
        .collect();
    assert_eq!(highlighted, [true, false]);
}

#[test]
fn a_fence_inside_a_component_is_highlighted_too() {
    let mut parsed = document(":::note\n```rust\nfn main() {}\n```\n:::\n");
    apply(&mut parsed, &Highlighter::default());
    assert!(blocks(&parsed.root).into_iter().any(|b| matches!(
        &b.kind,
        liyasa_core::document::BlockKind::CodeBlock {
            highlighted: Some(_),
            ..
        }
    )));
}

/// Whatever the body, highlighting returns rather than panicking.
#[test]
fn adversarial_bodies_do_not_panic() {
    let highlighter = Highlighter::default();
    for body in [
        "",
        "\n",
        "\u{0}",
        "🙂",
        &"{".repeat(5_000),
        &"a\n".repeat(20_000),
    ] {
        let _ = highlighter.code(Some("rust"), body);
        let _ = highlighter.code(Some("json"), body);
    }
}
