//! `snippets-<n>.bin`: the section text a highlighted result is cut from.
//!
//! One blob of UTF-8 with `(offset, length)` in `docs-<n>.bin`, so a result
//! list costs one slice per hit and no parsing. §12.2 calls the file
//! compressed and the budget it sets is a compressed one; the bytes are stored
//! plain and compressed by the transport, because no compression crate has a
//! row in §6.2.1 (plan/rfcs/0702-idx-file-layout.md).
// TODO(rfc-0702): a codec here changes the format version and nothing else.

/// A borrowed snippet blob. Every accessor returns `None` rather than
/// panicking on a range a corrupt `docs-<n>.bin` produced.
#[derive(Debug, Clone, Copy)]
pub struct Snippets<'a>(pub &'a [u8]);

impl<'a> Snippets<'a> {
    pub fn get(&self, at: u32, len: u32) -> Option<&'a str> {
        let end = (at as usize).checked_add(len as usize)?;
        std::str::from_utf8(self.0.get(at as usize..end)?).ok()
    }
}

/// Builds the blob, returning each section's `(offset, length)`.
#[derive(Debug, Default)]
pub struct SnippetWriter {
    bytes: Vec<u8>,
}

impl SnippetWriter {
    pub fn push(&mut self, text: &str) -> (u32, u32) {
        let at = self.bytes.len() as u32;
        self.bytes.extend_from_slice(text.as_bytes());
        (at, text.len() as u32)
    }

    pub fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_round_trip_by_offset() {
        let mut writer = SnippetWriter::default();
        let first = writer.push("Sign requests with an API key.");
        let second = writer.push("検索エンジン");
        let bytes = writer.finish();
        let snippets = Snippets(&bytes);
        assert_eq!(
            snippets.get(first.0, first.1),
            Some("Sign requests with an API key.")
        );
        assert_eq!(snippets.get(second.0, second.1), Some("検索エンジン"));
    }

    #[test]
    fn a_range_past_the_end_is_none() {
        let snippets = Snippets(b"abc");
        assert_eq!(snippets.get(0, 99), None);
        assert_eq!(snippets.get(99, 1), None);
        assert_eq!(snippets.get(u32::MAX, u32::MAX), None);
    }

    #[test]
    fn a_range_that_splits_a_character_is_none() {
        let snippets = Snippets("é".as_bytes());
        assert_eq!(snippets.get(0, 1), None);
        assert_eq!(snippets.get(0, 2), Some("é"));
    }
}
