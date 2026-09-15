//! `manifest.json`: what the worker fetches first, and the only place global
//! ranking statistics live (§12.2).
//!
//! The manifest is JSON rather than a binary header on purpose: it is the file
//! an operator opens when a ranking looks wrong, and it is a couple of
//! kilobytes on a site that does not shard.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::field::{ByField, Field};

/// The format version the writer emits and the reader accepts. A reader
/// refuses anything higher with `E1002`.
pub const FORMAT_VERSION: u32 = 1;

/// How the builder chose to cut the corpus (§12.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShardKey {
    /// One shard: a small site.
    Single,
    Locale,
    LocaleVersion,
    LocaleVersionTab,
}

impl ShardKey {
    /// In the order the builder tries them, coarsest first.
    pub const ESCALATION: [ShardKey; 4] = [
        ShardKey::Single,
        ShardKey::Locale,
        ShardKey::LocaleVersion,
        ShardKey::LocaleVersionTab,
    ];
}

/// Which reading contexts a shard serves. An empty list matches every value,
/// which is how two under-sized shards merge without inventing a key.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShardSelector {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub locales: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tabs: Vec<String>,
}

/// The context a reader is searching from: the locale, version, and tab of the
/// page the dialog was opened on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Context {
    pub locale: Option<String>,
    pub version: Option<String>,
    pub tab: Option<String>,
}

impl ShardSelector {
    pub fn matches(&self, context: &Context) -> bool {
        fn holds(allowed: &[String], value: Option<&String>) -> bool {
            allowed.is_empty() || value.is_some_and(|v| allowed.iter().any(|a| a == v))
        }
        holds(&self.locales, context.locale.as_ref())
            && holds(&self.versions, context.version.as_ref())
            && holds(&self.tabs, context.tab.as_ref())
    }

    /// How many dimensions this selector pins. The reader prefers the most
    /// specific shard that matches, so a merged catch-all is the fallback
    /// rather than the first hit.
    pub fn specificity(&self) -> usize {
        usize::from(!self.locales.is_empty())
            + usize::from(!self.versions.is_empty())
            + usize::from(!self.tabs.is_empty())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShardFiles {
    pub terms: String,
    pub postings: String,
    pub docs: String,
    pub snippets: String,
}

impl ShardFiles {
    /// The four names of shard `n` (plan/rfcs/0702-idx-file-layout.md).
    pub fn of(n: u32) -> Self {
        Self {
            terms: format!("terms-{n}.fst"),
            postings: format!("postings-{n}.bin"),
            docs: format!("docs-{n}.bin"),
            snippets: format!("snippets-{n}.bin"),
        }
    }

    pub fn all(&self) -> [&str; 4] {
        [&self.terms, &self.postings, &self.docs, &self.snippets]
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shard {
    pub id: u32,
    pub selector: ShardSelector,
    pub documents: u64,
    pub files: ShardFiles,
    /// Uncompressed total of the four files, which is what the builder sizes
    /// against `search.shardSize`.
    pub bytes: u64,
    /// Length of `docs-<n>.bin`'s ranking prefix. The worker range-requests
    /// `bytes=0-<stats_bytes - 1>` before the first result and the rest once
    /// it knows which documents it is showing (§12.2, SRC-05).
    pub stats_bytes: u64,
    /// `blake3:…` over the four files in the order [`ShardFiles::all`] lists
    /// them, so a shard is immutable and cacheable by name.
    pub hash: String,
}

/// What the tokenizers were when the index was written. A reader whose
/// tokenizers differ would look up terms that are not there, so the values are
/// recorded rather than assumed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenizerConfig {
    pub stemmer: String,
    pub cjk: String,
    /// Locale to Snowball algorithm, or `"none"` where SRC-02 has none.
    pub locales: BTreeMap<String, String>,
}

impl Default for TokenizerConfig {
    fn default() -> Self {
        Self {
            stemmer: "snowball/rust-stemmers 1.2".to_owned(),
            cjk: "bigram".to_owned(),
            locales: BTreeMap::new(),
        }
    }
}

/// Global inverse document frequency, quantized (§12.2).
///
/// Only terms that occur in more than one shard are listed: a term confined to
/// one shard has a local document frequency that already is the global one, so
/// listing it would grow the file the worker downloads first and change no
/// ranking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdfTable {
    pub scale: u32,
    pub terms: BTreeMap<String, u32>,
}

impl Default for IdfTable {
    fn default() -> Self {
        Self {
            scale: 1024,
            terms: BTreeMap::new(),
        }
    }
}

impl IdfTable {
    pub fn insert(&mut self, term: &str, idf: f32) {
        let quantized = (idf.max(0.0) * self.scale as f32).round() as u32;
        self.terms.insert(term.to_owned(), quantized);
    }

    pub fn get(&self, term: &str) -> Option<f32> {
        let scale = if self.scale == 0 { 1 } else { self.scale };
        self.terms.get(term).map(|&q| q as f32 / scale as f32)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub version: u32,
    /// Corpus-wide document count, the `N` of BM25.
    pub documents: u64,
    /// Field weights, by name so the file reads.
    pub weights: BTreeMap<String, f32>,
    /// Mean term count per field across the whole corpus, never per shard.
    pub average_length: BTreeMap<String, f32>,
    pub tokenizer: TokenizerConfig,
    pub idf: IdfTable,
    pub shard_key: ShardKey,
    pub shards: Vec<Shard>,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            version: FORMAT_VERSION,
            documents: 0,
            weights: Field::ALL
                .into_iter()
                .map(|f| (f.as_str().to_owned(), f.weight()))
                .collect(),
            average_length: Field::ALL
                .into_iter()
                .map(|f| (f.as_str().to_owned(), 0.0))
                .collect(),
            tokenizer: TokenizerConfig::default(),
            idf: IdfTable::default(),
            shard_key: ShardKey::Single,
            shards: Vec::new(),
        }
    }
}

impl Manifest {
    pub fn weights(&self) -> ByField<f32> {
        let mut out = ByField([0.0; 6]);
        for field in Field::ALL {
            out[field] = self
                .weights
                .get(field.as_str())
                .copied()
                .unwrap_or_else(|| field.weight());
        }
        out
    }

