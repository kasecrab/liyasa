//! AUTH-13: variant cache key completeness.
//!
//! Two readers differing in exactly one component of the key — each component
//! in turn — must never receive the same cache entry, in the build-time
//! pre-rendered set and in the server's variant LRU alike. The CDN layer the
//! requirement also names is in `auth_13_cdn.rs`.
//!
//! The components are enumerated below rather than spot-checked, and the
//! "`Variant` contains every field" assertion is driven by the struct's own
//! serialization, so a field added to `Variant` fails this test instead of
//! quietly leaving the key.

use std::collections::BTreeSet;
use std::num::NonZeroUsize;

use liyasa_core::build::{OutputFormat, Variant};
use liyasa_core::ids::{BuildId, Fingerprint, Locale, PageId, Version};
use liyasa_server::auth::variant::{
    CacheKey, Entry, PrerenderedSet, ReaderFields, VariantCache, canonical,
};

fn build(seed: &str) -> BuildId {
    BuildId(Fingerprint::of(seed))
}

fn page() -> PageId {
    PageId(liyasa_store::new_ulid())
}

/// A variant with every field set, so that changing any one of them is a
/// change from something rather than from nothing.
fn full_variant() -> Variant {
    Variant {
        version: Some(Version::new("v2")),
        locale: Some(Locale::new("en")),
        product: Some("cloud".to_owned()),
        groups: ["partner".to_owned(), "staff".to_owned()].into(),
        region: Some("EU".to_owned()),
        variation: Some("b".to_owned()),
    }
}

/// One way two readers can differ, and the name AUTH-13 gives it.
struct Component {
    name: &'static str,
    apply: fn(CacheKey) -> CacheKey,
}

fn components() -> Vec<Component> {
    vec![
        Component {
            name: "version",
            apply: |mut key| {
                key.variant.version = Some(Version::new("v3"));
                key
            },
        },
        Component {
            name: "locale",
            apply: |mut key| {
                key.variant.locale = Some(Locale::new("de"));
                key
            },
        },
        // `product` and `variation` are the custom dimensions of §6.6.3: the
        // per-site coordinates beyond version, locale and region.
        Component {
            name: "product (custom dimension)",
            apply: |mut key| {
                key.variant.product = Some("selfhost".to_owned());
                key
            },
        },
        Component {
            name: "variation (custom dimension)",
            apply: |mut key| {
                key.variant.variation = Some("a".to_owned());
                key
            },
        },
        Component {
            name: "a group added",
            apply: |mut key| {
                key.variant.groups.insert("admin".to_owned());
                key
            },
        },
        Component {
            name: "a group removed",
            apply: |mut key| {
                key.variant.groups.remove("staff");
                key
            },
        },
        Component {
            name: "a group replaced",
            apply: |mut key| {
                key.variant.groups.remove("staff");
                key.variant.groups.insert("admin".to_owned());
                key
            },
        },
        Component {
            name: "region",
            apply: |mut key| {
                key.variant.region = Some("US".to_owned());
                key
            },
        },
        Component {
            name: "output format",
            apply: |mut key| {
                key.format = OutputFormat::Markdown;
                key
            },
        },
        Component {
            name: "reader subject",
            apply: |mut key| {
                key.subject = Some("reader-b".to_owned());
                key
            },
        },
        Component {
            name: "the recorded reader-field set",
            apply: |mut key| {
                key.reader_fields = ReaderFields::of(["groups", "plan"]);
                key
            },
        },
        Component {
            name: "build id",
            apply: |mut key| {
                key.build = build("build-b");
                key
            },
        },
        Component {
            name: "page id",
            apply: |mut key| {
                key.page = page();
                key
            },
        },
    ]
}

fn variant_page_key() -> CacheKey {
    CacheKey::new(build("build-a"), page(), OutputFormat::Html, full_variant())
        .with_reader_fields(ReaderFields::of(["groups"]))
}

fn on_demand_key() -> CacheKey {
    variant_page_key().for_subject("reader-a")
}

#[test]
fn the_variant_struct_in_the_key_contains_every_field_it_has() {
    // Driven by the struct rather than by a list written here: a field added
    // to `Variant` appears in this value and must appear in the key.
    let value = serde_json::to_value(full_variant()).expect("a variant serializes");
    let fields = value.as_object().expect("an object");
    assert_eq!(fields.len(), 6, "the frozen `Variant` has six fields");

    let serialized = canonical(&full_variant());
    for name in fields.keys() {
        assert!(
            serialized.contains(&format!("{name}=")),
            "`{name}` is a field of Variant and is not in the key: {serialized}"
        );
    }
}

