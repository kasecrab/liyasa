use std::collections::BTreeMap;
use std::sync::OnceLock;

use liyasa_components::Registry;
use liyasa_core::components::ComponentRegistry;

use super::*;

/// The component set the built-in registry really has, so a test that says a
/// name converts cleanly is saying it about the shipped product.
struct Builtins {
    rename: BTreeMap<String, String>,
    consumed: Vec<&'static str>,
}

impl Builtins {
    fn new() -> Self {
        Self {
            rename: BTreeMap::new(),
            consumed: Vec::new(),
        }
    }

    fn registry() -> &'static Registry {
        static REGISTRY: OnceLock<Registry> = OnceLock::new();
        REGISTRY.get_or_init(Registry::builtins)
    }
}

impl Convert for Builtins {
    fn known(&self, name: &str) -> bool {
        Self::registry().get(name).is_some()
    }

    fn suggest(&self, name: &str) -> Option<String> {
        Self::registry().suggest(name).map(str::to_owned)
    }

    fn boilerplate(&self, specifier: &str) -> bool {
        specifier.starts_with("@theme/")
    }

    fn frontmatter(&self) -> &BTreeMap<String, String> {
        &self.rename
    }

    fn consumed(&self) -> &[&str] {
        &self.consumed
    }
}

fn convert_with(source: &str, convert: &dyn Convert, directives: bool) -> Page {
    super::convert(
        source,
        &Options {
            convert,
            directives,
        },
    )
}

fn page(source: &str) -> Page {
    convert_with(source, &Builtins::new(), false)
}

fn codes(page: &Page) -> Vec<&str> {
    page.attention.iter().map(|a| a.kind.as_str()).collect()
}

#[test]
fn plain_markdown_survives_byte_for_byte() {
    let source = "# Title\n\nSome *prose* with `code` and a [link](/x).\n";
    let converted = page(source);
    assert_eq!(converted.text, source);
    assert!(converted.attention.is_empty());
}

#[test]
fn a_built_in_component_keeps_its_tag_form() {
    let converted = page("<Card title=\"Install\" icon=\"download\">\nbody\n</Card>\n");
    assert_eq!(
        converted.text,
        "<Card title=\"Install\" icon=\"download\">\nbody\n</Card>\n"
    );
    assert!(converted.attention.is_empty(), "{:?}", codes(&converted));
}

#[test]
fn a_multi_line_tag_is_joined_onto_one_line_for_the_formatter() {
    let converted = page("<Card\n  title=\"Install\"\n  href=\"/install\"\n>\nbody\n</Card>\n");
    assert_eq!(
        converted.text,
        "<Card title=\"Install\" href=\"/install\">\nbody\n</Card>\n"
    );
    assert!(converted.attention.is_empty());
}

#[test]
fn the_directive_form_is_offered_rather_than_forced() {
    let converted = convert_with(
        "<Card title=\"Install\">\nbody\n</Card>\n",
        &Builtins::new(),
        true,
    );
    assert_eq!(converted.text, ":::card{title=\"Install\"}\nbody\n:::\n");
}

#[test]
fn an_unknown_component_is_named_with_a_suggestion() {
    let converted = page("<Callot>\nbody\n</Callot>\n");
    assert_eq!(codes(&converted), ["custom component"]);
    assert_eq!(converted.attention[0].what, "Callot");
    assert_eq!(
        converted.attention[0].help.as_deref(),
        Some("did you mean `callout`?")
    );
    assert!(
        converted.text.contains("<Callot>"),
        "an unconvertible component was dropped rather than carried across"
    );
}

#[test]
fn a_custom_component_with_no_near_name_says_what_to_do_instead() {
    let converted = page("<PricingTable plan=\"team\" />\n");
    assert_eq!(codes(&converted), ["custom component"]);
    assert!(
        converted.attention[0]
            .help
            .as_deref()
            .is_some_and(|help| help.contains("components/"))
    );
}

#[test]
fn a_closing_tag_is_not_a_second_report() {
    let converted = page("<Nope>\nbody\n</Nope>\n");
    assert_eq!(codes(&converted), ["custom component"]);
}

#[test]
fn a_lowercase_tag_stays_html() {
    let source = "<div class=\"x\">\n<br/>\nbody\n</div>\n";
    assert_eq!(page(source).text, source);
}

