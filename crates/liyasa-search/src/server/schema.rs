//! The tantivy schema (SRC-06, SRC-11).
//!
//! Text fields are always written pre-tokenized, by the same module that
//! writes the browser index (§12.2's Parity row), so tantivy never runs a
//! tokenizer of its own and the two term dictionaries hold the same strings.
//! Field lengths are stored exactly rather than through tantivy's quantized
//! fieldnorm, because the browser keeps them exactly and both must normalize
//! alike (plan/rfcs/0703-server-ranks-with-idx.md).

use tantivy::schema::{
    FAST, Field, INDEXED, IndexRecordOption, STORED, STRING, Schema, TextFieldIndexing, TextOptions,
};

use crate::idx::field::{ByField, Field as TextField};

/// The name every text field claims. Nothing is registered under it: a
/// pre-tokenized value never reaches a tokenizer, and tantivy's default
/// manager already holds a `raw` entry for the string fields.
pub const TOKENIZER: &str = "raw";

/// `route#anchor`, the identity SRC-07 re-indexes by.
pub const KEY: &str = "key";
pub const PAYLOAD: &str = "payload";
pub const BOOST: &str = "boost";
pub const UPDATED: &str = "updated";
pub const TAB: &str = "tab";
pub const VERSION: &str = "version";
pub const LOCALE: &str = "locale";
pub const KIND: &str = "type";
pub const GROUPS: &str = "groups";
pub const REGIONS: &str = "regions";

pub fn length_field(field: TextField) -> String {
    format!("{}_len", field.as_str())
}

#[derive(Debug, Clone)]
pub struct SearchSchema {
    pub schema: Schema,
    pub text: ByField<Field>,
    pub lengths: ByField<Field>,
    pub key: Field,
    pub payload: Field,
    pub boost: Field,
    pub updated: Field,
    pub tab: Field,
    pub version: Field,
    pub locale: Field,
    pub kind: Field,
    pub groups: Field,
    pub regions: Field,
}

impl Default for SearchSchema {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchSchema {
    pub fn new() -> Self {
        let mut builder = Schema::builder();
        let indexing = TextFieldIndexing::default()
            .set_tokenizer(TOKENIZER)
            // Positions are what an exact-phrase bonus is computed from.
            .set_index_option(IndexRecordOption::WithFreqsAndPositions);
        let text_options = TextOptions::default().set_indexing_options(indexing);

        let mut text = ByField([Field::from_field_id(0); 6]);
        for field in TextField::ALL {
            text[field] = builder.add_text_field(field.as_str(), text_options.clone());
        }
        let mut lengths = ByField([Field::from_field_id(0); 6]);
        for field in TextField::ALL {
            lengths[field] = builder.add_u64_field(&length_field(field), FAST | INDEXED);
        }
        let key = builder.add_text_field(KEY, STRING | STORED);
        let payload = builder.add_text_field(PAYLOAD, STORED);
        let boost = builder.add_f64_field(BOOST, FAST);
        let updated = builder.add_u64_field(UPDATED, FAST);
        let tab = builder.add_text_field(TAB, STRING | STORED);
        let version = builder.add_text_field(VERSION, STRING | STORED);
        let locale = builder.add_text_field(LOCALE, STRING | STORED);
        let kind = builder.add_text_field(KIND, STRING | STORED);
        let groups = builder.add_text_field(GROUPS, STRING | STORED);
        let regions = builder.add_text_field(REGIONS, STRING | STORED);

        Self {
            schema: builder.build(),
            text,
            lengths,
            key,
            payload,
            boost,
            updated,
            tab,
            version,
            locale,
            kind,
            groups,
            regions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_scored_field_is_indexed_with_positions() {
        let schema = SearchSchema::new();
        for field in TextField::ALL {
            let entry = schema.schema.get_field_entry(schema.text[field]);
            assert!(entry.is_indexed(), "{} is not indexed", field.as_str());
            let options = entry
                .field_type()
                .get_index_record_option()
                .unwrap_or(IndexRecordOption::Basic);
            assert!(
                options.has_positions(),
                "{} has no positions, so no phrase bonus",
                field.as_str()
            );
        }
    }

    #[test]
    fn every_scored_field_has_an_exact_length_column() {
        let schema = SearchSchema::new();
        for field in TextField::ALL {
            let entry = schema.schema.get_field_entry(schema.lengths[field]);
            assert!(entry.is_fast(), "{} has no length column", field.as_str());
        }
    }
}
