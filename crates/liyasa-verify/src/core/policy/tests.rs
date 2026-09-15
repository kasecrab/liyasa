use std::collections::BTreeMap;

use serde_json::json;

use super::*;

fn route(path: &str) -> Route {
    Route::new(path)
}

#[test]
fn the_defaults_are_the_table_ver_71_prints() {
    let policy = Policy::new();
    assert_eq!(policy.level(CheckClass::Code), PolicyLevel::Error);
    assert_eq!(policy.level(CheckClass::Facts), PolicyLevel::Error);
    assert_eq!(policy.level(CheckClass::Links), PolicyLevel::Warn);
    assert_eq!(policy.level(CheckClass::Screenshots), PolicyLevel::Warn);
    assert_eq!(policy.level(CheckClass::Prose), PolicyLevel::Warn);
    assert_eq!(
        policy.level(CheckClass::ExpiredAttestations),
        PolicyLevel::Error
    );
}

#[test]
fn a_level_maps_to_the_severity_a_finding_is_reported_at() {
    assert_eq!(PolicyLevel::Error.severity(), Some(Severity::Error));
    assert_eq!(PolicyLevel::Warn.severity(), Some(Severity::Warning));
    assert_eq!(PolicyLevel::Off.severity(), None);
}

#[test]
fn an_overlay_leaves_the_classes_it_does_not_name_alone() {
    let site = Policy::new().with(CheckClass::Links, PolicyLevel::Off);
    let page = Policy::new().with(CheckClass::Prose, PolicyLevel::Error);
    let merged = site.overlay(&page);
    assert_eq!(merged.level(CheckClass::Links), PolicyLevel::Off);
    assert_eq!(merged.level(CheckClass::Prose), PolicyLevel::Error);
    assert_eq!(merged.level(CheckClass::Code), PolicyLevel::Error);
}

#[test]
fn ver_71_a_page_override_promotes_links_while_others_stay_warnings() {
    // The acceptance criterion: links demoted site-wide, promoted on one page.
    let site = Policy::new().with(CheckClass::Links, PolicyLevel::Warn);
    let mut set = PolicySet::new(site);
    let (page, problems) = PageVerify::from_value(Some(&json!({ "links": "error" })));
    assert!(problems.is_empty(), "{problems:?}");
    set.set_page(route("/strict"), page);

    assert_eq!(
        set.severity(&route("/strict"), CheckClass::Links),
        Some(Severity::Error)
    );
    assert_eq!(
        set.severity(&route("/other"), CheckClass::Links),
        Some(Severity::Warning)
    );
    // The promotion is scoped to links, not to the page.
    assert_eq!(
        set.severity(&route("/strict"), CheckClass::Prose),
        Some(Severity::Warning)
    );
}

#[test]
fn a_page_override_may_be_written_under_a_policy_key() {
    let (direct, _) = PageVerify::from_value(Some(&json!({ "links": "error" })));
    let (nested, _) = PageVerify::from_value(Some(&json!({ "policy": { "links": "error" } })));
    assert_eq!(direct, nested);
}

#[test]
fn verify_false_turns_every_class_off_for_that_page() {
    let (page, problems) = PageVerify::from_value(Some(&json!(false)));
    assert_eq!(page, PageVerify::Disabled);
    assert!(problems.is_empty());

    let mut set = PolicySet::new(Policy::new());
    set.set_page(route("/draft"), page);
    for class in CheckClass::ALL {
        assert_eq!(set.severity(&route("/draft"), class), None, "{class}");
    }
}

#[test]
fn verify_true_and_an_absent_key_both_inherit() {
    assert_eq!(PageVerify::from_value(None).0, PageVerify::Inherit);
    assert_eq!(
        PageVerify::from_value(Some(&json!(true))).0,
        PageVerify::Inherit
    );
}

#[test]
fn a_page_verify_of_the_wrong_shape_is_a_diagnostic_and_inherits() {
    let (page, problems) = PageVerify::from_value(Some(&json!("yes please")));
    assert_eq!(page, PageVerify::Inherit);
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].code, code::E0635);
}

#[test]
fn both_spellings_of_expired_attestations_are_read() {
    for key in ["expired_attestations", "expiredAttestations"] {
        let (policy, problems) = Policy::from_value(&json!({ key: "warn" }));
        assert!(problems.is_empty(), "{key}: {problems:?}");
        assert_eq!(
            policy.declared(CheckClass::ExpiredAttestations),
            Some(PolicyLevel::Warn),
            "{key}"
        );
    }
}

#[test]
fn an_unknown_class_is_e0634_and_the_rest_still_applies() {
    let (policy, problems) = Policy::from_value(&json!({ "lnks": "error", "prose": "off" }));
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].code, code::E0634);
    assert!(
        problems[0].message.contains("lnks"),
        "{}",
        problems[0].message
    );
    assert_eq!(policy.declared(CheckClass::Prose), Some(PolicyLevel::Off));
}

#[test]
fn an_unknown_level_is_e0635_and_the_rest_still_applies() {
    let (policy, problems) = Policy::from_value(&json!({ "links": "loud", "code": "warn" }));
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].code, code::E0635);
    assert_eq!(policy.declared(CheckClass::Links), None);
    assert_eq!(policy.declared(CheckClass::Code), Some(PolicyLevel::Warn));
}

#[test]
fn a_policy_that_is_not_an_object_is_e0635() {
    let (policy, problems) = Policy::from_value(&json!(["links"]));
    assert!(policy.is_empty());
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].code, code::E0635);
}

#[test]
fn warning_is_an_accepted_spelling_of_warn() {
    let (policy, problems) = Policy::from_value(&json!({ "code": "warning" }));
    assert!(problems.is_empty());
    assert_eq!(policy.declared(CheckClass::Code), Some(PolicyLevel::Warn));
}

#[test]
fn a_policy_round_trips_through_json_with_the_documented_key() {
    let policy = Policy::new().with(CheckClass::ExpiredAttestations, PolicyLevel::Error);
    let json = serde_json::to_string(&policy).expect("serialize");
    assert_eq!(json, r#"{"expired_attestations":"error"}"#);
    let back: Policy = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, policy);
}

fn attrs(pairs: &[(&str, &str)]) -> FenceAttrs {
    FenceAttrs {
        kv: pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect::<BTreeMap<_, _>>(),
        ..FenceAttrs::default()
    }
}

#[test]
fn a_block_skips_with_its_reason() {
    let skip = block_skip(&attrs(&[
        ("verify", "skip"),
        ("reason", "needs a live account"),
    ]))
    .expect("skipped");
    assert_eq!(skip.reason.as_deref(), Some("needs a live account"));
    assert_eq!(skip.reason(), "needs a live account");
}

#[test]
fn a_block_skips_without_one_and_says_so() {
    let skip = block_skip(&attrs(&[("verify", "skip")])).expect("skipped");
    assert_eq!(skip.reason, None);
    assert!(skip.reason().contains("no reason"));
}

#[test]
fn an_empty_reason_counts_as_none() {
    let skip = block_skip(&attrs(&[("verify", "skip"), ("reason", "")])).expect("skipped");
    assert_eq!(skip.reason, None);
}

#[test]
fn a_block_that_asks_to_run_is_not_a_skip() {
    assert_eq!(block_skip(&attrs(&[("verify", "compile")])), None);
    assert_eq!(block_skip(&attrs(&[])), None);
}
