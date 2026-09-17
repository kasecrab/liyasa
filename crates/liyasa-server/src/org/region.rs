//! Data residency (HOST-11).
//!
//! A project's region is chosen when it is created and never afterwards:
//! moving it would mean moving data that was collected under a promise about
//! where it would live, which is the promise the requirement is about. The
//! type carries no setter, so the rule is a property of the API rather than a
//! check someone has to remember to call.

use liyasa_core::diagnostics::{Code, Diagnostic};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Region {
    #[default]
    Us,
    Eu,
    Apac,
}

impl Region {
    pub const ALL: &'static [Region] = &[Region::Us, Region::Eu, Region::Apac];

    pub fn as_str(self) -> &'static str {
        match self {
            Region::Us => "us",
            Region::Eu => "eu",
            Region::Apac => "apac",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Region::Us => "United States",
            Region::Eu => "European Union",
            Region::Apac => "Asia Pacific",
        }
    }

    pub fn parse(text: &str) -> Option<Region> {
        Region::ALL
            .iter()
            .copied()
            .find(|r| r.as_str().eq_ignore_ascii_case(text))
    }
}

/// A class of data HOST-11 pins to a region. Deployment artifacts and the
/// published site are not here: a CDN serves them everywhere by design, and
/// claiming otherwise would be a promise the product does not keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Resident {
    Analytics,
    Assistant,
}

impl Resident {
    pub fn as_str(self) -> &'static str {
        match self {
            Resident::Analytics => "analytics",
            Resident::Assistant => "assistant",
        }
    }
}

/// HOST-11: analytics and assistant data stay in region. `home` is where the
/// project was created; `writing_in` is where the request is being served.
pub fn check(home: Region, writing_in: Region, what: Resident) -> Result<(), Diagnostic> {
    if home == writing_in {
        return Ok(());
    }
    let code = Code::new("E0855").expect("E0855 is registered");
    Err(Diagnostic::new(
        code,
        format!(
            "{} data for a project in {} cannot be written in {}",
            what.as_str(),
            home.label(),
            writing_in.label()
        ),
    )
    .help(format!(
        "route the request to the {} region, or create a separate project there",
        home.as_str()
    )))
}

/// The refusal HOST-11 implies but does not name: a region is chosen at
/// project creation, so there is no "change region" operation to succeed
/// quietly and leave the old rows where they were.
pub fn cannot_move(project: &str, from: Region, to: Region) -> Diagnostic {
    let code = Code::new("E0854").expect("E0854 is registered");
    Diagnostic::new(
        code,
        format!(
            "`{project}` was created in {} and cannot be moved to {}",
            from.label(),
            to.label()
        ),
    )
    .help("create a project in the new region and export into it")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_regions_host_11_names_round_trip() {
        for region in Region::ALL {
            assert_eq!(Region::parse(region.as_str()), Some(*region));
            assert!(!region.label().is_empty());
        }
        assert_eq!(Region::parse("EU"), Some(Region::Eu));
        assert_eq!(Region::parse("uk"), None);
    }

    #[test]
    fn analytics_and_assistant_data_may_not_cross_a_region_boundary() {
        assert!(check(Region::Eu, Region::Eu, Resident::Analytics).is_ok());
        let refused = check(Region::Eu, Region::Us, Resident::Assistant)
            .expect_err("a cross-region write is refused");
        assert_eq!(refused.code.as_str(), "E0855");
        assert!(
            refused.message.contains("European Union"),
            "{}",
            refused.message
        );
        assert!(refused.help.is_some(), "a refusal an operator can act on");
    }

    #[test]
    fn a_region_cannot_be_changed_after_the_project_exists() {
        let refusal = cannot_move("docs", Region::Us, Region::Apac);
        assert_eq!(refusal.code.as_str(), "E0854");
        assert!(refusal.message.contains("docs"), "{}", refusal.message);
    }
}
