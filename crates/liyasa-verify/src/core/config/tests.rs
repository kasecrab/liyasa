use serde_json::json;

use super::*;
use crate::core::policy::{CheckClass, PolicyLevel};

/// VER-76's object, written the way the requirement prints it.
fn ver_76_example() -> Value {
    json!({
        "default": "tagged",
        "hidePrefix": "# ",
        "policy": {
            "code": "error",
            "facts": "error",
            "links": "warn",
            "screenshots": "warn",
            "prose": "warn",
            "expired_attestations": "error"
        },
        "runners": {
            "sandbox": "container",
            "registry": null,
            "images": { "rust": "docker.io/library/rust@sha256:abc" },
            "custom": []
        },
        "http": {
            "target": "mock",
            "staging": { "baseUrl": "https://staging.example.com", "auth": "secret:staging" }
        },
        "links": { "schedule": "6h", "grace": "72h", "allowHosts": [], "denyHosts": [] },
        "sources": {
            "commands": { "allow": [] },
            "trustedBranches": ["main"],
            "refresh": "24h"
        },
        "budget": { "deploy": "60s", "full": "2h" },
        "blockDeploy": false,
        "schedule": "0 2 * * *",
        "badges": { "enabled": false, "public": false },
        "screenshots": { "tolerance": 0.02 },
        "drift": { "batchSize": 25, "severityThreshold": "medium" }
    })
}

#[test]
fn ver_76_the_documented_object_reads_without_a_complaint() {
    let (config, problems) = VerifyConfig::from_value(&ver_76_example());
    assert!(problems.is_empty(), "{problems:#?}");

    assert_eq!(config.default, VerifyDefault::Tagged);
    assert_eq!(config.hide_prefix, "# ");
    assert_eq!(
        config.policy.declared(CheckClass::Links),
        Some(PolicyLevel::Warn)
    );
    assert_eq!(config.runners.sandbox, SandboxKind::Container);
    assert_eq!(config.runners.registry, None);
    assert_eq!(
        config.runners.images.get("rust").map(String::as_str),
        Some("docker.io/library/rust@sha256:abc")
    );
    assert_eq!(config.http.target, HttpTarget::Mock);
    assert_eq!(
        config.http.staging.as_ref().map(|s| s.base_url.as_str()),
        Some("https://staging.example.com")
    );
    assert_eq!(config.links.schedule.to_string(), "6h");
    assert_eq!(config.links.grace.to_string(), "72h");
    assert_eq!(config.sources.trusted_branches, ["main"]);
    assert_eq!(config.sources.refresh.to_string(), "24h");
    assert_eq!(config.budget.deploy.to_string(), "60s");
    assert_eq!(config.budget.full.to_string(), "2h");
    assert!(!config.block_deploy);
    assert_eq!(config.schedule, "0 2 * * *");
    assert_eq!(config.badges, BadgesConfig::default());
    assert!((config.screenshots.tolerance - 0.02).abs() < f32::EPSILON);
    assert_eq!(config.drift.batch_size, 25);
    assert_eq!(config.drift.severity_threshold, DriftSeverity::Medium);
}

#[test]
fn the_documented_values_are_the_ones_liyasa_defaults_to() {
    // VER-76 prints the object with its defaults filled in, except for the
    // three keys it uses to show a shape: `policy`, `runners.images`, and
    // `http.staging`. Strip those and the rest must be the default, or an
    // operator who copies the requirement changes their site's behaviour.
    let mut example = ver_76_example();
    let object = example.as_object_mut().expect("object");
    object.remove("policy");
    object["runners"]["images"] = json!({});
    object["http"]["staging"] = Value::Null;

    let (config, problems) = VerifyConfig::from_value(&example);
    assert!(problems.is_empty(), "{problems:#?}");
    assert_eq!(config, VerifyConfig::default());
}

#[test]
fn the_documented_policy_is_the_one_ver_71_prints() {
    let (config, problems) = VerifyConfig::from_value(&ver_76_example());
    assert!(problems.is_empty(), "{problems:#?}");
    // Every class written out in VER-76 agrees with VER-71's own table, which
    // is what `Policy` falls back to when a class is not declared.
    for class in CheckClass::ALL {
        assert_eq!(
            config.policy.declared(class),
            Some(Policy::new().level(class)),
            "{class}"
        );
    }
}

#[test]
fn an_absent_verify_key_is_the_default() {
    let (config, problems) = VerifyConfig::from_value(&Value::Null);
    assert!(problems.is_empty());
    assert_eq!(config, VerifyConfig::default());
}

#[test]
fn an_unreadable_key_costs_only_that_key() {
    let (config, problems) = VerifyConfig::from_value(&json!({
        "budget": { "deploy": "one minute" },
        "blockDeploy": true
    }));
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert_eq!(problems[0].code, code::E0635);
    assert!(
        problems[0].message.contains("budget"),
        "{}",
        problems[0].message
    );
    // The default survives the bad value, and the neighbouring key applies.
    assert_eq!(config.budget.deploy.to_string(), "60s");
    assert!(config.block_deploy);
}

#[test]
fn both_spellings_of_the_hidden_line_prefix_are_read() {
    for key in ["hidePrefix", "hide_prefix"] {
        let (config, problems) = VerifyConfig::from_value(&json!({ key: "//! " }));
        assert!(problems.is_empty(), "{key}: {problems:?}");
        assert_eq!(config.hide_prefix, "//! ", "{key}");
    }
}

