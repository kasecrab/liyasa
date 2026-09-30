//! The front matter fields the output gate watches (AGT-06).
//!
//! AGT-06 names four: `groups`, `access`, `regions` and `personalized`. They are
//! the keys that decide who is served a page, so a diff that changes one is a
//! change to the site's access control, whatever it says about itself in prose.
//!
//! The block is read here rather than through `liyasa-markdown`'s scanner for
//! one reason: this has to answer *unparseable*, and a scanner built for a build
//! reports a diagnostic and carries on with a default. "The YAML no longer parses
//! so no field changed" is the hole that would let a diff turn `groups: [staff]`
//! into nothing by deleting a closing fence.
//!
//! The fence rules match `liyasa-markdown`'s (`source/scan.rs`) exactly: `---`
//! and a newline at byte zero, then the first later line whose trimmed form is
//! `---`. `---\n---` is two thematic breaks and not an empty block, there too.
//!
//! The comparison is over `serde_json::Value`, not over a typed view. A typed
//! view normalises — an absent `access` and an `access` set to the default
//! deserialize alike — and normalising is exactly what hides a change.

use serde_json::Value;

/// The delimiter of a front matter block.
const FENCE: &str = "---";

/// The fields AGT-06 watches, in the order it names them.
pub const WATCHED: [&str; 4] = ["groups", "access", "regions", "personalized"];

/// A page's front matter block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Front {
    /// The page has no front matter block.
    Absent,
    /// There is a block and it is not valid YAML. Never treated as "no fields".
    Unparseable,
    Parsed(Value),
}

/// Reads the front matter block out of a page's text.
pub fn read(text: &str) -> Front {
    let Some(rest) = text.strip_prefix(FENCE).and_then(|r| r.strip_prefix('\n')) else {
        return Front::Absent;
    };
    let mut body_len = 0usize;
    let mut closed = false;
    for line in rest.split_inclusive('\n') {
        if body_len > 0 && line.trim_end() == FENCE {
            closed = true;
            break;
        }
        body_len += line.len();
    }
    if !closed {
        // An unterminated block is not front matter, here or in the build. The
        // fields are therefore absent, and a diff that removes a closing fence
        // reads as removing every field it had — which is what it does.
        return Front::Absent;
    }
    let body = &rest[..body_len];
    if body.trim().is_empty() {
        return Front::Parsed(Value::Null);
    }
    match liyasa_core::yaml::parse_value(body, None) {
        Ok(value) => Front::Parsed(value),
        Err(_) => Front::Unparseable,
    }
}

/// The watched fields of one page, each as written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Access {
    pub groups: Option<Value>,
    pub access: Option<Value>,
    pub regions: Option<Value>,
    pub personalized: Option<Value>,
}

impl Access {
    /// `None` when the block exists and does not parse: an unknown answer, which
    /// the gate must not read as "nothing changed".
    pub fn of(front: &Front) -> Option<Self> {
        let value = match front {
            Front::Absent => return Some(Self::default()),
            Front::Unparseable => return None,
            Front::Parsed(value) => value,
        };
        let get = |key: &str| value.get(key).cloned();
        Some(Self {
            groups: get("groups"),
            access: get("access"),
            regions: get("regions"),
            personalized: get("personalized"),
        })
    }

    fn field(&self, name: &str) -> Option<&Value> {
        match name {
            "groups" => self.groups.as_ref(),
            "access" => self.access.as_ref(),
            "regions" => self.regions.as_ref(),
            "personalized" => self.personalized.as_ref(),
            _ => None,
        }
    }

    /// The watched fields whose value differs, in AGT-06's order.
    pub fn changed(before: &Self, after: &Self) -> Vec<&'static str> {
        WATCHED
            .into_iter()
            .filter(|name| before.field(name) != after.field(name))
            .collect()
    }
}

