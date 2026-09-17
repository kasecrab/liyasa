//! The project index. Every load is best-effort: the point of these tests is
//! that a half-written project still answers.

use liyasa_config::vfs::MemVfs;
use liyasa_lsp::workspace::Workspace;

fn project() -> MemVfs {
    MemVfs::new()
        .with(
            "liyasa.json",
            br#"{ "name": "Acme", "variables": { "product": "Acme", "support": { "email": "help@acme.test" } } }"#
                .to_vec(),
        )
        .with(
            "facts/sources.toml",
            b"[[source]]\nid = \"pricing\"\nkind = \"file\"\npath = \"facts/pricing.json\"\n".to_vec(),
        )
        .with(
            "facts/pricing.json",
            br#"{ "pro": { "monthly_usd": 49 } }"#.to_vec(),
        )
        .with("snippets/legal/terms.md", b"Terms.\n".to_vec())
        .with("snippets/vars.yaml", b"region: eu\n".to_vec())
        .with("index.md", b"# Home\n".to_vec())
        .with("guides/install.md", b"---\ntitle: Install it\n---\n\nBody.\n".to_vec())
}

#[test]
fn an_empty_workspace_still_knows_the_built_in_components() {
    let workspace = Workspace::new();
    assert!(workspace.registry.resolve("note").is_some());
    assert!(workspace.facts.is_empty());
}

#[test]
fn variables_come_from_the_config_and_the_vars_file() {
    let workspace = Workspace::load(&project());
    assert_eq!(
        workspace.variables.get("product").and_then(|v| v.as_str()),
        Some("Acme")
    );
    assert_eq!(
        workspace
            .variables
            .get("support.email")
            .and_then(|v| v.as_str()),
        Some("help@acme.test"),
        "a nested variable is offered by its dotted path"
    );
    assert_eq!(
        workspace.variables.get("region").and_then(|v| v.as_str()),
        Some("eu"),
        "snippets/vars.yaml is a variable layer too (CM-12)"
    );
}

#[test]
fn a_fact_is_keyed_by_its_source_id_and_knows_its_file() {
    let workspace = Workspace::load(&project());
    let fact = workspace
        .facts
        .get("pricing.pro.monthly_usd")
        .expect("the fact is indexed under its dotted path");
    assert_eq!(fact.value, serde_json::json!(49));
    assert_eq!(fact.file.as_str(), "facts/pricing.json");
    assert!(
        workspace.facts.contains_key("pricing.pro"),
        "a container is offered as well as its leaves, so completion can walk in"
    );
}

#[test]
fn a_fact_file_with_no_row_in_sources_is_read_under_its_stem() {
    let vfs = MemVfs::new().with("facts/limits.json", br#"{ "seats": 25 }"#.to_vec());
    let workspace = Workspace::load(&vfs);
    assert!(workspace.facts.contains_key("limits.seats"));
}

#[test]
fn a_schema_file_is_not_a_fact_source() {
    let vfs = MemVfs::new()
        .with("facts/pricing.json", br#"{ "pro": 1 }"#.to_vec())
        .with(
            "facts/pricing.schema.json",
            br#"{ "type": "object" }"#.to_vec(),
        );
    let workspace = Workspace::load(&vfs);
    assert!(workspace.facts.contains_key("pricing.pro"));
    assert!(
        !workspace
            .facts
            .keys()
            .any(|k| k.starts_with("pricing.schema")),
        "the schema describes the fact; it is not one"
    );
}

#[test]
fn snippets_are_named_by_their_path_without_the_extension() {
    let workspace = Workspace::load(&project());
    let snippet = workspace
        .snippets
        .get("legal/terms")
        .expect("a nested snippet keeps its directory in its name");
    assert_eq!(snippet.file.as_str(), "snippets/legal/terms.md");
}

#[test]
fn pages_are_indexed_by_route_with_their_titles() {
    let workspace = Workspace::load(&project());
    assert!(
        workspace.pages.contains_key("/"),
        "{:?}",
        workspace.pages.keys().collect::<Vec<_>>()
    );
    let install = workspace
        .pages
        .get("/guides/install")
        .expect("a nested page is routed by its path");
    assert_eq!(install.title.as_deref(), Some("Install it"));
}

#[test]
fn a_file_under_snippets_is_not_a_page() {
    let workspace = Workspace::load(&project());
    assert!(
        !workspace
            .pages
            .values()
            .any(|p| p.file.as_str().starts_with("snippets/")),
        "CM-03: snippets/ holds no routes"
    );
}

#[test]
fn a_config_that_does_not_parse_leaves_the_rest_loaded() {
    let vfs = MemVfs::new()
        .with("liyasa.json", b"{ this is not json".to_vec())
        .with("facts/limits.json", br#"{ "seats": 25 }"#.to_vec())
        .with("index.md", b"# Home\n".to_vec());
    let workspace = Workspace::load(&vfs);
    assert!(workspace.variables.is_empty());
    assert!(workspace.facts.contains_key("limits.seats"));
    assert!(workspace.pages.contains_key("/"));
}
