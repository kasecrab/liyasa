//! The variant cache key, and the server's variant LRU (AUTH-13).
//!
//! AUTH-13 is a completeness requirement, not a performance one: the key of
//! every cached rendering is
//!
//! ```text
//! (build id, page id, output format, complete Variant, reader-field-set hash,
//!  reader subject for an on-demand page)
//! ```
//!
//! and the `Variant` goes in whole — every field, including the ones the page
//! never reads, normalized to its default — so that a component nobody
//! remembered cannot make two readers share an entry.
//!
//! Two things here exist only to make that impossible to get wrong later:
//!
//! * [`canonical`] destructures `Variant` rather than reading its fields, so
//!   adding a field in `liyasa-core` fails this build instead of silently
//!   dropping out of the key.
//! * every part is length-prefixed, so no value of one field can spell the
//!   separator-and-next-field of another. `liyasa_build::variants::key` joins
//!   with `,` and `+` and omits defaults, which is why it is a lookup key for
//!   a page's own variant list and is not this.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Mutex;

use liyasa_core::build::{OutputFormat, Variant};
use liyasa_core::ids::{BuildId, Fingerprint, PageId};

/// The reader fields a page recorded reading, hashed. A page reading nothing
/// still has a hash, so "no fields" is a value rather than an absence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReaderFields(pub Fingerprint);

impl ReaderFields {
    pub fn of<'a>(fields: impl IntoIterator<Item = &'a str>) -> Self {
        // Sorted and deduplicated: the set is a set, and a build that records
        // it in a different order must not produce a different key.
        let mut names: Vec<&str> = fields.into_iter().collect();
        names.sort_unstable();
        names.dedup();
        Self(Fingerprint::of_parts(
            names.iter().map(|name| name.as_bytes()),
        ))
    }

    pub fn none() -> Self {
        Self::of(std::iter::empty())
    }

    pub fn to_hex(self) -> String {
        self.0.to_hex()
    }
}

/// The complete key. Derived `Eq` over every field is what makes the in-memory
/// map correct; [`CacheKey::canonical`] is what makes the file-system set and
/// the CDN tag correct.
///
/// No `Hash`: `OutputFormat` is a frozen core contract and does not implement
/// it, and the map below is a `BTreeMap` over the canonical string anyway,
/// which compares the stored key as well as the string it was filed under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheKey {
    pub build: BuildId,
    pub page: PageId,
    pub format: OutputFormat,
    pub variant: Variant,
    pub reader_fields: ReaderFields,
    /// `Some` for an on-demand page, whose rendering is that reader's and no
    /// one else's. `None` for a page whose variant set is enumerable, which
    /// every reader of that variant may share.
    pub subject: Option<String>,
}

impl CacheKey {
    pub fn new(build: BuildId, page: PageId, format: OutputFormat, variant: Variant) -> Self {
        Self {
            build,
            page,
            format,
            variant,
            reader_fields: ReaderFields::none(),
            subject: None,
        }
    }

    pub fn with_reader_fields(mut self, fields: ReaderFields) -> Self {
        self.reader_fields = fields;
        self
    }

    /// Binds the entry to one reader. AUTH-12: a page reading free-form
    /// `reader.*` fields is on demand, and its rendering is never shared.
    pub fn for_subject(mut self, subject: impl Into<String>) -> Self {
        self.subject = Some(subject.into());
        self
    }

    pub fn is_on_demand(&self) -> bool {
        self.subject.is_some()
    }

    /// The unambiguous serialization of the whole key.
    pub fn canonical(&self) -> String {
        let mut out = String::new();
        part(&mut out, "build", &self.build.to_string());
        part(&mut out, "page", &self.page.to_string());
        part(&mut out, "format", format_name(self.format));
        out.push_str(&canonical(&self.variant));
        part(&mut out, "fields", &self.reader_fields.to_hex());
        // A key with no subject and one whose subject is empty are different
        // keys: the tag distinguishes them before the value is written.
        match &self.subject {
            Some(subject) => part(&mut out, "subject", subject),
            None => part(&mut out, "shared", ""),
        }
        out
    }

    /// A fixed-length name for the same key, which is what a file in the
    /// pre-rendered set and a CDN cache tag are called.
    pub fn digest(&self) -> Fingerprint {
        Fingerprint::of(self.canonical())
    }

    /// The purge tag a deploy uses: everything of one build, and nothing of
    /// another (AUTH-13's purge clause).
    pub fn build_tag(&self) -> String {
        format!("build-{}", self.build)
    }
}

