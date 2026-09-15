//! Specification extensions (API-05, API-51, API-52).
//!
//! `x-liyasa` is the only vendor namespace Liyasa reads at build time. Another
//! product's namespace is carried through untouched: squatting `x-mint` or
//! `x-readme` would make Liyasa's reading of someone else's extension the
//! authoritative one. The importer rewrites those once, at import time.

use serde::Serialize;

use super::map::OrderedMap;
use crate::tree::{Value, as_bool, as_seq, as_str};

pub const NAMESPACE: &str = "x-liyasa";

/// Every `x-` key of an object, in document order.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Extensions(pub OrderedMap<Value>);

impl Extensions {
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.0.get(name)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.0.iter()
    }

    /// Names a key an extension: `x-` and at least one more character.
    pub fn is_extension_key(key: &str) -> bool {
        key.len() > 2 && key.starts_with("x-")
    }
}

/// How a spec author steers Liyasa's rendering of one node (API-05).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct XLiyasa {
    pub title: Option<String>,
    /// Markdown, rendered where the spec's own `description` would be.
    pub description: Option<String>,
    /// Hidden from navigation, search, Markdown, and the download (API-51).
    pub hidden: bool,
    /// Shown only to readers in [`XLiyasa::groups`] (API-51).
    pub internal: bool,
    /// Reader groups this node is filtered to (API-52).
    pub groups: Vec<String>,
    pub deprecated_note: Option<String>,
    /// Sends the navigation entry somewhere else instead of the generated page.
    pub href: Option<String>,
    pub collapsed: bool,
    /// Overrides the tag an operation is grouped under (API-03).
    pub group: Option<String>,
    /// Sorts within the group; lower first, unset last.
    pub order: Option<i64>,
    /// Examples that replace the ones derived from the schema (API-31).
    pub examples: Vec<Value>,
    /// Languages this node's samples are generated for, overriding the site's
    /// list (API-30).
    pub code_samples: Vec<String>,
}

impl XLiyasa {
    /// Reads `x-liyasa` out of an already-collected extension set. An
    /// extension that is the wrong shape is skipped rather than fatal: a spec
    /// is an input Liyasa does not own, and a page that renders without one
    /// hint is better than a build that stops.
    pub fn read(extensions: &Extensions) -> Self {
        let Some(value) = extensions.get(NAMESPACE) else {
            return Self::default();
        };
        let text = |key: &str| -> Option<String> {
            as_str(crate::tree::field(value, key)?).map(str::to_owned)
        };
        let flag = |key: &str| -> bool {
            crate::tree::field(value, key)
                .and_then(as_bool)
                .unwrap_or(false)
        };
        let strings = |key: &str| -> Option<Vec<String>> {
            Some(
                as_seq(crate::tree::field(value, key)?)?
                    .iter()
                    .filter_map(as_str)
                    .map(str::to_owned)
                    .collect(),
            )
        };
        Self {
            title: text("title"),
            description: text("description"),
            hidden: flag("hidden"),
            internal: flag("internal"),
            groups: strings("groups").unwrap_or_default(),
            deprecated_note: text("deprecatedNote"),
            href: text("href"),
            collapsed: flag("collapsed"),
            group: text("group"),
            order: crate::tree::field(value, "order").and_then(Value::as_i64),
            examples: crate::tree::field(value, "examples")
                .and_then(as_seq)
                .map(<[Value]>::to_vec)
                .unwrap_or_default(),
            code_samples: strings("codeSamples").unwrap_or_default(),
        }
    }

    /// True when nothing was written, so a caller can skip the node entirely.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Whether a reader in `reader_groups` may see this node (API-51, API-52).
    ///
    /// `hidden` removes the node from everyone. `internal` and `groups` narrow
    /// it to the listed groups; a node with neither is public.
    pub fn visible_to(&self, reader_groups: &[String]) -> bool {
        if self.hidden {
            return false;
        }
        if self.groups.is_empty() {
            return !self.internal;
        }
        self.groups.iter().any(|want| reader_groups.contains(want))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::parse;

    fn extensions(source: &str) -> Extensions {
        let value = parse(source.as_bytes(), "test").expect("the fixture parses");
        Extensions(
            crate::tree::entries(&value)
                .filter(|(k, _)| Extensions::is_extension_key(k))
                .map(|(k, v)| (k.to_owned(), v.clone()))
                .collect(),
        )
    }

    #[test]
    fn x_liyasa_is_read_and_other_vendors_are_only_carried() {
        let set = extensions(
            "x-liyasa:\n  title: List users\n  hidden: true\n  order: 3\nx-mint:\n  title: Mint\n",
        );
        let liyasa = XLiyasa::read(&set);
        assert_eq!(liyasa.title.as_deref(), Some("List users"));
        assert!(liyasa.hidden);
        assert_eq!(liyasa.order, Some(3));
        assert!(
            set.get("x-mint").is_some(),
            "another vendor's key is carried"
        );
    }

    #[test]
    fn a_node_with_no_x_liyasa_reads_as_empty() {
        assert!(XLiyasa::read(&extensions("summary: x")).is_empty());
    }

    #[test]
    fn a_malformed_hint_is_skipped_rather_than_failing_the_build() {
        let liyasa = XLiyasa::read(&extensions(
            "x-liyasa:\n  title: [1, 2]\n  hidden: yes please\n",
        ));
        assert_eq!(liyasa.title, None);
        assert!(
            !liyasa.hidden,
            "a non-boolean `hidden` is not a hidden node"
        );
    }

    #[test]
    fn hidden_beats_every_group_and_a_plain_node_is_public() {
        let hidden = XLiyasa {
            hidden: true,
            groups: vec!["staff".to_owned()],
            ..XLiyasa::default()
        };
        assert!(!hidden.visible_to(&["staff".to_owned()]));
        assert!(XLiyasa::default().visible_to(&[]));
    }

    #[test]
    fn internal_narrows_to_the_listed_groups() {
        let internal = XLiyasa {
            internal: true,
            ..XLiyasa::default()
        };
        assert!(
            !internal.visible_to(&["staff".to_owned()]),
            "no group listed, no reader"
        );

        let staff = XLiyasa {
            internal: true,
            groups: vec!["staff".to_owned()],
            ..XLiyasa::default()
        };
        assert!(staff.visible_to(&["staff".to_owned()]));
        assert!(!staff.visible_to(&["customer".to_owned()]));
    }

    #[test]
    fn a_short_key_is_not_an_extension() {
        assert!(Extensions::is_extension_key("x-a"));
        assert!(!Extensions::is_extension_key("x-"));
        assert!(!Extensions::is_extension_key("summary"));
    }
}
