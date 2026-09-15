//! The `theme.*` slice of `liyasa.json` (CFG-02..CFG-10, §8.2).
//!
//! `liyasa-config` owns validation against `schemas/liyasa.schema.json`; this
//! mirror only reads the keys the theme layer needs, and ignores the rest so a
//! key added to the schema does not break the theme before it is used.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ThemeConfig {
    pub preset: Preset,
    pub colors: Colors,
    pub code_theme: CodeTheme,
    pub fonts: Fonts,
    pub icons: Icons,
    pub appearance: Appearance,
    pub layout: Layout,
    /// Custom stylesheets, appended after the theme stylesheet (CMP-100).
    pub css: Paths,
    /// Custom scripts, deferred after the theme runtime (CMP-101).
    pub js: Paths,
    /// Directory of partial overrides; `theme/partials` by default.
    pub overrides: Option<String>,
}

/// CFG-03. The nine presets are the enum in the schema; a community package
/// installs under its own name and is resolved as a path, not here.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Preset {
    #[default]
    Aurora,
    Atlas,
    Meadow,
    Slate,
    Ember,
    Harbor,
    Quill,
    Signal,
    Lumen,
}

impl Preset {
    pub const ALL: [Self; 9] = [
        Self::Aurora,
        Self::Atlas,
        Self::Meadow,
        Self::Slate,
        Self::Ember,
        Self::Harbor,
        Self::Quill,
        Self::Signal,
        Self::Lumen,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Aurora => "aurora",
            Self::Atlas => "atlas",
            Self::Meadow => "meadow",
            Self::Slate => "slate",
            Self::Ember => "ember",
            Self::Harbor => "harbor",
            Self::Quill => "quill",
            Self::Signal => "signal",
            Self::Lumen => "lumen",
        }
    }
}

/// CFG-04. Every field is any CSS colour; an unparseable one is reported
/// rather than silently dropped.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Colors {
    pub primary: Option<String>,
    /// The primary variant for the light scheme.
    pub light: Option<String>,
    /// The primary variant for the dark scheme.
    pub dark: Option<String>,
    pub text: Option<String>,
    pub muted: Option<String>,
    pub border: Option<String>,
    pub accent: Option<String>,
    pub success: Option<String>,
    pub warning: Option<String>,
    pub danger: Option<String>,
    pub background: Background,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Background {
    pub light: Option<String>,
    pub dark: Option<String>,
}

/// CFG-05. A Shiki theme name from the bundled catalogue, or `css-variables`
/// to drive the code colours from the `--ly-code-*` tokens alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CodeTheme {
    pub light: String,
    pub dark: String,
}

impl Default for CodeTheme {
    fn default() -> Self {
        Self {
            light: "github-light".to_owned(),
            dark: "github-dark".to_owned(),
        }
    }
}

impl CodeTheme {
    /// Whether both schemes hand the colours to the tokens (CFG-05).
    pub fn is_css_variables(&self) -> bool {
        self.light == "css-variables" && self.dark == "css-variables"
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Fonts {
    pub heading: Option<Face>,
    pub body: Option<Face>,
    pub mono: Option<Face>,
    /// Accepted and ignored with `W0716` until the subsetter exists (§6.2.1).
    pub subset: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Face {
    pub family: Option<String>,
    pub weight: Option<serde_json::Value>,
    /// A Google Fonts family, downloaded and self-hosted at build time
    /// (THM-32), or a local file path.
    pub source: Option<String>,
    pub format: Option<String>,
}

impl Face {
    /// The CSS font stack this face contributes, falling back to the theme's
    /// own stack so a missing file never leaves the page unstyled.
    pub fn stack(&self, fallback: &str) -> String {
        match &self.family {
            Some(family) if !family.is_empty() => format!("{}, {fallback}", quote_family(family)),
            _ => fallback.to_owned(),
        }
    }
}

fn quote_family(family: &str) -> String {
    let plain = family
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if plain {
        family.to_owned()
    } else {
        format!("\"{}\"", family.replace('"', ""))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Icons {
    pub library: Option<String>,
    pub default_type: Option<String>,
}

/// CFG-08.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub default: Scheme,
    /// Hides the toggle and pins the scheme (RX-40).
    pub strict: bool,
    pub background: AppearanceBackground,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    #[default]
    System,
    Light,
    Dark,
}

impl Scheme {
    pub const fn name(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceBackground {
    pub image: Option<String>,
    pub color: Option<String>,
    pub decoration: Decoration,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Decoration {
    #[default]
    None,
    Grid,
    Gradient,
    Windows,
}

/// CFG-09.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Layout {
    pub sidebar_width: Option<String>,
    pub content_width: Option<String>,
    pub toc_width: Option<String>,
    pub density: Density,
    pub radius: Option<String>,
    pub shadows: Option<bool>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    #[default]
    Comfortable,
    Compact,
}

/// `theme.css` and `theme.js` are each one path or a list of them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Paths {
    #[default]
    None,
    One(String),
    Many(Vec<String>),
}

impl Paths {
    pub fn as_slice(&self) -> Vec<&str> {
        match self {
            Self::None => Vec::new(),
            Self::One(path) => vec![path.as_str()],
            Self::Many(paths) => paths.iter().map(String::as_str).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_object_is_the_documented_default() {
        let config: ThemeConfig = serde_json::from_str("{}").expect("empty theme config");
        assert_eq!(config.preset, Preset::Aurora);
        assert_eq!(config.appearance.default, Scheme::System);
        assert_eq!(config.code_theme.light, "github-light");
        assert!(!config.appearance.strict);
        assert!(config.css.as_slice().is_empty());
    }

    #[test]
    fn keys_the_theme_does_not_read_are_ignored() {
        let config: ThemeConfig = serde_json::from_str(r#"{"somethingNew": 1, "preset": "slate"}"#)
            .expect("unknown keys do not break the theme");
        assert_eq!(config.preset, Preset::Slate);
    }

    #[test]
    fn css_and_js_accept_one_path_or_many() {
        let config: ThemeConfig =
            serde_json::from_str(r#"{"css": "brand.css", "js": ["a.js", "b.js"]}"#)
                .expect("paths parse");
        assert_eq!(config.css.as_slice(), vec!["brand.css"]);
        assert_eq!(config.js.as_slice(), vec!["a.js", "b.js"]);
    }

    #[test]
    fn a_font_family_with_spaces_is_quoted_in_the_stack() {
        let face = Face {
            family: Some("Fira Sans".to_owned()),
            ..Face::default()
        };
        assert_eq!(face.stack("sans-serif"), "\"Fira Sans\", sans-serif");
        assert_eq!(Face::default().stack("sans-serif"), "sans-serif");
    }
}