#[test]
fn a_field_the_page_does_not_read_is_still_normalized_into_the_key() {
    // AUTH-13: every field, "including those the page does not read,
    // normalized to their defaults". An empty variant therefore still names
    // all six, so a later variant that sets one cannot alias it.
    let empty = canonical(&Variant::default());
    for name in [
        "version",
        "locale",
        "product",
        "groups",
        "region",
        "variation",
    ] {
        assert!(empty.contains(&format!("{name}=")), "{name}: {empty}");
    }
    assert_ne!(empty, canonical(&full_variant()));
}

#[test]
fn two_readers_differing_in_exactly_one_component_never_share_a_key() {
    for base in [variant_page_key(), on_demand_key()] {
        for component in components() {
            let other = (component.apply)(base.clone());
            assert_ne!(base, other, "{}: the keys compare equal", component.name);
            assert_ne!(
                base.canonical(),
                other.canonical(),
                "{}: the keys serialize the same",
                component.name
            );
            assert_ne!(
                base.digest(),
                other.digest(),
                "{}: the keys digest the same",
                component.name
            );
        }
    }
}

#[test]
fn two_readers_differing_in_exactly_one_component_never_share_a_server_lru_entry() {
    for base in [variant_page_key(), on_demand_key()] {
        for component in components() {
            let cache = VariantCache::new(NonZeroUsize::new(64).expect("a capacity"));
            let other = (component.apply)(base.clone());
            cache.put(&base, Entry::new(&base, "reader a's page"));
            assert_eq!(
                cache.get(&other),
                None,
                "{}: the LRU served one reader's rendering to the other",
                component.name
            );
            cache.put(&other, Entry::new(&other, "reader b's page"));
            assert_eq!(
                cache.get(&base).map(|e| e.body),
                Some("reader a's page".to_owned()),
                "{}: the second write displaced the first",
                component.name
            );
            assert_eq!(cache.len(), 2, "{}", component.name);
        }
    }
}

#[test]
fn two_readers_differing_in_exactly_one_component_never_share_a_prerendered_entry() {
    for base in [variant_page_key(), on_demand_key()] {
        for component in components() {
            let mut set = PrerenderedSet::new();
            let other = (component.apply)(base.clone());
            set.insert(&base, Entry::new(&base, "reader a's page"));
            assert!(
                set.get(&other).is_none(),
                "{}: the build-time set served one reader's rendering to the other",
                component.name
            );
            set.insert(&other, Entry::new(&other, "reader b's page"));
            assert_eq!(set.len(), 2, "{}", component.name);
        }
    }
}

#[test]
fn a_group_set_cannot_be_spelled_by_a_group_name() {
    // The failure mode the length prefixes exist for: `{"a", "b"}` joined by a
    // separator is the same text as `{"a<sep>b"}` unless each part carries its
    // length. `liyasa_build::variants::key` joins groups with `+`.
    let two: BTreeSet<String> = ["a".to_owned(), "b".to_owned()].into();
    let one_named_like_two: BTreeSet<String> = ["a+b".to_owned()].into();
    let comma: BTreeSet<String> = ["a,b".to_owned()].into();
    for spelling in [one_named_like_two, comma] {
        let first = Variant {
            groups: two.clone(),
            ..Variant::default()
        };
        let second = Variant {
            groups: spelling.clone(),
            ..Variant::default()
        };
        assert_ne!(canonical(&first), canonical(&second), "{spelling:?}");
    }
}

#[test]
fn one_fields_value_cannot_spell_another_fields_tag() {
    let sneaky = Variant {
        version: Some(Version::new("v2;locale=2:de;")),
        ..Variant::default()
    };
    let honest = Variant {
        version: Some(Version::new("v2")),
        locale: Some(Locale::new("de")),
        ..Variant::default()
    };
    assert_ne!(canonical(&sneaky), canonical(&honest));
}

#[test]
fn an_absent_field_and_an_empty_one_select_the_same_rendering() {
    // Normalizing to the default rather than omitting means these are the same
    // key, which is correct: neither names a product.
    let absent = Variant::default();
    let empty = Variant {
        product: Some(String::new()),
        ..Variant::default()
    };
    assert_eq!(canonical(&absent), canonical(&empty));
}

#[test]
fn a_shared_entry_and_a_subject_bound_one_are_different_keys() {
    let shared = variant_page_key();
    let bound = shared.clone().for_subject("");
    assert_ne!(shared.canonical(), bound.canonical());
    assert!(!shared.is_on_demand());
    assert!(bound.is_on_demand());
}

#[test]
fn an_entry_is_private_when_and_only_when_it_is_bound_to_a_reader() {
    let shared = Entry::new(&variant_page_key(), "body");
    assert!(!shared.private);
    assert!(shared.cache_control().contains("public"));

    let bound = Entry::new(&on_demand_key(), "body");
    assert!(bound.private);
    assert!(bound.cache_control().contains("private"));
    assert!(bound.cache_control().contains("no-store"));
}

