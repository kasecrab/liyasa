//! Identity, the data inventory, and what the docs may say about consent
//! (ANA-03, ANA-04, ANA-07).
//!
//! Two pseudonymous identifiers exist. The daily session key is
//! `liyasa-server`'s and rotates into unrecoverability at midnight. The
//! subject hash is this module's and deliberately does not: segmenting a
//! private site by team over a quarter is the feature, and an identifier that
//! survives a quarter is personal data. [`INVENTORY`] says so in the form
//! §30.3 asks for, rather than claiming either one is anonymous.

use serde::{Deserialize, Serialize};

/// `analytics.identityLinkage`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityPolicy {
    /// False turns the whole thing off: no subject hash, no groups, no
    /// segmentation by reader (ANA-04).
    pub linkage: bool,
}

impl Default for IdentityPolicy {
    fn default() -> Self {
        Self { linkage: true }
    }
}

/// What is stored about a signed-in reader on a private site (ANA-04).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subject {
    /// `s1:` and 32 hex characters. Never the identifier it came from.
    pub hash: String,
    /// Group names, stored as names: segmentation by team is the point, and a
    /// hashed group name would be useless for it and no more private, since
    /// the set of a site's groups is small enough to enumerate.
    pub groups: Vec<String>,
}

impl IdentityPolicy {
    /// The stored identity for a reader, or `None` when linkage is off.
    ///
    /// `pepper` is an instance secret from the secret store, not the daily
    /// salt: a subject that changed every midnight could not be segmented over
    /// a quarter, which is what ANA-04 asks for. It follows that the hash is
    /// reversible by whoever holds the pepper, which is why [`INVENTORY`]
    /// lists it as personal data.
    pub fn subject(
        &self,
        identifier: &str,
        site: &str,
        pepper: &[u8],
        groups: &[String],
    ) -> Option<Subject> {
        if !self.linkage {
            return None;
        }
        let digest = liyasa_core::ids::Fingerprint::of_parts([
            pepper,
            identifier.as_bytes(),
            site.as_bytes(),
        ]);
        Some(Subject {
            hash: format!("s1:{}", &digest.to_hex()[..32]),
            groups: groups.to_vec(),
        })
    }
}

/// One row of the §30.3 data inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryEntry {
    pub field: &'static str,
    pub classification: &'static str,
    pub purpose: &'static str,
    pub retention: &'static str,
    /// What an operator can do about it.
    pub control: &'static str,
}

/// The analytics half of the data inventory (§30.3, ANA-04).
///
/// Both identifiers are listed as personal data. ANA-04 is explicit that
/// Liyasa's design minimises them and does not claim they are anonymous, and a
/// docs page generated from this table says the same thing.
pub const INVENTORY: &[InventoryEntry] = &[
    InventoryEntry {
        field: "session_key",
        classification: "pseudonymous personal data",
        purpose: "counting unique sessions within a day",
        retention: "with the raw event: analytics.retention.rawDays, 90 by default",
        control: "unavoidable while analytics.enabled is true; the salt it is derived from is destroyed at midnight UTC, after which no key can be traced to an address",
    },
    InventoryEntry {
        field: "subject_hash",
        classification: "pseudonymous personal data",
        purpose: "segmenting a private site's readers over time",
        retention: "with the raw event",
        control: "analytics.identityLinkage: false removes it entirely",
    },
    InventoryEntry {
        field: "groups",
        classification: "personal data",
        purpose: "segmentation by team",
        retention: "with the raw event",
        control: "analytics.identityLinkage: false removes it entirely",
    },
    InventoryEntry {
        field: "country",
        classification: "not personal data on its own",
        purpose: "regional traffic",
        retention: "with the raw event",
        control: "taken from an edge header when the host sends one; Liyasa never looks an address up",
    },
    InventoryEntry {
        field: "ip address",
        classification: "personal data, never stored",
        purpose: "input to the daily session key and nothing else",
        retention: "none: it is hashed in memory and not written",
        control: "none needed",
    },
    InventoryEntry {
        field: "search queries, feedback text, assistant messages",
        classification: "free text, scrubbed",
        purpose: "the reports of ANA-20 and ANA-30",
        retention: "with the raw event",
        control: "emails, phone numbers, card numbers, API keys and JWT-shaped strings are redacted before storage (ANA-03)",
    },
];

