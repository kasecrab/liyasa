//! `docs-<n>.bin`: what a hit is scored, filtered, and rendered from.
//!
//! Two sections in one file, because they are needed at different moments.
//! **Stats** comes first and holds only what ranking and filtering need —
//! field lengths, boost, and facets as dictionary ids — so it is a few bytes
//! per section and the worker can fetch it with one range request before the
//! first result appears (SRC-05's 200 KB budget). **Meta** follows and holds
//! the strings a result list displays; the worker fetches it once it knows
//! which handful of documents it is showing.
//!
//! `Shard::stats_bytes` in the manifest is the length of the prefix to fetch.

use super::field::{ByField, Field};
use super::varint;
use crate::doc::DocKind;

pub const MAGIC: &[u8; 4] = b"LYD1";

/// Ranking and filtering facts for one section.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DocStats {
    pub kind: DocKind,
    pub boost: f32,
    /// Milliseconds since the epoch; `None` encodes as zero.
    pub updated: Option<u64>,
    /// Term count per field, for BM25's length normalization.
    pub lengths: ByField<u32>,
    pub tab: Option<u32>,
    pub version: Option<u32>,
    pub locale: Option<u32>,
    pub groups: Vec<u32>,
    pub regions: Vec<u32>,
}

/// What a result list shows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DocMeta {
    pub route: String,
    pub anchor: String,
    pub title: String,
    pub section: String,
    pub breadcrumb: Vec<String>,
    pub snippet_at: u32,
    pub snippet_len: u32,
}

impl DocMeta {
    pub fn url(&self) -> String {
        if self.anchor.is_empty() {
            self.route.clone()
        } else {
            format!("{}#{}", self.route, self.anchor)
        }
    }
}

/// The strings facet ids point into. One per shard, so an id is shard-local
/// exactly like a document ordinal.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facets {
    pub tabs: Vec<String>,
    pub versions: Vec<String>,
    pub locales: Vec<String>,
    pub groups: Vec<String>,
    pub regions: Vec<String>,
}

impl Facets {
    /// The id of `value`, adding it if this is the first sighting.
    pub fn intern(list: &mut Vec<String>, value: &str) -> u32 {
        match list.iter().position(|v| v == value) {
            Some(at) => at as u32,
            None => {
                list.push(value.to_owned());
                (list.len() - 1) as u32
            }
        }
    }

    pub fn id_of(list: &[String], value: &str) -> Option<u32> {
        list.iter().position(|v| v == value).map(|at| at as u32)
    }
}

/// One document as both halves, which is what the writer is handed and what
/// the server — with the whole file in memory — reads back.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DocEntry {
    pub stats: DocStats,
    pub meta: DocMeta,
}

/// The encoded file plus the length of its stats prefix.
#[derive(Debug, Clone, PartialEq)]
pub struct Encoded {
    pub bytes: Vec<u8>,
    pub stats_bytes: u64,
}

pub fn encode(facets: &Facets, entries: &[DocEntry]) -> Encoded {
    let mut stats = Vec::with_capacity(entries.len() * 32);
    put_list(&mut stats, &facets.tabs);
    put_list(&mut stats, &facets.versions);
    put_list(&mut stats, &facets.locales);
    put_list(&mut stats, &facets.groups);
    put_list(&mut stats, &facets.regions);
    for entry in entries {
        encode_stats(&mut stats, &entry.stats);
    }

    let mut out = Vec::with_capacity(stats.len() + entries.len() * 128 + 16);
    out.extend_from_slice(MAGIC);
    varint::put(&mut out, entries.len() as u64);
    varint::put(&mut out, stats.len() as u64);
    out.extend_from_slice(&stats);
    let stats_bytes = out.len() as u64;
    for entry in entries {
        encode_meta(&mut out, &entry.meta);
    }
    Encoded {
        bytes: out,
        stats_bytes,
    }
}

/// Reads the prefix: facet dictionaries and per-document ranking facts.
/// Accepts a slice truncated at `stats_bytes`, which is what a range request
/// returns.
pub fn decode_stats(bytes: &[u8]) -> Option<(Facets, Vec<DocStats>)> {
    let (count, _, mut at) = header(bytes)?;
    let facets = Facets {
        tabs: get_list(bytes, &mut at)?,
        versions: get_list(bytes, &mut at)?,
        locales: get_list(bytes, &mut at)?,
        groups: get_list(bytes, &mut at)?,
        regions: get_list(bytes, &mut at)?,
    };
    let mut stats = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        stats.push(decode_stats_entry(bytes, &mut at)?);
    }
    Some((facets, stats))
}

