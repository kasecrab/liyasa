//! CMP-80 and CMP-82: a gated block reaches only the variant it admits
//! (`plan/rfcs/0401-what-an-unknown-variant-admits.md`).
//!
//! Every assertion here is about absence, so every one compares the WHOLE
//! output. A `contains` check on the gated string passes whether the content is
//! absent or merely wrapped in a `div` that nothing acts on, which is the shape
//! of the defect these tests exist to keep out.

use liyasa_components::render::Shared;
use liyasa_components::{HtmlCtx, MarkdownCtx, Reference, Registry, inst, nodes};
use liyasa_core::build::Variant;
use liyasa_core::components::ComponentInst;
use liyasa_core::document::PropValue;
use liyasa_core::ids::{Locale, Version};

/// What a reader would see, given what the build knows about them.
struct Rendered {
    html: String,
    markdown: String,
    text: String,
}

const SECRET: &str = "The admin console lives at /internal/console.";

fn render(inst: &ComponentInst, variant: &Variant) -> Rendered {
    let registry = Registry::builtins();
    let reference = Reference::with(&registry);
    let component = registry
        .resolve(&inst.name)
        .unwrap_or_else(|| panic!("`{}` is not registered", inst.name));

    let mut html = HtmlCtx::with(Shared::new(&reference).variant(variant.clone()));
    component.html(inst, &mut html).expect("renders");
    let mut markdown = MarkdownCtx::with(Shared::new(&reference).variant(variant.clone()));
    component.markdown(inst, &mut markdown).expect("serializes");

    Rendered {
        html: html.finish(),
        markdown: markdown.finish(),
        text: component.text_for(inst, variant),
    }
}

/// Nothing of the block survives: not the content, not a wrapper holding it.
fn assert_withheld(rendered: &Rendered, what: &str) {
    assert_eq!(rendered.html, "", "{what}: html");
    assert_eq!(rendered.markdown, "", "{what}: markdown");
    assert_eq!(rendered.text, "", "{what}: text");
}

fn assert_served(rendered: &Rendered, what: &str) {
    assert!(rendered.html.contains(SECRET), "{what}: html");
    assert!(rendered.markdown.contains(SECRET), "{what}: markdown");
    assert!(rendered.text.contains(SECRET), "{what}: text");
}

fn gated(component: &str, prop: &str, values: &[&str]) -> ComponentInst {
    inst::new(component)
        .prop(
            prop,
            PropValue::List(
                values
                    .iter()
                    .map(|value| PropValue::Str((*value).to_owned()))
                    .collect(),
            ),
        )
        .child(nodes::paragraph(SECRET))
        .build()
}

fn with_groups(groups: &[&str]) -> Variant {
    Variant {
        groups: groups.iter().map(|g| (*g).to_owned()).collect(),
        ..Variant::default()
    }
}

fn in_region(region: &str) -> Variant {
    Variant {
        region: Some(region.to_owned()),
        ..Variant::default()
    }
}

// ---- groups ----

#[test]
fn a_group_gated_block_is_withheld_from_a_reader_without_the_group() {
    let block = gated("visibility", "groups", &["admin"]);
    assert_withheld(&render(&block, &with_groups(&["support"])), "wrong group");
}

/// The case the defect actually shipped: a public site, no auth configured, so
/// the variant carries no groups at all.
#[test]
fn a_group_gated_block_is_withheld_from_an_anonymous_reader() {
    let block = gated("visibility", "groups", &["admin"]);
    assert_withheld(&render(&block, &Variant::default()), "anonymous");
}

#[test]
fn a_group_gated_block_is_served_to_a_reader_in_the_group() {
    let block = gated("visibility", "groups", &["admin"]);
    assert_served(&render(&block, &with_groups(&["admin"])), "in the group");
}

#[test]
fn one_group_in_common_is_enough() {
    let block = gated("visibility", "groups", &["admin", "staff"]);
    let rendered = render(&block, &with_groups(&["support", "staff"]));
    assert_served(&rendered, "overlapping groups");
}

// ---- regions, locales, versions ----

#[test]
fn a_region_gated_visibility_block_follows_the_variant() {
    let block = gated("visibility", "regions", &["eu"]);
    assert_withheld(&render(&block, &Variant::default()), "no region");
    assert_withheld(&render(&block, &in_region("us")), "another region");
    assert_served(&render(&block, &in_region("eu")), "the named region");
}

#[test]
fn a_locale_gated_block_follows_the_variant() {
    let block = gated("visibility", "locales", &["de"]);
    assert_withheld(&render(&block, &Variant::default()), "no locale");

    let english = Variant {
        locale: Some(Locale::new("en")),
        ..Variant::default()
    };
    assert_withheld(&render(&block, &english), "another locale");

    let german = Variant {
        locale: Some(Locale::new("de")),
        ..Variant::default()
    };
    assert_served(&render(&block, &german), "the named locale");
}

