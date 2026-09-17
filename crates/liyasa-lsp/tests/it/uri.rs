//! `file:` URIs are the only names the wire has for a file.

use std::path::Path;

use liyasa_lsp::uri;

#[test]
fn an_absolute_path_round_trips() {
    let path = Path::new("/home/w/docs/install.md");
    let text = uri::from_path(path);
    assert_eq!(text, "file:///home/w/docs/install.md");
    assert_eq!(uri::to_path(&text).as_deref(), Some(path));
}

#[test]
fn a_space_is_percent_encoded_and_decoded() {
    let path = Path::new("/home/w/my docs/a b.md");
    let text = uri::from_path(path);
    assert!(text.contains("%20"), "{text}");
    assert_eq!(uri::to_path(&text).as_deref(), Some(path));
}

#[test]
fn a_non_ascii_name_round_trips_through_utf8_escapes() {
    let path = Path::new("/docs/día.md");
    assert_eq!(uri::to_path(&uri::from_path(path)).as_deref(), Some(path));
}

#[test]
fn a_localhost_authority_is_the_same_file() {
    assert_eq!(
        uri::to_path("file://localhost/docs/a.md").as_deref(),
        Some(Path::new("/docs/a.md"))
    );
}

#[test]
fn a_windows_drive_letter_loses_the_leading_slash() {
    assert_eq!(
        uri::to_path("file:///C:/docs/a.md").as_deref(),
        Some(Path::new("C:/docs/a.md"))
    );
}

#[test]
fn a_buffer_that_is_not_a_file_has_no_path() {
    // An editor can hold an unsaved buffer open; it is still a document the
    // server answers about, and it still has no path.
    assert!(uri::to_path("untitled:Untitled-1").is_none());
    assert!(uri::to_path("https://example.test/a.md").is_none());
}

#[test]
fn a_remote_authority_is_not_a_local_file() {
    assert!(uri::to_path("file://server/share/a.md").is_none());
}

#[test]
fn a_lone_per_cent_is_a_literal_not_a_broken_escape() {
    assert_eq!(
        uri::to_path("file:///docs/100%.md").as_deref(),
        Some(Path::new("/docs/100%.md"))
    );
}