/// Reads the display half. Needs the whole file.
pub fn decode_meta(bytes: &[u8]) -> Option<Vec<DocMeta>> {
    let (count, stats_len, mut at) = header(bytes)?;
    at = at.checked_add(stats_len)?;
    let mut meta = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        meta.push(decode_meta_entry(bytes, &mut at)?);
    }
    Some(meta)
}

pub fn decode(bytes: &[u8]) -> Option<(Facets, Vec<DocEntry>)> {
    let (facets, stats) = decode_stats(bytes)?;
    let meta = decode_meta(bytes)?;
    if stats.len() != meta.len() {
        return None;
    }
    Some((
        facets,
        stats
            .into_iter()
            .zip(meta)
            .map(|(stats, meta)| DocEntry { stats, meta })
            .collect(),
    ))
}

/// `(document count, stats length, offset just past the header)`.
fn header(bytes: &[u8]) -> Option<(usize, usize, usize)> {
    if bytes.get(..4)? != MAGIC {
        return None;
    }
    let mut at = 4;
    let count = varint::get(bytes, &mut at)? as usize;
    let stats_len = varint::get(bytes, &mut at)? as usize;
    Some((count, stats_len, at))
}

fn encode_stats(out: &mut Vec<u8>, stats: &DocStats) {
    varint::put_str(out, stats.kind.as_str());
    out.extend_from_slice(&stats.boost.to_le_bytes());
    varint::put(out, stats.updated.unwrap_or(0));
    for field in Field::ALL {
        varint::put(out, u64::from(stats.lengths[field]));
    }
    put_optional_id(out, stats.tab);
    put_optional_id(out, stats.version);
    put_optional_id(out, stats.locale);
    put_ids(out, &stats.groups);
    put_ids(out, &stats.regions);
}

fn decode_stats_entry(bytes: &[u8], at: &mut usize) -> Option<DocStats> {
    let kind = DocKind::parse(varint::get_str(bytes, at)?)?;
    let boost = f32::from_le_bytes(bytes.get(*at..at.checked_add(4)?)?.try_into().ok()?);
    *at += 4;
    let updated = match varint::get(bytes, at)? {
        0 => None,
        millis => Some(millis),
    };
    let mut lengths = ByField([0u32; 6]);
    for field in Field::ALL {
        lengths[field] = u32::try_from(varint::get(bytes, at)?).ok()?;
    }
    Some(DocStats {
        kind,
        boost,
        updated,
        lengths,
        tab: get_optional_id(bytes, at)?,
        version: get_optional_id(bytes, at)?,
        locale: get_optional_id(bytes, at)?,
        groups: get_ids(bytes, at)?,
        regions: get_ids(bytes, at)?,
    })
}

fn encode_meta(out: &mut Vec<u8>, meta: &DocMeta) {
    varint::put_str(out, &meta.route);
    varint::put_str(out, &meta.anchor);
    varint::put_str(out, &meta.title);
    varint::put_str(out, &meta.section);
    put_list(out, &meta.breadcrumb);
    varint::put(out, u64::from(meta.snippet_at));
    varint::put(out, u64::from(meta.snippet_len));
}

fn decode_meta_entry(bytes: &[u8], at: &mut usize) -> Option<DocMeta> {
    Some(DocMeta {
        route: varint::get_str(bytes, at)?.to_owned(),
        anchor: varint::get_str(bytes, at)?.to_owned(),
        title: varint::get_str(bytes, at)?.to_owned(),
        section: varint::get_str(bytes, at)?.to_owned(),
        breadcrumb: get_list(bytes, at)?,
        snippet_at: u32::try_from(varint::get(bytes, at)?).ok()?,
        snippet_len: u32::try_from(varint::get(bytes, at)?).ok()?,
    })
}

/// `None` encodes as 0 so the common "no version" case is one byte.
fn put_optional_id(out: &mut Vec<u8>, id: Option<u32>) {
    varint::put(out, id.map_or(0, |id| u64::from(id) + 1));
}

fn get_optional_id(bytes: &[u8], at: &mut usize) -> Option<Option<u32>> {
    Some(match varint::get(bytes, at)? {
        0 => None,
        raw => Some(u32::try_from(raw - 1).ok()?),
    })
}