#[test]
fn a_less_than_sign_in_prose_is_prose() {
    let source = "Use it when a < b and b > c.\n";
    let converted = page(source);
    assert_eq!(converted.text, source);
    assert!(converted.attention.is_empty());
}

#[test]
fn a_prop_value_containing_an_angle_bracket_does_not_end_the_tag() {
    let converted = page("<Note title=\"a > b\">\nbody\n</Note>\n");
    assert_eq!(converted.text, "<Note title=\"a > b\">\nbody\n</Note>\n");
    assert!(converted.attention.is_empty());
}

#[test]
fn an_mdx_comment_becomes_a_template_comment() {
    let converted = page("{/* a note */}\n\nbody\n");
    assert_eq!(converted.text, "{# a note #}\n\nbody\n");
    assert!(converted.attention.is_empty());
}

#[test]
fn a_javascript_expression_is_carried_across_and_reported() {
    let converted = page("Total: {items.map(i => i.name)}\n");
    assert_eq!(codes(&converted), ["JavaScript expression"]);
    assert!(converted.text.contains("{items.map(i => i.name)}"));
}

#[test]
fn a_liyasa_construct_is_not_an_mdx_expression() {
    let source = "{{ page.title }}\n\n{% if x %}y{% endif %}\n\n{# note #}\n";
    let converted = page(source);
    assert_eq!(converted.text, source);
    assert!(converted.attention.is_empty(), "{:?}", codes(&converted));
}

#[test]
fn an_exported_literal_becomes_front_matter_and_its_reads_become_template_output() {
    let converted = page("export const version = \"2.4\"\n\nRun v{version} now.\n");
    assert!(
        converted.text.starts_with("---\nversion: \"2.4\"\n---\n"),
        "{}",
        converted.text
    );
    assert!(converted.text.contains("Run v{{ version }} now."));
    assert!(converted.attention.is_empty(), "{:?}", codes(&converted));
    assert_eq!(converted.front["version"], serde_json::json!("2.4"));
}

#[test]
fn an_exported_expression_is_reported_rather_than_guessed_at() {
    let converted = page("export const rows = items.filter(Boolean)\n\nbody\n");
    assert_eq!(codes(&converted), ["import or export"]);
    assert!(!converted.text.contains("export const"));
}

#[test]
fn a_partial_import_becomes_a_snippet_include() {
    let converted =
        page("import AuthNote from '/snippets/auth-note.mdx'\n\n## Auth\n\n<AuthNote />\n");
    assert_eq!(converted.text, "\n## Auth\n\n{% snippet \"auth-note\" %}\n");
    assert_eq!(
        converted.snippets,
        [("/snippets/auth-note.mdx".to_owned(), "auth-note".to_owned())]
    );
    assert!(converted.attention.is_empty());
}

#[test]
fn a_partial_carries_its_props() {
    let converted =
        page("import Note from './_note.mdx'\n\n<Note audience=\"admin\" count={3} />\n");
    assert!(
        converted
            .text
            .contains("{% snippet \"note\" audience=\"admin\" count={3} %}"),
        "{}",
        converted.text
    );
    assert_eq!(converted.snippets[0].1, "note");
}

#[test]
fn a_boilerplate_import_costs_the_page_nothing() {
    let converted = page("import Tabs from '@theme/Tabs'\n\nbody\n");
    assert!(converted.attention.is_empty());
    assert!(!converted.text.contains("import"));
}

#[test]
fn an_unknown_import_is_reported_with_the_snippet_alternative() {
    let converted = page("import { chart } from '../lib/chart.js'\n\nbody\n");
    assert_eq!(codes(&converted), ["import or export"]);
    assert!(
        converted.attention[0]
            .help
            .as_deref()
            .is_some_and(|help| help.contains("snippet"))
    );
}

#[test]
fn a_module_statement_inside_a_fence_is_sample_code() {
    let source = "```js\nimport x from 'y'\nexport const z = 1\n```\n";
    let converted = page(source);
    assert_eq!(converted.text, source);
    assert!(converted.attention.is_empty(), "{:?}", codes(&converted));
}

