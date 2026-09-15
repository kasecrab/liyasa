//! Theme presets (CFG-03, THM-24).
//!
//! A preset is a token patch and, optionally, partial overrides: never a fork.
//! The nine built-in presets are declared as the few decisions that separate
//! them — a brand colour, a neutral tint, a measure, a density — and every
//! derived colour goes through the same role derivation a configured
//! `theme.colors.primary` does, so a preset cannot ship a palette that fails
//! WCAG AA.

use std::collections::BTreeMap;

use liyasa_core::diagnostics::{Diagnostic, code};
use serde::{Deserialize, Serialize};

use crate::color::Color;
use crate::config::{Density, Preset};
use crate::theme::Overrides;
use crate::tokens::{Scheme, Tokens};

/// What separates one built-in preset from another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recipe {
    pub name: &'static str,
    pub summary: &'static str,
    /// The brand colour, per scheme; both are adapted to clear AA.
    pub primary: (&'static str, &'static str),
    pub accent: (&'static str, &'static str),
    /// Token patches applied after the roles: measure, radius, chrome.
    pub tokens: &'static [(&'static str, &'static str)],
    pub density: Density,
    /// `system` unless the preset is opinionated about it (CFG-08).
    pub appearance: &'static str,
}

const RECIPES: &[Recipe] = &[
    Recipe {
        name: "aurora",
        summary: "Sidebar plus table of contents; the default.",
        primary: ("#1e34c8", "#7484e7"),
        accent: ("#087581", "#52d2e0"),
        tokens: &[],
        density: Density::Comfortable,
        appearance: "system",
    },
    Recipe {
        name: "atlas",
        summary: "Top tabs and a dense reference layout.",
        primary: ("#15489c", "#7aa7f0"),
        accent: ("#8129ae", "#d096ee"),
        tokens: &[
            ("--ly-layout-content", "52rem"),
            ("--ly-layout-sidebar", "16rem"),
            ("--ly-radius-md", "6px"),
        ],
        density: Density::Compact,
        appearance: "system",
    },
    Recipe {
        name: "meadow",
        summary: "Soft and marketing-adjacent, with generous spacing.",
        primary: ("#116f40", "#6adca3"),
        accent: ("#8e5c06", "#f5b547"),
        tokens: &[
            ("--ly-radius-md", "12px"),
            ("--ly-radius-lg", "18px"),
            ("--ly-layout-content", "44rem"),
            ("--ly-text-leading-normal", "1.75"),
        ],
        density: Density::Comfortable,
        appearance: "system",
    },
    Recipe {
        name: "slate",
        summary: "Minimal and monochrome; the brand is the typography.",
        primary: ("#1a1e2b", "#f0f2f7"),
        accent: ("#4b5163", "#afb5c8"),
        tokens: &[("--ly-radius-md", "4px"), ("--ly-radius-sm", "3px")],
        density: Density::Comfortable,
        appearance: "system",
    },
    Recipe {
        name: "ember",
        summary: "Dark first, with a warm accent.",
        primary: ("#9c4911", "#f4a967"),
        accent: ("#a92350", "#ef8aac"),
        tokens: &[("--ly-radius-md", "6px")],
        density: Density::Comfortable,
        appearance: "dark",
    },
    Recipe {
        name: "harbor",
        summary: "A help centre: cards, wide gutters, friendly radii.",
        primary: ("#08717d", "#6adae7"),
        accent: ("#1e34c8", "#7484e7"),
        tokens: &[
            ("--ly-radius-md", "14px"),
            ("--ly-radius-lg", "20px"),
            ("--ly-layout-gutter", "var(--ly-space-6)"),
        ],
        density: Density::Comfortable,
        appearance: "system",
    },
    Recipe {
        name: "quill",
        summary: "Documentation for writers: wide prose, quiet chrome.",
        primary: ("#5b3ba6", "#b79bf0"),
        accent: ("#0d7340", "#60dc9e"),
        tokens: &[
            ("--ly-layout-content", "40rem"),
            ("--ly-text-base", "clamp(1rem, 0.96rem + 0.2vw, 1.0625rem)"),
            ("--ly-text-leading-normal", "1.8"),
        ],
        density: Density::Comfortable,
        appearance: "system",
    },
    Recipe {
        name: "signal",
        summary: "API first: three columns with a code rail.",
        primary: ("#1c5987", "#84c9eb"),
        accent: ("#8129ae", "#d096ee"),
        tokens: &[
            ("--ly-layout-content", "38rem"),
            ("--ly-layout-toc", "20rem"),
            ("--ly-radius-md", "6px"),
        ],
        density: Density::Compact,
        appearance: "system",
    },
    Recipe {
        name: "lumen",
        summary: "Accessibility first: maximum contrast and heavier borders.",
        primary: ("#12227f", "#b9c4ff"),
        accent: ("#0b4f58", "#9ae8f2"),
        tokens: &[
            ("--ly-border-width", "1px"),
            ("--ly-border-width-strong", "3px"),
            ("--ly-color-border", "var(--ly-color-neutral-9)"),
            ("--ly-color-border-subtle", "var(--ly-color-neutral-8)"),
            ("--ly-color-text-subtle", "var(--ly-color-neutral-11)"),
        ],
        density: Density::Comfortable,
        appearance: "system",
    },
];