    pub fn average_length(&self) -> ByField<f32> {
        let mut out = ByField([0.0; 6]);
        for field in Field::ALL {
            out[field] = self
                .average_length
                .get(field.as_str())
                .copied()
                .unwrap_or(0.0);
        }
        out
    }

    pub fn set_average_length(&mut self, lengths: ByField<f32>) {
        for field in Field::ALL {
            self.average_length
                .insert(field.as_str().to_owned(), lengths[field]);
        }
    }

    /// The shard to search from `context`: the most specific match, and the
    /// catch-all when nothing pins the context.
    pub fn shard_for(&self, context: &Context) -> Option<&Shard> {
        self.shards
            .iter()
            .filter(|shard| shard.selector.matches(context))
            .max_by_key(|shard| shard.selector.specificity())
    }

    /// Every shard, which is what a query with no context searches.
    pub fn all_shards(&self) -> &[Shard] {
        &self.shards
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shard(id: u32, selector: ShardSelector) -> Shard {
        Shard {
            id,
            selector,
            documents: 1,
            files: ShardFiles::of(id),
            bytes: 1,
            stats_bytes: 1,
            hash: "blake3:0".to_owned(),
        }
    }

    #[test]
    fn a_default_manifest_round_trips_through_json() {
        let manifest = Manifest::default();
        let json = serde_json::to_string(&manifest).expect("manifest serializes");
        let back: Manifest = serde_json::from_str(&json).expect("manifest parses");
        assert_eq!(back, manifest);
        assert_eq!(back.version, FORMAT_VERSION);
    }

    #[test]
    fn the_weights_survive_the_round_trip() {
        let manifest = Manifest::default();
        assert_eq!(manifest.weights()[Field::Title], 5.0);
        assert_eq!(manifest.weights()[Field::Code], 0.5);
    }

    #[test]
    fn an_empty_selector_matches_everything() {
        let selector = ShardSelector::default();
        assert!(selector.matches(&Context::default()));
        assert!(selector.matches(&Context {
            locale: Some("de".to_owned()),
            ..Context::default()
        }));
        assert_eq!(selector.specificity(), 0);
    }

    #[test]
    fn a_pinned_locale_matches_only_that_locale() {
        let selector = ShardSelector {
            locales: vec!["de".to_owned()],
            ..ShardSelector::default()
        };
        assert!(selector.matches(&Context {
            locale: Some("de".to_owned()),
            ..Context::default()
        }));
        assert!(!selector.matches(&Context {
            locale: Some("en".to_owned()),
            ..Context::default()
        }));
        assert!(
            !selector.matches(&Context::default()),
            "a reader with no locale cannot be served a locale-pinned shard"
        );
    }

    #[test]
    fn the_most_specific_matching_shard_wins() {
        let manifest = Manifest {
            shards: vec![
                shard(0, ShardSelector::default()),
                shard(
                    1,
                    ShardSelector {
                        locales: vec!["de".to_owned()],
                        ..ShardSelector::default()
                    },
                ),
            ],
            ..Manifest::default()
        };
        let german = Context {
            locale: Some("de".to_owned()),
            ..Context::default()
        };
        assert_eq!(manifest.shard_for(&german).map(|s| s.id), Some(1));
        assert_eq!(
            manifest.shard_for(&Context::default()).map(|s| s.id),
            Some(0)
        );
    }

    #[test]
    fn quantized_idf_survives_to_three_decimals() {
        let mut table = IdfTable::default();
        let idf = 2.593_17;
        table.insert("connect", idf);
        let back = table.get("connect").expect("the term is listed");
        assert!((back - idf).abs() < 0.001, "{back}");
        assert_eq!(table.get("absent"), None);
    }

    #[test]
    fn a_negative_idf_quantizes_to_zero() {
        let mut table = IdfTable::default();
        table.insert("the", -0.5);
        assert_eq!(table.get("the"), Some(0.0));
    }

    #[test]
    fn shard_files_are_named_after_their_shard() {
        assert_eq!(
            ShardFiles::of(3).all(),
            [
                "terms-3.fst",
                "postings-3.bin",
                "docs-3.bin",
                "snippets-3.bin"
            ]
        );
    }
}
