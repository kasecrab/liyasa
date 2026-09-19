//! The licence gate (NFR-15): `deny.toml` may not widen past the requirement,
//! and a bundled asset may not arrive without a row.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use xtask::licences::{self, ALLOWED, EXTENSIONS, Scope};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

#[test]
fn the_allow_array_is_readable_and_not_empty() {
    let allow = licences::deny_allow(&root()).expect("deny.toml parses");
    assert!(
        allow.len() > 5,
        "only {} licences came out of deny.toml; the read is broken, not the file",
        allow.len()
    );
    for expected in ["MIT", "Apache-2.0"] {
        assert!(
            allow.contains(&expected.to_owned()),
            "deny.toml lost {expected}"
        );
    }
}

#[test]
fn every_allowed_licence_is_named_by_the_requirement_or_recorded_as_an_extension() {
    let loose = licences::unreconciled(&root()).expect("deny.toml parses");
    assert!(
        loose.is_empty(),
        "deny.toml allows {loose:?}, which NFR-15 does not name.\n\
         Either drop the row, or add an Extension to xtask/src/licences.rs saying \
         which crate reaches it and why the requirement's list should widen."
    );
}

#[test]
fn the_dictionary_licences_never_reach_the_crate_gate() {
    let bundled = licences::on_demand_in_the_crate_gate(&root()).expect("deny.toml parses");
    assert!(
        bundled.is_empty(),
        "deny.toml allows {bundled:?} for crates; NFR-15 allows them for an on-demand \
         CJK dictionary download only, never for something that ships"
    );
}

#[test]
fn an_unnamed_licence_is_caught() {
    // The branch that matters fires only on a licence nobody has recorded, and
    // the real file has none by construction — so drive it directly rather than
    // trusting that a green run means the comparison happened.
    let known = licences::allowed_for_crates();
    assert!(!known.contains("WTFPL"), "the allow set is not a filter");
    assert!(
        known.contains("MIT"),
        "the allow set lost NFR-15's own list"
    );
    assert!(
        known.contains("CDLA-Permissive-2.0"),
        "the allow set lost the recorded extensions"
    );
    assert!(
        !known.contains("IPADIC"),
        "IPADIC is on-demand only and must not reach the crate gate"
    );
}

#[test]
fn every_extension_says_what_reaches_it_and_why() {
    let named: BTreeSet<&str> = ALLOWED.iter().map(|l| l.spdx).collect();
    for extension in EXTENSIONS {
        assert!(
            !named.contains(extension.spdx),
            "{} is on NFR-15's list; it is not an extension",
            extension.spdx
        );
        assert!(
            extension.reached_by.len() > 8,
            "{} does not say which crate reaches it",
            extension.spdx
        );
        assert!(
            extension.why.len() > 20,
            "{} does not say why the requirement's list should widen",
            extension.spdx
        );
    }
}

#[test]
fn an_on_demand_only_licence_carries_its_condition() {
    for licence in ALLOWED.iter().filter(|l| l.scope == Scope::OnDemandOnly) {
        assert!(
            !licence.condition.is_empty(),
            "{} is scoped and says nothing about why",
            licence.spdx
        );
    }
}

#[test]
fn every_bundled_asset_has_a_row() {
    let orphans = licences::assets_without_a_row(&root()).expect("the inventory parses");
    assert!(
        orphans.is_empty(),
        "{orphans:?} ship with no row in xtask/assets.toml.\n\
         Add one with the asset's source and its SPDX licence; NFR-15 fails CI on \
         a bundled asset whose licence nobody has looked at."
    );
}

#[test]
fn the_scan_finds_the_faces_that_are_there() {
    let found = licences::vendored_files(&root()).expect("the tree is readable");
    assert!(
        found
            .iter()
            .any(|f| f.ends_with("assets/fonts/inter-variable.woff2")),
        "the scan missed the bundled faces, so an empty result proves nothing: {found:?}"
    );
}

#[test]
fn no_row_names_a_file_that_is_not_there() {
    let missing = licences::rows_without_a_file(&root()).expect("the inventory parses");
    assert!(missing.is_empty(), "xtask/assets.toml names {missing:?}");
}

#[test]
fn every_inventoried_licence_is_allowed_where_the_row_puts_it() {
    let named: BTreeSet<&str> = ALLOWED.iter().map(|l| l.spdx).collect();
    for asset in licences::inventory(&root()).expect("the inventory parses") {
        for spdx in asset.licences() {
            assert!(
                named.contains(spdx),
                "{} is inventoried under {spdx}, which NFR-15 does not allow",
                asset.name
            );
            let licence = ALLOWED
                .iter()
                .find(|l| l.spdx == spdx)
                .expect("just checked");
            assert!(
                !(asset.bundled && licence.scope == Scope::OnDemandOnly),
                "{} is bundled under {spdx}, which NFR-15 admits for an on-demand \
                 download only",
                asset.name
            );
        }
    }
}

