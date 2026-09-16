//! MIG-01. Given a Mintlify project with `docs.json`, MDX pages, snippets, a
//! `.mintignore`, and an OpenAPI spec carrying `x-mint`; when it is imported;
//! then the config, navigation, theme, colours, fonts, redirects, integrations,
//! versions, and locales reach `liyasa.json`, the pages become Liyasa Markdown
//! with their tag form intact, the snippets reach `snippets/`, `x-mint` becomes
//! `x-liyasa`, `.mintignore` becomes `.liyasaignore`, and the report names
//! every construct that needs a human and nothing else.

use liyasa_config::vfs::MemVfs;
use liyasa_core::vfs::VfsPath;
use liyasa_import::mintlify;
use liyasa_import::report::Kind;
use liyasa_import::stubs::Stubs;

use crate::support::{Builtins, config, pages_scan, paths, text_at, validate};

const DOCS_JSON: &str = r##"{
  "$schema": "https://mintlify.com/docs.json",
  "theme": "maple",
  "name": "Acme Docs",
  "description": "Everything about Acme.",
  "colors": { "primary": "#16A34A", "light": "#07C983", "dark": "#15803D" },
  "logo": { "light": "/logo/light.svg", "dark": "/logo/dark.svg" },
  "favicon": "/favicon.svg",
  "fonts": { "heading": { "family": "Inter", "weight": 600 }, "body": { "family": "Inter" } },
  "icons": { "library": "lucide" },
  "appearance": { "default": "system" },
  "navbar": {
    "links": [{ "label": "Support", "href": "https://acme.example/support" }],
    "primary": { "type": "button", "label": "Dashboard", "href": "https://acme.example/app" }
  },
  "footer": { "socials": { "x": "https://x.com/acme", "github": "https://github.com/acme" } },
  "banner": { "content": "Acme 2.0 is out", "dismissible": true },
  "redirects": [{ "source": "/old-quickstart", "destination": "/quickstart" }],
  "integrations": { "ga4": { "measurementId": "G-XYZ" } },
  "seo": { "indexing": "navigable" },
  "search": { "prompt": "Search Acme" },
  "api": { "openapi": "api-reference/openapi.json", "playground": { "display": "interactive" } },
  "contextual": { "options": ["copy", "chatgpt"] },
  "navigation": {
    "tabs": [
      {
        "tab": "Guides",
        "icon": "book",
        "groups": [
          { "group": "Get started", "pages": ["index", "quickstart"] },
          { "group": "Essentials", "pages": ["essentials/settings"] }
        ]
      },
      {
        "tab": "API reference",
        "groups": [{ "group": "Endpoints", "openapi": "api-reference/openapi.json" }]
      }
    ],
    "global": {
      "anchors": [{ "anchor": "Community", "href": "https://acme.example/slack", "icon": "slack" }]
    }
  }
}
"##;

const INDEX: &str = "---\ntitle: Acme Docs\ndescription: Start here\n---\n\n# Welcome\n\n<CardGroup cols={2}>\n  <Card title=\"Quickstart\" icon=\"rocket\" href=\"/quickstart\">\n    Ship in five minutes.\n  </Card>\n</CardGroup>\n";

const QUICKSTART: &str = "---\ntitle: Quickstart\nsidebarTitle: Quick start\nicon: rocket\n---\n\nimport AuthNote from '/snippets/auth-note.mdx'\n\n## Install\n\n<Steps>\n  <Step title=\"Install the CLI\">\n    Run it.\n  </Step>\n</Steps>\n\n<AuthNote />\n\n<Note>Keep your key secret.</Note>\n";

const SETTINGS: &str = "---\ntitle: Settings\n---\n\nexport const version = \"2.4\"\n\nAcme {version} reads `acme.json`.\n\n<PricingTable plan=\"team\" />\n\n{rows.map(row => row.name)}\n";

const SPEC: &str = r##"{
  "openapi": "3.1.0",
  "info": { "title": "Acme", "version": "1.0.0" },
  "paths": {
    "/widgets": {
      "get": {
        "operationId": "listWidgets",
        "x-mint": { "href": "/api-reference/widgets" },
        "responses": { "200": { "description": "ok" } }
      }
    }
  }
}
"##;

fn project() -> MemVfs {
    MemVfs::new()
        .with("docs.json", DOCS_JSON.as_bytes().to_vec())
        .with("index.mdx", INDEX.as_bytes().to_vec())
        .with("quickstart.mdx", QUICKSTART.as_bytes().to_vec())
        .with("essentials/settings.mdx", SETTINGS.as_bytes().to_vec())
        .with("api-reference/openapi.json", SPEC.as_bytes().to_vec())
        .with(
            "snippets/auth-note.mdx",
            "Use a scoped key.\n".as_bytes().to_vec(),
        )
        .with(".mintignore", "drafts/\n".as_bytes().to_vec())
        .with("images/hero.png", b"\x89PNG".to_vec())
        .with(
            "node_modules/mintlify/index.js",
            b"module.exports={}".to_vec(),
        )
}

