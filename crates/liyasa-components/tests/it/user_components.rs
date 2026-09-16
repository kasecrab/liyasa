//! CMP-90 to CMP-94: components defined by a file.

use liyasa_components::pack;
use liyasa_components::user::UserComponent;
use liyasa_components::{HtmlCtx, MarkdownCtx, Reference, Registry, inst, nodes};
use liyasa_core::components::{Component, ComponentRegistry};
use liyasa_core::conformance::fixtures::MemoryVfs;
use liyasa_core::diagnostics::Diagnostics;
use liyasa_core::document::PropValue;
use liyasa_core::vfs::VfsPath;

const PRICING: &str = r#"{# ---
name: pricing-card
aliases: [PricingCard]
kind: container
props:
  tier:
    type: string
    required: true
    doc: Which plan this card describes.
  price:
    type: number
    default: 0
    doc: Monthly price in dollars.
  featured:
    type: boolean
    doc: Draws the card with the accent colour.
  audience:
    type: enum
    values: [teams, solo]
    default: teams
    doc: Who the plan is for.
slots:
  footer:
    doc: Shown under the price.
markdown: |
  ### {{ props.tier }}

  ${{ props.price }} a month.

  {{ content }}
editor:
  icon: credit-card
  category: Marketing
--- #}
<div class="pricing-card" data-tier="{{ props.tier }}"{% if props.featured %} data-featured{% endif %}>
  <h3>{{ props.tier }}</h3>
  <p class="price">${{ props.price }}</p>
  <div class="body">{{ content }}</div>
  <footer>{{ slots.footer }}</footer>
</div>
"#;

fn render(
    registry: &Registry,
    name: &str,
    inst: &liyasa_core::components::ComponentInst,
) -> (String, String) {
    let reference = Reference::with(registry);
    let component = registry.resolve(name).expect("registered");
    let mut html = HtmlCtx::new(&reference);
    component.html(inst, &mut html).expect("html");
    let mut markdown = MarkdownCtx::new(&reference);
    component.markdown(inst, &mut markdown).expect("markdown");
    (html.finish(), markdown.finish())
}

fn pricing_instance() -> liyasa_core::components::ComponentInst {
    inst::new("pricing-card")
        .prop("tier", PropValue::Str("Pro".into()))
        .prop("price", PropValue::Num(20.0))
        .prop("featured", PropValue::Bool(true))
        .child(nodes::paragraph("Everything in Free, plus verification."))
        .slot("footer", [nodes::paragraph("Cancel any time.")])
        .build()
}

#[test]
fn a_file_defines_a_component() {
    let component = UserComponent::parse("pricing-card", PRICING).expect("parses");
    assert_eq!(component.name(), "pricing-card");
    assert_eq!(component.aliases(), ["PricingCard"]);
    let schema = component.schema();
    assert_eq!(schema.props.len(), 4);
    assert!(schema.prop("tier").is_some_and(|p| p.required));
    assert_eq!(
        schema.prop("price").and_then(|p| p.default.as_ref()),
        Some(&serde_json::json!(0))
    );
    assert_eq!(schema.slots.len(), 1);
}

#[test]
fn the_editor_form_is_generated_from_the_schema() {
    let component = UserComponent::parse("pricing-card", PRICING).expect("parses");
    let editor = component.editor_block();
    assert_eq!(editor.icon, "credit-card");
    assert_eq!(editor.category, "Marketing");
    assert_eq!(editor.form.len(), 4);
    let widget = |prop: &str| {
        editor
            .form
            .iter()
            .find(|f| f.prop == prop)
            .map(|f| f.widget)
    };
    use liyasa_core::components::Widget;
    assert_eq!(widget("price"), Some(Widget::Number));
    assert_eq!(widget("featured"), Some(Widget::Toggle));
    assert_eq!(widget("audience"), Some(Widget::Select));
}

#[test]
fn a_user_component_renders_props_content_and_slots() {
    let mut registry = Registry::builtins();
    registry.add(UserComponent::parse("pricing-card", PRICING).expect("parses"));
    let (html, markdown) = render(&registry, "pricing-card", &pricing_instance());

    assert!(html.contains(r#"data-tier="Pro""#), "{html}");
    assert!(html.contains("data-featured"), "{html}");
    assert!(html.contains("<p class=\"price\">$20</p>"), "{html}");
    assert!(
        html.contains("<p>Everything in Free, plus verification.</p>"),
        "{html}"
    );
    assert!(
        html.contains("<footer><p>Cancel any time.</p></footer>"),
        "{html}"
    );

    assert!(markdown.starts_with("### Pro"), "{markdown}");
    assert!(markdown.contains("$20 a month."), "{markdown}");
}

#[test]
fn a_user_component_escapes_what_an_author_wrote() {
    let mut registry = Registry::builtins();
    registry.add(UserComponent::parse("pricing-card", PRICING).expect("parses"));
    let inst = inst::new("pricing-card")
        .prop("tier", PropValue::Str("<script>alert(1)</script>".into()))
        .build();
    let (html, _) = render(&registry, "pricing-card", &inst);
    assert!(!html.contains("<script>"), "{html}");
    assert!(html.contains("&lt;script&gt;"), "{html}");
}

#[test]
fn a_component_with_no_markdown_template_falls_back_to_a_heading() {
    const PLAIN: &str = "{# ---\nprops: {}\n--- #}\n<div class=\"plain\">{{ content }}</div>\n";
    let mut registry = Registry::builtins();
    registry.add(UserComponent::parse("plain-box", PLAIN).expect("parses"));
    let inst = inst::new("plain-box")
        .child(nodes::paragraph("Inside."))
        .build();
    let (html, markdown) = render(&registry, "plain-box", &inst);
    assert_eq!(html, "<div class=\"plain\"><p>Inside.</p></div>");
    assert_eq!(markdown, "### Plain box\n\nInside.\n");
}

#[test]
fn a_user_component_is_a_component_registry_member() {
    let mut registry = Registry::builtins();
    registry.add(UserComponent::parse("pricing-card", PRICING).expect("parses"));
    liyasa_core::conformance::component_registry::check(&registry);
    assert!(registry.get("PricingCard").is_some());
}

#[test]
fn slots_are_validated_against_the_declaration() {
    let mut registry = Registry::builtins();
    registry.add(UserComponent::parse("pricing-card", PRICING).expect("parses"));
    let component = registry.resolve("pricing-card").expect("registered");
    let inst = inst::new("pricing-card")
        .prop("tier", PropValue::Str("Pro".into()))
        .slot("head", [nodes::paragraph("Nope.")])
        .build();
    let mut diagnostics = Diagnostics::new();
    liyasa_components::validate(component, &inst, &mut diagnostics);
    let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, ["E0350"], "{diagnostics:?}");
}

#[test]
fn props_are_validated_against_the_declared_schema() {
    let mut registry = Registry::builtins();
    registry.add(UserComponent::parse("pricing-card", PRICING).expect("parses"));
    let component = registry.resolve("pricing-card").expect("registered");
    let inst = inst::new("pricing-card")
        .prop("price", PropValue::Str("free".into()))
        .prop("audience", PropValue::Str("everyone".into()))
        .build();
    let mut diagnostics = Diagnostics::new();
    liyasa_components::validate(component, &inst, &mut diagnostics);
    let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, ["E0314", "E0315", "E0315"], "{diagnostics:?}");
}