#[test]
fn the_schemas_budget_total_is_read_as_full() {
    let (config, problems) = VerifyConfig::from_value(&json!({ "budget": { "total": "4h" } }));
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(config.budget.full.to_string(), "4h");
}

#[test]
fn hosts_are_written_as_plain_strings() {
    let (config, problems) = VerifyConfig::from_value(&json!({
        "links": { "allowHosts": ["example.com", "*.example.org", ".example.net", "*"] }
    }));
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(
        config.links.allow_hosts.0,
        vec![
            HostPattern::Exact("example.com".to_owned()),
            HostPattern::Suffix("example.org".to_owned()),
            HostPattern::Suffix("example.net".to_owned()),
            HostPattern::Any,
        ]
    );
    assert!(config.links.allow_hosts.matches("docs.example.org"));
    assert!(!config.links.deny_hosts.matches("docs.example.org"));
}

#[test]
fn the_tagged_host_form_is_read_too() {
    let (config, problems) = VerifyConfig::from_value(&json!({
        "links": { "denyHosts": [{ "exact": "blocked.example" }] }
    }));
    assert!(problems.is_empty(), "{problems:?}");
    assert!(config.links.deny_hosts.matches("blocked.example"));
}

#[test]
fn a_host_list_that_is_not_a_list_is_e0635() {
    let (_, problems) = VerifyConfig::from_value(&json!({
        "links": { "allowHosts": "example.com" }
    }));
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].code, code::E0635);
}

#[test]
fn a_tolerance_outside_zero_to_one_is_e0635() {
    let (_, problems) = VerifyConfig::from_value(&json!({ "screenshots": { "tolerance": 12.0 } }));
    assert_eq!(problems.len(), 1);
    assert!(problems[0].message.contains("tolerance"));
}

#[test]
fn a_schedule_that_is_not_cron_shaped_is_e0635() {
    let (_, problems) = VerifyConfig::from_value(&json!({ "schedule": "nightly" }));
    assert_eq!(problems.len(), 1);
    assert!(problems[0].message.contains("schedule"));

    let (_, six) = VerifyConfig::from_value(&json!({ "schedule": "0 0 2 * * *" }));
    assert!(
        six.is_empty(),
        "a six-field expression is a cron too: {six:?}"
    );
}

#[test]
fn a_command_allowed_without_a_digest_is_e0635() {
    let (_, problems) = VerifyConfig::from_value(&json!({
        "sources": { "commands": { "allow": [{ "path": "./x.sh", "sha256": "nope" }] } }
    }));
    assert_eq!(problems.len(), 1);
    assert!(problems[0].message.contains("./x.sh"));
}

#[test]
fn a_command_allowed_with_a_digest_passes() {
    let (config, problems) = VerifyConfig::from_value(&json!({
        "sources": { "commands": { "allow": [
            { "path": "./scripts/export-limits.sh", "sha256": "ab".repeat(32) }
        ] } }
    }));
    assert!(problems.is_empty(), "{problems:?}");
    assert!(config.sources.commands.allow[0].hash_is_well_formed());
}

#[test]
fn a_custom_runner_reads_its_command_and_image() {
    let (config, problems) = VerifyConfig::from_value(&json!({
        "runners": { "custom": [{
            "id": "zig",
            "languages": ["zig"],
            "image": "ghcr.io/example/zig@sha256:def",
            "command": ["zig", "run", "{file}"],
            "timeout": "45s",
            "network": false
        }]}
    }));
    assert!(problems.is_empty(), "{problems:?}");
    let runner = &config.runners.custom[0];
    assert_eq!(runner.id, "zig");
    assert_eq!(runner.languages, ["zig"]);
    assert_eq!(runner.command, ["zig", "run", "{file}"]);
    assert_eq!(
        runner.timeout.map(|t| t.to_string()).as_deref(),
        Some("45s")
    );
    assert!(!runner.network);
}

#[test]
fn the_sandbox_and_target_enums_read_every_documented_value() {
    for (text, want) in [
        ("container", SandboxKind::Container),
        ("remote", SandboxKind::Remote),
        ("local", SandboxKind::Local),
    ] {
        let (config, problems) =
            VerifyConfig::from_value(&json!({ "runners": { "sandbox": text } }));
        assert!(problems.is_empty(), "{text}: {problems:?}");
        assert_eq!(config.runners.sandbox, want);
    }
    for (text, want) in [
        ("mock", HttpTarget::Mock),
        ("staging", HttpTarget::Staging),
        ("both", HttpTarget::Both),
    ] {
        let (config, problems) = VerifyConfig::from_value(&json!({ "http": { "target": text } }));
        assert!(problems.is_empty(), "{text}: {problems:?}");
        assert_eq!(config.http.target, want);
    }
}

#[test]
fn a_config_round_trips_through_json() {
    let mut config = VerifyConfig::default();
    config.links.allow_hosts = HostSet(vec![HostPattern::Suffix("example.org".to_owned())]);
    config.policy.set(CheckClass::Links, PolicyLevel::Error);
    let json = serde_json::to_value(&config).expect("serialize");
    let back: VerifyConfig = serde_json::from_value(json).expect("deserialize");
    assert_eq!(back, config);
}

#[test]
fn verify_of_the_wrong_shape_is_e0635_and_keeps_the_defaults() {
    let (config, problems) = VerifyConfig::from_value(&json!("on"));
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].code, code::E0635);
    assert_eq!(config, VerifyConfig::default());
}