/// What the docs say about whether a banner is needed (ANA-07).
///
/// It is worded as a description of what Liyasa does and a statement that the
/// legal question is the operator's, because it is: ANA-07 says the docs must
/// say so "rather than asserting it", and a documentation tool telling
/// operators they do not need a cookie banner would be giving legal advice it
/// is not qualified to give.
pub const CONSENT_STATEMENT: &str = "\
Liyasa's own analytics set no cookies, read no cookies, and store no directly \
identifying data. Addresses are hashed with a salt that is destroyed every \
midnight UTC and never written to storage. This is designed to fit the \
\"strictly necessary\" and \"anonymous statistics\" exemptions that most \
jurisdictions provide, but whether a consent banner is required for your site \
is a legal question about your jurisdiction and your users, and it is yours to \
answer. Third-party integrations are a separate matter: they are gated behind \
your consent provider and do not load before consent is given.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_subject_is_stable_for_one_reader_and_differs_between_sites() {
        let policy = IdentityPolicy::default();
        let pepper = b"instance-secret";
        let one = policy
            .subject("user-7", "acme-docs", pepper, &["payments".to_owned()])
            .expect("a subject");
        assert_eq!(
            one,
            policy
                .subject("user-7", "acme-docs", pepper, &["payments".to_owned()])
                .expect("a subject"),
            "segmentation over a quarter needs a stable hash"
        );
        assert_ne!(
            one.hash,
            policy
                .subject("user-8", "acme-docs", pepper, &[])
                .expect("a subject")
                .hash
        );
        assert_ne!(
            one.hash,
            policy
                .subject("user-7", "other-docs", pepper, &[])
                .expect("a subject")
                .hash,
            "one site's subjects never join another's"
        );
        assert_ne!(
            one.hash,
            policy
                .subject("user-7", "acme-docs", b"a-different-secret", &[])
                .expect("a subject")
                .hash
        );
        assert!(one.hash.starts_with("s1:"));
        assert_eq!(one.hash.len(), 35);
        assert!(
            !one.hash.contains("user-7"),
            "the identifier does not survive into the hash"
        );
        assert_eq!(one.groups, ["payments"]);
    }

    #[test]
    fn turning_linkage_off_removes_the_identity_rather_than_hashing_it_harder() {
        let policy = IdentityPolicy { linkage: false };
        assert_eq!(
            policy.subject("user-7", "acme-docs", b"secret", &["payments".to_owned()]),
            None
        );
    }

    #[test]
    fn both_pseudonymous_identifiers_are_in_the_inventory_as_personal_data() {
        for field in ["session_key", "subject_hash"] {
            let entry = INVENTORY
                .iter()
                .find(|e| e.field == field)
                .unwrap_or_else(|| panic!("{field} is missing from the data inventory"));
            assert!(
                entry.classification.contains("personal data"),
                "ANA-04 says these are not claimed to be anonymous"
            );
            assert!(!entry.control.is_empty());
        }
        assert!(
            INVENTORY
                .iter()
                .any(|e| e.field == "ip address" && e.classification.contains("never stored")),
            "ANA-03 is a claim the inventory has to carry"
        );
    }

    #[test]
    fn the_consent_statement_describes_and_does_not_advise() {
        assert!(CONSENT_STATEMENT.contains("set no cookies"));
        assert!(CONSENT_STATEMENT.contains("yours to answer"));
        assert!(
            !CONSENT_STATEMENT.contains("you do not need"),
            "ANA-07 asks the docs to say the decision is the operator's, not to make it"
        );
    }
}
