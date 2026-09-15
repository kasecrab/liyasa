//! Variant cardinality (PRD §6.6.3).
//!
//! A variant is a per-page concept, never the site-wide product of every
//! dimension: a page that mentions two groups has four group variants, and a
//! page that mentions none has one. A page that reads a free-form `reader.*`
//! field cannot be enumerated at all and is rendered on demand instead.

use std::collections::{BTreeMap, BTreeSet};

use liyasa_core::build::Variant;
use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::ids::{Locale, Route, Version};
use liyasa_core::markdown::ExpansionRecord;

/// `reader.*` fields a build can enumerate. Everything else — a name, a plan,
/// an API key — has no finite set of values and forces on-demand rendering.
pub const ENUMERABLE_READER_FIELDS: &[&str] = &["groups", "region"];

#[derive(Debug, Clone)]
pub struct Caps {
    /// `build.maxVariantsPerPage`.
    pub per_page: usize,
    /// `build.maxVariants`.
    pub site: usize,
    /// `build.variantDiscoveryIterations`.
    pub iterations: u32,
}

impl Default for Caps {
    fn default() -> Self {
        Self {
            per_page: 16,
            site: 10_000,
            iterations: 4,
        }
    }
}

/// The coordinates a page exists at, from the content tree rather than from the
/// page's own text.
#[derive(Debug, Clone, Default)]
pub struct Coordinates {
    pub versions: Vec<Version>,
    pub locales: Vec<Locale>,
    pub products: Vec<String>,
}

/// What one page asked for while it expanded.
#[derive(Debug, Clone, Default)]
pub struct Reads {
    /// `reader.<field>` names, as recorded by expansion.
    pub reader_fields: BTreeSet<String>,
    /// Group names the page mentions.
    pub groups: BTreeSet<String>,
    /// Regions the page names.
    pub regions: BTreeSet<String>,
    /// Whether the page declared `personalized: true`.
    pub personalized: bool,
}

impl Reads {
    /// The union of what the syntactic walk found and what a render recorded
    /// (§6.6.3 item 1): discovery repeats until this stops growing.
    pub fn absorb(&mut self, record: &ExpansionRecord) -> bool {
        let before = self.reader_fields.len();
        self.reader_fields
            .extend(record.reader_fields.iter().cloned());
        self.reader_fields.len() != before
    }

