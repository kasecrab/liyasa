//! The token layer: every themeable value as a `--ly-` custom property
//! (THM-10, THM-11, THM-12).
//!
//! One table, [`aurora::SPECS`], is both the reference documentation and the
//! source of the emitted CSS, so the two cannot drift. A token is either fixed
//! or scheme-dependent; scheme-dependent tokens carry the same name in both
//! schemes (THM-12), which is what lets an operator override one once.

pub mod aurora;

use std::collections::BTreeMap;
use std::fmt::Write as _;

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, Severity, code};

use crate::color::{AA_BODY, AA_LARGE, Color};
use crate::config::{Colors, Density, Layout, ThemeConfig};

/// The groups THM-11 names. A token's name prefix follows its group, so
/// `--ly-color-*` is [`Group::Color`] and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    Color,
    Font,
    Text,
    Space,
    Radius,
    Shadow,
    Border,
    Motion,
    ZIndex,
    Layout,
    Code,
}

impl Group {
    pub const ALL: [Self; 11] = [
        Self::Color,
        Self::Font,
        Self::Text,
        Self::Space,
        Self::Radius,
        Self::Shadow,
        Self::Border,
        Self::Motion,
        Self::ZIndex,
        Self::Layout,
        Self::Code,
    ];

    /// The name prefix every token in the group carries.
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::Color => "--ly-color-",
            Self::Font => "--ly-font-",
            Self::Text => "--ly-text-",
            Self::Space => "--ly-space-",
            Self::Radius => "--ly-radius-",
            Self::Shadow => "--ly-shadow-",
            Self::Border => "--ly-border",
            Self::Motion => "--ly-motion-",
            Self::ZIndex => "--ly-z-",
            Self::Layout => "--ly-layout-",
            Self::Code => "--ly-code-",
        }
    }

    pub const fn title(self) -> &'static str {
        match self {
            Self::Color => "Colour",
            Self::Font => "Typography",
            Self::Text => "Type scale",
            Self::Space => "Spacing",
            Self::Radius => "Radius",
            Self::Shadow => "Shadow",
            Self::Border => "Border",
            Self::Motion => "Motion",
            Self::ZIndex => "Z-index",
            Self::Layout => "Layout widths",
            Self::Code => "Code theme",
        }
    }
}

