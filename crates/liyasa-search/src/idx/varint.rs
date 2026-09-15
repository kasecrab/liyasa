//! LEB128 over a byte slice.
//!
//! Every count, length, and delta in the format is a varint: doc IDs and term
//! positions are delta-encoded and small, and a fixed 32-bit field would about
//! double `postings-<n>.bin`, which is the file the worker fetches ranges of.

/// Appends `value` to `out`.
pub fn put(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

/// Reads at `*at`, advancing it. `None` on a truncated or over-long encoding,
/// which is how a corrupt shard becomes `E1003` instead of a panic.
pub fn get(bytes: &[u8], at: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *bytes.get(*at)?;
        *at += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

pub fn put_bytes(out: &mut Vec<u8>, value: &[u8]) {
    put(out, value.len() as u64);
    out.extend_from_slice(value);
}

pub fn put_str(out: &mut Vec<u8>, value: &str) {
    put_bytes(out, value.as_bytes());
}

pub fn get_bytes<'a>(bytes: &'a [u8], at: &mut usize) -> Option<&'a [u8]> {
    let len = get(bytes, at)? as usize;
    let end = at.checked_add(len)?;
    let slice = bytes.get(*at..end)?;
    *at = end;
    Some(slice)
}

pub fn get_str<'a>(bytes: &'a [u8], at: &mut usize) -> Option<&'a str> {
    std::str::from_utf8(get_bytes(bytes, at)?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_every_boundary() {
        let mut out = Vec::new();
        let values = [0, 1, 127, 128, 300, u32::MAX as u64, u64::MAX];
        for value in values {
            put(&mut out, value);
        }
        let mut at = 0;
        for value in values {
            assert_eq!(get(&out, &mut at), Some(value));
        }
        assert_eq!(at, out.len());
    }

    #[test]
    fn a_truncated_varint_is_none_not_a_panic() {
        assert_eq!(get(&[0x80], &mut 0), None);
        assert_eq!(get(&[], &mut 0), None);
    }

    #[test]
    fn an_overlong_varint_is_rejected() {
        assert_eq!(get(&[0x80; 12], &mut 0), None);
    }

    #[test]
    fn a_length_that_runs_past_the_end_is_none() {
        let mut out = Vec::new();
        put(&mut out, 40);
        out.push(b'x');
        assert_eq!(get_bytes(&out, &mut 0), None);
    }

    #[test]
    fn strings_round_trip() {
        let mut out = Vec::new();
        put_str(&mut out, "検索");
        put_str(&mut out, "");
        let mut at = 0;
        assert_eq!(get_str(&out, &mut at), Some("検索"));
        assert_eq!(get_str(&out, &mut at), Some(""));
    }
}
