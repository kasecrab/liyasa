//! THM-02: every colour pair the theme puts on screen meets WCAG 2.2 AA in
//! both schemes — 4.5:1 for body text, 3:1 for large text and controls.

use liyasa_theme::color::{AA_BODY, AA_LARGE, Color};
use liyasa_theme::config::{Colors, ThemeConfig};
use liyasa_theme::tokens::{Scheme, Tokens, aurora, reference};

#[test]
fn the_default_theme_passes_aa_in_both_schemes() {
    let failures = Tokens::aurora().contrast_failures();
    assert!(
        failures.is_empty(),
        "{}",
        failures
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn every_documented_pair_resolves_to_a_colour() {
    let tokens = Tokens::aurora();
    for (foreground, background, minimum) in aurora::AA_PAIRS {
        assert!(
            *minimum == AA_BODY || *minimum == AA_LARGE,
            "`{foreground}` on `{background}` asks for {minimum}:1, which is not an AA threshold"
        );
        for scheme in Scheme::BOTH {
            for token in [foreground, background] {
                assert!(
                    tokens.color(token, scheme).is_some(),
                    "`{token}` does not resolve to a colour in the {} scheme",
                    scheme.name()
                );
            }
        }
    }
}

#[test]
fn every_colour_token_is_a_colour() {
    let tokens = Tokens::aurora();
    for spec in reference() {
        if !spec.name.starts_with("--ly-color-") && !spec.name.starts_with("--ly-code-token-") {
            continue;
        }
        for scheme in Scheme::BOTH {
            assert!(
                tokens.color(spec.name, scheme).is_some(),
                "`{}` is in the colour group but does not parse as a colour",
                spec.name
            );
        }
    }
}

#[test]
fn a_brand_colour_a_designer_would_pick_still_passes() {
    // One brand colour, no per-scheme variant: the case an operator hits first.
    for brand in [
        "#0a7cff", "#d63384", "#ff6b35", "#10b981", "#7c3aed", "#005f73", "#111111",
    ] {
        let config = ThemeConfig {
            colors: Colors {
                primary: Some(brand.to_owned()),
                ..Colors::default()
            },
            ..ThemeConfig::default()
        };
        let (tokens, diagnostics) = Tokens::from_config(&config);
        assert!(diagnostics.is_empty(), "`{brand}` did not parse");
        let failures = tokens.contrast_failures();
        assert!(
            failures.is_empty(),
            "`{brand}` produced {} failure(s): {}",
            failures.len(),
            failures
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
}

#[test]
fn the_focus_ring_clears_three_to_one_against_what_surrounds_it() {
    let tokens = Tokens::aurora();
    for scheme in Scheme::BOTH {
        let ring = tokens
            .color("--ly-color-focus", scheme)
            .expect("the focus ring is a colour");
        for surface in [
            "--ly-color-bg",
            "--ly-color-bg-subtle",
            "--ly-color-surface",
            "--ly-color-elevated",
        ] {
            let background: Color = tokens
                .color(surface, scheme)
                .unwrap_or_else(|| panic!("`{surface}` is a colour"));
            assert!(
                ring.contrast(background) >= AA_LARGE,
                "the focus ring is {:.2}:1 on `{surface}` in the {} scheme",
                ring.contrast(background),
                scheme.name()
            );
        }
    }
}
