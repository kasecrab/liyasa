//! The clauses of AUTH-54 and AUTH-55 that are about rendered output rather
//! than about a model.
//!
//! Everything here goes through `crate::render::page` and `crate::variants`,
//! not through a fixture of this module's own. The distinction matters: the
//! question "is the withheld text in the bytes" cannot be answered by a type
//! that decides what to withhold, only by looking at what came out.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use liyasa_components::registry::Registry;
use liyasa_core::build::Variant;
use liyasa_core::ids::{Locale, Route, Version};
use liyasa_core::markdown::{SiteMeta, TemplateContext};
use liyasa_core::source_map::SourceMap;
use liyasa_core::vfs::VfsPath;
use url::Url;

use super::config::Regions;
use super::regions::{self, Detector};
use crate::render::{Options, Page, page};
use crate::variants::{Caps, Coordinates, Mode, Reads, of_page};

fn site(locale: &str) -> SiteMeta {
    SiteMeta {
        name: "Acme".to_owned(),
        canonical_origin: Url::parse("https://docs.acme.com").expect("an origin"),
        llms_txt: Url::parse("https://docs.acme.com/llms.txt").expect("a url"),
        version: None,
        locale: Locale::new(locale),
    }
}

fn context() -> TemplateContext {
    TemplateContext {
        values: minijinja::context! {
            site => minijinja::context! { name => "Acme" },
            page => minijinja::context! { title => "Pricing" },
        },
        tracking: false,
    }
}

/// One page through the production pipeline, for one variant.
fn rendered(text: &str, variant: Variant) -> Page {
    let mut map = SourceMap::new();
    let id = map.intern(VfsPath::new("pricing.md"), Arc::from(text));
    let (source, scan) = liyasa_markdown::scan(text, id);
    assert!(!scan.has_errors(), "the fixture scans: {scan:?}");
    let registry = Registry::builtins();
    let locale = variant
        .locale
        .as_ref()
        .map_or_else(|| "en".to_owned(), |locale| locale.as_str().to_owned());
    let site = site(&locale);
    let options = Options::new(&registry, &site).variant(variant);
    page(&map, &source, &context(), &options)
}

/// The `<route>.md` the engine writes, which is the gated serializer.
fn agent_twin(text: &str) -> String {
    let mut map = SourceMap::new();
    let id = map.intern(VfsPath::new("pricing.md"), Arc::from(text));
    let (source, scan) = liyasa_markdown::scan(text, id);
    assert!(!scan.has_errors(), "the fixture scans: {scan:?}");
    let registry = Registry::builtins();
    let site = site("en");
    let rendered = page(&map, &source, &context(), &Options::new(&registry, &site));
    let document = rendered.document.as_ref().expect("the page parsed");
    let routes = BTreeSet::from([Route::new("/pricing")]);
    let produced = crate::agents::render_page(
        document,
        &crate::agents::markdown::Options {
            site: &site,
            registry: &registry,
            route: &Route::new("/pricing"),
            frontmatter: None,
            routes: &routes,
            site_instructions: None,
            openapi_schema: None,
        },
    );
    produced.markdown
}

const GATED: &str = ":::region{only=\"us\"}\nInstalments are available.\n:::\n\n\
                    Prices exclude tax.\n";

/// AUTH-54, second clause. A block another region sees must not be in this
/// variant's bytes at all — a `hidden` attribute or a CSS class would still
/// ship the sentence to every reader and to every crawler.
#[test]
fn auth_54_a_withheld_block_is_absent_from_the_html_rather_than_hidden() {
    let inside = rendered(
        GATED,
        Variant {
            region: Some("us".to_owned()),
            ..Variant::default()
        },
    );
    assert!(
        inside.html.contains("Instalments are available."),
        "{}",
        inside.html
    );

    let outside = rendered(
        GATED,
        Variant {
            region: Some("eu".to_owned()),
            ..Variant::default()
        },
    );
    assert!(
        !outside.html.contains("Instalments"),
        "the text reached a reader outside the region: {}",
        outside.html
    );
    assert!(
        !outside.html.contains("hidden") && !outside.html.contains("display:none"),
        "withheld by omission, not by a style: {}",
        outside.html
    );
    assert!(
        outside.html.contains("Prices exclude tax."),
        "the rest of the page is still served: {}",
        outside.html
    );
}

