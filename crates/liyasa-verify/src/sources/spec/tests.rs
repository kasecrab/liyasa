use liyasa_core::ids::FactId;
use liyasa_core::verify::{FactValue, SourceKind};
use serde_json::{Value, json};

use super::*;

fn codes(problems: &[Diagnostic]) -> Vec<&str> {
    problems.iter().map(|d| d.code.as_str()).collect()
}

fn spec(declaration: Value) -> SourceSpec {
    let (spec, problems) = SourceSpec::parse("pricing", &declaration);
    assert_eq!(codes(&problems), Vec::<&str>::new(), "{problems:#?}");
    spec
}

#[test]
fn a_url_source_names_its_document_and_its_facts() {
    let spec = spec(json!({
        "kind": "url",
        "url": "https://api.example.com/plans",
        "facts": { "plan.pro.price": "/plans/pro/price_cents" }
    }));
    assert_eq!(spec.kind, SourceKind::Url);
    assert_eq!(spec.url.as_deref(), Some("https://api.example.com/plans"));
    assert_eq!(spec.declared_facts(), vec![FactId::new("plan.pro.price")]);
}

#[test]
fn every_kind_says_what_it_is_missing() {
    for (declaration, want) in [
        (json!({ "kind": "file" }), "has no `path`"),
        (json!({ "kind": "url" }), "has no `url`"),
        (json!({ "kind": "openapi" }), "has no `url` or `path`"),
        (
            json!({ "kind": "command", "path": "./f.sh" }),
            "has no `command`",
        ),
        (
            json!({ "kind": "manual", "owner": "ops", "expires": "2026-12-31" }),
            "has no `values`",
        ),
    ] {
        let (_, problems) = SourceSpec::parse("s", &declaration);
        assert!(
            problems.iter().any(|d| d.message.contains(want)),
            "`{want}` is not in {problems:#?}"
        );
        assert_eq!(codes(&problems), vec!["E0635"; problems.len()]);
    }
}

#[test]
fn an_unknown_kind_is_a_problem_about_the_kind() {
    let (_, problems) = SourceSpec::parse("s", &json!({ "kind": "ftp", "path": "x" }));
    assert_eq!(codes(&problems), ["E0635"]);
    assert!(problems[0].message.contains("kind"), "{problems:#?}");
}

#[test]
fn a_declaration_with_no_kind_says_which_kinds_exist() {
    let (_, problems) = SourceSpec::parse("s", &json!({ "path": "x" }));
    let help = problems[0].help.as_deref().unwrap_or_default();
    for kind in [
        "file",
        "repo",
        "url",
        "openapi",
        "command",
        "manual",
        "screenshot",
    ] {
        assert!(help.contains(kind), "`{kind}` is not in `{help}`");
    }
}

#[test]
fn a_manual_source_needs_an_owner_and_an_expiry() {
    let (_, problems) = SourceSpec::parse(
        "sla",
        &json!({ "kind": "manual", "values": { "sla.uptime": 99.95 } }),
    );
    let messages: Vec<&str> = problems.iter().map(|d| d.message.as_str()).collect();
    assert!(
        messages.iter().any(|m| m.contains("`owner`")),
        "{messages:?}"
    );
    assert!(
        messages.iter().any(|m| m.contains("`expires`")),
        "{messages:?}"
    );
}

#[test]
fn an_expiry_that_is_not_a_date_is_refused() {
    let (_, problems) = SourceSpec::parse(
        "sla",
        &json!({
            "kind": "manual", "owner": "ops", "expires": "next tuesday",
            "values": { "sla.uptime": 99.95 }
        }),
    );
    assert!(
        problems.iter().any(|d| d.message.contains("RFC 3339")),
        "{problems:#?}"
    );
}

#[test]
fn a_pin_is_a_hex_sha_256() {
    let (_, problems) = SourceSpec::parse(
        "s",
        &json!({ "kind": "url", "url": "https://x.test/", "pin": "sha256:whatever" }),
    );
    assert!(
        problems.iter().any(|d| d.message.contains("pin")),
        "{problems:#?}"
    );
    let good = spec(json!({
        "kind": "url", "url": "https://x.test/", "pin": "a".repeat(64)
    }));
    assert!(good.pin.is_some());
}

#[test]
fn commands_is_the_allow_list_and_never_a_source() {
    let (set, problems) = SourceSet::parse(&json!({
        "commands": { "allow": [{ "path": "./f.sh", "sha256": "ab" }] },
        "limits": { "kind": "file", "path": "facts/limits.json" }
    }));
    assert_eq!(codes(&problems), Vec::<&str>::new(), "{problems:#?}");
    assert!(set.get("commands").is_none());
    assert!(set.get("limits").is_some());
}

#[test]
fn a_fact_declared_twice_has_no_one_source() {
    let (set, problems) = SourceSet::parse(&json!({
        "a": { "kind": "file", "path": "a.json", "facts": { "plan.pro.price": "/p" } },
        "b": { "kind": "file", "path": "b.json", "facts": { "plan.pro.price": "/p" } }
    }));
    assert_eq!(codes(&problems), ["E0635"]);
    assert!(
        problems[0].message.contains("no one source"),
        "{problems:#?}"
    );
    // The first declaration keeps the fact rather than the last write winning.
    assert_eq!(set.source_of(&FactId::new("plan.pro.price")), Some("a"));
}