pub fn recipe(preset: Preset) -> &'static Recipe {
    RECIPES
        .iter()
        .find(|recipe| recipe.name == preset.name())
        .unwrap_or(&RECIPES[0])
}

pub fn recipes() -> &'static [Recipe] {
    RECIPES
}

/// Applies a built-in preset to a token set.
pub fn apply(preset: Preset, tokens: &mut Tokens) {
    let recipe = recipe(preset);
    for (role, (light, dark)) in [("primary", recipe.primary), ("accent", recipe.accent)] {
        if let Ok(color) = Color::parse(light) {
            tokens.set_role(role, Scheme::Light, color, true);
        }
        if let Ok(color) = Color::parse(dark) {
            tokens.set_role(role, Scheme::Dark, color, true);
        }
    }
    if recipe.density == Density::Compact {
        for (token, value) in crate::tokens::aurora::COMPACT_SPACE {
            tokens.set(token, None, value);
        }
    }
    for (token, value) in recipe.tokens {
        tokens.set(token, None, value);
    }
}

/// `preset.json`: what a theme package declares (THM-24).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: Option<String>,
    pub license: Option<String>,
    /// A built-in preset this package starts from.
    pub extends: Option<String>,
    /// Path inside the package, relative to `preset.json`.
    pub tokens: String,
    pub partials: String,
    pub layouts: String,
    pub assets: String,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            name: String::new(),
            version: "0.1.0".to_owned(),
            description: String::new(),
            author: None,
            license: None,
            extends: None,
            tokens: "tokens.css".to_owned(),
            partials: "partials".to_owned(),
            layouts: "layouts".to_owned(),
            assets: "assets".to_owned(),
        }
    }
}

/// Why a `preset.json` was rejected. Small by design: the diagnostic is built
/// on demand, so the error path costs nothing to return.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PresetError {
    #[error("`preset.json` is not valid JSON: {0}")]
    Json(String),
    #[error("`preset.json` has no `name`; a preset is installed by name")]
    Unnamed,
    #[error("`preset.json` extends `{0}`, which is not a built-in preset")]
    UnknownBase(String),
}

impl PresetError {
    pub fn diagnostic(&self) -> Diagnostic {
        let code = match self {
            Self::Json(_) => code::E0101,
            Self::Unnamed | Self::UnknownBase(_) => code::E0102,
        };
        Diagnostic::new(code, self.to_string())
    }
}

impl Manifest {
    pub fn parse(json: &str) -> Result<Self, PresetError> {
        let manifest: Self =
            serde_json::from_str(json).map_err(|error| PresetError::Json(error.to_string()))?;
        if manifest.name.is_empty() {
            return Err(PresetError::Unnamed);
        }
        if let Some(extends) = &manifest.extends
            && !Preset::ALL.iter().any(|preset| preset.name() == extends)
        {
            return Err(PresetError::UnknownBase(extends.clone()));
        }
        Ok(manifest)
    }

    pub fn base(&self) -> Preset {
        self.extends
            .as_deref()
            .and_then(|name| {
                Preset::ALL
                    .iter()
                    .copied()
                    .find(|preset| preset.name() == name)
            })
            .unwrap_or_default()
    }
}

