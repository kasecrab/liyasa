//! CFG-03, CFG-07, CFG-08, CFG-09: the enums `theme.*` offers. The schema is
//! the single source (CFG-94) and the theme layer keeps its own mirror of the
//! slice it reads, so the two are checked against each other here.

use liyasa_config::schema;
use liyasa_theme::config::{Preset, ThemeConfig};
use serde_json::{Value, json};

fn enum_at(pointer: &str) -> Vec<String> {
    let schema: Value = serde_json::from_str(schema::CONFIG_SCHEMA).expect("valid JSON");
    schema
        .pointer(pointer)
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{pointer} is an enum in the schema"))
        .iter()
        .map(|value| {
            value
                .as_str()
                .unwrap_or_else(|| panic!("{pointer} holds strings"))
                .to_owned()
        })
        .collect()
}

/// Every value of one `theme.*` enum, read through the theme's mirror and
/// written back out, so a value the schema offers and the theme cannot name
/// fails here rather than at render time.
fn round_trips(pointer: &str, at: impl Fn(&str) -> Value, back: &str) {
    for value in enum_at(pointer) {
        let theme: ThemeConfig =
            serde_json::from_value(at(&value)).unwrap_or_else(|e| panic!("{value}: {e}"));
        let written = serde_json::to_value(&theme).expect("a theme config serializes");
        assert_eq!(
            written.pointer(back).and_then(Value::as_str),
            Some(value.as_str()),
            "the theme reads `{value}` as something else"
        );
    }
}

#[test]
fn the_nine_presets_are_the_nine_the_theme_ships() {
    let named: Vec<String> = Preset::ALL.iter().map(|p| p.name().to_owned()).collect();
    assert_eq!(enum_at("/properties/theme/properties/preset/enum"), named);
}

#[test]
fn every_preset_is_a_preset_the_theme_can_read() {
    round_trips(
        "/properties/theme/properties/preset/enum",
        |value| json!({ "preset": value }),
        "/preset",
    );
}

#[test]
fn every_appearance_and_decoration_is_one_the_theme_can_read() {
    round_trips(
        "/properties/theme/properties/appearance/properties/default/enum",
        |value| json!({ "appearance": { "default": value } }),
        "/appearance/default",
    );
    round_trips(
        "/properties/theme/properties/appearance/properties/background/properties/decoration/enum",
        |value| json!({ "appearance": { "background": { "decoration": value } } }),
        "/appearance/background/decoration",
    );
}

#[test]
fn every_density_is_one_the_theme_can_read() {
    round_trips(
        "/properties/theme/properties/layout/properties/density/enum",
        |value| json!({ "layout": { "density": value } }),
        "/layout/density",
    );
}

#[test]
fn the_icon_libraries_are_the_four_that_ship() {
    assert_eq!(
        enum_at("/properties/theme/properties/icons/properties/library/enum"),
        ["lucide", "phosphor", "tabler", "fontawesome"],
        "CFG-07 bundles these four; Font Awesome Pro is user-supplied"
    );
}
