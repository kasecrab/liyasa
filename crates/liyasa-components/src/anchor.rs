//! Heading and step anchors.
//!
//! One slug function for the whole crate, so `#installing-the-cli` means the
//! same thing whether it came from a heading, a step title, or an accordion.

/// A lowercase, hyphen-separated slug: letters, digits, and single hyphens.
pub fn slug(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            out.extend(ch.to_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_owned()
}

/// `slug`, with a numeric suffix when the page has seen it before.
pub fn unique(text: &str, seen: &mut std::collections::BTreeMap<String, u32>) -> String {
    let base = slug(text);
    let count = seen.entry(base.clone()).or_default();
    *count += 1;
    if *count == 1 {
        base
    } else {
        format!("{base}-{}", *count - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn punctuation_becomes_one_hyphen() {
        assert_eq!(slug("Install the CLI (v2)"), "install-the-cli-v2");
    }

    #[test]
    fn the_slug_has_no_edge_hyphens() {
        assert_eq!(slug("  -- Hello --  "), "hello");
    }

    #[test]
    fn non_ascii_letters_survive() {
        assert_eq!(slug("Präfix für Größe"), "präfix-für-größe");
    }

    #[test]
    fn a_repeat_gets_a_suffix() {
        let mut seen = std::collections::BTreeMap::new();
        assert_eq!(unique("Setup", &mut seen), "setup");
        assert_eq!(unique("Setup", &mut seen), "setup-1");
        assert_eq!(unique("Setup", &mut seen), "setup-2");
    }
}
