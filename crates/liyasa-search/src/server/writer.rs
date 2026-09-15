//! Building and updating the tantivy index.
//!
//! One `add` per section document, pre-tokenized by `idx::tokenize` so both
//! indexes hold the same terms, and one `delete_term` on the section's key so
//! a rebuild replaces a page rather than duplicating it (SRC-07).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use tantivy::merge_policy::{LogMergePolicy, NoMergePolicy};
use tantivy::schema::Value;
use tantivy::tokenizer::{PreTokenizedString, Token};
use tantivy::{Index, IndexWriter, TantivyDocument, Term};

use super::schema::SearchSchema;
use crate::doc::SectionDocument;
use crate::idx::field::{ByField, Field};
use crate::idx::score::Stats;
use crate::idx::tokenize::{self, Tokenizer};

/// Corpus statistics, kept beside the index because BM25 needs them and
/// tantivy's own are per-segment (plan/rfcs/0703-server-ranks-with-idx.md).
///
/// `document_frequency` is deliberately term-level rather than per field:
/// tantivy's `doc_freq` counts documents holding a term *in one field*, and
/// scoring a term as rare in `title` and common in `body` is exactly the
/// divergence §12.2's parity gate exists to catch. The browser index stores
/// the same number, once per term, in its postings header.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct IndexStats {
    pub documents: u64,
    pub total_length: ByField<u64>,
    pub document_frequency: BTreeMap<String, u32>,
}

impl IndexStats {
    pub fn stats(&self) -> Stats {
        let count = self.documents.max(1) as f32;
        let mut average_length = ByField([0.0f32; 6]);
        for field in Field::ALL {
            average_length[field] = self.total_length[field] as f32 / count;
        }
        Stats {
            documents: self.documents,
            average_length,
            ..Stats::default()
        }
    }

    /// Corpus-wide document frequency, the `df` of BM25.
    pub fn document_frequency(&self, term: &str) -> Option<u32> {
        self.document_frequency.get(term).copied()
    }

    fn record(&mut self, indexed: &Indexed) {
        self.documents += 1;
        for field in Field::ALL {
            self.total_length[field] += u64::from(indexed.lengths[field]);
        }
        for term in &indexed.terms {
            *self.document_frequency.entry(term.clone()).or_default() += 1;
        }
    }

    fn forget(&mut self, indexed: &Indexed) {
        self.documents = self.documents.saturating_sub(1);
        for field in Field::ALL {
            self.total_length[field] =
                self.total_length[field].saturating_sub(u64::from(indexed.lengths[field]));
        }
        for term in &indexed.terms {
            if let Some(count) = self.document_frequency.get_mut(term) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    self.document_frequency.remove(term);
                }
            }
        }
    }
}

/// What one indexed section contributed to the corpus statistics, kept so
/// removing it subtracts exactly that.
#[derive(Debug, Clone, Default, PartialEq)]
struct Indexed {
    lengths: ByField<u32>,
    terms: BTreeSet<String>,
}

/// How segments are merged (SRC-11).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Merges {
    /// Production: the build writes the index once into an immutable,
    /// content-addressed bundle and the server opens it read-only, so there is
    /// nothing to merge while it is being served.
    #[default]
    Never,
    /// `liyasa dev`: a log merge policy whose merges are deferred to idle time,
    /// so a rebuild never waits on one and §6.6's reload budget is not charged
    /// for indexing.
    Deferred,
}

pub struct ServerIndex {
    pub index: Index,
    pub schema: SearchSchema,
    merges: Merges,
    stats: IndexStats,
    indexed: BTreeMap<String, Indexed>,
}

impl ServerIndex {
    pub fn in_memory() -> Self {
        let schema = SearchSchema::new();
        Self {
            index: Index::create_in_ram(schema.schema.clone()),
            schema,
            merges: Merges::default(),
            stats: IndexStats::default(),
            indexed: BTreeMap::new(),
        }
    }

    /// The bundle's index: written once per build, opened read-only by the
    /// server, swapped on deploy (SRC-11).
    pub fn create_in_dir(path: &Path) -> tantivy::Result<Self> {
        let schema = SearchSchema::new();
        Ok(Self {
            index: Index::create_in_dir(path, schema.schema.clone())?,
            schema,
            merges: Merges::default(),
            stats: IndexStats::default(),
            indexed: BTreeMap::new(),
        })
    }

    #[must_use]
    pub fn with_merges(mut self, merges: Merges) -> Self {
        self.merges = merges;
        self
    }

    pub fn merges(&self) -> Merges {
        self.merges
    }

    pub fn stats(&self) -> &IndexStats {
        &self.stats
    }

    /// A writer with this index's merge policy applied.
    fn writer(&self) -> tantivy::Result<IndexWriter> {
        let writer: IndexWriter = self.index.writer(50_000_000)?;
        match self.merges {
            Merges::Never => writer.set_merge_policy(Box::new(NoMergePolicy)),
            Merges::Deferred => writer.set_merge_policy(Box::new(LogMergePolicy::default())),
        }
        Ok(writer)
    }

