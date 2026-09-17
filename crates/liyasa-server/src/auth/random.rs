//! The one source of unguessable bytes in this module.
//!
//! Every session identifier, CSRF token, magic-link token, PKCE verifier and
//! TXT challenge comes through here, so there is one place to look at when
//! asking where the entropy came from.

use crate::auth::base64url;

/// 32 bytes, which is the tag size NFR-12 fixes and more than a session
/// identifier needs.
pub const TOKEN_BYTES: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the operating system supplied no randomness")]
pub struct NoEntropy;

pub fn bytes<const N: usize>() -> Result<[u8; N], NoEntropy> {
    let mut out = [0u8; N];
    getrandom::fill(&mut out).map_err(|_| NoEntropy)?;
    Ok(out)
}

/// An opaque token: 32 random bytes as unpadded base64url, 43 characters.
pub fn token() -> Result<String, NoEntropy> {
    Ok(base64url::encode(&bytes::<TOKEN_BYTES>()?))
}

/// Constant-time equality, for comparing a token a caller supplied against one
/// this server minted. `==` on `&str` returns early on the first differing
/// byte, which leaks the length of the shared prefix.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    // The length is not secret — a token's length is fixed and public — but the
    // contents are, so the fold runs over the whole of the longer input.
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_is_forty_three_characters_of_base64url() {
        let token = token().expect("entropy");
        assert_eq!(token.len(), 43, "{token}");
        assert!(
            token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
            "{token}"
        );
        assert_eq!(
            base64url::decode(&token).map(|b| b.len()),
            Some(TOKEN_BYTES)
        );
    }

    #[test]
    fn two_tokens_are_not_the_same_token() {
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..256 {
            assert!(seen.insert(token().expect("entropy")), "a token repeated");
        }
    }

    #[test]
    fn constant_time_equality_agrees_with_ordinary_equality() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(!constant_time_eq(b"", b"a"));
        assert!(constant_time_eq(b"", b""));
    }
}
