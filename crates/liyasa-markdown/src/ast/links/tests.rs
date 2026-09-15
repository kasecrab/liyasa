//! `spec/markdown/cm-35/` and `spec/markdown/cm-36/`.

use super::*;

#[test]
fn every_form_cm_36_lists_is_recognized() {
    assert_eq!(form_of("./other.md"), Form::Relative);
    assert_eq!(form_of("../up/other.md"), Form::Relative);
    assert_eq!(form_of("other.md"), Form::Relative);
    assert_eq!(form_of("/route"), Form::Route);
    assert_eq!(form_of("/getting-started/install"), Form::Route);
    assert_eq!(form_of("page:install"), Form::PageId);
    assert_eq!(form_of("#anchor"), Form::Anchor);
    assert_eq!(form_of("https://example.com"), Form::External);
    assert_eq!(form_of("mailto:a@example.com"), Form::External);
}

/// A colon in a path is not a scheme.
#[test]
fn a_colon_in_a_relative_path_is_not_a_scheme() {
    assert_eq!(form_of("./a:b.md"), Form::Relative);
    assert_eq!(form_of("a b:c.md"), Form::Relative);
}

#[test]
fn a_protocol_relative_reference_is_external() {
    assert_eq!(form_of("//cdn.example.com/x.png"), Form::External);
}

#[test]
fn surrounding_space_does_not_change_the_form() {
    assert_eq!(form_of("  /route  "), Form::Route);
    assert_eq!(form_of(" page:install "), Form::PageId);
}

#[test]
fn an_anchor_is_read_off_any_form() {
    assert_eq!(anchor_of("./other.md#install"), Some("install"));
    assert_eq!(anchor_of("page:guide#install"), Some("install"));
    assert_eq!(anchor_of("#install"), Some("install"));
    assert_eq!(anchor_of("/route"), None);
    assert_eq!(anchor_of("/route#"), None);
}

#[test]
fn a_page_id_is_read_without_its_scheme_or_anchor() {
    assert_eq!(page_id_of("page:install"), Some("install"));
    assert_eq!(page_id_of("page:install#step-2"), Some("install"));
    assert_eq!(page_id_of("page:"), None);
    assert_eq!(page_id_of("/route"), None);
}

/// CM-35: `image.png` pairs with `image.dark.png`.
#[test]
fn a_local_image_names_its_dark_twin() {
    assert_eq!(dark_variant("image.png"), Some("image.dark.png".to_owned()));
    assert_eq!(
        dark_variant("./assets/flow.svg"),
        Some("./assets/flow.dark.svg".to_owned())
    );
    assert_eq!(
        dark_variant("/assets/a.JPG"),
        Some("/assets/a.dark.JPG".to_owned())
    );
}

#[test]
fn the_dark_half_has_no_twin_of_its_own() {
    assert!(dark_variant("image.dark.png").is_none());
    assert!(dark_variant("image.DARK.png").is_none());
    assert!(is_dark("image.dark.png"));
    assert!(!is_dark("image.png"));
    assert!(!is_dark("darkness.png"));
}

#[test]
fn a_remote_image_and_a_non_image_have_no_twin() {
    assert!(dark_variant("https://example.com/a.png").is_none());
    assert!(dark_variant("data:image/png;base64,AAAA").is_none());
    assert!(dark_variant("./notes.md").is_none());
    assert!(dark_variant("./archive.tar.gz").is_none());
    assert!(dark_variant("noextension").is_none());
    assert!(dark_variant(".png").is_none());
    assert!(dark_variant("").is_none());
}

#[test]
fn the_extension_list_is_sorted_and_lowercase() {
    let mut sorted = IMAGE_EXTENSIONS.to_vec();
    sorted.sort_unstable();
    assert_eq!(sorted, IMAGE_EXTENSIONS);
    assert!(
        IMAGE_EXTENSIONS
            .iter()
            .all(|e| e.chars().all(char::is_lowercase))
    );
}

#[test]
fn malformed_references_never_panic() {
    for href in [
        "", " ", ":", "://", "#", "/", "page:", "🙂", "a:", ".", "..",
    ] {
        let _ = form_of(href);
        let _ = anchor_of(href);
        let _ = page_id_of(href);
        let _ = dark_variant(href);
    }
}