/// Every field of `Variant`, in full, length-prefixed.
///
/// The destructuring is load-bearing: `liyasa-core`'s `Variant` is frozen, and
/// if it is ever unfrozen and gains a field, this stops compiling rather than
/// quietly leaving the new field out of every cache key in the product.
pub fn canonical(variant: &Variant) -> String {
    let Variant {
        version,
        locale,
        product,
        groups,
        region,
        variation,
    } = variant;
    let mut out = String::new();
    // An absent field is normalized to its default — the empty string under
    // its own tag — rather than omitted. A field that is literally empty
    // yields the same key, which is correct: it selects the same rendering.
    part(
        &mut out,
        "version",
        version.as_ref().map_or("", |v| v.as_str()),
    );
    part(
        &mut out,
        "locale",
        locale.as_ref().map_or("", |v| v.as_str()),
    );
    part(&mut out, "product", product.as_deref().unwrap_or(""));
    // The count goes in as well as the members, so one group named like two
    // groups cannot spell them.
    part(&mut out, "groups", &groups.len().to_string());
    for group in groups {
        part(&mut out, "group", group);
    }
    part(&mut out, "region", region.as_deref().unwrap_or(""));
    part(&mut out, "variation", variation.as_deref().unwrap_or(""));
    out
}

fn part(out: &mut String, tag: &str, value: &str) {
    out.push_str(tag);
    out.push('=');
    out.push_str(&value.len().to_string());
    out.push(':');
    out.push_str(value);
    out.push(';');
}

pub fn format_name(format: OutputFormat) -> &'static str {
    match format {
        OutputFormat::Html => "html",
        OutputFormat::Markdown => "markdown",
    }
}

/// What a cached rendering is, plus the two things a shared cache needs to
/// hold it safely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub body: String,
    /// `private` for anything bound to a reader subject, `public` otherwise.
    /// Computed from the key rather than supplied, so it cannot disagree
    /// with it.
    pub private: bool,
    pub tags: Vec<String>,
}

impl Entry {
    pub fn new(key: &CacheKey, body: impl Into<String>) -> Self {
        Self {
            body: body.into(),
            private: key.is_on_demand(),
            tags: vec![key.build_tag()],
        }
    }

    pub fn cache_control(&self) -> &'static str {
        match self.private {
            true => "private, no-store",
            false => "public, max-age=0, must-revalidate",
        }
    }
}

/// The server's variant LRU (§6.6.3 item 5).
#[derive(Debug)]
pub struct VariantCache {
    inner: Mutex<Lru>,
}

#[derive(Debug)]
struct Lru {
    capacity: usize,
    /// Insertion counter; the smallest is the least recently used.
    tick: u64,
    entries: BTreeMap<String, (u64, CacheKey, Entry)>,
}

impl VariantCache {
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self {
            inner: Mutex::new(Lru {
                capacity: capacity.get(),
                tick: 0,
                entries: BTreeMap::new(),
            }),
        }
    }

    pub fn put(&self, key: &CacheKey, entry: Entry) {
        let mut lru = self.lock();
        lru.tick += 1;
        let tick = lru.tick;
        lru.entries
            .insert(key.canonical(), (tick, key.clone(), entry));
        while lru.entries.len() > lru.capacity {
            let Some(oldest) = lru
                .entries
                .iter()
                .min_by_key(|(_, (tick, _, _))| *tick)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            lru.entries.remove(&oldest);
        }
    }

    /// A hit only when every component of the key matches. The stored key is
    /// compared as well as the string it was filed under, so a serialization
    /// that ever aliased would still not serve the wrong reader.
    pub fn get(&self, key: &CacheKey) -> Option<Entry> {
        let mut lru = self.lock();
        lru.tick += 1;
        let tick = lru.tick;
        let slot = lru.entries.get_mut(&key.canonical())?;
        if &slot.1 != key {
            return None;
        }
        slot.0 = tick;
        Some(slot.2.clone())
    }

    /// A deploy purges by build tag.
    pub fn purge_tag(&self, tag: &str) -> usize {
        let mut lru = self.lock();
        let before = lru.entries.len();
        lru.entries
            .retain(|_, (_, _, entry)| !entry.tags.iter().any(|t| t == tag));
        before - lru.entries.len()
    }

    pub fn len(&self) -> usize {
        self.lock().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Lru> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// The pre-rendered set a build writes, addressed by the same key (AUTH-13
/// says "every entry in the pre-rendered variant set **and** in the server's
/// variant LRU", so there is one key type and not two).
#[derive(Debug, Default)]
pub struct PrerenderedSet {
    entries: BTreeMap<String, (CacheKey, Entry)>,
}

impl PrerenderedSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, key: &CacheKey, entry: Entry) {
        self.entries.insert(key.canonical(), (key.clone(), entry));
    }

    pub fn get(&self, key: &CacheKey) -> Option<&Entry> {
        let (stored, entry) = self.entries.get(&key.canonical())?;
        (stored == key).then_some(entry)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