#[test]
fn a_fact_no_declaration_produces_has_no_source() {
    let (set, _) = SourceSet::parse(&json!({
        "a": { "kind": "file", "path": "a.json", "facts": { "plan.pro.price": "/p" } }
    }));
    assert_eq!(set.source_of(&FactId::new("plan.free.price")), None);
    assert_eq!(set.facts_of("a"), vec![FactId::new("plan.pro.price")]);
    assert_eq!(set.facts_of("nope"), Vec::<FactId>::new());
}

#[test]
fn the_four_types_json_cannot_express_are_declared_not_inferred() {
    for (declared, value, want) in [
        (
            json!({ "type": "currency", "code": "USD", "minor": 2 }),
            json!(20.0),
            FactValue::Currency {
                amount: 2000,
                minor: 2,
                code: "USD".to_owned(),
            },
        ),
        (json!("percentage"), json!(99.95), FactValue::Percent(99.95)),
        (
            json!("date"),
            json!("2026-09-17"),
            FactValue::Date("2026-09-17".to_owned()),
        ),
        (
            json!({ "type": "enum", "values": ["ga", "beta"] }),
            json!("beta"),
            FactValue::Enum("beta".to_owned()),
        ),
    ] {
        let kind = fact_type(&declared).expect("a declared type");
        assert_eq!(kind.coerce(&value), Ok(want));
        // Nothing infers these: the same value with no declaration is plain.
        let inferred = FactType::inferred(&value).expect("a JSON type");
        assert!(matches!(
            inferred.coerce(&value),
            Ok(FactValue::Num(_) | FactValue::Str(_))
        ));
    }
}

#[test]
fn a_currency_amount_is_in_minor_units() {
    let usd = fact_type(&json!({ "type": "currency", "code": "USD" })).expect("a type");
    assert_eq!(
        usd.coerce(&json!(20.005)),
        Ok(FactValue::Currency {
            amount: 2001,
            minor: 2,
            code: "USD".to_owned()
        })
    );
    let jpy = fact_type(&json!({ "type": "currency", "code": "JPY", "minor": 0 })).expect("a type");
    assert_eq!(
        jpy.coerce(&json!(1200)),
        Ok(FactValue::Currency {
            amount: 1200,
            minor: 0,
            code: "JPY".to_owned()
        })
    );
}

#[test]
fn a_currency_needs_a_code_and_an_enum_needs_its_values() {
    assert!(fact_type(&json!({ "type": "currency" })).is_err());
    assert!(fact_type(&json!({ "type": "enum" })).is_err());
    assert!(fact_type(&json!("nonsense")).is_err());
}

#[test]
fn a_value_that_does_not_fit_its_type_is_refused() {
    let date = fact_type(&json!("date")).expect("a type");
    assert!(date.coerce(&json!("17/09/2026")).is_err());
    assert!(date.coerce(&json!(20260917)).is_err());
    let variant = fact_type(&json!({ "type": "enum", "values": ["ga"] })).expect("a type");
    assert!(variant.coerce(&json!("beta")).is_err());
    let number = fact_type(&json!("number")).expect("a type");
    assert!(number.coerce(&json!("20")).is_err());
}

#[test]
fn a_document_with_no_fact_map_is_flattened_by_dotted_path() {
    let flat = flatten(&json!({
        "plan": { "pro": { "price": 20, "name": "Pro" } },
        "regions": ["eu", "us"]
    }));
    let keys: Vec<String> = flat.keys().map(ToString::to_string).collect();
    assert_eq!(
        keys,
        ["plan.pro.name", "plan.pro.price", "regions.0", "regions.1"]
    );
}

#[test]
fn a_fact_map_takes_exactly_the_facts_it_names() {
    let spec = spec(json!({
        "kind": "url",
        "url": "https://x.test/",
        "facts": { "plan.pro.price": "/plans/pro/price" },
        "types": { "plan.pro.price": { "type": "currency", "code": "USD" } }
    }));
    let document = json!({ "plans": { "pro": { "price": 20, "seats": 5 } } });
    let facts = read_facts(&spec, &document).expect("the pointer resolves");
    assert_eq!(facts.len(), 1, "`seats` is not a declared fact: {facts:?}");
    assert_eq!(
        facts[&FactId::new("plan.pro.price")],
        FactValue::Currency {
            amount: 2000,
            minor: 2,
            code: "USD".to_owned()
        }
    );
}

#[test]
fn a_pointer_that_resolves_to_nothing_is_an_error_naming_it() {
    let spec = spec(json!({
        "kind": "url", "url": "https://x.test/",
        "facts": { "plan.pro.price": "/plans/pro/price" }
    }));
    let error = read_facts(&spec, &json!({ "plans": {} })).expect_err("no such pointer");
    assert!(error.contains("/plans/pro/price"), "{error}");
    assert!(error.contains("plan.pro.price"), "{error}");
}

#[test]
fn a_json_null_is_the_absence_of_a_value_not_a_fact() {
    let spec = spec(json!({ "kind": "file", "path": "f.json" }));
    let facts = read_facts(&spec, &json!({ "a": 1, "b": null })).expect("flattened");
    assert_eq!(
        facts.keys().map(ToString::to_string).collect::<Vec<_>>(),
        ["a"]
    );
}
