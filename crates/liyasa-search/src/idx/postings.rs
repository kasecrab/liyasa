//! `postings-<n>.bin`: one block per term, delta-encoded (§12.2).
//!
//! A block is self-contained and its byte range is the value the term FST
//! maps to, so the worker can fetch exactly one term's postings with an HTTP
//! range request and decode it without the rest of the file.

use super::field::Field;
use super::varint;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldPosting {
    pub field: Field,
    /// Term positions within the field, ascending. The term frequency is the
    /// length; storing it separately would let the two disagree.
    pub positions: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Posting {
    /// Document ordinal within the shard.
    pub doc: u32,
    pub fields: Vec<FieldPosting>,
}

impl Posting {
    pub fn term_frequency(&self, field: Field) -> u32 {
        self.fields
            .iter()
            .find(|f| f.field == field)
            .map_or(0, |f| f.positions.len() as u32)
    }

    pub fn positions(&self, field: Field) -> &[u32] {
        self.fields
            .iter()
            .find(|f| f.field == field)
            .map_or(&[][..], |f| &f.positions)
    }
}

/// One term's postings, with the corpus-wide document frequency the reader
/// scores against rather than the shard's own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub global_df: u32,
    pub postings: Vec<Posting>,
}

impl Block {
    pub fn encode(&self, out: &mut Vec<u8>) {
        varint::put(out, u64::from(self.global_df));
        varint::put(out, self.postings.len() as u64);
        let mut previous = 0u32;
        for posting in &self.postings {
            varint::put(out, u64::from(posting.doc.wrapping_sub(previous)));
            previous = posting.doc;
            varint::put(out, posting.fields.len() as u64);
            for field in &posting.fields {
                varint::put(out, u64::from(field.field.id()));
                varint::put(out, field.positions.len() as u64);
                let mut last = 0u32;
                for &position in &field.positions {
                    varint::put(out, u64::from(position.wrapping_sub(last)));
                    last = position;
                }
            }
        }
    }

    /// `None` for any byte sequence that is not a block, which the caller
    /// turns into `E1003`.
    pub fn decode(bytes: &[u8], at: &mut usize) -> Option<Self> {
        let global_df = u32::try_from(varint::get(bytes, at)?).ok()?;
        let count = varint::get(bytes, at)? as usize;
        // A count is one varint; a hostile one must not pre-allocate gigabytes.
        let mut postings = Vec::with_capacity(count.min(1024));
        let mut doc = 0u32;
        for _ in 0..count {
            doc = doc.checked_add(u32::try_from(varint::get(bytes, at)?).ok()?)?;
            let field_count = varint::get(bytes, at)? as usize;
            if field_count > Field::ALL.len() {
                return None;
            }
            let mut fields = Vec::with_capacity(field_count);
            for _ in 0..field_count {
                let field = Field::from_id(u8::try_from(varint::get(bytes, at)?).ok()?)?;
                let positions_len = varint::get(bytes, at)? as usize;
                let mut positions = Vec::with_capacity(positions_len.min(1024));
                let mut position = 0u32;
                for _ in 0..positions_len {
                    position =
                        position.checked_add(u32::try_from(varint::get(bytes, at)?).ok()?)?;
                    positions.push(position);
                }
                fields.push(FieldPosting { field, positions });
            }
            postings.push(Posting { doc, fields });
        }
        Some(Self {
            global_df,
            postings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block() -> Block {
        Block {
            global_df: 7,
            postings: vec![
                Posting {
                    doc: 0,
                    fields: vec![
                        FieldPosting {
                            field: Field::Title,
                            positions: vec![0],
                        },
                        FieldPosting {
                            field: Field::Body,
                            positions: vec![3, 9, 400],
                        },
                    ],
                },
                Posting {
                    doc: 1000,
                    fields: vec![FieldPosting {
                        field: Field::Code,
                        positions: vec![2],
                    }],
                },
            ],
        }
    }

    #[test]
    fn a_block_round_trips() {
        let mut bytes = Vec::new();
        block().encode(&mut bytes);
        let mut at = 0;
        assert_eq!(Block::decode(&bytes, &mut at), Some(block()));
        assert_eq!(at, bytes.len());
    }

    #[test]
    fn blocks_decode_from_their_own_offset() {
        let mut bytes = Vec::new();
        block().encode(&mut bytes);
        let second = bytes.len();
        block().encode(&mut bytes);
        let mut at = second;
        assert_eq!(Block::decode(&bytes, &mut at), Some(block()));
    }

    #[test]
    fn truncation_is_none_not_a_panic() {
        let mut bytes = Vec::new();
        block().encode(&mut bytes);
        for cut in 0..bytes.len() {
            let mut at = 0;
            let _ = Block::decode(&bytes[..cut], &mut at);
        }
    }

    #[test]
    fn an_unknown_field_id_is_rejected() {
        let mut bytes = Vec::new();
        varint::put(&mut bytes, 1);
        varint::put(&mut bytes, 1);
        varint::put(&mut bytes, 0);
        varint::put(&mut bytes, 1);
        varint::put(&mut bytes, 99);
        assert_eq!(Block::decode(&bytes, &mut 0), None);
    }

    #[test]
    fn term_frequency_is_the_position_count() {
        let block = block();
        assert_eq!(block.postings[0].term_frequency(Field::Body), 3);
        assert_eq!(block.postings[0].term_frequency(Field::Code), 0);
        assert_eq!(block.postings[0].positions(Field::Body), [3, 9, 400]);
    }
}
