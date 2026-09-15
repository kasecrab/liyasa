//! SARIF 2.1.0, which is what turns a verification failure into a GitHub
//! code-scanning annotation (VER-70).
//!
//! Only the parts of the format a consumer requires: one run, one tool driver
//! carrying a rule per diagnostic code the report used, and one result per
//! finding with a level, a message, and a physical location.

use std::collections::BTreeMap;

use liyasa_core::diagnostics::Code;
use serde_json::{Value, json};

use crate::core::scrub::Scrubber;

use super::{Report, findings, level_name};

pub const SCHEMA: &str = "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json";
pub const VERSION: &str = "2.1.0";

pub fn render(report: &Report, scrubber: &Scrubber) -> String {
    serde_json::to_string_pretty(&document(report, scrubber))
        .unwrap_or_else(|error| format!("{{\"error\":\"{error}\"}}"))
}

pub fn document(report: &Report, scrubber: &Scrubber) -> Value {
    let findings = findings(report, scrubber);

    // One rule per code the report actually used; SARIF wants the rule before
    // the result that points at it.
    let mut rules: BTreeMap<String, Value> = BTreeMap::new();
    for finding in &findings {
        rules.entry(finding.rule.clone()).or_insert_with(|| {
            let code = Code::new(&finding.rule);
            json!({
                "id": finding.rule,
                "name": finding.rule,
                "shortDescription": {
                    "text": code.map_or("Verification finding", |code| code.title()),
                },
                "helpUri": code.map_or_else(
                    || liyasa_core::diagnostics::HELP_URL_BASE.to_owned(),
                    |code| code.url(),
                ),
                "defaultConfiguration": {
                    "level": code.map_or("warning", |code| level_name(code.severity())),
                },
            })
        });
    }

    let results: Vec<Value> = findings
        .iter()
        .map(|finding| {
            json!({
                "ruleId": finding.rule,
                "level": level_name(finding.level),
                "message": { "text": finding.message },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": { "uri": finding.location },
                    },
                }],
                "properties": { "page": finding.page.as_str() },
            })
        })
        .collect();

    json!({
        "$schema": SCHEMA,
        "version": VERSION,
        "runs": [{
            "tool": {
                "driver": {
                    "name": "liyasa",
                    "informationUri": "https://kasecrab.github.io/liyasa",
                    "rules": rules.into_values().collect::<Vec<_>>(),
                },
            },
            "results": results,
        }],
    })
}