/// One row of the token reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spec {
    pub name: &'static str,
    pub group: Group,
    pub doc: &'static str,
    pub value: SpecValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecValue {
    /// The same in both schemes.
    Fixed(&'static str),
    /// THM-12: one name, two values.
    Scheme {
        light: &'static str,
        dark: &'static str,
    },
}

impl Spec {
    pub const fn is_scheme_dependent(&self) -> bool {
        matches!(self.value, SpecValue::Scheme { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scheme {
    Light,
    Dark,
}

impl Scheme {
    pub const BOTH: [Self; 2] = [Self::Light, Self::Dark];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

/// Every documented token, in emission order.
pub fn reference() -> &'static [Spec] {
    aurora::SPECS
}

pub fn spec(name: &str) -> Option<&'static Spec> {
    aurora::SPECS.iter().find(|spec| spec.name == name)
}

/// A resolved token set: the defaults, plus whatever the config and the
/// operator's `theme/tokens.css` changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tokens {
    /// Name, light value, dark value, in the reference's order.
    values: Vec<(&'static str, String, String)>,
    /// Tokens an operator invented; emitted after the documented ones.
    extra: BTreeMap<String, (Option<String>, Option<String>)>,
}

impl Default for Tokens {
    fn default() -> Self {
        Self::aurora()
    }
}

impl Tokens {
    /// The default preset's values (THM-01).
    pub fn aurora() -> Self {
        let values = aurora::SPECS
            .iter()
            .map(|spec| match spec.value {
                SpecValue::Fixed(value) => (spec.name, value.to_owned(), value.to_owned()),
                SpecValue::Scheme { light, dark } => (spec.name, light.to_owned(), dark.to_owned()),
            })
            .collect();
        Self {
            values,
            extra: BTreeMap::new(),
        }
    }

    /// The token set `theme.*` asks for (THM-10). A colour that does not parse
    /// is reported and the default kept, so a typo costs one token, not a page.
    pub fn from_config(config: &ThemeConfig) -> (Self, Diagnostics) {
        let mut tokens = Self::aurora();
        let mut diagnostics = Diagnostics::new();
        tokens.apply_colors(&config.colors, &mut diagnostics);
        tokens.apply_fonts(config);
        tokens.apply_layout(&config.layout);
        (tokens, diagnostics)
    }

    pub fn get(&self, name: &str, scheme: Scheme) -> Option<&str> {
        if let Some((_, light, dark)) = self.values.iter().find(|(token, _, _)| *token == name) {
            return Some(match scheme {
                Scheme::Light => light,
                Scheme::Dark => dark,
            });
        }
        let (light, dark) = self.extra.get(name)?;
        match scheme {
            Scheme::Light => light.as_deref(),
            Scheme::Dark => dark.as_deref(),
        }
    }

    /// The colour a token resolves to, following one level of `var()`
    /// indirection, which is all the default table uses.
    pub fn color(&self, name: &str, scheme: Scheme) -> Option<Color> {
        let value = self.get(name, scheme)?;
        let value = match value.strip_prefix("var(") {
            Some(inner) => self.get(inner.trim_end_matches(')').trim(), scheme)?,
            None => value,
        };
        Color::parse(value).ok()
    }

    pub fn set(&mut self, name: &str, scheme: Option<Scheme>, value: &str) {
        if let Some(entry) = self.values.iter_mut().find(|(token, _, _)| *token == name) {
            match scheme {
                Some(Scheme::Light) => entry.1 = value.to_owned(),
                Some(Scheme::Dark) => entry.2 = value.to_owned(),
                None => {
                    entry.1 = value.to_owned();
                    entry.2 = value.to_owned();
                }
            }
            return;
        }
        let entry = self.extra.entry(name.to_owned()).or_default();
        match scheme {
            Some(Scheme::Light) => entry.0 = Some(value.to_owned()),
            Some(Scheme::Dark) => entry.1 = Some(value.to_owned()),
            None => {
                entry.0 = Some(value.to_owned());
                entry.1 = Some(value.to_owned());
            }
        }
    }

    /// Token names in emission order, the documented ones first.
    pub fn names(&self) -> Vec<&str> {
        self.values
            .iter()
            .map(|(name, _, _)| *name)
            .chain(self.extra.keys().map(String::as_str))
            .collect()
    }

    /// The stylesheet's first block (THM-12): one `:root` carrying the light
    /// scheme, then the dark values under both the media query and the
    /// attribute, so an explicit toggle wins over the system preference in both
    /// directions.
    pub fn to_css(&self) -> String {
        let mut out = String::new();
        out.push_str(":root {\n");
        out.push_str(&self.declarations(Scheme::Light, "  "));
        out.push_str("  color-scheme: light dark;\n}\n\n");

        let dark = self.dark_only();
        out.push_str(
            "@media (prefers-color-scheme: dark) {\n  :root:not([data-theme=\"light\"]) {\n",
        );
        out.push_str(&dark_block(&dark, "    "));
        out.push_str("    color-scheme: dark;\n  }\n}\n\n");

        out.push_str("[data-theme=\"dark\"] {\n");
        out.push_str(&dark_block(&dark, "  "));
        out.push_str("  color-scheme: dark;\n}\n");
        out
    }

    /// The shell's tokens alone, minified, for the inlined critical block
    /// (THM-30). The full set arrives with the cached stylesheet.
    pub fn critical_css(&self) -> String {
        let wanted = |name: &str| aurora::CRITICAL.contains(&name);
        let mut light = String::new();
        let mut dark = String::new();
        for (name, light_value, dark_value) in &self.values {
            if !wanted(name) {
                continue;
            }
            let _ = write!(light, "{name}:{light_value};");
            if light_value != dark_value {
                let _ = write!(dark, "{name}:{dark_value};");
            }
        }
        let light = light.trim_end_matches(';');
        let dark = dark.trim_end_matches(';');
        format!(
            ":root{{{light};color-scheme:light dark}}\
             @media (prefers-color-scheme:dark){{:root:not([data-theme=\"light\"]){{{dark}}}}}\
             [data-theme=\"dark\"]{{{dark}}}"
        )
    }

    fn declarations(&self, scheme: Scheme, indent: &str) -> String {
        let mut out = String::new();
        for (name, light, dark) in &self.values {
            let value = match scheme {
                Scheme::Light => light,
                Scheme::Dark => dark,
            };
            let _ = writeln!(out, "{indent}{name}: {value};");
        }
        for (name, (light, dark)) in &self.extra {
            let value = match scheme {
                Scheme::Light => light.as_ref(),
                Scheme::Dark => dark.as_ref(),
            };
            if let Some(value) = value {
                let _ = writeln!(out, "{indent}{name}: {value};");
            }
        }
        out
    }

    /// Only the tokens whose dark value differs, so the dark blocks stay small.
    fn dark_only(&self) -> Vec<(&str, &str)> {
        let documented = self
            .values
            .iter()
            .filter(|(_, light, dark)| light != dark)
            .map(|(name, _, dark)| (*name, dark.as_str()));
        let extra = self.extra.iter().filter_map(|(name, (light, dark))| {
            let dark = dark.as_deref()?;
            (light.as_deref() != Some(dark)).then_some((name.as_str(), dark))
        });
        documented.chain(extra).collect()
    }

    fn apply_colors(&mut self, colors: &Colors, diagnostics: &mut Diagnostics) {
        let mut parse = |path: &str, value: &Option<String>| -> Option<Color> {
            let text = value.as_ref()?;
            match Color::parse(text) {
                Ok(color) => Some(color),
                Err(error) => {
                    diagnostics.push(
                        Diagnostic::new(code::E0102, format!("`theme.{path}`: {error}"))
                            .help("expected a hex, rgb(), or hsl() colour"),
                    );
                    None
                }
            }
        };

        let primary = parse("colors.primary", &colors.primary);
        let light_primary = parse("colors.light", &colors.light);
        let dark_primary = parse("colors.dark", &colors.dark);
        // A per-scheme variant is the operator's own call and is used as given;
        // one `primary` for both schemes is adapted per scheme, which is what
        // CFG-04's "primary variants per scheme" means when only one is set.
        if let Some(color) = light_primary.or(primary) {
            self.set_primary(Scheme::Light, color, light_primary.is_none());
        }
        if let Some(color) = dark_primary.or(primary) {
            self.set_primary(Scheme::Dark, color, dark_primary.is_none());
        }

        for (path, value, token) in [
            ("colors.text", &colors.text, "--ly-color-text"),
            ("colors.muted", &colors.muted, "--ly-color-text-muted"),
            ("colors.border", &colors.border, "--ly-color-border"),
            ("colors.accent", &colors.accent, "--ly-color-accent"),
            ("colors.success", &colors.success, "--ly-color-success"),
            ("colors.warning", &colors.warning, "--ly-color-warning"),
            ("colors.danger", &colors.danger, "--ly-color-danger"),
        ] {
            if let Some(color) = parse(path, value) {
                self.set(token, None, &color.to_string());
            }
        }
        if let Some(color) = parse("colors.background.light", &colors.background.light) {
            self.set("--ly-color-bg", Some(Scheme::Light), &color.to_string());
        }
        if let Some(color) = parse("colors.background.dark", &colors.background.dark) {
            self.set("--ly-color-bg", Some(Scheme::Dark), &color.to_string());
        }
    }

    /// One configured colour drives the whole primary role: hover and pressed
    /// states, the tint, and the text colour that sits on the page background
    /// rather than on the fill.
    fn set_primary(&mut self, scheme: Scheme, configured: Color, adapt: bool) {
        let background = self
            .color("--ly-color-bg", scheme)
            .unwrap_or(Color::rgb(255, 255, 255));
        let text = self
            .color("--ly-color-text", scheme)
            .unwrap_or(Color::rgb(0, 0, 0));
        // Every surface the role can land on, so a derived colour is checked
        // against the hardest one rather than only against the page.
        let surfaces: Vec<Color> = [
            "--ly-color-bg",
            "--ly-color-bg-subtle",
            "--ly-color-surface",
            "--ly-color-elevated",
        ]
        .iter()
        .filter_map(|token| self.color(token, scheme))
        .collect();
        let color = if adapt {
            adapt_fill(configured, &surfaces, background, text)
        } else {
            configured
        };

        self.set("--ly-color-primary", Some(scheme), &color.to_string());
        // Both schemes move toward the text colour on hover: darker on light,
        // lighter on dark. The fill gains contrast either way, so the label on
        // it keeps the ratio it had at rest.
        let (hover, active) = (color.mix(text, 0.12), color.mix(text, 0.22));
        self.set("--ly-color-primary-hover", Some(scheme), &hover.to_string());
        self.set(
            "--ly-color-primary-active",
            Some(scheme),
            &active.to_string(),
        );
        let subtle = color.mix(background, 0.88);
        self.set(
            "--ly-color-primary-subtle",
            Some(scheme),
            &subtle.to_string(),
        );
        self.set(
            "--ly-color-primary-border",
            Some(scheme),
            &color.mix(background, 0.6).to_string(),
        );
        self.set(
            "--ly-color-primary-contrast",
            Some(scheme),
            &label_for(&[color, hover, active], background, text)
                .0
                .to_string(),
        );
        let mut text_surfaces = surfaces.clone();
        text_surfaces.push(subtle);
        self.set(
            "--ly-color-primary-text",
            Some(scheme),
            &visible_on(color, &text_surfaces, text, AA_BODY).to_string(),
        );
        self.set("--ly-color-focus", Some(scheme), &color.to_string());
    }

    fn apply_fonts(&mut self, config: &ThemeConfig) {
        let fonts = &config.fonts;
        if let Some(face) = &fonts.body {
            let stack = face.stack(aurora::SANS_FALLBACK);
            self.set("--ly-font-sans", None, &stack);
        }
        if let Some(face) = &fonts.heading {
            let stack = face.stack(aurora::SANS_FALLBACK);
            self.set("--ly-font-heading", None, &stack);
        }
        if let Some(face) = &fonts.mono {
            let stack = face.stack(aurora::MONO_FALLBACK);
            self.set("--ly-font-mono", None, &stack);
        }
    }

    fn apply_layout(&mut self, layout: &Layout) {
        for (value, token) in [
            (&layout.sidebar_width, "--ly-layout-sidebar"),
            (&layout.content_width, "--ly-layout-content"),
            (&layout.toc_width, "--ly-layout-toc"),
        ] {
            if let Some(width) = value {
                self.set(token, None, width);
            }
        }
        if let Some(radius) = &layout.radius {
            self.set("--ly-radius-md", None, radius);
        }
        if layout.density == Density::Compact {
            for (token, value) in aurora::COMPACT_SPACE {
                self.set(token, None, value);
            }
        }
        if layout.shadows == Some(false) {
            for token in ["--ly-shadow-sm", "--ly-shadow-md", "--ly-shadow-lg"] {
                self.set(token, None, "none");
            }
        }
    }

    /// THM-02: every documented pair, in both schemes.
    pub fn contrast_failures(&self) -> Vec<ContrastFailure> {
        let mut out = Vec::new();
        for scheme in Scheme::BOTH {
            for (foreground, background, minimum) in aurora::AA_PAIRS {
                let (Some(front), Some(back)) = (
                    self.color(foreground, scheme),
                    self.color(background, scheme),
                ) else {
                    continue;
                };
                let ratio = front.contrast(back);
                if ratio + 0.005 < *minimum {
                    out.push(ContrastFailure {
                        foreground,
                        background,
                        scheme,
                        ratio,
                        minimum: *minimum,
                    });
                }
            }
        }
        out
    }

    /// The same check as diagnostics. CFG-04 says the contrast check warns, so
    /// `E0107` is emitted at warning severity; policy may promote it.
    pub fn contrast_diagnostics(&self) -> Diagnostics {
        self.contrast_failures()
            .into_iter()
            .map(|failure| {
                Diagnostic::new(code::E0107, failure.to_string())
                    .with_severity(Severity::Warning)
                    .help("adjust `theme.colors` or override the token in `theme/tokens.css`")
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContrastFailure {
    pub foreground: &'static str,
    pub background: &'static str,
    pub scheme: Scheme,
    pub ratio: f32,
    pub minimum: f32,
}

impl std::fmt::Display for ContrastFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "`{}` on `{}` is {:.2}:1 in the {} scheme, below the WCAG 2.2 AA minimum of {:.1}:1",
            self.foreground,
            self.background,
            self.ratio,
            self.scheme.name(),
            self.minimum
        )
    }
}

fn dark_block(values: &[(&str, &str)], indent: &str) -> String {
    let mut out = String::new();
    for (name, value) in values {
        let _ = writeln!(out, "{indent}{name}: {value};");
    }
    out
}

/// The label colour for a fill and its hover and pressed states, with the ratio
/// it holds in the worst of them: a label chosen for the resting state alone
/// can fall below AA the moment the pointer arrives.
fn label_for(states: &[Color], background: Color, text: Color) -> (Color, f32) {
    let worst = |candidate: Color| {
        states
            .iter()
            .map(|state| candidate.contrast(*state))
            .fold(f32::INFINITY, f32::min)
    };
    [Color::rgb(255, 255, 255), text, background]
        .into_iter()
        .map(|candidate| (candidate, worst(candidate)))
        .fold((text, f32::NEG_INFINITY), |best, candidate| {
            if candidate.1 > best.1 {
                candidate
            } else {
                best
            }
        })
}

/// A fill an operator configured, moved just far enough toward the scheme's
/// text colour to be visible on every surface (3:1) and to carry a label at
/// 4.5:1. A mid-tone brand blue carries neither white nor black text, so
/// leaving it alone would fail AA whichever label the theme picked.
fn adapt_fill(color: Color, surfaces: &[Color], background: Color, text: Color) -> Color {
    let ok = |candidate: Color| {
        let visible = surfaces
            .iter()
            .all(|surface| candidate.contrast(*surface) >= AA_LARGE);
        let states = [
            candidate,
            candidate.mix(text, 0.12),
            candidate.mix(text, 0.22),
        ];
        visible && label_for(&states, background, text).1 >= AA_BODY
    };
    let mut candidate = color;
    let mut step = 0.0;
    while !ok(candidate) && step < 1.0 {
        step += 0.02;
        candidate = color.mix(text, step);
    }
    candidate
}

/// The nearest colour to `color`, on the line toward the scheme's text colour,
/// that clears `minimum` against every background it can land on. Mixing toward
/// the text colour is the direction contrast lies in either scheme: darker on
/// light, lighter on dark.
fn visible_on(color: Color, backgrounds: &[Color], text: Color, minimum: f32) -> Color {
    let clears = |candidate: Color| {
        backgrounds
            .iter()
            .all(|background| candidate.contrast(*background) >= minimum)
    };
    let mut candidate = color;
    let mut step = 0.0;
    while !clears(candidate) && step < 1.0 {
        step += 0.02;
        candidate = color.mix(text, step);
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Background;

    #[test]
    fn every_emitted_token_is_documented_and_grouped() {
        let tokens = Tokens::aurora();
        for name in tokens.names() {
            let spec = spec(name).unwrap_or_else(|| panic!("`{name}` is not in the reference"));
            assert!(
                name.starts_with(spec.group.prefix()),
                "`{name}` is filed under {}, whose prefix is `{}`",
                spec.group.title(),
                spec.group.prefix()
            );
            assert!(!spec.doc.is_empty(), "`{name}` has no documentation");
        }
        assert_eq!(tokens.names().len(), reference().len());
        for group in Group::ALL {
            assert!(
                reference().iter().any(|spec| spec.group == group),
                "{} has no tokens",
                group.title()
            );
        }
    }

    #[test]
    fn dark_values_reuse_the_light_names() {
        let css = Tokens::aurora().to_css();
        assert!(css.contains("[data-theme=\"dark\"]"));
        assert!(css.contains("@media (prefers-color-scheme: dark)"));
        for spec in reference().iter().filter(|spec| spec.is_scheme_dependent()) {
            let occurrences = css.matches(&format!("{}:", spec.name)).count();
            assert!(
                occurrences >= 3,
                "`{}` is declared {occurrences} times; it needs the light, media, and attribute declarations",
                spec.name
            );
        }
    }

    #[test]
    fn the_toggle_can_win_over_the_system_preference() {
        let css = Tokens::aurora().to_css();
        let media = css
            .find("@media (prefers-color-scheme: dark)")
            .expect("the media block is emitted");
        let attribute = css
            .find("\n[data-theme=\"dark\"] {")
            .expect("the attribute block is emitted");
        assert!(
            attribute > media,
            "the attribute block must come last so an explicit choice wins"
        );
        assert!(css.contains(":root:not([data-theme=\"light\"])"));
    }

    #[test]
    fn a_configured_primary_drives_the_whole_role() {
        let config = ThemeConfig {
            colors: Colors {
                primary: Some("#005f73".to_owned()),
                ..Colors::default()
            },
            ..ThemeConfig::default()
        };
        let (tokens, diagnostics) = Tokens::from_config(&config);
        assert!(diagnostics.is_empty());
        assert_eq!(
            tokens.get("--ly-color-primary", Scheme::Light),
            Some("#005f73"),
            "a colour that already works is used as given"
        );
        assert_ne!(
            tokens.get("--ly-color-primary", Scheme::Dark),
            Some("#005f73"),
            "one `primary` is adapted for the dark scheme"
        );
        assert_ne!(
            tokens.get("--ly-color-primary-hover", Scheme::Light),
            tokens.get("--ly-color-primary", Scheme::Light)
        );
        assert_eq!(tokens.contrast_failures(), Vec::new());
    }

    #[test]
    fn a_colour_that_does_not_parse_is_reported_and_the_default_kept() {
        let config = ThemeConfig {
            colors: Colors {
                primary: Some("nearly-blue".to_owned()),
                background: Background {
                    light: Some("#ffffff".to_owned()),
                    dark: None,
                },
                ..Colors::default()
            },
            ..ThemeConfig::default()
        };
        let (tokens, diagnostics) = Tokens::from_config(&config);
        assert_eq!(diagnostics.len(), 1);
        let default = match spec("--ly-color-primary").map(|spec| spec.value) {
            Some(SpecValue::Scheme { light, .. }) => light,
            Some(SpecValue::Fixed(value)) => value,
            None => panic!("`--ly-color-primary` is in the reference"),
        };
        assert_eq!(
            tokens.get("--ly-color-primary", Scheme::Light),
            Some(default)
        );
        assert_eq!(tokens.get("--ly-color-bg", Scheme::Light), Some("#ffffff"));
    }

    #[test]
    fn a_per_scheme_variant_is_used_as_given_and_warned_about() {
        let config = ThemeConfig {
            colors: Colors {
                light: Some("#fdf6b2".to_owned()),
                ..Colors::default()
            },
            ..ThemeConfig::default()
        };
        let (tokens, _) = Tokens::from_config(&config);
        assert_eq!(
            tokens.get("--ly-color-primary", Scheme::Light),
            Some("#fdf6b2")
        );
        let diagnostics = tokens.contrast_diagnostics();
        assert!(!diagnostics.is_empty(), "a pale fill fails 3:1 on the page");
        assert!(!diagnostics.has_errors());
        for diagnostic in &diagnostics {
            assert_eq!(diagnostic.code.as_str(), "E0107");
        }
    }

    #[test]
    fn compact_density_shrinks_the_spacing_scale() {
        let config = ThemeConfig {
            layout: Layout {
                density: Density::Compact,
                ..Layout::default()
            },
            ..ThemeConfig::default()
        };
        let (compact, _) = Tokens::from_config(&config);
        let comfortable = Tokens::aurora();
        assert_ne!(
            compact.get("--ly-space-5", Scheme::Light),
            comfortable.get("--ly-space-5", Scheme::Light)
        );
    }
}
