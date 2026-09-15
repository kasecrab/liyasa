//! What every `SecretSource` must do (PRD §30.2, §34.9).

use super::require;
use crate::verify::SecretSource;

/// `known` must resolve to `value`; `missing` must not resolve at all.
pub fn check(source: &dyn SecretSource, known: &str, value: &str, missing: &str) {
    let found = source.get(known).expect("a configured secret resolves");
    require!(found.as_str() == value, "get returns the secret verbatim");
    require!(
        source.get(missing).is_none(),
        "an unknown name resolves to None, never to an empty string"
    );
    require!(
        source.get(&known.to_uppercase()).is_none() || known == known.to_uppercase(),
        "secret names are case-sensitive"
    );
}
