//! The rules `codes.toml` is held to (PRD §34.5).

use std::collections::BTreeMap;

use liyasa_core::diagnostics::{self, Code, Severity};

#[test]
fn registry_is_not_empty() {
    assert!(
        diagnostics::registry().len() >= 115,
        "every code named in PRD §34.5 must have a row"
    );
}

#[test]
fn codes_are_unique() {
    let mut seen = BTreeMap::new();
    for info in diagnostics::registry() {
        assert!(
            seen.insert(info.code.as_str(), info).is_none(),
            "duplicate code {}",
            info.code
        );
    }
}

#[test]
fn registry_is_sorted() {
    // `Code::new` binary-searches it.
    let mut previous = "";
    for info in diagnostics::registry() {
        assert!(
            previous < info.code.as_str(),
            "{} is out of order",
            info.code
        );
        previous = info.code.as_str();
    }
}

#[test]
fn numbers_are_never_reused_across_prefixes() {
    let mut seen: BTreeMap<u16, Code> = BTreeMap::new();
    for info in diagnostics::registry() {
        if let Some(other) = seen.insert(info.code.number(), info.code) {
            panic!("{} reuses the number of {other}", info.code);
        }
    }
}

#[test]
fn prefix_agrees_with_default_severity() {
    for info in diagnostics::registry() {
        let expected = match info.code.as_str().as_bytes()[0] {
            b'E' => Severity::Error,
            b'W' => Severity::Warning,
            other => panic!("{} has prefix `{}`, not E or W", info.code, other as char),
        };
        assert_eq!(
            info.severity, expected,
            "{} disagrees with its prefix",
            info.code
        );
    }
}

#[test]
fn every_code_falls_in_a_range_its_crate_owns() {
    for info in diagnostics::registry() {
        let number = info.code.number();
        let range = diagnostics::ranges()
            .iter()
            .find(|r| (r.first..=r.last).contains(&number))
            .unwrap_or_else(|| panic!("{} is outside every declared range", info.code));
        assert_eq!(
            info.krate, range.krate,
            "{} is claimed by {} but {}-{} belongs to {}",
            info.code, info.krate, range.first, range.last, range.krate
        );
    }
}

#[test]
fn ranges_are_ordered_and_do_not_overlap() {
    let mut end = 0u16;
    for range in diagnostics::ranges() {
        assert!(
            range.first <= range.last,
            "{}-{} is inverted",
            range.first,
            range.last
        );
        assert!(
            range.first > end,
            "{}-{} overlaps the range ending at {end}",
            range.first,
            range.last
        );
        end = range.last;
    }
}

#[test]
fn lookup_round_trips_and_rejects_strangers() {
    for info in diagnostics::registry() {
        let looked_up = Code::new(info.code.as_str()).expect("registered code resolves");
        assert_eq!(looked_up, info.code);
        assert_eq!(looked_up.title(), info.title);
    }
    assert!(Code::new("E9999").is_none());
    assert!(Code::new("").is_none());
    assert!(Code::new("nonsense").is_none());
}

#[test]
fn help_urls_are_generated_from_the_code() {
    let code = diagnostics::code::E0210;
    assert_eq!(
        code.url(),
        format!("{}E0210", liyasa_core::site::HELP_URL_BASE)
    );
}

#[test]
fn deserializing_an_unregistered_code_is_an_error() {
    let err = serde_json::from_str::<Code>("\"E9999\"").expect_err("must reject");
    assert!(err.to_string().contains("unknown code"), "{err}");
}