/// The first code a failed parse reported.
fn first_code(result: Result<UserComponent, Diagnostics>) -> Option<&'static str> {
    result.err()?.iter().next().map(|d| d.code.as_str())
}

#[test]
fn a_broken_front_matter_is_reported_not_panicked() {
    let bad = UserComponent::parse(
        "broken",
        "{# ---\nprops:\n  x:\n    type: rainbow\n--- #}\n<p></p>",
    );
    assert_eq!(first_code(bad), Some("E0352"));
    let missing = UserComponent::parse("broken", "<p>no front matter</p>");
    assert_eq!(first_code(missing), Some("E0356"));
    let bad_name = UserComponent::parse("Broken", "{# ---\nprops: {}\n--- #}\n<p></p>");
    assert_eq!(first_code(bad_name), Some("E0356"));
}

#[test]
fn a_required_prop_may_not_also_have_a_default() {
    let bad = UserComponent::parse(
        "broken",
        "{# ---\nprops:\n  x:\n    required: true\n    default: 1\n--- #}\n<p></p>",
    );
    assert_eq!(first_code(bad), Some("E0352"));
}

#[test]
fn a_directory_of_files_loads_as_a_pack() {
    let vfs = MemoryVfs::new()
        .with("components/pricing-card.jinja", PRICING)
        .with(
            "components/pricing-card.css",
            ".pricing-card { border: 1px solid }",
        )
        .with("components/pricing-card.js", "export default () => {}")
        .with("components/broken.jinja", "<p>no front matter</p>");
    let mut registry = Registry::builtins();
    let loaded = pack::load(&vfs, &VfsPath::new("components"), &mut registry);

    assert!(registry.get("pricing-card").is_some());
    assert_eq!(
        loaded
            .assets
            .get("pricing-card")
            .and_then(|a| a.css.as_deref()),
        Some(".pricing-card { border: 1px solid }")
    );
    assert_eq!(
        loaded
            .assets
            .get("pricing-card")
            .and_then(|a| a.js.as_deref()),
        Some("export default () => {}")
    );
    let codes: Vec<&str> = loaded.diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, ["E0356"]);
}

#[test]
fn a_file_overrides_a_builtin_and_keeps_its_schema() {
    const CARD: &str = "{# ---\nprops: {}\n--- #}\n<article class=\"my-card\">{{ props.title }}{{ content }}</article>\n";
    let vfs = MemoryVfs::new().with("components/card.jinja", CARD);
    let mut registry = Registry::builtins();
    let before = registry.len();
    pack::load(&vfs, &VfsPath::new("components"), &mut registry);
    assert_eq!(registry.len(), before, "an override replaces, never adds");

    let card = registry.resolve("card").expect("registered");
    // The built-in's schema and editor block survive the override (CMP-94).
    assert!(card.schema().prop("horizontal").is_some());
    assert_eq!(card.editor_block().category, "Layout");
    assert_eq!(card.aliases(), ["Card"]);

    let inst = inst::new("card")
        .prop("title", PropValue::Str("Quickstart".into()))
        .prop("href", PropValue::Str("/start".into()))
        .child(nodes::paragraph("Five minutes."))
        .build();
    let (html, markdown) = render(&registry, "card", &inst);
    assert!(
        html.starts_with("<article class=\"my-card\">Quickstart"),
        "{html}"
    );
    // The agent serialization is still the built-in's.
    assert_eq!(markdown, "- [Quickstart](/start)\n  Five minutes.\n");
}

#[test]
fn a_template_that_fails_is_a_render_error_not_a_panic() {
    const BOOM: &str = "{# ---\nprops: {}\n--- #}\n{{ undefined_function() }}\n";
    let component = UserComponent::parse("boom", BOOM).expect("parses");
    let reference = Reference::new();
    let mut ctx = HtmlCtx::new(&reference);
    let inst = inst::new("boom").build();
    assert!(liyasa_components::Render::html(&component, &inst, &mut ctx).is_err());
}