/// A loaded theme package: the manifest, its token file, and its overrides.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Package {
    pub manifest: Manifest,
    pub tokens_css: String,
    pub partials: BTreeMap<String, String>,
    pub layouts: BTreeMap<String, String>,
    /// Asset paths the package ships, copied into the build's output.
    pub assets: Vec<String>,
}

impl Package {
    /// The token set and the overrides this package produces.
    pub fn resolve(&self) -> (Tokens, Overrides) {
        let mut tokens = Tokens::aurora();
        apply(self.manifest.base(), &mut tokens);
        tokens.with_overrides(&self.tokens_css);
        (
            tokens,
            Overrides {
                partials: self.partials.clone(),
                layouts: self.layouts.clone(),
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_the_schema_names_has_a_recipe() {
        for preset in Preset::ALL {
            let recipe = recipe(preset);
            assert_eq!(recipe.name, preset.name());
            assert!(!recipe.summary.is_empty());
        }
        assert_eq!(recipes().len(), Preset::ALL.len());
    }

    #[test]
    fn every_preset_passes_aa_in_both_schemes() {
        for preset in Preset::ALL {
            let mut tokens = Tokens::aurora();
            apply(preset, &mut tokens);
            let failures = tokens.contrast_failures();
            assert!(
                failures.is_empty(),
                "`{}`: {}",
                preset.name(),
                failures
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; ")
            );
        }
    }

    #[test]
    fn a_preset_changes_the_brand_and_nothing_it_did_not_ask_to() {
        let aurora = Tokens::aurora();
        let mut meadow = Tokens::aurora();
        apply(Preset::Meadow, &mut meadow);
        assert_ne!(
            meadow.get("--ly-color-primary", Scheme::Light),
            aurora.get("--ly-color-primary", Scheme::Light)
        );
        assert_eq!(
            meadow.get("--ly-color-neutral-12", Scheme::Light),
            aurora.get("--ly-color-neutral-12", Scheme::Light),
            "a preset that did not touch the ramp keeps it"
        );
        assert_eq!(meadow.get("--ly-radius-md", Scheme::Light), Some("12px"));
    }

    #[test]
    fn a_manifest_needs_a_name_and_a_real_base() {
        let manifest = Manifest::parse(r#"{"name":"acme","extends":"slate"}"#).expect("parses");
        assert_eq!(manifest.base(), Preset::Slate);
        assert_eq!(manifest.tokens, "tokens.css");

        let error = Manifest::parse("{}").expect_err("a nameless preset is rejected");
        assert_eq!(error.diagnostic().code.as_str(), "E0102");
        let error = Manifest::parse(r#"{"name":"a","extends":"nope"}"#)
            .expect_err("an unknown base is rejected");
        assert!(error.to_string().contains("nope"));
        let error = Manifest::parse("{").expect_err("invalid JSON is reported, not panicked on");
        assert_eq!(error.diagnostic().code.as_str(), "E0101");
    }

    #[test]
    fn a_package_resolves_to_tokens_and_overrides() {
        let package = Package {
            manifest: Manifest {
                name: "acme".to_owned(),
                extends: Some("ember".to_owned()),
                ..Manifest::default()
            },
            tokens_css: ":root { --ly-layout-content: 60rem; }".to_owned(),
            partials: BTreeMap::from([("footer".to_owned(), "<footer>acme</footer>".to_owned())]),
            layouts: BTreeMap::new(),
            assets: vec!["logo.svg".to_owned()],
        };
        let (tokens, overrides) = package.resolve();
        assert_eq!(
            tokens.get("--ly-layout-content", Scheme::Light),
            Some("60rem")
        );
        assert_eq!(
            tokens.get("--ly-color-primary", Scheme::Dark),
            recipe(Preset::Ember)
                .primary
                .1
                .parse::<String>()
                .ok()
                .as_deref()
                .or(tokens.get("--ly-color-primary", Scheme::Dark)),
            "the package starts from the preset it extends"
        );
        assert_eq!(overrides.partials.len(), 1);
        assert!(!overrides.is_empty());
    }
}
