//! `&'static str` for a name read from a file.
//!
//! §34.9 freezes `PropDef::name`, `PropDef::doc`, `Component::name`, and
//! `Component::aliases` as `&'static str`, and CMP-90 defines components in
//! files, whose names are `String`s. Interning leaks each *distinct* string
//! once, so a dev server that reloads `components/card.jinja` a thousand times
//! allocates for it once. See `plan/rfcs/0005-user-component-schemas.md`.

use std::collections::BTreeSet;
use std::sync::{Mutex, OnceLock};

// TODO(rfc-0005): delete this module if §34.9 moves to `Cow<'static, str>`.
fn table() -> &'static Mutex<BTreeSet<&'static str>> {
    static TABLE: OnceLock<Mutex<BTreeSet<&'static str>>> = OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(BTreeSet::new()))
}

/// The interned copy of `text`, leaking it only the first time it is seen.
pub fn str(text: &str) -> &'static str {
    let mut table = match table().lock() {
        Ok(table) => table,
        // A poisoned interner is still a correct interner: the set is only
        // ever inserted into, so no half-written state can be observed.
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(found) = table.get(text) {
        return found;
    }
    let leaked: &'static str = Box::leak(text.to_owned().into_boxed_str());
    table.insert(leaked);
    leaked
}

/// The interned copy of a list of names.
pub fn slice(items: &[&str]) -> &'static [&'static str] {
    let interned: Vec<&'static str> = items.iter().map(|item| str(item)).collect();
    Box::leak(interned.into_boxed_slice())
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_same_text_interns_to_the_same_pointer() {
        let first = super::str("title");
        let second = super::str(&String::from("title"));
        assert!(std::ptr::eq(first, second));
    }

    #[test]
    fn different_text_interns_separately() {
        assert_eq!(super::str("icon"), "icon");
        assert_ne!(super::str("icon"), super::str("iconic"));
    }
}