#[test]
fn the_reader_field_set_is_a_set_rather_than_a_sequence() {
    assert_eq!(
        ReaderFields::of(["groups", "region"]),
        ReaderFields::of(["region", "groups"]),
        "order must not change the key"
    );
    assert_eq!(
        ReaderFields::of(["groups", "groups"]),
        ReaderFields::of(["groups"]),
        "a repeated field is the same set"
    );
    assert_ne!(ReaderFields::none(), ReaderFields::of(["groups"]));
    assert_ne!(ReaderFields::of(["ab", "c"]), ReaderFields::of(["a", "bc"]));
}

#[test]
fn an_lru_eviction_never_turns_into_a_wrong_answer() {
    let cache = VariantCache::new(NonZeroUsize::new(2).expect("a capacity"));
    let a = variant_page_key();
    let b = (components()[0].apply)(a.clone());
    let c = (components()[1].apply)(a.clone());
    cache.put(&a, Entry::new(&a, "a"));
    cache.put(&b, Entry::new(&b, "b"));
    cache.put(&c, Entry::new(&c, "c"));

    assert_eq!(cache.len(), 2, "the capacity is honoured");
    // Whatever survived, it is its own body and never another key's.
    for (key, body) in [(&a, "a"), (&b, "b"), (&c, "c")] {
        if let Some(entry) = cache.get(key) {
            assert_eq!(entry.body, body);
        }
    }
}

#[test]
fn a_deploy_purge_drops_one_builds_entries_and_leaves_the_others() {
    let cache = VariantCache::new(NonZeroUsize::new(64).expect("a capacity"));
    let old = variant_page_key();
    let mut new = old.clone();
    new.build = build("build-b");
    cache.put(&old, Entry::new(&old, "old"));
    cache.put(&new, Entry::new(&new, "new"));

    assert_eq!(cache.purge_tag(&old.build_tag()), 1);
    assert_eq!(cache.get(&old), None);
    assert_eq!(cache.get(&new).map(|e| e.body), Some("new".to_owned()));
}

/// AUTH-11: a page with `:::visibility{groups="..."}` is rendered once per
/// group variant it references, and a rendered variant is never served to a
/// reader whose variant key differs.
#[test]
fn a_visibility_variant_is_never_served_to_a_reader_whose_key_differs() {
    use liyasa_server::auth::groups::{referenced_groups, variant_groups};
    use liyasa_server::auth::session::Principal;

    // The page mentions two groups, so it has four variants — not one per
    // subset of every group on the site.
    let referenced = referenced_groups(["admin,partner"]);
    assert_eq!(referenced.len(), 2);

    let page = page();
    let build = build("build-a");
    let key_for = |reader: Option<&Principal>| {
        let variant = Variant {
            groups: variant_groups(&referenced, reader),
            ..Variant::default()
        };
        CacheKey::new(build, page, OutputFormat::Html, variant)
            .with_reader_fields(ReaderFields::of(["groups"]))
    };
    // The body names the key it was rendered for, so a reader served somebody
    // else's rendering fails with both keys in the message.
    let body_for =
        |reader: Option<&Principal>| format!("rendered for {}", key_for(reader).canonical());

    let readers: Vec<(&str, Option<Principal>)> = vec![
        ("anonymous", None),
        (
            "admin only",
            Some(Principal::new("a").with_groups(["admin"])),
        ),
        (
            "partner only",
            Some(Principal::new("b").with_groups(["partner"])),
        ),
        (
            "both",
            Some(Principal::new("c").with_groups(["admin", "partner"])),
        ),
        // A reader whose only group the page never mentions gets the same
        // rendering as an anonymous reader, because the page has nothing
        // different to say to them (§6.6.3).
        (
            "an unmentioned group",
            Some(Principal::new("d").with_groups(["staff"])),
        ),
    ];

    let cache = VariantCache::new(NonZeroUsize::new(16).expect("a capacity"));
    for (_, reader) in &readers {
        let key = key_for(reader.as_ref());
        let body = body_for(reader.as_ref());
        cache.put(&key, Entry::new(&key, body));
    }
    assert_eq!(cache.len(), 4, "four variants and not five");

    for (name, reader) in &readers {
        let entry = cache
            .get(&key_for(reader.as_ref()))
            .unwrap_or_else(|| panic!("{name} has a rendering"));
        assert_eq!(entry.body, body_for(reader.as_ref()), "{name}");
    }

    // The unmentioned group is the anonymous variant, and the three group
    // variants are three.
    assert_eq!(
        body_for(readers[0].1.as_ref()),
        body_for(readers[4].1.as_ref())
    );
    let admin = key_for(readers[1].1.as_ref());
    let partner = key_for(readers[2].1.as_ref());
    let both = key_for(readers[3].1.as_ref());
    for (a, b) in [(&admin, &partner), (&admin, &both), (&partner, &both)] {
        assert_ne!(a.canonical(), b.canonical());
        assert_ne!(
            cache.get(a).map(|e| e.body),
            cache.get(b).map(|e| e.body),
            "two group variants shared a rendering"
        );
    }
}
