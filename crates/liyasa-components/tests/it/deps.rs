//! What each component contributes to the truth graph (§14.12, CMP-91).

use liyasa_components::{Registry, inst};
use liyasa_core::components::{ComponentInst, ComponentRegistry};
use liyasa_core::document::{Dep, DepTarget, EdgeKind, EdgeOrigin};
use liyasa_core::ids::{FactId, Route};

fn str(value: &str) -> liyasa_core::document::PropValue {
    liyasa_core::document::PropValue::Str(value.to_owned())
}

fn deps_of(inst: &ComponentInst) -> Vec<Dep> {
    Registry::builtins()
        .resolve(&inst.name)
        .unwrap_or_else(|| panic!("`{}` is not registered", inst.name))
        .deps(inst)
}

fn targets(inst: &ComponentInst) -> Vec<(DepTarget, EdgeKind)> {
    deps_of(inst)
        .into_iter()
        .map(|edge| (edge.to, edge.kind))
        .collect()
}

#[test]
fn every_instance_documents_the_component_it_is() {
    for name in Registry::builtins().names() {
        let inst = inst::new(name).build();
        let edges = deps_of(&inst);
        assert!(
            edges
                .iter()
                .any(|edge| edge.to == DepTarget::Component(name.to_owned())
                    && edge.kind == EdgeKind::Documents),
            "`{name}` must record which component it is"
        );
    }
}

#[test]
fn an_edge_carries_the_block_it_came_from() {
    let card = inst::new("card").prop("href", str("/start")).build();
    for edge in deps_of(&card) {
        match edge.from {
            EdgeOrigin::Block(_, block) => assert_eq!(block, card.id),
            EdgeOrigin::Page(_) => panic!("a component's edge starts at its block"),
        }
    }
}

#[test]
fn a_route_prop_is_a_link_and_an_asset_prop_is_an_embed() {
    let card = inst::new("card")
        .prop("href", str("/start"))
        .prop("img", str("/img/start.png"))
        .build();
    let found = targets(&card);
    assert!(
        found.contains(&(DepTarget::Page(Route::new("/start")), EdgeKind::Links)),
        "{found:?}"
    );
    assert!(
        found.contains(&(
            DepTarget::Asset("/img/start.png".to_owned()),
            EdgeKind::Embeds
        )),
        "{found:?}"
    );
}

#[test]
fn an_external_url_is_not_a_route() {
    let card = inst::new("card")
        .prop("href", str("https://example.com/x"))
        .build();
    assert!(
        targets(&card).contains(&(
            DepTarget::ExternalUrl("https://example.com/x".to_owned()),
            EdgeKind::Links
        )),
        "{:?}",
        targets(&card)
    );
}

#[test]
fn an_empty_or_fragment_prop_is_no_edge() {
    let card = inst::new("card")
        .prop("href", str("#section"))
        .prop("img", str(""))
        .build();
    assert_eq!(
        deps_of(&card).len(),
        1,
        "only the component edge: {:?}",
        targets(&card)
    );
}

#[test]
fn a_fact_reads_the_fact_it_names() {
    let fact = inst::new("fact").prop("id", str("pricing.pro")).build();
    assert!(
        targets(&fact).contains(&(DepTarget::Fact(FactId::new("pricing.pro")), EdgeKind::Reads)),
        "{:?}",
        targets(&fact)
    );
}

#[test]
fn a_snippet_includes_the_file_it_reads() {
    let snippet = inst::new("snippet-from")
        .prop("file", str("src/lib.rs"))
        .build();
    assert!(
        targets(&snippet).contains(&(
            DepTarget::Source("src/lib.rs".to_owned()),
            EdgeKind::Includes
        )),
        "{:?}",
        targets(&snippet)
    );

    let from_repo = inst::new("snippet-from")
        .prop("file", str("src/lib.rs"))
        .prop("repo", str("kasecrab/liyasa"))
        .build();
    assert!(
        targets(&from_repo).contains(&(
            DepTarget::Source("kasecrab/liyasa:src/lib.rs".to_owned()),
            EdgeKind::Includes
        )),
        "{:?}",
        targets(&from_repo)
    );
}

#[test]
fn an_endpoint_includes_its_operation_only_when_both_halves_are_given() {
    let full = inst::new("endpoint")
        .prop("spec", str("main"))
        .prop("operation", str("listItems"))
        .build();
    assert!(
        targets(&full).contains(&(
            DepTarget::Operation {
                spec: "main".to_owned(),
                op: "listItems".to_owned()
            },
            EdgeKind::Includes
        )),
        "{:?}",
        targets(&full)
    );

    let manual = inst::new("endpoint").prop("path", str("/v1/items")).build();
    assert_eq!(deps_of(&manual).len(), 1, "{:?}", targets(&manual));
}

#[test]
fn a_spec_schema_includes_the_schema_it_renders() {
    let schema = inst::new("openapi-schema")
        .prop("spec", str("main"))
        .prop("schema", str("Item"))
        .build();
    assert!(
        targets(&schema).contains(&(
            DepTarget::Operation {
                spec: "main".to_owned(),
                op: "schema:Item".to_owned()
            },
            EdgeKind::Includes
        )),
        "{:?}",
        targets(&schema)
    );
}

#[test]
fn a_screenshot_is_what_the_verifier_tracks() {
    let shot = inst::new("screenshot")
        .prop("src", str("/shots/builds.png"))
        .prop("alt", str("Builds"))
        .build();
    assert!(
        targets(&shot).contains(&(
            DepTarget::Screenshot("/shots/builds.png".to_owned()),
            EdgeKind::Embeds
        )),
        "{:?}",
        targets(&shot)
    );
}

#[test]
fn an_embed_and_a_repo_card_record_what_they_fetch() {
    let embed = inst::new("embed")
        .prop("url", str("https://www.youtube.com/watch?v=abc"))
        .build();
    assert!(
        targets(&embed).contains(&(
            DepTarget::ExternalUrl("https://www.youtube.com/watch?v=abc".to_owned()),
            EdgeKind::Embeds
        )),
        "{:?}",
        targets(&embed)
    );

    let repo = inst::new("github")
        .prop("repo", str("kasecrab/liyasa"))
        .build();
    assert!(
        targets(&repo).contains(&(
            DepTarget::ExternalUrl("https://api.github.com/repos/kasecrab/liyasa".to_owned()),
            EdgeKind::Embeds
        )),
        "{:?}",
        targets(&repo)
    );
}

#[test]
fn a_user_component_contributes_edges_from_its_own_schema() {
    use liyasa_components::user::UserComponent;

    const LINKED: &str = "{# ---\nprops:\n  to:\n    type: route\n    doc: Where it points.\n--- #}\n<a href=\"{{ props.to }}\">x</a>\n";
    let mut registry = Registry::builtins();
    registry.add(UserComponent::parse("linked-box", LINKED).expect("parses"));
    let inst = inst::new("linked-box").prop("to", str("/guides")).build();
    let edges = registry
        .resolve("linked-box")
        .expect("registered")
        .deps(&inst);
    assert!(
        edges
            .iter()
            .any(|edge| edge.to == DepTarget::Page(Route::new("/guides"))
                && edge.kind == EdgeKind::Links),
        "{edges:?}"
    );
}
