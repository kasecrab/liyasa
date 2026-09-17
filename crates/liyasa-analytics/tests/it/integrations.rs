//! ANA-60, ANA-62 and the consent gate of ANA-07.
//!
//! The vendor table is held against the real `schemas/liyasa.schema.json`
//! rather than against a copy of it, because a vendor Liyasa knows about and
//! the schema rejects is a key an operator cannot turn on, and a key the
//! schema accepts and Liyasa does not know is a script with no CSP sources.

use liyasa_analytics::integrations::{self, Consent, VendorKind};
use serde_json::{Value, json};

/// ANA-60 names these seventeen. The list is in the requirement, so it is
/// written out here rather than derived from the table under test.
const ANA_60: &[&str] = &[
    "adobeAnalytics",
    "amplitude",
    "clarity",
    "clearbit",
    "fathom",
    "ga4",
    "gtm",
    "heap",
    "hightouch",
    "hotjar",
    "koala",
    "logrocket",
    "mixpanel",
    "pirsch",
    "plausible",
    "posthog",
    "segment",
];

fn schema() -> Value {
    let text = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../schemas/liyasa.schema.json"
    ));
    serde_json::from_str(text).expect("the config schema parses")
}

#[test]
fn every_vendor_ana_60_names_is_in_the_table_and_in_the_schema() {
    let schema = schema();
    let keys = schema["properties"]["integrations"]["properties"]
        .as_object()
        .expect("the integrations block has properties");
    for key in ANA_60 {
        let found = integrations::vendor(key)
            .unwrap_or_else(|| panic!("ANA-60 names `{key}` and the vendor table does not"));
        assert_eq!(found.kind, VendorKind::Analytics);
        assert!(
            keys.contains_key(*key),
            "`{key}` is a vendor Liyasa knows and the schema rejects, so nobody can enable it"
        );
        assert!(
            !found.script_src.is_empty() || found.key == "builtin",
            "ANA-62 maintains the CSP sources; `{key}` has none"
        );
    }
}

#[test]
fn the_support_widgets_and_consent_providers_of_ana_61_are_there_too() {
    for key in ["intercom", "front", "crisp", "zendesk", "plain"] {
        assert_eq!(
            integrations::vendor(key).expect(key).kind,
            VendorKind::Support
        );
    }
    for key in ["osano", "transcend", "onetrust", "cookiebot", "builtin"] {
        assert_eq!(
            integrations::vendor(key).expect(key).kind,
            VendorKind::Consent
        );
    }
}

#[test]
fn the_schema_and_the_table_name_the_same_consent_providers() {
    let schema = schema();
    let listed =
        schema["properties"]["integrations"]["properties"]["cookieConsent"]["oneOf"][0]["enum"]
            .as_array()
            .expect("the provider enum");
    for value in listed {
        let key = value.as_str().expect("a string");
        assert_eq!(
            integrations::vendor(key).expect(key).kind,
            VendorKind::Consent,
            "the schema offers `{key}` as a consent provider"
        );
    }
}

#[test]
fn a_key_that_is_absent_false_or_null_is_off() {
    let (enabled, unknown) = integrations::configure(&json!({
        "ga4": false,
        "posthog": null,
    }));
    assert!(enabled.is_empty(), "false and null mean off (CFG-81)");
    assert!(unknown.is_empty());
    assert!(integrations::configure(&json!({})).0.is_empty());
}