fn import(vfs: &MemVfs) -> liyasa_import::Plan {
    mintlify::import(
        vfs,
        &VfsPath::new(""),
        &mintlify::Options {
            components: &Builtins::default(),
            directives: false,
            mapping: &Stubs,
        },
    )
}

#[test]
fn the_config_carries_everything_mig_01_names() {
    let plan = import(&project());
    let config = config(&plan);

    assert_eq!(config["name"], "Acme Docs");
    assert_eq!(config["description"], "Everything about Acme.");
    assert_eq!(config["theme"]["colors"]["primary"], "#16A34A");
    assert_eq!(config["theme"]["colors"]["light"], "#07C983");
    assert_eq!(config["theme"]["colors"]["dark"], "#15803D");
    assert_eq!(config["theme"]["fonts"]["heading"]["family"], "Inter");
    assert_eq!(config["theme"]["fonts"]["heading"]["weight"], 600);
    assert_eq!(config["theme"]["icons"]["library"], "lucide");
    assert_eq!(config["theme"]["appearance"]["default"], "system");
    assert_eq!(config["logo"]["light"], "/logo/light.svg");
    assert_eq!(config["favicon"]["light"], "/favicon.svg");
    assert_eq!(config["navbar"]["links"][0]["label"], "Support");
    assert_eq!(
        config["navbar"]["primary"]["href"],
        "https://acme.example/app"
    );
    assert_eq!(config["footer"]["socials"]["x"], "https://x.com/acme");
    assert_eq!(config["banner"]["content"], "Acme 2.0 is out");
    assert_eq!(config["redirects"][0]["source"], "/old-quickstart");
    assert_eq!(config["integrations"]["ga4"]["measurementId"], "G-XYZ");
    assert_eq!(config["seo"]["indexing"], "navigable");
    assert_eq!(config["search"]["placeholder"], "Search Acme");
    assert_eq!(config["openapi"][0], "api-reference/openapi.json");
}

#[test]
fn a_tab_holding_groups_becomes_a_tab_holding_pages() {
    let plan = import(&project());
    let config = config(&plan);
    let tabs = &config["navigation"]["tabs"];

    assert_eq!(tabs[0]["tab"], "Guides");
    assert_eq!(tabs[0]["icon"], "book");
    assert_eq!(tabs[0]["pages"][0]["group"], "Get started");
    assert_eq!(tabs[0]["pages"][0]["pages"][0], "index");
    assert_eq!(tabs[0]["pages"][1]["group"], "Essentials");
    assert_eq!(
        tabs[0]["pages"][0].get("groups"),
        None,
        "a Mintlify `groups` array of objects is children, not a reader-group gate"
    );
    assert_eq!(
        tabs[1]["pages"][0]["pages"][0]["openapi"], "api-reference/openapi.json",
        "a spec named beside a group is a node of its own"
    );
}

#[test]
fn a_global_anchor_stays_an_anchor() {
    let plan = import(&project());
    let config = config(&plan);
    let anchor = &config["navigation"]["pages"][0];
    assert_eq!(anchor["anchor"], "Community");
    assert_eq!(anchor["href"], "https://acme.example/slack");
    assert_eq!(anchor["icon"], "slack");
}

#[test]
fn pages_become_markdown_at_the_same_path_with_their_tag_form_intact() {
    let plan = import(&project());
    assert!(paths(&plan).contains(&"index.md"), "{:?}", paths(&plan));
    assert!(paths(&plan).contains(&"essentials/settings.md"));
    assert!(!paths(&plan).contains(&"index.mdx"));

    let index = text_at(&plan, "index.md");
    assert!(index.contains("<CardGroup cols={2}>"), "{index}");
    assert!(index.contains("<Card title=\"Quickstart\" icon=\"rocket\" href=\"/quickstart\">"));
    assert!(index.starts_with("---\ntitle: Acme Docs\n"));
}

#[test]
fn a_snippet_import_becomes_a_snippet_include_and_the_file_comes_along() {
    let plan = import(&project());
    let quickstart = text_at(&plan, "quickstart.md");
    assert!(
        quickstart.contains("{% snippet \"auth-note\" %}"),
        "{quickstart}"
    );
    assert!(!quickstart.contains("import AuthNote"));
    assert!(paths(&plan).contains(&"snippets/auth-note.md"));
}

#[test]
fn an_exported_literal_becomes_front_matter() {
    let plan = import(&project());
    let settings = text_at(&plan, "essentials/settings.md");
    assert!(settings.contains("version: \"2.4\""), "{settings}");
    assert!(settings.contains("Acme {{ version }} reads"));
}