#[test]
fn a_bundled_font_ships_its_licence_text() {
    let inventory = licences::inventory(&root()).expect("the inventory parses");
    let fonts: Vec<_> = inventory
        .iter()
        .filter(|a| a.bundled && a.licences().contains(&"OFL-1.1") && !a.paths.is_empty())
        .collect();
    assert_eq!(
        fonts.len(),
        2,
        "the two bundled faces are what §34.11 lists"
    );
    for font in fonts {
        assert!(
            font.notice.is_some(),
            "{} ships under OFL 1.1, which requires its licence text beside it",
            font.name
        );
    }
}

// ---- THIRD_PARTY_LICENSES.md ----

#[test]
fn the_notices_file_lists_the_assets_the_inventory_does() {
    let drift = xtask::notices::asset_section_matches(&root()).expect("the file is readable");
    assert!(drift.is_none(), "{}", drift.unwrap_or_default());
}

#[test]
fn a_bundled_row_and_an_on_demand_row_read_differently() {
    use xtask::licences::Asset;
    use xtask::notices;

    let section = notices::asset_section(&[
        Asset {
            name: "Shipped face".to_owned(),
            source: "somewhere".to_owned(),
            licence: "OFL-1.1".to_owned(),
            bundled: true,
            paths: vec!["a/b.woff2".to_owned()],
            notice: Some("a/LICENSE.txt".to_owned()),
        },
        Asset {
            name: "Fetched dictionary".to_owned(),
            source: "elsewhere".to_owned(),
            licence: "IPADIC".to_owned(),
            bundled: false,
            paths: Vec::new(),
            notice: None,
        },
    ]);
    assert!(
        section.contains("| Shipped face | somewhere | OFL-1.1 | yes | `a/LICENSE.txt` |"),
        "{section}"
    );
    assert!(
        section.contains("| Fetched dictionary | elsewhere | IPADIC | on demand | — |"),
        "{section}"
    );
    assert!(section.contains(notices::ASSETS_BEGIN) && section.contains(notices::ASSETS_END));
}

#[test]
fn a_crate_that_declares_no_licence_is_not_rendered_as_a_blank() {
    use xtask::notices::{self, Crate};

    let rendered = notices::render(
        &[],
        &[
            Crate {
                name: "declared".to_owned(),
                version: "1.0.0".to_owned(),
                licence: Some("MIT".to_owned()),
            },
            Crate {
                name: "silent".to_owned(),
                version: "0.1.0".to_owned(),
                licence: None,
            },
        ],
    );
    assert!(
        rendered.contains("| declared | 1.0.0 | MIT |"),
        "{rendered}"
    );
    assert!(
        rendered.contains("| silent | 0.1.0 | **none declared** |"),
        "{rendered}"
    );
}

#[test]
fn the_metadata_reader_drops_this_workspace_and_keeps_the_rest() {
    let mut packages: Vec<serde_json::Value> = (0..25)
        .map(|n| {
            serde_json::json!({
                "id": format!("registry+https://x#dep-{n}@1.0.0"),
                "name": format!("dep-{n}"),
                "version": "1.0.0",
                "license": "MIT",
            })
        })
        .collect();
    packages.push(serde_json::json!({
        "id": "path+file:///w/crates/liyasa-core#0.1.0",
        "name": "liyasa-core",
        "version": "0.1.0",
        "license": "MIT OR Apache-2.0",
    }));
    let json = serde_json::json!({
        "workspace_members": ["path+file:///w/crates/liyasa-core#0.1.0"],
        "packages": packages,
    });
    let crates = xtask::notices::from_metadata(&json).expect("the metadata parses");
    assert_eq!(
        crates.len(),
        25,
        "the workspace's own crate is not third party"
    );
    assert!(!crates.iter().any(|c| c.name == "liyasa-core"));
    assert!(crates.iter().any(|c| c.name == "dep-0"));
}

#[test]
fn a_metadata_document_that_parsed_almost_nothing_is_an_error() {
    // A silent near-empty result would write a notices file that says Liyasa
    // depends on three crates, which is worse than no file.
    let json: serde_json::Value = serde_json::json!({
        "workspace_members": [],
        "packages": [{"id": "a", "name": "one", "version": "1.0.0", "license": "MIT"}],
    });
    assert!(xtask::notices::from_metadata(&json).is_err());
}
