//! What the assistant knows about the reader asking (AST-11).
//!
//! The entitlement half is not advisory. `ChunkQuery` is built from this and
//! nothing else, so a reader never receives content they could not browse —
//! the retrieval cannot return it, rather than the answer declining to mention
//! it.

use liyasa_core::ai::TrustLevel;
use liyasa_core::ids::{Locale, Route, Version};
use serde::{Deserialize, Serialize};

use crate::config::Availability;
use crate::index::ChunkQuery;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReaderContext {
    pub current_page: Option<Route>,
    /// What the reader had selected when they opened the assistant.
    pub selection: Option<String>,
    pub version: Option<Version>,
    pub locale: Option<Locale>,
    pub region: Option<String>,
    pub groups: Vec<String>,
    pub signed_in: bool,
}

impl ReaderContext {
    /// The retrieval filter for this reader.
    pub fn query(&self) -> ChunkQuery {
        ChunkQuery {
            groups: self.groups.clone(),
            region: self.region.clone(),
            version: self.version.clone(),
            locale: self.locale.clone(),
            kind: None,
            routes: Vec::new(),
        }
    }

    /// A reader's own text is `anonymous`, and stays `anonymous` when they sign
    /// in: being a known person does not make a question an instruction.
    pub fn trust(&self) -> TrustLevel {
        TrustLevel::Anonymous
    }

    /// Whether `ai.assistant.availability` lets this reader use it at all.
    pub fn may_use(&self, availability: Availability) -> bool {
        match availability {
            Availability::All => true,
            Availability::SignedIn => self.signed_in,
            Availability::Groups => self.signed_in && !self.groups.is_empty(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_filter_carries_the_readers_entitlements() {
        let reader = ReaderContext {
            groups: vec!["staff".to_owned()],
            region: Some("eu".to_owned()),
            version: Some(Version::new("v2")),
            locale: Some(Locale::new("de")),
            ..Default::default()
        };
        let query = reader.query();
        assert_eq!(query.groups, ["staff"]);
        assert_eq!(query.region.as_deref(), Some("eu"));
        assert_eq!(query.version.as_ref().map(Version::as_str), Some("v2"));
    }

    #[test]
    fn signing_in_does_not_promote_a_readers_question() {
        let mut reader = ReaderContext::default();
        assert_eq!(reader.trust(), TrustLevel::Anonymous);
        reader.signed_in = true;
        assert_eq!(reader.trust(), TrustLevel::Anonymous);
    }

    #[test]
    fn availability_groups_needs_a_group_not_just_a_session() {
        let mut reader = ReaderContext::default();
        assert!(reader.may_use(Availability::All));
        assert!(!reader.may_use(Availability::SignedIn));
        reader.signed_in = true;
        assert!(reader.may_use(Availability::SignedIn));
        assert!(!reader.may_use(Availability::Groups));
        reader.groups.push("staff".to_owned());
        assert!(reader.may_use(Availability::Groups));
    }
}
