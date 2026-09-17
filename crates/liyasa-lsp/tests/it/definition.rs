//! Go to definition, which ED-61 requires for snippets and facts.

use std::path::Path;

use liyasa_config::vfs::MemVfs;
use liyasa_lsp::definition;
use liyasa_lsp::text::Text;
use liyasa_lsp::workspace::Workspace;

const ROOT: &str = "/w/docs";

fn cursor(fixture: &str) -> (Text, u32) {
    let at = fixture
        .find('|')
        .expect("the fixture marks the cursor with |");
    (
        Text::new(fixture.replace('|', "")),
        u32::try_from(at).expect("a fixture is short"),
    )
}

fn project() -> MemVfs {
    MemVfs::new()
        .with(
            "facts/pricing.json",
            b"{\n  \"pro\": {\n    \"monthly_usd\": 49\n  }\n}\n".to_vec(),
        )
        .with(
            "facts/limits.yaml",
            b"seats: 25\nregions:\n  - eu\n".to_vec(),
        )
        .with("snippets/legal/terms.md", b"Terms.\n".to_vec())
        .with("guides/install.md", b"# Install\n".to_vec())
}

fn go(fixture: &str) -> Option<liyasa_lsp::protocol::Location> {
    let vfs = project();
    let workspace = Workspace::load(&vfs);
    let (text, at) = cursor(fixture);
    definition::at(&text, &workspace, &vfs, Some(Path::new(ROOT)), at)
}

#[test]
fn a_snippet_goes_to_its_file() {
    let location = go("{% include \"legal/te|rms\" %}\n").expect("a snippet has a definition");
    assert_eq!(location.uri, "file:///w/docs/snippets/legal/terms.md");
    assert_eq!(location.range.start.line, 0);
}

#[test]
fn a_fact_goes_to_the_line_that_declares_its_key() {
    let location =
        go("Cost {{ facts.pricing.pro.month|ly_usd }}.\n").expect("a fact has a definition");
    assert_eq!(location.uri, "file:///w/docs/facts/pricing.json");
    assert_eq!(
        location.range.start.line, 2,
        "`\"monthly_usd\"` is on the third line"
    );
    assert_eq!(location.range.start.character, 4, "past its indentation");
}

#[test]
fn a_yaml_fact_is_found_by_its_bare_key() {
    let location = go("Seats: {{ facts.limits.se|ats }}.\n").expect("a yaml fact has a definition");
    assert_eq!(location.uri, "file:///w/docs/facts/limits.yaml");
    assert_eq!(location.range.start.line, 0);
}

#[test]
fn a_link_target_goes_to_the_page() {
    let location = go("See [the guide](/guides/ins|tall).\n").expect("a route has a definition");
    assert_eq!(location.uri, "file:///w/docs/guides/install.md");
}

#[test]
fn a_component_has_no_file_to_go_to() {
    // The built-ins are Rust; a user component's file is the loader's to know.
    assert!(go(":::no|te\n").is_none());
}

#[test]
fn a_name_nothing_defines_goes_nowhere() {
    assert!(go("{{ unkno|wn }}\n").is_none());
    assert!(go("See [it](/no/such/pa|ge).\n").is_none());
}

#[test]
fn without_a_project_root_there_is_no_file_to_point_at() {
    let vfs = project();
    let workspace = Workspace::load(&vfs);
    let (text, at) = cursor("{% include \"legal/te|rms\" %}\n");
    assert!(definition::at(&text, &workspace, &vfs, None, at).is_none());
}