fn put_ids(out: &mut Vec<u8>, ids: &[u32]) {
    varint::put(out, ids.len() as u64);
    for &id in ids {
        varint::put(out, u64::from(id));
    }
}

fn get_ids(bytes: &[u8], at: &mut usize) -> Option<Vec<u32>> {
    let count = varint::get(bytes, at)? as usize;
    let mut out = Vec::with_capacity(count.min(64));
    for _ in 0..count {
        out.push(u32::try_from(varint::get(bytes, at)?).ok()?);
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
    let mut out = Vec::with_capacity(count.min(256));
    for _ in 0..count {
        out.push(varint::get_str(bytes, at)?.to_owned());
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facets() -> Facets {
        Facets {
            tabs: vec!["docs".to_owned()],
            versions: vec!["v2".to_owned()],
            locales: vec!["en".to_owned()],
            groups: vec!["beta".to_owned()],
            regions: Vec::new(),
        }
    }

    fn entry() -> DocEntry {
        DocEntry {
            stats: DocStats {
                kind: DocKind::Endpoint,
                boost: 1.5,
                updated: Some(1_700_000_000_000),
                lengths: ByField([1, 2, 3, 4, 5, 6]),
                tab: Some(0),
                version: Some(0),
                locale: Some(0),
                groups: vec![0],
                regions: Vec::new(),
            },
            meta: DocMeta {
                route: "/guides/auth".to_owned(),
                anchor: "api-keys".to_owned(),
                title: "Authentication".to_owned(),
                section: "API keys".to_owned(),
                breadcrumb: vec!["Guides".to_owned()],
                snippet_at: 12,
                snippet_len: 340,
            },
        }
    }

    #[test]
    fn entries_round_trip() {
        let entries = vec![entry(), DocEntry::default()];
        let encoded = encode(&facets(), &entries);
        let (back_facets, back) = decode(&encoded.bytes).expect("decodes");
        assert_eq!(back_facets, facets());
        assert_eq!(back, entries);
    }

    #[test]
    fn the_stats_prefix_decodes_on_its_own() {
        let entries = vec![entry(), entry()];
        let encoded = encode(&facets(), &entries);
        let prefix = &encoded.bytes[..encoded.stats_bytes as usize];
        let (back_facets, stats) = decode_stats(prefix).expect("the prefix is self-contained");
        assert_eq!(back_facets, facets());
        assert_eq!(stats.len(), 2);
        assert_eq!(stats[0], entry().stats);
        assert!(
            decode_meta(prefix).is_none(),
            "the display half is not in the prefix"
        );
    }

    #[test]
    fn the_stats_prefix_is_a_fraction_of_the_file() {
        let entries = vec![entry(); 50];
        let encoded = encode(&facets(), &entries);
        assert!(
            encoded.stats_bytes * 2 < encoded.bytes.len() as u64,
            "stats {} of {} bytes",
            encoded.stats_bytes,
            encoded.bytes.len()
        );
    }

    #[test]
    fn a_file_without_the_magic_is_rejected() {
        let mut encoded = encode(&facets(), &[entry()]);
        encoded.bytes[0] = b'X';
        assert_eq!(decode(&encoded.bytes), None);
        assert_eq!(decode(b"LY"), None);
    }

    #[test]
    fn truncation_is_none_not_a_panic() {
        let encoded = encode(&facets(), &[entry(), entry()]);
        for cut in 0..encoded.bytes.len() {
            assert_eq!(decode(&encoded.bytes[..cut]), None, "{cut} bytes decoded");
        }
    }

    #[test]
    fn a_lead_documents_url_has_no_fragment() {
        let mut lead = entry().meta;
        lead.anchor.clear();
        assert_eq!(lead.url(), "/guides/auth");
        assert_eq!(entry().meta.url(), "/guides/auth#api-keys");
    }

    #[test]
    fn interning_a_facet_twice_returns_the_same_id() {
        let mut list = Vec::new();
        assert_eq!(Facets::intern(&mut list, "docs"), 0);
        assert_eq!(Facets::intern(&mut list, "api"), 1);
        assert_eq!(Facets::intern(&mut list, "docs"), 0);
        assert_eq!(Facets::id_of(&list, "api"), Some(1));
        assert_eq!(Facets::id_of(&list, "absent"), None);
    }
}
