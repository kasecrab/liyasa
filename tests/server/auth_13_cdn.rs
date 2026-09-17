//! AUTH-13 through a shared cache.
//!
//! The requirement names three layers and this is the third: a caching proxy
//! in front of the server that honours `Cache-Control`, `Vary` and
//! purge-by-tag. Two claims are under test here that the in-process caches
//! cannot make — that the variant a reader gets through a CDN is still keyed
//! on their whole variant, and that a deploy purge never causes a `private`
//! response to be served from the shared cache.

use std::num::NonZeroUsize;

use liyasa_core::build::{OutputFormat, Variant};
use liyasa_core::ids::{BuildId, Fingerprint, Locale, PageId, Version};
use liyasa_server::auth::variant::{CacheKey, Entry, ReaderFields, VariantCache};
use liyasa_tests::cdn::{Cdn, Hit, Request, Response};

const URL: &str = "https://docs.example.com/guides/install";

fn build(seed: &str) -> BuildId {
    BuildId(Fingerprint::of(seed))
}

/// The header names a variant page varies on. A real deployment sets these at
/// the edge from the session; the names are what the CDN keys on.
const VARY: &[&str] = &[
    "x-liyasa-version",
    "x-liyasa-locale",
    "x-liyasa-product",
    "x-liyasa-groups",
    "x-liyasa-region",
    "x-liyasa-variation",
    "accept",
];

/// One reader, as a set of request headers and as the variant they resolve to.
#[derive(Debug, Clone)]
struct Reader {
    subject: &'static str,
    version: &'static str,
    locale: &'static str,
    product: &'static str,
    groups: &'static str,
    region: &'static str,
    variation: &'static str,
    accept: &'static str,
}

impl Reader {
    fn base() -> Self {
        Self {
            subject: "reader-a",
            version: "v2",
            locale: "en",
            product: "cloud",
            groups: "partner",
            region: "EU",
            variation: "b",
            accept: "text/html",
        }
    }

    fn request(&self) -> Request {
        Request::new(URL)
            .header("x-liyasa-version", self.version)
            .header("x-liyasa-locale", self.locale)
            .header("x-liyasa-product", self.product)
            .header("x-liyasa-groups", self.groups)
            .header("x-liyasa-region", self.region)
            .header("x-liyasa-variation", self.variation)
            .header("accept", self.accept)
    }

    fn variant(&self) -> Variant {
        Variant {
            version: Some(Version::new(self.version)),
            locale: Some(Locale::new(self.locale)),
            product: Some(self.product.to_owned()),
            groups: self
                .groups
                .split(',')
                .filter(|g| !g.is_empty())
                .map(str::to_owned)
                .collect(),
            region: Some(self.region.to_owned()),
            variation: Some(self.variation.to_owned()),
        }
    }

    fn format(&self) -> OutputFormat {
        match self.accept.contains("markdown") {
            true => OutputFormat::Markdown,
            false => OutputFormat::Html,
        }
    }

    fn key(&self, page: PageId, on_demand: bool) -> CacheKey {
        let key = CacheKey::new(build("build-a"), page, self.format(), self.variant())
            .with_reader_fields(ReaderFields::of(["groups"]));
        match on_demand {
            true => key.for_subject(self.subject),
            false => key,
        }
    }
}

/// The server behind the CDN: a variant LRU keyed by AUTH-13's key, rendering
/// on a miss. It is the same code path the in-process test drives, with the
/// CDN in front of it.
struct Origin {
    page: PageId,
    cache: VariantCache,
    on_demand: bool,
}

impl Origin {
    fn new(on_demand: bool) -> Self {
        Self {
            page: PageId(liyasa_store::new_ulid()),
            cache: VariantCache::new(NonZeroUsize::new(256).expect("a capacity")),
            on_demand,
        }
    }

    fn serve(&self, reader: &Reader) -> Response {
        let key = reader.key(self.page, self.on_demand);
        let entry = self.cache.get(&key).unwrap_or_else(|| {
            // The body names the whole key, so a reader served someone else's
            // rendering fails the assertion with both keys in the message.
            let entry = Entry::new(&key, format!("rendered for {}", key.canonical()));
            self.cache.put(&key, entry.clone());
            entry
        });
        let cache_control = entry.cache_control();
        let mut response = Response::new(entry.body.clone(), cache_control).vary(VARY);
        for tag in &entry.tags {
            response = response.tag(tag);
        }
        response
    }

    fn expected_body(&self, reader: &Reader) -> String {
        format!(
            "rendered for {}",
            reader.key(self.page, self.on_demand).canonical()
        )
    }
}