    /// Fields that cannot be enumerated, which is what makes a page dynamic.
    pub fn free_form(&self) -> Vec<&str> {
        self.reader_fields
            .iter()
            .map(String::as_str)
            .filter(|field| !ENUMERABLE_READER_FIELDS.contains(field))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// Every variant is rendered at build time and served as a file.
    Static,
    /// Rendered per request from the cached Source Document (§6.6.4).
    Dynamic,
}

#[derive(Debug, Clone)]
pub struct Outcome {
    pub mode: Mode,
    pub variants: Vec<Variant>,
    pub diagnostics: Diagnostics,
}

impl Outcome {
    pub fn is_dynamic(&self) -> bool {
        self.mode == Mode::Dynamic
    }
}

/// The variant set of one page.
pub fn of_page(route: &Route, reads: &Reads, coordinates: &Coordinates, caps: &Caps) -> Outcome {
    let mut diagnostics = Diagnostics::new();
    let base = coordinate_variants(coordinates);

    let free_form = reads.free_form();
    if !free_form.is_empty() {
        diagnostics.push(
            Diagnostic::new(
                code::W0715,
                format!(
                    "`{route}` is rendered on demand because it reads {}",
                    list(&free_form)
                ),
            )
            .help("keep on-demand pages few; `liyasa validate --personalization` lists them"),
        );
        if !reads.personalized {
            diagnostics.push(
                Diagnostic::new(
                    code::E0208,
                    format!("`{route}` reads a reader field without `personalized: true`"),
                )
                .help("add `personalized: true` to the front matter"),
            );
        }
        return Outcome {
            mode: Mode::Dynamic,
            variants: base,
            diagnostics,
        };
    }

    let mut variants = Vec::new();
    for coordinate in &base {
        for groups in subsets(&reads.groups) {
            for region in regions(&reads.regions) {
                variants.push(Variant {
                    groups: groups.clone(),
                    region: region.clone(),
                    ..coordinate.clone()
                });
            }
        }
    }
    variants.sort();
    variants.dedup();

    if variants.len() > caps.per_page {
        diagnostics.push(
            Diagnostic::new(
                code::W0710,
                format!(
                    "`{route}` has {} variants, over the cap of {}; it is served on demand",
                    variants.len(),
                    caps.per_page
                ),
            )
            .help("raise `build.maxVariantsPerPage` or narrow what the page reads"),
        );
        return Outcome {
            mode: Mode::Dynamic,
            variants: base,
            diagnostics,
        };
    }

    Outcome {
        mode: Mode::Static,
        variants,
        diagnostics,
    }
}

/// Checks the whole build against `build.maxVariants` (`E0711`).
pub fn check_site_cap(total: usize, caps: &Caps) -> Option<Diagnostic> {
    (total > caps.site).then(|| {
        Diagnostic::new(
            code::E0711,
            format!(
                "the site renders {total} variants, over the cap of {}",
                caps.site
            ),
        )
        .help("raise `build.maxVariants` or reduce the dimensions pages read")
    })
}

/// Reports a discovery that never settled (`E0712`).
pub fn did_not_converge(route: &Route, caps: &Caps) -> Diagnostic {
    Diagnostic::new(
        code::E0712,
        format!(
            "`{route}` was still discovering new reader fields after {} passes",
            caps.iterations
        ),
    )
    .help("raise `build.variantDiscoveryIterations`, or mark the page `personalized: true`")
}

/// Reports an include whose name is only known at render time on a page that is
/// not rendered on demand (`E0717`).
pub fn dynamic_include(route: &Route, name: &str) -> Diagnostic {
    Diagnostic::new(
        code::E0717,
        format!("`{route}` includes `{name}`, whose name is computed at render time"),
    )
    .help("use a literal include name, or mark the page `personalized: true`")
}

/// The build-time coordinates: one variant per (version, locale, product) the
/// page exists at.
fn coordinate_variants(coordinates: &Coordinates) -> Vec<Variant> {
    let versions: Vec<Option<Version>> = match coordinates.versions.is_empty() {
        true => vec![None],
        false => coordinates.versions.iter().cloned().map(Some).collect(),
    };
    let locales: Vec<Option<Locale>> = match coordinates.locales.is_empty() {
        true => vec![None],
        false => coordinates.locales.iter().cloned().map(Some).collect(),
    };
    let products: Vec<Option<String>> = match coordinates.products.is_empty() {
        true => vec![None],
        false => coordinates.products.iter().cloned().map(Some).collect(),
    };

    let mut out = Vec::new();
    for version in &versions {
        for locale in &locales {
            for product in &products {
                out.push(Variant {
                    version: version.clone(),
                    locale: locale.clone(),
                    product: product.clone(),
                    ..Variant::default()
                });
            }
        }
    }
    out
}

/// Every subset of the groups a page mentions — four for two groups, never
/// `2^N` over the site's groups (§6.6.3 item 2).
fn subsets(groups: &BTreeSet<String>) -> Vec<BTreeSet<String>> {
    let names: Vec<&String> = groups.iter().collect();
    let mut out = vec![BTreeSet::new()];
    for name in names {
        let mut grown = Vec::new();
        for set in &out {
            let mut with = set.clone();
            with.insert(name.clone());
            grown.push(with);
        }
        out.extend(grown);
    }
    out.sort();
    out
}

/// The regions a page names, plus the default: every other region collapses
/// into the default variant.
fn regions(named: &BTreeSet<String>) -> Vec<Option<String>> {
    let mut out = vec![None];
    out.extend(named.iter().cloned().map(Some));
    out
}

fn list(items: &[&str]) -> String {
    items
        .iter()
        .map(|item| format!("`reader.{item}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A stable key for one variant, for the manifest and the server's lookup.
pub fn key(variant: &Variant) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(version) = &variant.version {
        parts.push(format!("v={version}"));
    }
    if let Some(locale) = &variant.locale {
        parts.push(format!("l={locale}"));
    }
    if let Some(product) = &variant.product {
        parts.push(format!("p={product}"));
    }
    if !variant.groups.is_empty() {
        parts.push(format!(
            "g={}",
            variant.groups.iter().cloned().collect::<Vec<_>>().join("+")
        ));
    }
    if let Some(region) = &variant.region {
        parts.push(format!("r={region}"));
    }
    if let Some(variation) = &variant.variation {
        parts.push(format!("x={variation}"));
    }
    match parts.is_empty() {
        true => "default".to_owned(),
        false => parts.join(","),
    }
}

/// The part of a variant the file name has to carry.
///
/// Version and locale decide the route itself (CM-91, CM-101), so repeating
/// them in the file name would put the default version's page at
/// `index.v-v2.html` instead of `index.html`.
pub fn path_key(variant: &Variant) -> String {
    key(&Variant {
        version: None,
        locale: None,
        ..variant.clone()
    })
}

/// Where a variant's HTML lands under `dist/`, relative to the route.
pub fn output_paths(route: &Route, variants: &[Variant]) -> BTreeMap<String, String> {
    variants
        .iter()
        .map(|variant| {
            let key = key(variant);
            let name = path_key(variant);
            let trimmed = route.as_str().trim_matches('/');
            let base = match trimmed.is_empty() {
                true => "index".to_owned(),
                false => format!("{trimmed}/index"),
            };
            let path = match name.as_str() {
                "default" => format!("{base}.html"),
                other => format!("{base}.{}.html", slug(other)),
            };
            (key, path)
        })
        .collect()
}

fn slug(key: &str) -> String {
    key.chars()
        .map(|ch| match ch.is_ascii_alphanumeric() {
            true => ch.to_ascii_lowercase(),
            false => '-',
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route() -> Route {
        Route::new("/guides/install")
    }

    fn reads(groups: &[&str], regions: &[&str], reader: &[&str]) -> Reads {
        Reads {
            reader_fields: reader.iter().map(|f| (*f).to_owned()).collect(),
            groups: groups.iter().map(|g| (*g).to_owned()).collect(),
            regions: regions.iter().map(|r| (*r).to_owned()).collect(),
            personalized: false,
        }
    }

    #[test]
    fn a_page_that_reads_nothing_has_one_variant() {
        let outcome = of_page(
            &route(),
            &Reads::default(),
            &Coordinates::default(),
            &Caps::default(),
        );
        assert_eq!(outcome.mode, Mode::Static);
        assert_eq!(outcome.variants, vec![Variant::default()]);
        assert!(outcome.diagnostics.is_empty());
    }

    #[test]
    fn two_groups_make_four_variants_not_two_to_the_site() {
        let outcome = of_page(
            &route(),
            &reads(&["admin", "partner"], &[], &[]),
            &Coordinates::default(),
            &Caps::default(),
        );
        assert_eq!(outcome.variants.len(), 4);
        let keys: Vec<String> = outcome.variants.iter().map(key).collect();
        assert!(keys.contains(&"default".to_owned()));
        assert!(keys.contains(&"g=admin+partner".to_owned()));
    }

    #[test]
    fn only_the_regions_a_page_names_matter() {
        let outcome = of_page(
            &route(),
            &reads(&[], &["eu"], &[]),
            &Coordinates::default(),
            &Caps::default(),
        );
        assert_eq!(outcome.variants.len(), 2);
        let keys: Vec<String> = outcome.variants.iter().map(key).collect();
        assert_eq!(keys, vec!["default".to_owned(), "r=eu".to_owned()]);
    }

    #[test]
    fn coordinates_multiply_into_the_variant_set() {
        let coordinates = Coordinates {
            versions: vec![Version::new("v1"), Version::new("v2")],
            locales: vec![Locale::new("en")],
            products: Vec::new(),
        };
        let outcome = of_page(&route(), &Reads::default(), &coordinates, &Caps::default());
        assert_eq!(outcome.variants.len(), 2);
    }

    #[test]
    fn a_free_form_reader_field_makes_the_page_dynamic() {
        let outcome = of_page(
            &route(),
            &reads(&[], &[], &["name"]),
            &Coordinates::default(),
            &Caps::default(),
        );
        assert_eq!(outcome.mode, Mode::Dynamic);
        let codes: Vec<&str> = outcome
            .diagnostics
            .iter()
            .map(|d| d.code.as_str())
            .collect();
        assert_eq!(codes, ["W0715", "E0208"]);
    }

    #[test]
    fn a_declared_personalized_page_is_dynamic_without_the_error() {
        let mut reads = reads(&[], &[], &["plan"]);
        reads.personalized = true;
        let outcome = of_page(&route(), &reads, &Coordinates::default(), &Caps::default());
        assert_eq!(outcome.mode, Mode::Dynamic);
        let codes: Vec<&str> = outcome
            .diagnostics
            .iter()
            .map(|d| d.code.as_str())
            .collect();
        assert_eq!(codes, ["W0715"]);
    }

    #[test]
    fn groups_and_region_are_enumerable_and_stay_static() {
        let outcome = of_page(
            &route(),
            &reads(&["admin"], &["eu"], &["groups", "region"]),
            &Coordinates::default(),
            &Caps::default(),
        );
        assert_eq!(outcome.mode, Mode::Static);
        assert_eq!(outcome.variants.len(), 4);
    }

    #[test]
    fn a_page_over_the_per_page_cap_is_demoted_to_dynamic() {
        let caps = Caps {
            per_page: 3,
            ..Caps::default()
        };
        let outcome = of_page(
            &route(),
            &reads(&["a", "b"], &[], &[]),
            &Coordinates::default(),
            &caps,
        );
        assert_eq!(outcome.mode, Mode::Dynamic);
        assert!(
            outcome
                .diagnostics
                .iter()
                .any(|d| d.code.as_str() == "W0710")
        );
    }

    #[test]
    fn the_site_cap_is_checked_over_the_whole_build() {
        let caps = Caps {
            site: 10,
            ..Caps::default()
        };
        assert!(check_site_cap(10, &caps).is_none());
        let over = check_site_cap(11, &caps).expect("over the cap");
        assert_eq!(over.code.as_str(), "E0711");
    }

    #[test]
    fn discovery_absorbs_what_a_render_recorded() {
        let mut reads = Reads::default();
        let mut record = ExpansionRecord::default();
        record.reader_fields.insert("groups".to_owned());
        assert!(reads.absorb(&record));
        assert!(!reads.absorb(&record));
    }

    #[test]
    fn a_variant_has_a_stable_key_and_output_path() {
        let variant = Variant {
            version: Some(Version::new("v2")),
            groups: ["admin".to_owned()].into_iter().collect(),
            ..Variant::default()
        };
        assert_eq!(key(&variant), "v=v2,g=admin");
        let paths = output_paths(&Route::new("/guides/install"), &[variant]);
        // The version is in the route, so only the groups reach the file name.
        assert_eq!(
            paths.get("v=v2,g=admin").map(String::as_str),
            Some("guides/install/index.g-admin.html")
        );
        let root = output_paths(&Route::new("/"), &[Variant::default()]);
        assert_eq!(root.get("default").map(String::as_str), Some("index.html"));
    }
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn a_versioned_page_keeps_the_plain_file_name() {
        let variant = Variant {
            version: Some(Version::new("v1")),
            locale: Some(Locale::new("de")),
            ..Variant::default()
        };
        assert_eq!(path_key(&variant), "default");
        let paths = output_paths(&Route::new("/v1/guides/install"), &[variant]);
        assert_eq!(
            paths.values().next().map(String::as_str),
            Some("v1/guides/install/index.html")
        );
    }
}