#[test]
fn the_mintignore_becomes_a_liyasaignore() {
    let plan = import(&project());
    assert!(
        paths(&plan).contains(&".liyasaignore"),
        "{:?}",
        paths(&plan)
    );
    assert!(!paths(&plan).contains(&".mintignore"));
}

#[test]
fn the_spec_extension_namespace_is_rewritten_and_nothing_else_moves() {
    let plan = import(&project());
    let spec = text_at(&plan, "api-reference/openapi.json");
    assert!(
        spec.contains("\"x-liyasa\": { \"href\": \"/api-reference/widgets\" }"),
        "{spec}"
    );
    assert!(!spec.contains("x-mint"));
    assert!(
        spec.contains("\"operationId\": \"listWidgets\""),
        "the rest of the spec was reformatted"
    );
}

#[test]
fn assets_are_carried_where_they_were_so_every_link_still_resolves() {
    let plan = import(&project());
    assert!(paths(&plan).contains(&"images/hero.png"));
    assert!(
        !paths(&plan)
            .iter()
            .any(|path| path.starts_with("node_modules")),
        "a toolchain directory was imported"
    );
    assert!(!paths(&plan).contains(&"docs.json"));
}

#[test]
fn the_report_names_the_constructs_that_need_a_human_and_no_others() {
    let plan = import(&project());
    let report = &plan.report;

    let settings = report
        .pages
        .iter()
        .find(|page| page.from.as_str() == "essentials/settings.mdx")
        .expect("the settings page is in the report");
    // TODO(rfc-2902): the custom component is a stub and one project-level
    // entry; what is left on the page is the expression, which is a page
    // someone has to open.
    let kinds: Vec<Kind> = settings.attention.iter().map(|item| item.kind).collect();
    assert_eq!(kinds, [Kind::Expression]);
    assert_eq!(settings.confidence(), 70);

    for page in &report.pages {
        if page.from.as_str() == "essentials/settings.mdx" {
            continue;
        }
        assert!(
            page.is_clean(),
            "{} was not clean: {:?}",
            page.from,
            page.attention
        );
    }

    let project: Vec<&str> = report
        .attention
        .iter()
        .map(|item| item.what.as_str())
        .collect();
    assert!(project.contains(&"PricingTable"), "{project:?}");
    assert!(
        plan.text_at("components/pricing-table.jinja")
            .is_some_and(|stub| stub.contains("  plan: { type: string }")),
        "the component Liyasa cannot render has no stub"
    );
    assert!(project.contains(&"theme: \"maple\""), "{project:?}");
    assert!(project.contains(&"contextual"), "{project:?}");
    assert!(project.contains(&"api.playground"), "{project:?}");
}

#[test]
fn the_report_is_written_beside_the_project() {
    let plan = import(&project());
    let report = text_at(&plan, "migration-report.md");
    assert!(report.starts_with("# mintlify import"));
    assert!(report.contains("4 pages, 3 clean (75.0%)"));
    assert!(report.contains("PricingTable"));
    assert!(report.contains("components/pricing-table.jinja"));
}

#[test]
fn the_imported_project_loads_and_validates() {
    let source = project();
    let plan = import(&source);
    let problems = validate(&plan, &source);
    assert!(problems.is_empty(), "{problems:?}");
    pages_scan(&plan);
}

#[test]
fn the_directive_form_is_offered() {
    let vfs = project();
    let plan = mintlify::import(
        &vfs,
        &VfsPath::new(""),
        &mintlify::Options {
            components: &Builtins::default(),
            directives: true,
            mapping: &Stubs,
        },
    );
    let index = text_at(&plan, "index.md");
    assert!(
        index.contains("::::cards{cols={{ 2 }}}") || index.contains(":::card{"),
        "{index}"
    );
    assert!(!index.contains("<Card "), "{index}");
}

#[test]
fn a_directory_that_is_not_a_mintlify_project_is_a_diagnostic_not_a_panic() {
    let vfs = MemVfs::new().with("README.md", b"# Nothing here".to_vec());
    let plan = import(&vfs);
    assert!(plan.is_empty());
    assert_eq!(
        plan.report
            .diagnostics
            .iter()
            .map(|d| d.code.as_str())
            .collect::<Vec<_>>(),
        ["E1101"]
    );
}

#[test]
fn a_broken_config_is_a_diagnostic_not_a_panic() {
    let vfs = MemVfs::new().with("docs.json", b"{ \"name\": ".to_vec());
    let plan = import(&vfs);
    assert_eq!(
        plan.report
            .diagnostics
            .iter()
            .map(|d| d.code.as_str())
            .collect::<Vec<_>>(),
        ["E1104"]
    );
}