#[test]
fn an_integration_that_does_not_say_otherwise_waits_for_consent() {
    let (enabled, _) = integrations::configure(&json!({
        "ga4": "G-ABC123",
        "plausible": { "id": "acme.com" },
        "fathom": { "id": "ABCDEFGH", "consent": "none" },
        "posthog": true,
    }));
    let by_key = |key: &str| {
        enabled
            .iter()
            .find(|c| c.key == key)
            .unwrap_or_else(|| panic!("{key} should be enabled"))
    };

    assert_eq!(by_key("ga4").consent, Consent::Required);
    assert_eq!(by_key("ga4").id.as_deref(), Some("G-ABC123"));
    assert!(!by_key("ga4").loads_before_consent());

    assert_eq!(
        by_key("plausible").consent,
        Consent::Required,
        "a row with no `consent` is gated: a forgotten field must not be a privacy incident (RFC 1703)"
    );
    assert!(!by_key("plausible").loads_before_consent());

    assert_eq!(by_key("fathom").consent, Consent::None);
    assert!(
        by_key("fathom").loads_before_consent(),
        "an operator who decided this on purpose gets what they asked for"
    );

    assert_eq!(by_key("posthog").consent, Consent::Required);
    assert_eq!(by_key("posthog").id, None);
}

#[test]
fn a_consent_provider_loads_before_consent_because_it_is_what_asks() {
    let (enabled, _) = integrations::configure(&json!({
        "cookieConsent": "onetrust",
        "ga4": "G-ABC123",
    }));
    let provider = integrations::consent_provider(&enabled).expect("a provider");
    assert_eq!(provider.kind, VendorKind::Consent);
    assert!(provider.loads_before_consent());
    assert!(integrations::gated_without_a_provider(&enabled).is_empty());
}

#[test]
fn a_gated_integration_with_no_provider_is_reported_rather_than_left_silent() {
    let (enabled, _) = integrations::configure(&json!({
        "ga4": "G-ABC123",
        "mixpanel": "token",
        "fathom": { "id": "ABCDEFGH", "consent": "none" },
    }));
    assert!(integrations::consent_provider(&enabled).is_none());
    let stuck: Vec<&str> = integrations::gated_without_a_provider(&enabled)
        .iter()
        .map(|c| c.key.as_str())
        .collect();
    assert_eq!(
        stuck,
        ["ga4", "mixpanel"],
        "these would never load, and an operator should be told"
    );
}

#[test]
fn an_unknown_key_is_reported_and_not_silently_dropped() {
    let (enabled, unknown) = integrations::configure(&json!({
        "ga4": "G-ABC123",
        "gogle_analytics": "G-TYPO",
    }));
    assert_eq!(enabled.len(), 1);
    assert_eq!(unknown.len(), 1);
    assert_eq!(unknown[0].0, "gogle_analytics");
}

#[test]
fn a_consent_value_the_schema_would_reject_falls_to_the_safe_side() {
    let (enabled, _) = integrations::configure(&json!({
        "ga4": { "id": "G-ABC123", "consent": "optional" },
    }));
    assert_eq!(enabled[0].consent, Consent::Required);
    assert!(!enabled[0].loads_before_consent());
}

#[test]
fn the_policy_carries_every_enabled_vendors_origins_and_no_others() {
    let (enabled, _) = integrations::configure(&json!({
        "ga4": "G-ABC123",
        "gtm": "GTM-XYZ",
        "posthog": "phc_1",
    }));
    let policy = integrations::csp(&enabled);
    assert!(
        policy
            .script_src
            .contains(&"https://www.googletagmanager.com".to_owned())
    );
    assert!(
        policy
            .connect_src
            .contains(&"https://www.google-analytics.com".to_owned())
    );
    assert!(
        policy
            .script_src
            .contains(&"https://*.posthog.com".to_owned())
    );
    assert!(
        !policy
            .script_src
            .contains(&"https://cdn.segment.com".to_owned()),
        "a vendor that is off contributes nothing"
    );
    // GA4 and GTM share an origin; it appears once.
    assert_eq!(
        policy
            .script_src
            .iter()
            .filter(|s| *s == "https://www.googletagmanager.com")
            .count(),
        1
    );
    let mut sorted = policy.script_src.clone();
    sorted.sort();
    assert_eq!(policy.script_src, sorted, "a stable header between builds");
}

#[test]
fn no_vendor_key_appears_twice() {
    let mut seen = std::collections::BTreeSet::new();
    for vendor in integrations::VENDORS {
        assert!(seen.insert(vendor.key), "{} appears twice", vendor.key);
        assert!(!vendor.name.is_empty());
    }
}
