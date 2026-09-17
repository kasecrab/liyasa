//! What is offered where. The cursor is written `|` in each fixture and the
//! helper cuts it out, so a test reads as the buffer the author is looking at.

use liyasa_config::vfs::MemVfs;
use liyasa_core::components::ComponentRegistry;
use liyasa_lsp::completion::{self, Context};
use liyasa_lsp::protocol::PositionEncoding::Utf16;
use liyasa_lsp::text::Text;
use liyasa_lsp::workspace::Workspace;

/// Splits a fixture at `|` into its text and the cursor's byte offset.
fn cursor(fixture: &str) -> (Text, u32) {
    let at = fixture
        .find('|')
        .expect("the fixture marks the cursor with |");
    (
        Text::new(fixture.replace('|', "")),
        u32::try_from(at).expect("a fixture is short"),
    )
}

fn labels(fixture: &str, workspace: &Workspace) -> Vec<String> {
    let (text, at) = cursor(fixture);
    completion::at(&text, workspace, at, Utf16)
        .into_iter()
        .map(|item| item.label)
        .collect()
}

fn project() -> Workspace {
    Workspace::load(
        &MemVfs::new()
            .with(
                "liyasa.json",
                br#"{ "variables": { "product": "Acme", "support": { "email": "help@acme.test" } } }"#
                    .to_vec(),
            )
            .with("facts/pricing.json", br#"{ "pro": { "monthly_usd": 49 } }"#.to_vec())
            .with("snippets/legal/terms.md", b"Terms.\n".to_vec())
            .with("guides/install.md", b"# Install\n".to_vec())
            .with("index.md", b"# Home\n".to_vec()),
    )
}

// ---- components ----

#[test]
fn three_colons_offer_container_components_only() {
    let workspace = Workspace::new();
    let offered = labels(":::|\n", &workspace);
    assert!(offered.contains(&"note".to_owned()), "{offered:?}");
    assert!(
        !offered.contains(&"image".to_owned()),
        "`image` is a leaf and is not written with three colons: {offered:?}"
    );
}

#[test]
fn two_colons_offer_leaf_components_only() {
    let workspace = Workspace::new();
    let offered = labels("::|\n", &workspace);
    assert!(offered.contains(&"image".to_owned()), "{offered:?}");
    assert!(
        !offered.contains(&"note".to_owned()),
        "`note` is a container: {offered:?}"
    );
}

#[test]
fn a_partial_name_narrows_the_list() {
    let workspace = Workspace::new();
    let offered = labels(":::no|\n", &workspace);
    assert!(offered.contains(&"note".to_owned()), "{offered:?}");
    assert!(
        offered.iter().all(|label| label.starts_with("no")),
        "{offered:?}"
    );
}

#[test]
fn a_directive_indented_inside_a_list_still_completes() {
    let workspace = Workspace::new();
    let offered = labels("- item\n  :::no|\n", &workspace);
    assert!(offered.contains(&"note".to_owned()), "{offered:?}");
}

#[test]
fn a_finished_directive_name_offers_nothing_more() {
    let workspace = Workspace::new();
    assert!(labels(":::note |\n", &workspace).is_empty());
}

#[test]
fn a_completion_replaces_the_name_not_the_colons() {
    let workspace = Workspace::new();
    let (text, at) = cursor(":::no|\n");
    let item = completion::at(&text, &workspace, at, Utf16)
        .into_iter()
        .find(|item| item.label == "note")
        .expect("`note` is offered");
    let edit = item.text_edit.expect("a completion carries its edit");
    assert_eq!(edit.range.start.character, 3, "the colons stay");
    assert_eq!(edit.range.end.character, 5);
    assert_eq!(edit.new_text, "note");
}

// ---- props ----

#[test]
fn a_brace_offers_the_components_props() {
    let workspace = Workspace::new();
    let (text, at) = cursor(":::card{|\n");
    let context = completion::context_at(&text, at).expect("inside a prop list");
    assert!(
        matches!(&context, Context::Prop { component, .. } if component == "card"),
        "{context:?}"
    );
    let offered = labels(":::card{|\n", &workspace);
    assert!(!offered.is_empty(), "a card takes props");
}

#[test]
fn a_prop_already_written_is_not_offered_again() {
    let workspace = Workspace::new();
    let first = labels(":::card{|\n", &workspace);
    let prop = first.first().cloned().expect("a card takes props");
    let offered = labels(&format!(":::card{{{prop}=\"x\" |\n"), &workspace);
    assert!(
        !offered.contains(&prop),
        "`{prop}` was written already: {offered:?}"
    );
}

#[test]
fn a_closed_prop_list_offers_nothing() {
    let workspace = Workspace::new();
    assert!(labels(":::card{title=\"x\"}|\n", &workspace).is_empty());
}

#[test]
fn a_boolean_prop_offers_true_and_false() {
    let workspace = Workspace::new();
    let component = workspace
        .registry
        .names()
        .into_iter()
        .find(|name| {
            workspace.registry.resolve(name).is_some_and(|component| {
                component
                    .schema()
                    .props
                    .iter()
                    .any(|p| matches!(p.ty, liyasa_core::components::PropType::Bool))
            })
        })
        .expect("some component takes a boolean");
    let prop = workspace
        .registry
        .resolve(component)
        .and_then(|c| {
            c.schema()
                .props
                .iter()
                .find(|p| matches!(p.ty, liyasa_core::components::PropType::Bool))
                .map(|p| p.name)
        })
        .expect("the boolean prop is there");
    let offered = labels(&format!(":::{component}{{{prop}=|\n"), &workspace);
    assert_eq!(offered, vec!["true".to_owned(), "false".to_owned()]);
}

// ---- variables and facts ----

#[test]
fn an_expression_offers_variables_and_facts() {
    let workspace = project();
    let offered = labels("Cost: {{ |\n", &workspace);
    assert!(offered.contains(&"product".to_owned()), "{offered:?}");
    assert!(
        offered.contains(&"facts.pricing.pro.monthly_usd".to_owned()),
        "{offered:?}"
    );
}

#[test]
fn a_dotted_prefix_narrows_to_the_fact_namespace() {
    let workspace = project();
    let offered = labels("Cost: {{ facts.pricing.|\n", &workspace);
    assert!(
        offered
            .iter()
            .all(|label| label.starts_with("facts.pricing.")),
        "{offered:?}"
    );
    assert!(offered.contains(&"facts.pricing.pro.monthly_usd".to_owned()));
}

#[test]
fn a_completion_replaces_the_whole_dotted_path() {
    let workspace = project();
    let (text, at) = cursor("{{ facts.pri|\n");
    let item = completion::at(&text, &workspace, at, Utf16)
        .into_iter()
        .find(|item| item.label == "facts.pricing.pro.monthly_usd")
        .expect("the fact is offered");
    let edit = item.text_edit.expect("a completion carries its edit");
    assert_eq!(edit.range.start.character, 3, "from after `{{ `");
    assert_eq!(edit.new_text, "facts.pricing.pro.monthly_usd");
}

#[test]
fn a_closed_expression_is_not_an_expression_context() {
    let workspace = project();
    assert!(labels("{{ product }} |\n", &workspace).is_empty());
}

#[test]
fn a_statement_completes_names_too() {
    let workspace = project();
    let offered = labels("{% if prod|\n", &workspace);
    assert!(offered.contains(&"product".to_owned()), "{offered:?}");
}

// ---- snippets ----

#[test]
fn an_include_offers_snippet_names() {
    let workspace = project();
    let offered = labels("{% include \"|\n", &workspace);
    assert_eq!(offered, vec!["legal/terms".to_owned()]);
}

// ---- links ----

#[test]
fn a_link_target_offers_routes() {
    let workspace = project();
    let offered = labels("See [the guide](|) for more.\n", &workspace);
    assert!(
        offered.contains(&"/guides/install".to_owned()),
        "{offered:?}"
    );
    assert!(offered.contains(&"/".to_owned()), "{offered:?}");
}

#[test]
fn a_partial_route_narrows_the_list() {
    let workspace = project();
    let offered = labels("See [the guide](/gui|\n", &workspace);
    assert_eq!(offered, vec!["/guides/install".to_owned()]);
}

#[test]
fn a_closed_link_is_not_a_link_context() {
    let workspace = project();
    assert!(labels("See [the guide](/guides/install) |\n", &workspace).is_empty());
}

#[test]
fn ordinary_prose_offers_nothing() {
    let workspace = project();
    assert!(labels("Just a sentence.|\n", &workspace).is_empty());
}