/// The same for the Markdown twin an agent fetches, which is the surface
/// SRC-12 cares about: a gated sentence in `<route>.md` is one reader's content
/// in a file every reader can fetch.
///
/// Through `crate::agents::render_page`, which is what the engine writes that
/// file with. Not through `crate::render::Page::markdown`, which is a second
/// serializer that walks a component's children without asking the component
/// (`liyasa_markdown::render::markdown` has no `Render::markdown` call in it).
#[test]
fn auth_54_the_markdown_twin_withholds_it_too() {
    let twin = agent_twin(GATED);
    assert!(!twin.contains("Instalments"), "{twin}");
    assert!(twin.contains("Prices exclude tax."), "{twin}");
}

/// RFC 0401 through the production renderer: a build that does not know the
/// reader's region cannot show a gate held, so the default variant withholds a
/// region-gated block.
#[test]
fn auth_54_the_default_variant_admits_no_region_gated_block() {
    let anonymous = rendered(GATED, Variant::default());
    assert!(
        !anonymous.html.contains("Instalments"),
        "{}",
        anonymous.html
    );
}

/// AUTH-55. A German reader in the US sees `de` content with `us` availability:
/// the locale decides the words, the region decides the content, and neither
/// consults the other.
#[test]
fn auth_55_a_german_reader_in_the_us_gets_both_axes() {
    let german_in_us = rendered(
        GATED,
        Variant {
            locale: Some(Locale::new("de")),
            region: Some("us".to_owned()),
            ..Variant::default()
        },
    );
    assert!(
        german_in_us.html.contains("Instalments are available."),
        "a locale must not close a region gate: {}",
        german_in_us.html
    );

    let german_in_eu = rendered(
        GATED,
        Variant {
            locale: Some(Locale::new("de")),
            region: Some("eu".to_owned()),
            ..Variant::default()
        },
    );
    assert!(
        !german_in_eu.html.contains("Instalments"),
        "nor open one: {}",
        german_in_eu.html
    );
}

/// The two axes multiply rather than interfere: `crate::variants` gives a page
/// that names one region, in a site with two locales, a variant per pair.
#[test]
fn auth_55_the_axes_multiply_in_the_variant_set() {
    let reads = Reads {
        regions: BTreeSet::from(["us".to_owned()]),
        ..Reads::default()
    };
    let coordinates = Coordinates {
        locales: vec![Locale::new("en"), Locale::new("de")],
        ..Coordinates::default()
    };
    let outcome = of_page(
        &Route::new("/pricing"),
        &reads,
        &coordinates,
        &Caps::default(),
    );
    assert_eq!(outcome.mode, Mode::Static);
    assert_eq!(outcome.variants.len(), 4, "two locales times two regions");
    let pairs: BTreeSet<(Option<String>, Option<String>)> = outcome
        .variants
        .iter()
        .map(|variant| {
            (
                variant.locale.as_ref().map(|l| l.as_str().to_owned()),
                variant.region.clone(),
            )
        })
        .collect();
    assert!(pairs.contains(&(Some("de".to_owned()), Some("us".to_owned()))));
    assert!(pairs.contains(&(Some("de".to_owned()), None)));
}

/// A locale is in the route and a region is in the variant, so the two never
/// collide in a file name: `path_key` drops the locale and keeps the region.
#[test]
fn auth_55_the_route_carries_the_locale_and_the_file_name_the_region() {
    let variant = Variant {
        locale: Some(Locale::new("de")),
        region: Some("us".to_owned()),
        version: Some(Version::new("v2")),
        ..Variant::default()
    };
    assert_eq!(crate::variants::key(&variant), "v=v2,l=de,r=us");
    assert_eq!(crate::variants::path_key(&variant), "r=us");
    let paths = crate::variants::output_paths(&Route::new("/de/v2/pricing"), &[variant]);
    assert_eq!(
        paths.values().next().map(String::as_str),
        Some("de/v2/pricing/index.r-us.html")
    );
}

