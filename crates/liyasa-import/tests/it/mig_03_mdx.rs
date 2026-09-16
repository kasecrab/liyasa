//! MIG-03. Given an arbitrary MDX tree with components Liyasa does not know;
//! when it is imported; then every page converts, the mapping decides what each
//! unknown component becomes, and the default answer leaves a project that
//! builds: a component stub per unknown name, declaring the props the tree used
//! it with.

use std::collections::BTreeMap;

use liyasa_config::vfs::MemVfs;
use liyasa_core::vfs::VfsPath;
use liyasa_import::mdx::{self, Choice, LeaveAll, Mapping, Stubs};

use crate::support::{Builtins, config, pages_scan, paths, text_at, validate};

const INDEX: &str = "---\ntitle: Home\n---\n\n# Acme\n\n<PricingTable plan=\"team\" highlight />\n\n<Note>Standard component.</Note>\n";

const GUIDE: &str = "---\ntitle: Guide\n---\n\n<Callout2 tone=\"loud\">\nRead this.\n</Callout2>\n\n<PricingTable plan=\"solo\" />\n";

fn project() -> MemVfs {
    MemVfs::new()
        .with("index.mdx", INDEX.as_bytes().to_vec())
        .with("guide.mdx", GUIDE.as_bytes().to_vec())
        .with("images/logo.svg", b"<svg/>".to_vec())
}

fn import_with(mapping: &dyn Mapping) -> liyasa_import::Plan {
    mdx::import(
        &project(),
        &VfsPath::new(""),
        &mdx::Options {
            components: &Builtins::default(),
            mapping,
            directives: false,
            name: "Acme",
        },
    )
}

/// What the CLI's prompt does, answered from a table instead of a terminal.
struct Table(BTreeMap<String, Choice>);

impl Mapping for Table {
    fn choose(&self, name: &str) -> Choice {
        self.0.get(name).cloned().unwrap_or(Choice::Stub)
    }
}

#[test]
fn every_page_converts_and_keeps_its_place() {
    let plan = import_with(&Stubs);
    assert!(paths(&plan).contains(&"index.md"), "{:?}", paths(&plan));
    assert!(paths(&plan).contains(&"guide.md"));
    assert!(paths(&plan).contains(&"images/logo.svg"));
    assert_eq!(plan.report.pages.len(), 2);
}

#[test]
fn the_default_answer_writes_a_stub_per_unknown_component() {
    let plan = import_with(&Stubs);
    let stub = text_at(&plan, "components/pricing-table.jinja");

    assert!(stub.contains("name: pricing-table"), "{stub}");
    assert!(stub.contains("aliases: [\"PricingTable\"]"), "{stub}");
    assert!(stub.contains("kind: leaf"), "{stub}");
    assert!(stub.contains("  plan: { type: string }"), "{stub}");
    assert!(
        stub.contains("  highlight: { type: string }"),
        "a prop used on only one page is still declared: {stub}"
    );
    assert!(paths(&plan).contains(&"components/callout2.jinja"));
}

#[test]
fn a_component_that_wrapped_content_becomes_a_container() {
    let plan = import_with(&Stubs);
    let stub = text_at(&plan, "components/callout2.jinja");
    assert!(stub.contains("kind: container"), "{stub}");
    assert!(stub.contains("{{ content }}"), "{stub}");
    assert!(stub.contains("{{ props.tone }}"), "{stub}");
}

#[test]
fn a_stubbed_component_costs_the_page_no_confidence() {
    let plan = import_with(&Stubs);
    for page in &plan.report.pages {
        assert!(
            page.is_clean(),
            "{} was not clean: {:?}",
            page.from,
            page.attention
        );
        assert_eq!(page.confidence(), 100);
    }
}

#[test]
fn the_mapping_can_send_a_component_to_a_built_in_instead() {
    let mapping = Table(
        [("Callout2".to_owned(), Choice::Use("Callout".to_owned()))]
            .into_iter()
            .collect(),
    );
    let plan = import_with(&mapping);
    let guide = text_at(&plan, "guide.md");

    assert!(guide.contains("<Callout tone=\"loud\">"), "{guide}");
    assert!(!paths(&plan).contains(&"components/callout2.jinja"));
    assert!(
        paths(&plan).contains(&"components/pricing-table.jinja"),
        "an unanswered component still gets the default"
    );
}

#[test]
fn leaving_a_component_alone_reports_it_instead_of_hiding_it() {
    let plan = import_with(&LeaveAll);
    assert!(
        !paths(&plan)
            .iter()
            .any(|path| path.starts_with("components/")),
        "{:?}",
        paths(&plan)
    );
    let named: Vec<&str> = plan
        .report
        .pages
        .iter()
        .flat_map(|page| page.attention.iter())
        .map(|item| item.what.as_str())
        .collect();
    assert!(named.contains(&"PricingTable"), "{named:?}");
    assert!(named.contains(&"Callout2"), "{named:?}");
}

#[test]
fn a_tree_with_no_config_gets_one() {
    let plan = import_with(&Stubs);
    let config = config(&plan);
    assert_eq!(config["name"], "Acme");
    assert_eq!(config["navigation"]["autofill"], true);
}

#[test]
fn a_tree_that_already_has_a_config_keeps_it() {
    let source = MemVfs::new()
        .with("liyasa.json", br#"{"name":"Mine"}"#.to_vec())
        .with("index.mdx", b"# Hi\n".to_vec());
    let plan = mdx::import(
        &source,
        &VfsPath::new(""),
        &mdx::Options {
            components: &Builtins::default(),
            mapping: &Stubs,
            directives: false,
            name: "Ignored",
        },
    );
    assert_eq!(
        plan.get("liyasa.json"),
        Some(&liyasa_import::Content::Copy(VfsPath::new("liyasa.json"))),
        "an existing config was rewritten instead of carried"
    );
}

#[test]
fn the_imported_project_loads_and_validates() {
    let source = project();
    let plan = mdx::import(
        &source,
        &VfsPath::new(""),
        &mdx::Options {
            components: &Builtins::default(),
            mapping: &Stubs,
            directives: false,
            name: "Acme",
        },
    );
    let problems = validate(&plan, &source);
    assert!(problems.is_empty(), "{problems:?}");
    pages_scan(&plan);
}
