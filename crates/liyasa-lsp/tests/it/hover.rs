//! What the card under the cursor says.

use liyasa_config::vfs::MemVfs;
use liyasa_lsp::hover;
use liyasa_lsp::protocol::PositionEncoding::Utf16;
use liyasa_lsp::text::Text;
use liyasa_lsp::workspace::Workspace;

fn cursor(fixture: &str) -> (Text, u32) {
    let at = fixture
        .find('|')
        .expect("the fixture marks the cursor with |");
    (
        Text::new(fixture.replace('|', "")),
        u32::try_from(at).expect("a fixture is short"),
    )
}

fn card(fixture: &str, workspace: &Workspace) -> Option<String> {
    let (text, at) = cursor(fixture);
    hover::at(&text, workspace, at, Utf16).map(|hover| hover.contents.value)
}

fn project() -> Workspace {
    Workspace::load(
        &MemVfs::new()
            .with(
                "liyasa.json",
                br#"{ "variables": { "product": "Acme" } }"#.to_vec(),
            )
            .with(
                "facts/pricing.json",
                b"{\n  \"pro\": {\n    \"monthly_usd\": 49\n  }\n}\n".to_vec(),
            )
            .with("snippets/legal/terms.md", b"Terms.\n".to_vec())
            .with(
                "guides/install.md",
                b"---\ntitle: Install it\n---\n".to_vec(),
            ),
    )
}

#[test]
fn a_component_name_shows_its_form_and_its_props() {
    let workspace = Workspace::new();
    let card = card(":::no|te\nBody.\n:::\n", &workspace).expect("a component has a card");
    assert!(card.contains("`note`"), "{card}");
    assert!(card.contains("container directive"), "{card}");
}

#[test]
fn hovering_a_component_marks_the_name_not_the_colons() {
    let workspace = Workspace::new();
    let (text, at) = cursor(":::no|te\n");
    let range = hover::at(&text, &workspace, at, Utf16)
        .and_then(|hover| hover.range)
        .expect("a hover carries the range it describes");
    assert_eq!(range.start.character, 3);
    assert_eq!(range.end.character, 7);
}

#[test]
fn a_prop_name_shows_its_type_and_whether_it_is_required() {
    let workspace = Workspace::new();
    let component = "card";
    let prop = workspace
        .registry
        .resolve(component)
        .and_then(|c| c.schema().props.first().map(|p| p.name))
        .expect("a card takes props");
    let card = card(
        &format!(":::{component}{{{}|{}=\"x\"}}\n", &prop[..1], &prop[1..]),
        &workspace,
    )
    .expect("a prop has a card");
    assert!(card.contains(prop), "{card}");
    assert!(card.contains(component), "{card}");
}

#[test]
fn a_fact_shows_its_value_and_the_file_it_comes_from() {
    let workspace = project();
    let card = card("Cost {{ facts.pricing.pro.mont|hly_usd }}.\n", &workspace)
        .expect("a fact has a card");
    assert!(card.contains("49"), "{card}");
    assert!(card.contains("facts/pricing.json"), "{card}");
}

#[test]
fn a_variable_shows_its_value() {
    let workspace = project();
    let card = card("Welcome to {{ prod|uct }}.\n", &workspace).expect("a variable has a card");
    assert!(card.contains("Acme"), "{card}");
    assert!(card.contains("variable"), "{card}");
}

#[test]
fn a_snippet_shows_the_file_it_lives_in() {
    let workspace = project();
    let card = card("{% include \"legal/te|rms\" %}\n", &workspace).expect("a snippet has a card");
    assert!(card.contains("snippets/legal/terms.md"), "{card}");
}

#[test]
fn a_link_target_shows_the_page_it_reaches() {
    let workspace = project();
    let card =
        card("See [the guide](/guides/ins|tall).\n", &workspace).expect("a route has a card");
    assert!(card.contains("Install it"), "{card}");
    assert!(card.contains("guides/install.md"), "{card}");
}

#[test]
fn a_name_nothing_defines_has_no_card() {
    let workspace = project();
    assert!(card("Welcome to {{ unkn|own }}.\n", &workspace).is_none());
    assert!(card(":::nonsu|ch\n", &workspace).is_none());
}

#[test]
fn ordinary_prose_has_no_card() {
    let workspace = project();
    assert!(card("Just a sen|tence.\n", &workspace).is_none());
}