#[test]
fn a_component_inside_a_fence_is_sample_code() {
    let source = "```mdx\n<PricingTable />\n{items.map(i => i)}\n```\n";
    let converted = page(source);
    assert_eq!(converted.text, source);
    assert!(converted.attention.is_empty(), "{:?}", codes(&converted));
}

#[test]
fn a_component_in_an_inline_code_span_is_content() {
    let source = "Write `<PricingTable />` to use it.\n";
    let converted = page(source);
    assert_eq!(converted.text, source);
    assert!(converted.attention.is_empty());
}

#[test]
fn front_matter_keys_are_renamed_and_consumed_keys_are_dropped() {
    let mut convert = Builtins::new();
    convert
        .rename
        .insert("sidebar_label".to_owned(), "sidebarTitle".to_owned());
    convert.consumed.push("sidebar_position");
    let converted = convert_with(
        "---\ntitle: Install\nsidebar_label: Setup\nsidebar_position: 3\n---\n\nbody\n",
        &convert,
        false,
    );
    assert_eq!(
        converted.text,
        "---\ntitle: Install\nsidebarTitle: Setup\n---\n\nbody\n"
    );
    assert_eq!(converted.front["sidebarTitle"], serde_json::json!("Setup"));
}

#[test]
fn a_dropped_key_takes_its_nested_lines_with_it() {
    let mut convert = Builtins::new();
    convert.consumed.push("pagination");
    let converted = convert_with(
        "---\ntitle: Install\npagination:\n  next: /next\n  prev: /prev\nicon: rocket\n---\n\nbody\n",
        &convert,
        false,
    );
    assert_eq!(
        converted.text,
        "---\ntitle: Install\nicon: rocket\n---\n\nbody\n"
    );
}

#[test]
fn front_matter_key_order_and_comments_survive() {
    let source =
        "---\n# the page\ntitle: Install\nicon: rocket\ndescription: How to install\n---\n\nbody\n";
    assert_eq!(page(source).text, source);
}

#[test]
fn a_page_with_no_front_matter_gains_none() {
    let converted = page("# Title\n");
    assert_eq!(converted.text, "# Title\n");
    assert_eq!(converted.front, serde_json::Value::Null);
}

#[test]
fn a_snippet_name_is_the_file_stem_without_a_partial_underscore() {
    assert_eq!(snippet_name("/snippets/auth-note.mdx"), "auth-note");
    assert_eq!(snippet_name("./_partial.md"), "partial");
    assert_eq!(snippet_name("docs/a/b/c.mdx"), "c");
}

#[test]
fn line_endings_and_the_byte_order_mark_are_normalized() {
    let converted = page("\u{feff}# Title\r\n\r\nbody\r\n");
    assert_eq!(converted.text, "# Title\n\nbody\n");
}

#[test]
fn a_tag_resolves_through_its_directive_spelling() {
    assert_eq!(directive_name("CodeGroup"), "code-group");
    assert_eq!(directive_name("Note"), "note");
    assert_eq!(directive_name("OpenApiSchema"), "open-api-schema");

    let converted = page("<Note>\nbody\n</Note>\n\n<CodeGroup>\n\n```js\nx\n```\n\n</CodeGroup>\n");
    assert!(converted.attention.is_empty(), "{:?}", codes(&converted));
}

#[test]
fn a_run_of_liyasa_constructs_on_one_line_is_left_alone() {
    let source = "{{ a }}{{ b }} and {% if c %}{{ d }}{% endif %}\n";
    let converted = page(source);
    assert_eq!(converted.text, source);
    assert!(converted.attention.is_empty(), "{:?}", codes(&converted));
}

#[test]
fn directive_props_are_not_an_expression_container() {
    let source = ":::tip{title=\"Pro tip\"}\nbody\n:::\n\n::image{src=\"/a.png\"}\n";
    let converted = page(source);
    assert_eq!(converted.text, source);
    assert!(converted.attention.is_empty(), "{:?}", codes(&converted));
}

#[test]
fn inline_directive_props_are_not_an_expression_container() {
    let source = "Press :kbd[Ctrl+K]{.key} to search.\n";
    let converted = page(source);
    assert_eq!(converted.text, source);
    assert!(converted.attention.is_empty(), "{:?}", codes(&converted));
}