/// The name of a key component and the edit that changes it.
type Change = (&'static str, fn(&mut Reader));

fn readers_differing_in_one_component() -> Vec<(&'static str, Reader)> {
    let each: Vec<Change> = vec![
        ("version", |r| r.version = "v3"),
        ("locale", |r| r.locale = "de"),
        ("product (custom dimension)", |r| r.product = "selfhost"),
        ("variation (custom dimension)", |r| r.variation = "a"),
        ("a group", |r| r.groups = "staff"),
        ("region", |r| r.region = "US"),
        ("output format", |r| r.accept = "text/markdown"),
        ("reader subject", |r| r.subject = "reader-b"),
    ];
    each.into_iter()
        .map(|(name, apply)| {
            let mut reader = Reader::base();
            apply(&mut reader);
            (name, reader)
        })
        .collect()
}

#[test]
fn a_variant_page_through_the_cdn_never_serves_one_reader_another_reader_s_variant() {
    let origin = Origin::new(false);
    let base = Reader::base();

    for (component, other) in readers_differing_in_one_component() {
        // A fresh CDN per component, so a hit can only come from this pair.
        let cdn = Cdn::new();
        let first = cdn.fetch(&base.request(), |r| {
            let _ = r;
            origin.serve(&base)
        });
        assert_eq!(cdn.last(), Hit::Miss);
        assert_eq!(first.body, origin.expected_body(&base));

        let second = cdn.fetch(&other.request(), |r| {
            let _ = r;
            origin.serve(&other)
        });
        if component == "reader subject" {
            // A variant page is not bound to a subject: two readers of the
            // same variant share, which is the point of pre-rendering.
            assert_eq!(cdn.last(), Hit::Hit, "{component}");
            assert_eq!(second.body, first.body, "{component}");
            continue;
        }
        assert_eq!(
            cdn.last(),
            Hit::Miss,
            "{component}: the CDN aliased two variants"
        );
        assert_ne!(second.body, first.body, "{component}");
        assert_eq!(second.body, origin.expected_body(&other), "{component}");
    }
}

#[test]
fn an_on_demand_page_is_never_stored_in_the_shared_cache_at_all() {
    let origin = Origin::new(true);
    let base = Reader::base();
    let cdn = Cdn::new();

    let first = cdn.fetch(&base.request(), |_| origin.serve(&base));
    assert_eq!(
        cdn.last(),
        Hit::Uncacheable,
        "an on-demand rendering is `private` and a shared cache may not hold it"
    );
    assert!(cdn.is_empty(), "nothing was stored");
    assert_eq!(first.body, origin.expected_body(&base));

    // Every other reader goes to the origin and gets their own rendering.
    for (component, other) in readers_differing_in_one_component() {
        let response = cdn.fetch(&other.request(), |_| origin.serve(&other));
        assert_eq!(cdn.last(), Hit::Uncacheable, "{component}");
        assert_eq!(response.body, origin.expected_body(&other), "{component}");
        assert_ne!(response.body, first.body, "{component}");
    }
}

#[test]
fn a_deploy_purge_never_causes_a_private_response_to_be_served_from_the_shared_cache() {
    // Both page shapes behind one CDN, then a deploy.
    let shared = Origin::new(false);
    let personal = Origin::new(true);
    let reader = Reader::base();
    let other = Reader {
        subject: "reader-b",
        ..Reader::base()
    };
    let cdn = Cdn::new();

    let public_body = cdn.fetch(&reader.request(), |_| shared.serve(&reader)).body;
    assert_eq!(cdn.last(), Hit::Miss);
    assert_eq!(cdn.len(), 1);

    let personal_body = cdn
        .fetch(&reader.request(), |_| personal.serve(&reader))
        .body;
    assert_eq!(
        cdn.last(),
        Hit::Uncacheable,
        "a private response must not displace or join the shared entry"
    );
    assert_eq!(
        cdn.len(),
        1,
        "the shared cache still holds only the public entry"
    );
    assert_ne!(personal_body, public_body);

    // The deploy purges the old build's tag. Everything of that build goes.
    let purged = cdn.purge_tag(&reader.key(shared.page, false).build_tag());
    assert_eq!(purged, 1);
    assert!(cdn.is_empty());

    // After the purge, the next reader is served from the origin — and the
    // body they get is theirs, not the private one that was in flight.
    let after = cdn.fetch(&other.request(), |_| personal.serve(&other));
    assert_eq!(cdn.last(), Hit::Uncacheable);
    assert_eq!(after.body, personal.expected_body(&other));
    assert_ne!(
        after.body, personal_body,
        "a purge must not let one reader's private rendering reach another"
    );
    assert!(
        cdn.is_empty(),
        "a private response is still not stored after a purge"
    );
}

#[test]
fn a_purge_of_another_builds_tag_leaves_this_builds_entries_alone() {
    let origin = Origin::new(false);
    let reader = Reader::base();
    let cdn = Cdn::new();
    cdn.fetch(&reader.request(), |_| origin.serve(&reader));
    assert_eq!(cdn.len(), 1);

    assert_eq!(cdn.purge_tag("build-somebody-elses"), 0);
    assert_eq!(cdn.len(), 1);
    cdn.fetch(&reader.request(), |_| origin.serve(&reader));
    assert_eq!(cdn.last(), Hit::Hit);
}

#[test]
fn a_response_the_origin_marks_private_is_never_storable_in_a_shared_cache() {
    // The property the purge test leans on, stated on its own.
    assert!(Response::new("b", "public, max-age=0, must-revalidate").storable_in_shared_cache());
    for refusing in [
        "private, no-store",
        "private",
        "no-store",
        "Private, max-age=60",
    ] {
        assert!(
            !Response::new("b", refusing).storable_in_shared_cache(),
            "{refusing}"
        );
    }
}