#[test]
fn a_version_gated_block_follows_the_variant() {
    let block = gated("visibility", "versions", &["0.5"]);
    assert_withheld(&render(&block, &Variant::default()), "no version");

    let old = Variant {
        version: Some(Version::new("0.4")),
        ..Variant::default()
    };
    assert_withheld(&render(&block, &old), "another version");

    let named = Variant {
        version: Some(Version::new("0.5")),
        ..Variant::default()
    };
    assert_served(&render(&block, &named), "the named version");
}

#[test]
fn every_gate_on_a_block_has_to_admit_it() {
    let block = inst::new("visibility")
        .prop(
            "groups",
            PropValue::List(vec![PropValue::Str("admin".into())]),
        )
        .prop(
            "regions",
            PropValue::List(vec![PropValue::Str("eu".into())]),
        )
        .child(nodes::paragraph(SECRET))
        .build();

    let admin_elsewhere = Variant {
        region: Some("us".to_owned()),
        ..with_groups(&["admin"])
    };
    assert_withheld(
        &render(&block, &admin_elsewhere),
        "right group, wrong region",
    );

    let stranger_in_the_eu = Variant {
        region: Some("eu".to_owned()),
        ..Variant::default()
    };
    assert_withheld(
        &render(&block, &stranger_in_the_eu),
        "right region, no group",
    );

    let both = Variant {
        region: Some("eu".to_owned()),
        ..with_groups(&["admin"])
    };
    assert_served(&render(&block, &both), "both gates admit");
}

#[test]
fn an_ungated_visibility_block_still_renders() {
    let block = inst::new("visibility")
        .child(nodes::paragraph(SECRET))
        .build();
    assert_served(&render(&block, &Variant::default()), "no gate at all");
}

// ---- region ----

#[test]
fn a_region_block_with_only_follows_the_variant() {
    let block = gated("region", "only", &["eu"]);
    assert_withheld(&render(&block, &Variant::default()), "no region");
    assert_withheld(&render(&block, &in_region("us")), "another region");
    assert_served(&render(&block, &in_region("eu")), "the named region");
}

/// "Hidden in the EU" reads as though an unknown region should pass, and it
/// must not: the build cannot show the reader is outside the excluded set.
#[test]
fn a_region_block_with_except_withholds_what_it_cannot_check() {
    let block = gated("region", "except", &["eu"]);
    assert_withheld(&render(&block, &Variant::default()), "no region");
    assert_withheld(&render(&block, &in_region("eu")), "the excluded region");
    assert_served(&render(&block, &in_region("us")), "another region");
}

#[test]
fn an_ungated_region_block_still_renders() {
    let block = inst::new("region").child(nodes::paragraph(SECRET)).build();
    assert_served(&render(&block, &Variant::default()), "no gate at all");
}

// ---- the surfaces that are built once and served to everyone ----

/// The frozen `render_text` has no variant (§34.9), so it renders under the
/// default one. The search index is shared, so that has to mean "withheld".
#[test]
fn the_ctx_free_text_serialization_withholds_every_gated_block() {
    let registry = Registry::builtins();
    for block in [
        gated("visibility", "groups", &["admin"]),
        gated("visibility", "regions", &["eu"]),
        gated("region", "only", &["eu"]),
    ] {
        let component = registry.resolve(&block.name).expect("registered");
        assert_eq!(component.text(&block), "", "`{}`", block.name);
    }
}

/// A gated block nested inside an ordinary one is gated too: the child walk
/// carries the same variant, so a card cannot smuggle one through.
#[test]
fn a_gate_holds_inside_another_component() {
    let card = inst::new("card")
        .prop("title", PropValue::Str("Operations".into()))
        .child(nodes::paragraph("Everyone sees this."))
        .child(inst::nested(
            inst::new("visibility")
                .prop(
                    "groups",
                    PropValue::List(vec![PropValue::Str("admin".into())]),
                )
                .child(nodes::paragraph(SECRET)),
        ))
        .build();

    let anonymous = render(&card, &Variant::default());
    assert!(!anonymous.html.contains(SECRET), "{}", anonymous.html);
    assert!(anonymous.html.contains("Everyone sees this."));
    assert!(
        !anonymous.markdown.contains(SECRET),
        "{}",
        anonymous.markdown
    );
    assert!(!anonymous.text.contains(SECRET), "{}", anonymous.text);

    let admin = render(&card, &with_groups(&["admin"]));
    assert!(admin.html.contains(SECRET), "{}", admin.html);
}