/// AUTH-53. With no `?region=`, an agent is shown every region's content with a
/// label per block rather than one region's page with no sign anything is
/// missing.
#[test]
fn auth_53_the_union_render_labels_what_each_block_is_available_in() {
    let settings = Regions {
        enabled: true,
        list: vec!["us".to_owned(), "ca".to_owned(), "eu".to_owned()],
        default: Some("us".to_owned()),
        ..Regions::default()
    };
    assert_eq!(
        regions::scope_of_query("", &settings),
        regions::Scope::Union
    );

    let gate = liyasa_core::frontmatter::RegionGate {
        only: Some(vec!["us".to_owned(), "ca".to_owned()]),
        except: None,
    };
    assert_eq!(
        regions::label(Some(&gate), &settings).as_deref(),
        Some("Available in: US, CA")
    );

    // And the same page rendered for one region does reduce, which is why the
    // union exists: an agent served this HTML has no sign that a paragraph was
    // withheld.
    let one = rendered(
        GATED,
        Variant {
            region: Some("eu".to_owned()),
            ..Variant::default()
        },
    );
    assert!(!one.html.contains("Instalments"), "{}", one.html);
    assert!(
        !one.html.contains("Available in"),
        "nothing labels the gap yet: `crate::agents::markdown` renders under the \
         anonymous variant and has no union mode, so AUTH-53's labelled union is \
         not reachable from here: {}",
        one.html
    );
}

/// The static export of AUTH-52 carries every region plus a default, and the
/// default is the one a crawler indexes (AUTH-54, first clause).
#[test]
fn auth_52_the_export_carries_every_variant_and_one_is_indexable() {
    let settings = Regions {
        enabled: true,
        list: vec!["us".to_owned(), "ca".to_owned()],
        default: Some("us".to_owned()),
        detection: vec![super::config::Detection::Choice],
        ..Regions::default()
    };
    let detector = Detector::new(&settings);
    let variants = detector.export_variants();
    assert_eq!(variants.len(), 3);
    let indexable: Vec<&Option<String>> = variants
        .iter()
        .filter(|region| detector.is_indexable(region.as_deref()))
        .collect();
    assert_eq!(
        indexable.len(),
        2,
        "the default region and the region-less page, not one per region"
    );
    assert!(indexable.contains(&&Some("us".to_owned())));
    assert!(!indexable.contains(&&Some("ca".to_owned())));
}

/// CM-101 end to end: the alternates of a page and the switcher beside it are
/// built from the same route table, so a locale cannot be offered a link and
/// then announced as an alternate it does not have.
#[test]
fn cm_101_the_switcher_offers_more_than_the_alternates_announce() {
    let locales = super::locales::Locales::new(&[
        super::LocaleDecl {
            code: "en".to_owned(),
            label: "English".to_owned(),
            default: true,
        },
        super::LocaleDecl {
            code: "de".to_owned(),
            label: "Deutsch".to_owned(),
            default: false,
        },
    ]);
    let mut table: BTreeMap<Locale, BTreeSet<Route>> = BTreeMap::new();
    table.insert(Locale::new("en"), BTreeSet::from([Route::new("/pricing")]));
    table.insert(Locale::new("de"), BTreeSet::new());

    let base = Route::new("/pricing");
    let switcher = locales.switcher(&base, Some(&Locale::new("en")), &table);
    assert_eq!(switcher.len(), 2, "both languages are offered");
    assert!(switcher[1].untranslated);

    assert!(
        locales
            .alternates(&base, &table, "https://docs.acme.com")
            .is_empty(),
        "and no alternate claims a German page exists"
    );
}
