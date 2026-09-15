//! `docs-<n>.bin`: the section metadata a hit is rendered and filtered from.
//!
//! Loaded whole for the shard in use — one entry is about 150 bytes, so a
//! 1,000-page shard is well inside the budget of §12.2 — and never merged
//! across shards, because a document ordinal is shard-local.

use super::field::{ByField, Field};
use super::varint;
use crate::doc::DocKind;

pub const MAGIC: &[u8; 4] = b"LYD1";

/// One section's facets, filters, and the slice of `snippets-<n>.bin` its
/// text lives in.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DocEntry {
    pub route: String,
    pub anchor: String,
    pub title: String,
    pub section: String,
    pub breadcrumb: Vec<String>,
    pub tab: Option<String>,
    pub version: Option<String>,
    pub locale: String,
    pub kind: DocKind,
    pub boost: f32,
    pub groups: Vec<String>,
    pub regions: Vec<String>,
    /// Milliseconds since the epoch; `None` encodes as zero.
    pub updated: Option<u64>,
    /// Term count per field, for BM25's length normalization.
    pub lengths: ByField<u32>,
    pub snippet_at: u32,
    pub snippet_len: u32,
}

impl DocEntry {
    pub fn url(&self) -> String {
        if self.anchor.is_empty() {
            self.route.clone()
        } else {
            format!("{}#{}", self.route, self.anchor)
        }
    }

    fn encode(&self, out: &mut Vec<u8>) {
        varint::put_str(out, &self.route);
        varint::put_str(out, &self.anchor);
        varint::put_str(out, &self.title);
        varint::put_str(out, &self.section);
        put_list(out, &self.breadcrumb);
        varint::put_str(out, self.tab.as_deref().unwrap_or_default());
        varint::put_str(out, self.version.as_deref().unwrap_or_default());
        varint::put_str(out, &self.locale);
        varint::put_str(out, self.kind.as_str());
        out.extend_from_slice(&self.boost.to_le_bytes());
        put_list(out, &self.groups);
        put_list(out, &self.regions);
        varint::put(out, self.updated.unwrap_or(0));
        for field in Field::ALL {
            varint::put(out, u64::from(self.lengths[field]));
        }
        varint::put(out, u64::from(self.snippet_at));
        varint::put(out, u64::from(self.snippet_len));
    }

    fn decode(bytes: &[u8], at: &mut usize) -> Option<Self> {
        let route = varint::get_str(bytes, at)?.to_owned();
        let anchor = varint::get_str(bytes, at)?.to_owned();
        let title = varint::get_str(bytes, at)?.to_owned();
        let section = varint::get_str(bytes, at)?.to_owned();
        let breadcrumb = get_list(bytes, at)?;
        let tab = non_empty(varint::get_str(bytes, at)?);
        let version = non_empty(varint::get_str(bytes, at)?);
        let locale = varint::get_str(bytes, at)?.to_owned();
        let kind = DocKind::parse(varint::get_str(bytes, at)?)?;
        let boost = f32::from_le_bytes(bytes.get(*at..*at + 4)?.try_into().ok()?);
        *at += 4;
        let groups = get_list(bytes, at)?;
        let regions = get_list(bytes, at)?;
        let updated = match varint::get(bytes, at)? {
            0 => None,
            millis => Some(millis),
        };
        let mut lengths = ByField([0u32; 6]);
        for field in Field::ALL {
            lengths[field] = u32::try_from(varint::get(bytes, at)?).ok()?;
        }
        let snippet_at = u32::try_from(varint::get(bytes, at)?).ok()?;
        let snippet_len = u32::try_from(varint::get(bytes, at)?).ok()?;
        Some(Self {
            route,
            anchor,
            title,
            section,
            breadcrumb,
            tab,
            version,
            locale,
            kind,
            boost,
            groups,
            regions,
            updated,
            lengths,
            snippet_at,
            snippet_len,
        })
    }
}

pub fn encode(entries: &[DocEntry]) -> Vec<u8> {
    let mut out = Vec::with_capacity(entries.len() * 160 + 8);
    out.extend_from_slice(MAGIC);
    varint::put(&mut out, entries.len() as u64);
    for entry in entries {
        entry.encode(&mut out);
    }
    out
}

pub fn decode(bytes: &[u8]) -> Option<Vec<DocEntry>> {
    if bytes.get(..4)? != MAGIC {
        return None;
    }
    let mut at = 4;
    let count = varint::get(bytes, &mut at)? as usize;
    let mut out = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        out.push(DocEntry::decode(bytes, &mut at)?);
    }
    Some(out)
}

fn put_list(out: &mut Vec<u8>, values: &[String]) {
    varint::put(out, values.len() as u64);
    for value in values {
        varint::put_str(out, value);
    }
}

fn get_list(bytes: &[u8], at: &mut usize) -> Option<Vec<String>> {
    let count = varint::get(bytes, at)? as usize;
    let mut out = Vec::with_capacity(count.min(64));
    for _ in 0..count {
        out.push(varint::get_str(bytes, at)?.to_owned());
    }
    Some(out)
}

fn non_empty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> DocEntry {
        DocEntry {
            route: "/guides/auth".to_owned(),
            anchor: "api-keys".to_owned(),
            title: "Authentication".to_owned(),
            section: "API keys".to_owned(),
            breadcrumb: vec!["Guides".to_owned(), "Authentication".to_owned()],
            tab: Some("docs".to_owned()),
            version: None,
            locale: "en".to_owned(),
            kind: DocKind::Endpoint,
            boost: 1.5,
            groups: vec!["beta".to_owned()],
            regions: Vec::new(),
            updated: Some(1_700_000_000_000),
            lengths: ByField([1, 2, 3, 4, 5, 6]),
            snippet_at: 12,
            snippet_len: 340,
        }
    }

    #[test]
    fn entries_round_trip() {
        let entries = vec![entry(), DocEntry::default()];
        assert_eq!(decode(&encode(&entries)).as_deref(), Some(&entries[..]));
    }

    #[test]
    fn a_file_without_the_magic_is_rejected() {
        let mut bytes = encode(&[entry()]);
        bytes[0] = b'X';
        assert_eq!(decode(&bytes), None);
        assert_eq!(decode(b"LY"), None);
    }

    #[test]
    fn truncation_is_none_not_a_panic() {
        let bytes = encode(&[entry(), entry()]);
        for cut in 0..bytes.len() {
            assert_eq!(decode(&bytes[..cut]), None, "{cut} bytes decoded");
        }
    }

    #[test]
    fn a_lead_documents_url_has_no_fragment() {
        let mut lead = entry();
        lead.anchor.clear();
        assert_eq!(lead.url(), "/guides/auth");
        assert_eq!(entry().url(), "/guides/auth#api-keys");
    }
}
