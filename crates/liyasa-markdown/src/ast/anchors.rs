//! Heading anchors (CM-31).
//!
//! An anchor is a URL that other people have already written down, so the rule
//! that matters is not how pretty it is but that it does not move. The slug
//! comes from the heading's own text and the de-duplication counter is kept per
//! slug, so a heading added anywhere else on the page leaves it alone.

use std::collections::{BTreeMap, BTreeSet};

/// Hands out one anchor per heading, in document order.
#[derive(Debug, Clone, Default)]
pub struct Anchors {
    /// The next counter to try for a slug, so `a`, `a`, `a` do not rescan.
    next: BTreeMap<String, u32>,
    taken: BTreeSet<String>,
}

impl Anchors {
    pub fn new() -> Self {
        Self::default()
    }

    /// The anchor for a heading, given the explicit `{#id}` it carried if any.
    ///
    /// An explicit anchor is taken as written and still occupies its name, so a
    /// later heading that slugifies to the same text does not collide with it.
    pub fn assign(&mut self, text: &str, explicit: Option<&str>) -> String {
        let base = match explicit {
            Some(id) => id.to_owned(),
            None => slugify(text),
        };
        // Every heading needs a target, including one written entirely in
        // punctuation.
        let base = if base.is_empty() {
            "section".to_owned()
        } else {
            base
        };

        let mut counter = self.next.get(&base).copied().unwrap_or(0);
        let mut anchor = suffixed(&base, counter);
        // `a`, `a`, `a-1` must not all want `a-1`.
        while self.taken.contains(&anchor) {
            counter += 1;
            anchor = suffixed(&base, counter);
        }
        self.next.insert(base, counter + 1);
        self.taken.insert(anchor.clone());
        anchor
    }

    pub fn contains(&self, anchor: &str) -> bool {
        self.taken.contains(anchor)
    }
}

fn suffixed(base: &str, counter: u32) -> String {
    if counter == 0 {
        base.to_owned()
    } else {
        format!("{base}-{counter}")
    }
}

/// Lowercase, spaces to hyphens, punctuation dropped, hyphen runs collapsed.
/// Letters and digits of every script survive, because a heading in Chinese
/// still needs an anchor.
pub fn slugify(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut gap = false;
    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            if gap && !out.is_empty() && !out.ends_with('-') {
                out.push('-');
            }
            gap = false;
            out.extend(ch.to_lowercase());
        } else if ch == '-' {
            gap = false;
            if !out.ends_with('-') {
                out.push('-');
            }
        } else if ch.is_whitespace() {
            gap = true;
        }
    }
    out.trim_matches('-').to_owned()
}

#[cfg(test)]
mod tests;
