//! THM-23: `liyasa theme eject <partial>` hands over the default, and
//! `liyasa theme diff` says what an override changed after an upgrade.

use std::collections::BTreeMap;

use liyasa_theme::config::ThemeConfig;
use liyasa_theme::context::partial_names;
use liyasa_theme::theme::{Change, Overrides, Theme, diff, is_changed};

#[test]
fn every_partial_can_be_ejected_to_a_path_the_theme_reads_back() {
    for partial in partial_names() {
        let (path, source) =
            Theme::eject(partial).unwrap_or_else(|| panic!("`{partial}` can be ejected"));
        assert_eq!(path, format!("theme/partials/{partial}.html"));
        assert!(!source.is_empty());

        // The ejected file is a working override: it loads and renders.
        let overrides = Overrides {
            partials: BTreeMap::from([(partial.to_owned(), source.to_owned())]),
            ..Overrides::default()
        };
        let theme = Theme::with_overrides(&ThemeConfig::default(), &overrides)
            .unwrap_or_else(|error| panic!("the ejected `{partial}` loads: {error}"));
        assert!(theme.is_overridden(partial));
    }
}

#[test]
fn a_layout_ejects_to_the_layouts_directory() {
    let (path, source) = Theme::eject("layouts/wide").expect("wide is a layout");
    assert_eq!(path, "theme/layouts/wide.html");
    assert!(source.contains("extends"));
    assert!(Theme::eject("nonsense").is_none());
}

#[test]
fn a_fresh_eject_differs_from_the_default_in_nothing() {
    let source = Theme::default_partial("footer").expect("the footer has a default");
    let changes = diff(source, source);
    assert!(!is_changed(&changes));
    assert!(
        changes
            .iter()
            .all(|change| matches!(change, Change::Kept(_)))
    );
}

#[test]
fn the_diff_names_the_lines_an_override_changed() {
    let default = "<footer>\n  <p>a</p>\n  <p>b</p>\n</footer>";
    let overridden = "<footer>\n  <p>a</p>\n  <p>brand</p>\n  <p>c</p>\n</footer>";
    let changes = diff(default, overridden);
    assert!(is_changed(&changes));
    assert!(changes.contains(&Change::Removed("  <p>b</p>".to_owned())));
    assert!(changes.contains(&Change::Added("  <p>brand</p>".to_owned())));
    assert!(changes.contains(&Change::Added("  <p>c</p>".to_owned())));
    assert_eq!(
        changes
            .iter()
            .filter(|change| matches!(change, Change::Kept(_)))
            .count(),
        3
    );
}

#[test]
fn the_diff_survives_an_upgrade_that_changed_the_default() {
    // After an upgrade the operator's copy is compared against the new default,
    // which is the question `liyasa theme diff` answers.
    let old_default = "<nav>\n  <a>one</a>\n</nav>";
    let operator_copy = "<nav class=\"brand\">\n  <a>one</a>\n</nav>";
    let new_default = "<nav>\n  <a>one</a>\n  <a>two</a>\n</nav>";

    let against_old = diff(old_default, operator_copy);
    let against_new = diff(new_default, operator_copy);
    assert!(is_changed(&against_old));
    assert!(
        against_new
            .iter()
            .any(|change| matches!(change, Change::Removed(line) if line.contains("two"))),
        "the upgrade's new line shows as missing from the override"
    );
}

#[test]
fn an_empty_override_removes_everything_the_default_had() {
    let default = "<p>a</p>\n<p>b</p>";
    let changes = diff(default, "");
    assert_eq!(changes.len(), 2);
    assert!(
        changes
            .iter()
            .all(|change| matches!(change, Change::Removed(_)))
    );
}