    /// Runs the merges `Merges::Deferred` put off, at a moment the caller
    /// judges idle. A no-op in production, where nothing is deferred.
    pub fn merge_now(&self) -> tantivy::Result<()> {
        if self.merges == Merges::Never {
            return Ok(());
        }
        let mut writer: IndexWriter = self.index.writer(50_000_000)?;
        writer.set_merge_policy(Box::new(LogMergePolicy::default()));
        writer.commit()?;
        Ok(())
    }

    /// Keys currently in the index, in order. SRC-07 compares this with the
    /// build's section list to decide what to re-index.
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.indexed.keys()
    }

    pub fn contains(&self, key: &str) -> bool {
        self.indexed.contains_key(key)
    }

    /// Adds or replaces every document, then commits.
    pub fn index_all(&mut self, documents: &[SectionDocument]) -> tantivy::Result<()> {
        let mut writer = self.writer()?;
        for document in documents {
            self.write(&mut writer, document);
        }
        writer.commit()?;
        Ok(())
    }

    /// Re-indexes only what changed, and removes what is gone (SRC-07).
    ///
    /// Returns the keys it actually wrote, which is what the acceptance test
    /// reads as the writer log.
    pub fn index_changed(
        &mut self,
        documents: &[SectionDocument],
        changed: &dyn Fn(&SectionDocument) -> bool,
    ) -> tantivy::Result<Vec<String>> {
        let mut writer = self.writer()?;
        let mut written = Vec::new();
        let present: Vec<String> = documents.iter().map(SectionDocument::key).collect();

        for key in self.indexed.keys().cloned().collect::<Vec<_>>() {
            if !present.contains(&key) {
                self.remove(&mut writer, &key);
            }
        }
        for document in documents {
            let key = document.key();
            if self.indexed.contains_key(&key) && !changed(document) {
                continue;
            }
            self.write(&mut writer, document);
            written.push(key);
        }
        writer.commit()?;
        Ok(written)
    }

    fn remove(&mut self, writer: &mut IndexWriter, key: &str) {
        if let Some(indexed) = self.indexed.remove(key) {
            self.stats.forget(&indexed);
        }
        writer.delete_term(Term::from_field_text(self.schema.key, key));
    }

    fn write(&mut self, writer: &mut IndexWriter, document: &SectionDocument) {
        let key = document.key();
        self.remove(writer, &key);

        let prose = Tokenizer::for_locale(document.locale.as_str());
        let breadcrumb = document.breadcrumb.join(" ");
        let keywords = document.keywords.join(" ");
        let texts = [
            (Field::Title, document.title.clone()),
            (Field::Section, document.section.clone()),
            (Field::Keywords, keywords),
            (Field::Breadcrumb, breadcrumb),
            (Field::Body, document.body.clone()),
        ];

        let mut tantivy_document = TantivyDocument::new();
        let mut indexed = Indexed::default();
        for (field, text) in texts {
            let tokens = prose.tokenize(&text);
            indexed.lengths[field] = tokens.len() as u32;
            indexed.terms.extend(tokens.iter().map(|t| t.text.clone()));
            tantivy_document.add_pre_tokenized_text(
                self.schema.text[field],
                PreTokenizedString {
                    text,
                    tokens: tokens.into_iter().map(into_tantivy).collect(),
                },
            );
        }
        let code = tokenize::code(&document.code);
        indexed.lengths[Field::Code] = code.len() as u32;
        indexed.terms.extend(code.iter().map(|t| t.text.clone()));
        tantivy_document.add_pre_tokenized_text(
            self.schema.text[Field::Code],
            PreTokenizedString {
                text: document.code.clone(),
                tokens: code.into_iter().map(into_tantivy).collect(),
            },
        );

        for field in Field::ALL {
            tantivy_document.add_u64(
                self.schema.lengths[field],
                u64::from(indexed.lengths[field]),
            );
        }
        tantivy_document.add_text(self.schema.key, &key);
        tantivy_document.add_text(
            self.schema.payload,
            serde_json::to_string(document).unwrap_or_default(),
        );
        tantivy_document.add_f64(self.schema.boost, f64::from(document.boost));
        tantivy_document.add_u64(self.schema.updated, document.updated.unwrap_or(0));
        tantivy_document.add_text(self.schema.tab, document.tab.as_deref().unwrap_or_default());
        tantivy_document.add_text(
            self.schema.version,
            document.version.as_ref().map_or("", |v| v.as_str()),
        );
        tantivy_document.add_text(self.schema.locale, document.locale.as_str());
        tantivy_document.add_text(self.schema.kind, document.kind.as_str());
        for group in &document.groups {
            tantivy_document.add_text(self.schema.groups, group);
        }
        for region in &document.regions {
            tantivy_document.add_text(self.schema.regions, region);
        }

        self.stats.record(&indexed);
        self.indexed.insert(key, indexed);
        // `add_document` fails only when the writer's channel is closed, which
        // happens when a worker thread panicked; the next `commit` reports it.
        let _ = writer.add_document(tantivy_document);
    }

    /// The stored section document behind a hit.
    pub fn payload(&self, document: &TantivyDocument) -> Option<SectionDocument> {
        let value = document.get_first(self.schema.payload)?.as_str()?;
        serde_json::from_str(value).ok()
    }
}

fn into_tantivy(token: crate::idx::tokenize::Token) -> Token {
    Token {
        offset_from: token.start as usize,
        offset_to: token.end as usize,
        position: token.position as usize,
        text: token.text,
        position_length: 1,
    }
}