/// Which watched fields a change to a page touched.
///
/// `Err` says the comparison could not be made, which the gate treats as a
/// rejection rather than as no change.
pub fn changed_access(before: Option<&str>, after: Option<&str>) -> Result<Vec<&'static str>, ()> {
    let read_side = |text: Option<&str>| match text {
        None => Some(Access::default()),
        Some(text) => Access::of(&read(text)),
    };
    let (Some(before), Some(after)) = (read_side(before), read_side(after)) else {
        return Err(());
    };
    Ok(Access::changed(&before, &after))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn page(front: &str) -> String {
        format!("---\n{front}---\n\n# Title\n")
    }

    #[test]
    fn a_block_parses_and_its_watched_fields_are_read() {
        let front = read(&page("title: Install\ngroups: [staff]\naccess: private\n"));
        let access = Access::of(&front).expect("it parses");
        assert_eq!(access.groups, Some(json!(["staff"])));
        assert_eq!(access.access, Some(json!("private")));
        assert_eq!(access.regions, None);
        assert_eq!(access.personalized, None);
    }

    #[test]
    fn a_page_with_no_block_has_no_watched_fields() {
        assert_eq!(read("# Title\n"), Front::Absent);
        assert_eq!(
            Access::of(&Front::Absent).expect("absent is a known answer"),
            Access::default()
        );
    }

    #[test]
    fn an_empty_block_is_a_parsed_null_not_an_absence() {
        assert_eq!(read("---\n\n---\n"), Front::Parsed(Value::Null));
    }

    #[test]
    fn three_dashes_twice_is_two_thematic_breaks() {
        // The build reads it that way (RFC 0200); reading it as an empty block
        // here would put the gate and the build in disagreement about where a
        // page's body starts.
        assert_eq!(read("---\n---\n"), Front::Absent);
    }

    #[test]
    fn a_block_that_does_not_parse_is_not_an_absence_of_fields() {
        // The hole this closes: a diff that breaks the YAML would otherwise
        // report every watched field as unset, and "unset to unset" is no
        // change.
        assert_eq!(read("---\ngroups: [staff\n---\n"), Front::Unparseable);
        assert_eq!(Access::of(&Front::Unparseable), None);
    }

    #[test]
    fn removing_a_closing_fence_reads_as_removing_the_fields() {
        let before = page("groups: [staff]\n");
        let after = "---\ngroups: [staff]\n\n# Title\n";
        assert_eq!(
            changed_access(Some(&before), Some(after)),
            Ok(vec!["groups"])
        );
    }

    #[test]
    fn breaking_the_yaml_is_a_comparison_that_cannot_be_made() {
        let before = page("groups: [staff]\n");
        let after = page("groups: [staff\n");
        assert_eq!(changed_access(Some(&before), Some(&after)), Err(()));
    }

    #[test]
    fn every_field_agt_06_names_is_compared() {
        let before = page("title: t\n");
        for (key, value) in [
            ("groups", "[staff]"),
            ("access", "private"),
            ("regions", "{allow: [DE]}"),
            ("personalized", "true"),
        ] {
            let after = page(&format!("title: t\n{key}: {value}\n"));
            assert_eq!(
                changed_access(Some(&before), Some(&after)),
                Ok(vec![key]),
                "a change to `{key}` was not seen"
            );
        }
    }

    #[test]
    fn a_change_the_gate_does_not_watch_is_not_reported() {
        let before = page("title: Install\n");
        let after = page("title: Installation\n");
        assert_eq!(changed_access(Some(&before), Some(&after)), Ok(Vec::new()));
    }

    #[test]
    fn adding_a_page_with_an_access_field_is_a_change() {
        // There is no `before`, so the comparison is against nothing set. A new
        // page that arrives already restricted is still the gate's business.
        assert_eq!(
            changed_access(None, Some(&page("groups: [staff]\n"))),
            Ok(vec!["groups"])
        );
    }

    #[test]
    fn deleting_a_restricted_page_is_a_change() {
        assert_eq!(
            changed_access(Some(&page("access: private\n")), None),
            Ok(vec!["access"])
        );
    }

    #[test]
    fn a_reordered_list_is_not_a_change_but_a_different_one_is() {
        let before = page("groups: [a, b]\n");
        assert_eq!(
            changed_access(Some(&before), Some(&page("groups: [a, b]\n"))),
            Ok(Vec::new())
        );
        assert_eq!(
            changed_access(Some(&before), Some(&page("groups: [b, a]\n"))),
            Ok(vec!["groups"]),
            "list order decides which group is matched first, so it is a change"
        );
    }
}
