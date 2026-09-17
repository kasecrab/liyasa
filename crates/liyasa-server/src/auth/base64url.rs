//! Unpadded base64url (RFC 4648 §5), which is what JWT, JWKS, PKCE and every
//! opaque token here are written in.
//!
//! Hand-rolled for the same reason `liyasa-build` hand-rolls standard base64
//! twice: it is thirty lines against a dependency row, and the PRD's table has
//! no base64 crate in it.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        let take = chunk.len() + 1;
        for i in 0..take {
            let index = (n >> (18 - 6 * i)) & 0x3f;
            out.push(ALPHABET[index as usize] as char);
        }
    }
    out
}

pub fn decode(text: &str) -> Option<Vec<u8>> {
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    for byte in text.bytes() {
        // Padding is accepted on input and never written on output: a JWKS
        // in the wild carries it often enough.
        if byte == b'=' {
            break;
        }
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            _ => return None,
        };
        acc = acc << 6 | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    // Leftover bits must be zero, or the text encoded something else.
    match acc & ((1 << bits) - 1) {
        0 => Some(out),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rfc_4648_vectors_round_trip() {
        for (bytes, text) in [
            (&b""[..], ""),
            (b"f", "Zg"),
            (b"fo", "Zm8"),
            (b"foo", "Zm9v"),
            (b"foob", "Zm9vYg"),
            (b"fooba", "Zm9vYmE"),
            (b"foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(encode(bytes), text, "{bytes:?}");
            assert_eq!(decode(text).as_deref(), Some(bytes), "{text}");
        }
    }

    #[test]
    fn the_url_safe_alphabet_is_the_one_used() {
        let bytes = [0xfb, 0xff, 0xbf];
        assert_eq!(encode(&bytes), "-_-_");
        assert_eq!(decode("-_-_").as_deref(), Some(&bytes[..]));
        // And the standard alphabet still decodes, because JWKS in the wild
        // is not consistent about it.
        assert_eq!(decode("+/+/").as_deref(), Some(&bytes[..]));
    }

    #[test]
    fn padded_input_decodes_and_encoded_output_never_pads() {
        assert_eq!(decode("Zg==").as_deref(), Some(&b"f"[..]));
        assert!(!encode(b"f").contains('='));
    }

    #[test]
    fn a_character_outside_the_alphabet_is_not_a_decode() {
        assert_eq!(decode("Zg*="), None);
        assert_eq!(decode("Z g"), None);
    }

    #[test]
    fn leftover_bits_that_are_not_zero_are_refused() {
        // "Zh" decodes `f` with a non-zero tail; a strict decoder rejects it
        // so two texts cannot mean the same token.
        assert_eq!(decode("Zg"), Some(vec![0x66]));
        assert_eq!(decode("Zh"), None);
    }

    #[test]
    fn every_byte_round_trips() {
        let all: Vec<u8> = (0..=255u8).collect();
        assert_eq!(decode(&encode(&all)), Some(all));
    }
}
