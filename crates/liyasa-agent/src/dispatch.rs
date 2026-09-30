//! Authorising one tool call (AGT-03, AGT-04).
//!
//! AGT-03's acceptance test is one sentence with two halves: a call with invalid
//! input or below its `min_trust` "is rejected with `E0903` **and audited**". Both
//! halves are structural here.
//!
//! [`ToolGate::authorise`] takes `&mut RunRecord`, so there is no way to ask
//! whether a call is permitted without the record being present to write the
//! answer into. Every path through the function records — the permitted one too,
//! because AGT-05 wants every tool call and not every refused one.
//!
//! `E0903` is [`Rejection::diagnostic`]. The code's row in `codes.toml` reads
//! `crate = "liyasa-ai"` because the `0900-0999` range declares that crate and
//! `liyasa-core`'s registry test requires the row to match its range; the field
//! names the area, and nothing ties it to the crate that raises the code.
//! `xtask`'s unraised scan reads `crates/*/src/`, so this file is what takes
//! `E0903` off that list.
//!
//! The checks run in a fixed order, and the order is the cheap-and-broad first:
//! a tool that does not exist, then trust, then the schema, then the budget, then
//! whatever that particular tool is bound by. A call refused at the first check is
//! recorded once with that reason, not once per check it would also have failed.

use std::collections::BTreeSet;

use liyasa_core::ai::{ToolSpec, TrustLevel};
use liyasa_core::diagnostics::{Diagnostic, code};
use serde_json::Value;

use crate::config::AgentConfig;
use crate::record::{CallOutcome, RunRecord};
use crate::repos::RepoDenial;
use crate::scope::{Layout, Target};
use crate::trust::{Denial, Restrictions};

/// Why one call was refused. Every variant is `E0903`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Rejection {
    #[error("`{name}` is not a tool this agent has")]
    Unknown { name: String },
    #[error(
        "`{tool}` needs at least `{min_trust:?}` trust and this run is `{trust:?}`: \
         it is not offered to this run at all"
    )]
    BelowTrust {
        tool: String,
        min_trust: TrustLevel,
        trust: TrustLevel,
    },
    #[error("`{tool}` was called with input its schema refuses: {message}")]
    InvalidInput { tool: String, message: String },
    #[error("`{tool}` cannot write here: {denial}")]
    OutOfScope { tool: String, denial: Denial },
    #[error("`{tool}` cannot read `{path}` in `{repo}`: {denial}")]
    Repo {
        tool: String,
        repo: String,
        path: String,
        /// Boxed: three strings and a two-string denial put this variant over
        /// clippy's `result_large_err` threshold and made every function that
        /// returns a `Rejection` pay for it.
        denial: Box<RepoDenial>,
    },
    #[error("`{tool}` cannot reach `{host}`: it is not in `security.allowHosts.agentFetch`")]
    HostNotAllowed { tool: String, host: String },
    #[error("`{tool}` was refused: this run has spent its {budget} budget of {limit}")]
    BudgetExhausted {
        tool: String,
        /// Which budget: `tool-call`, `token` or `wall-time` (AGT-04).
        budget: &'static str,
        limit: u64,
    },
}

impl Rejection {
    /// `E0903`, with the reason as the message.
    pub fn diagnostic(&self) -> Diagnostic {
        let mut diagnostic = Diagnostic::new(code::E0903, self.to_string());
        diagnostic.help = Some(match self {
            Rejection::Unknown { .. } => {
                "the tool names are fixed; see `liyasa_agent::tools::NAMES`".to_owned()
            }
            Rejection::BelowTrust { .. } => {
                "a run's trust is the minimum over its inputs; an untrusted-trigger run \
                 has a narrower tool surface by design (AGT-04)"
                    .to_owned()
            }
            Rejection::InvalidInput { .. } => {
                "the tool's `input_schema` is the contract; it refuses keys it does not declare"
                    .to_owned()
            }
            Rejection::OutOfScope { .. } => {
                "an untrusted-trigger run may write only the pages its trigger named \
                 (AGT-04)"
                    .to_owned()
            }
            Rejection::Repo { repo, .. } => {
                format!("add the path to `{repo}`'s allow list under `ai.agent.contextRepos`")
            }
            Rejection::HostNotAllowed { .. } => {
                "add the host to `security.allowHosts.agentFetch`".to_owned()
            }
            Rejection::BudgetExhausted { budget, .. } => {
                format!("raise the run's {budget} budget, or split the task across runs")
            }
        });
        diagnostic
    }

    /// The tool the call was for, where there was one.
    pub fn tool(&self) -> &str {
        match self {
            Rejection::Unknown { name } => name,
            Rejection::BelowTrust { tool, .. }
            | Rejection::InvalidInput { tool, .. }
            | Rejection::OutOfScope { tool, .. }
            | Rejection::Repo { tool, .. }
            | Rejection::HostNotAllowed { tool, .. }
            | Rejection::BudgetExhausted { tool, .. } => tool,
        }
    }
}

/// What decides a call.
///
/// Holds no mutable state of its own: the call count comes off the record, so two
/// gates over one record cannot each allow a full budget.
pub struct ToolGate<'a> {
    pub restrictions: &'a Restrictions,
    pub config: &'a AgentConfig,
    pub layout: &'a Layout,
}

impl<'a> ToolGate<'a> {
    pub fn new(
        restrictions: &'a Restrictions,
        config: &'a AgentConfig,
        layout: &'a Layout,
    ) -> Self {
        Self {
            restrictions,
            config,
            layout,
        }
    }

    /// The tools this run may be offered, which is AGT-03's `min_trust` filter.
    pub fn offered(&self) -> Vec<&'static ToolSpec> {
        liyasa_ai::prompt::tools_for(crate::tools::specs(), self.restrictions.trust())
    }

    /// Decides one call and records it, whichever way it goes.
    pub fn authorise(
        &self,
        record: &mut RunRecord,
        name: &str,
        input: &Value,
    ) -> Result<&'static ToolSpec, Rejection> {
        let outcome = self.decide(record, name, input);
        record.record_call(
            name,
            input,
            match &outcome {
                Ok(_) => CallOutcome::Ok,
                Err(rejection) => CallOutcome::Rejected {
                    reason: rejection.to_string(),
                },
            },
        );
        outcome
    }

    fn decide(
        &self,
        record: &RunRecord,
        name: &str,
        input: &Value,
    ) -> Result<&'static ToolSpec, Rejection> {
        let Some(spec) = crate::tools::spec(name) else {
            return Err(Rejection::Unknown {
                name: name.to_owned(),
            });
        };
        let trust = self.restrictions.trust();
        if trust > spec.min_trust {
            return Err(Rejection::BelowTrust {
                tool: name.to_owned(),
                min_trust: spec.min_trust,
                trust,
            });
        }
        validate(spec, input)?;
        // AGT-04's three budgets. Wall time is not one of them here: the record
        // has no clock, so `Run` holds the deadline and checks it before it gets
        // this far. See `Run::authorise`.
        let calls = self.config.budget.max_tool_calls;
        if record.call_count() >= calls {
            return Err(Rejection::BudgetExhausted {
                tool: name.to_owned(),
                budget: "tool-call",
                limit: u64::from(calls),
            });
        }
        let tokens = self.config.budget.max_tokens;
        if record.tokens_spent() >= tokens {
            return Err(Rejection::BudgetExhausted {
                tool: name.to_owned(),
                budget: "token",
                limit: u64::from(tokens),
            });
        }
        self.per_tool(spec, input)?;
        Ok(spec)
    }

    /// The checks that belong to one tool rather than to all of them.
    fn per_tool(&self, spec: &ToolSpec, input: &Value) -> Result<(), Rejection> {
        let tool = spec.name.as_str();
        let string = |key: &str| input.get(key).and_then(Value::as_str).unwrap_or_default();
        match tool {
            crate::tools::WRITE_PAGE => self.writable(tool, string("route")),
            crate::tools::MOVE_PAGE => {
                // Both ends. A run that may write `/a` and not `/b` must not move
                // `/b` onto `/a`, and must not move `/a` out to `/b` either.
                self.writable(tool, string("from"))?;
                self.writable(tool, string("to"))
            }
            crate::tools::EDIT_NAVIGATION => {
                self.permits(tool, &Target::Config(crate::scope::ConfigArea::Navigation))
            }
            crate::tools::PROPOSE_FACT_UPDATE => self.permits(tool, &Target::Facts),
            crate::tools::READ_REPO_FILE => {
                let repo = string("repo");
                let path = string("path");
                crate::repos::permits(&self.config.context_repos, repo, path).map_err(|denial| {
                    Rejection::Repo {
                        tool: tool.to_owned(),
                        repo: repo.to_owned(),
                        path: path.to_owned(),
                        denial: Box::new(denial),
                    }
                })
            }
            crate::tools::SEARCH_REPO => {
                // No path in the input, so the check is at the repository. Each
                // hit's path is filtered by `repos::permits` where the results
                // come back, which is the layer that knows them.
                let repo = string("repo");
                crate::repos::permits_repo(&self.config.context_repos, repo).map_err(|denial| {
                    Rejection::Repo {
                        tool: tool.to_owned(),
                        repo: repo.to_owned(),
                        path: "anything".to_owned(),
                        denial: Box::new(denial),
                    }
                })
            }
            crate::tools::WEB_FETCH => {
                let url = string("url");
                let host = url::Url::parse(url)
                    .ok()
                    .and_then(|u| u.host_str().map(str::to_ascii_lowercase))
                    .unwrap_or_else(|| url.to_owned());
                if self.config.fetch_hosts.contains(&host) {
                    Ok(())
                } else {
                    Err(Rejection::HostNotAllowed {
                        tool: tool.to_owned(),
                        host,
                    })
                }
            }
            _ => Ok(()),
        }
    }

    /// The route, as a page this run may write.
    ///
    /// Normalised first. The schema only asks for a leading slash, so
    /// `/guides/../secret` reaches here, and `Target::Page` of an unnormalised
    /// route would be compared against the scope as a string.
    fn writable(&self, tool: &str, route: &str) -> Result<(), Rejection> {
        let target = match crate::scope::normalise_route(route) {
            Some(route) => Target::Page(route),
            None => Target::Unknown {
                path: route.to_owned(),
            },
        };
        self.permits(tool, &target)
    }

    fn permits(&self, tool: &str, target: &Target) -> Result<(), Rejection> {
        self.restrictions
            .permits(target)
            .map_err(|denial| Rejection::OutOfScope {
                tool: tool.to_owned(),
                denial,
            })
    }
}

/// Validates one input against its tool's schema.
///
/// Compiled per call rather than cached: sixteen small schemas, and a cache keyed
/// on a tool name would have to be invalidated when a schema changes, which is
/// never, so the cache would be correct and the reasoning about it would not be
/// worth the lines. If this ever shows up in a profile, `OnceLock<Vec<Validator>>`
/// beside `tools::specs` is the shape.
pub fn validate(spec: &ToolSpec, input: &Value) -> Result<(), Rejection> {
    let validator = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .build(&spec.input_schema)
        .map_err(|e| Rejection::InvalidInput {
            tool: spec.name.clone(),
            message: format!("the tool's own schema does not compile: {e}"),
        })?;
    let messages: BTreeSet<String> = validator
        .iter_errors(input)
        .map(|error| {
            let at = error.instance_path().to_string();
            if at.is_empty() {
                error.to_string()
            } else {
                format!("{at}: {error}")
            }
        })
        .collect();
    if messages.is_empty() {
        return Ok(());
    }
    Err(Rejection::InvalidInput {
        tool: spec.name.clone(),
        message: messages.into_iter().collect::<Vec<_>>().join("; "),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ContextRepo;
    use crate::policy::Policy;
    use crate::record::{Entry, GraphAccess, Header, RunId, Task};
    use crate::trust::{Trigger, TriggerKind, WriteScope};
    use liyasa_core::ids::Route;
    use serde_json::json;

    fn layout() -> &'static Layout {
        static LAYOUT: std::sync::OnceLock<Layout> = std::sync::OnceLock::new();
        LAYOUT.get_or_init(Layout::default)
    }

    fn config() -> AgentConfig {
        AgentConfig {
            context_repos: vec![ContextRepo {
                name: "acme/api".to_owned(),
                allow: vec!["src".to_owned()],
                deny: Vec::new(),
            }],
            fetch_hosts: crate::hosts::KnownHosts::new(["docs.example.com"]),
            ..AgentConfig::default()
        }
    }

    fn restrictions(trust: TrustLevel, pages: &[&str]) -> Restrictions {
        let kind = if crate::trust::is_untrusted(trust) {
            TriggerKind::Feedback
        } else {
            TriggerKind::Prompt
        };
        Restrictions::for_run(
            trust,
            &Trigger::new(kind, trust).about(pages.iter().map(|p| Route::new(*p))),
        )
    }

    fn record(trust: TrustLevel) -> RunRecord {
        RunRecord::open(
            RunId::new("run-1"),
            Header {
                task: Task {
                    trigger: TriggerKind::Prompt,
                    text: "do the thing".to_owned(),
                    pages: Default::default(),
                },
                trust,
                scope: WriteScope::Anywhere,
                inputs: Vec::new(),
                content_tree: None,
                graph: GraphAccess::Read,
                context_repos: Vec::new(),
                policy: Policy::Proposal,
                retention_days: 90,
            },
        )
    }

    fn rejected_reason(record: &RunRecord) -> Option<String> {
        record.entries().iter().rev().find_map(|entry| match entry {
            Entry::ToolCall {
                outcome: CallOutcome::Rejected { reason },
                ..
            } => Some(reason.clone()),
            _ => None,
        })
    }

    #[test]
    fn a_valid_call_is_authorised_and_recorded() {
        let config = config();
        let restrictions = restrictions(TrustLevel::Member, &[]);
        let gate = ToolGate::new(&restrictions, &config, layout());
        let mut record = record(TrustLevel::Member);
        let spec = gate
            .authorise(&mut record, "read_page", &json!({ "route": "/pricing" }))
            .expect("a member may read a page");
        assert_eq!(spec.name, "read_page");
        assert_eq!(record.call_count(), 1);
        assert_eq!(rejected_reason(&record), None);
    }

    #[test]
    fn an_unknown_tool_is_rejected_and_recorded() {
        let config = config();
        let restrictions = restrictions(TrustLevel::Operator, &[]);
        let gate = ToolGate::new(&restrictions, &config, layout());
        let mut record = record(TrustLevel::Operator);
        let rejection = gate
            .authorise(&mut record, "rm_rf", &json!({}))
            .expect_err("no such tool");
        assert_eq!(
            rejection,
            Rejection::Unknown {
                name: "rm_rf".to_owned()
            }
        );
        assert_eq!(rejection.diagnostic().code, code::E0903);
        assert!(
            rejected_reason(&record).is_some(),
            "the call was not audited"
        );
    }

    #[test]
    fn a_call_below_min_trust_is_rejected_with_e0903_and_audited() {
        // AGT-03's acceptance row, half one.
        let config = config();
        let restrictions = restrictions(TrustLevel::Anonymous, &["/guides/install"]);
        let gate = ToolGate::new(&restrictions, &config, layout());
        let mut record = record(TrustLevel::Anonymous);
        let rejection = gate
            .authorise(
                &mut record,
                "edit_navigation",
                &json!({ "operation": "remove", "route": "/guides/install" }),
            )
            .expect_err("below min_trust");
        assert!(
            matches!(rejection, Rejection::BelowTrust { .. }),
            "{rejection:?}"
        );
        assert_eq!(rejection.diagnostic().code, code::E0903);
        let reason = rejected_reason(&record).expect("the call was not audited");
        assert!(reason.contains("edit_navigation"), "{reason}");
    }

    #[test]
    fn each_shape_of_invalid_input_is_refused() {
        let config = config();
        let restrictions = restrictions(TrustLevel::Member, &[]);
        let gate = ToolGate::new(&restrictions, &config, layout());
        let mut record = record(TrustLevel::Member);
        for bad in [
            json!({}),
            json!({ "route": "pricing" }),
            json!({ "route": "/pricing", "extra": 1 }),
            json!({ "route": 7 }),
        ] {
            let rejection = gate
                .authorise(&mut record, "read_page", &bad)
                .expect_err(&format!("{bad} passed the schema"));
            assert!(
                matches!(rejection, Rejection::InvalidInput { .. }),
                "{bad}: {rejection:?}"
            );
            assert_eq!(rejection.diagnostic().code, code::E0903);
        }
        // Four calls, four audit entries. "And audited" is about every one.
        assert_eq!(record.call_count(), 4);
    }

    #[test]
    fn a_write_outside_the_scope_is_rejected() {
        let config = config();
        let restrictions = restrictions(TrustLevel::Anonymous, &["/guides/install"]);
        let gate = ToolGate::new(&restrictions, &config, layout());
        let mut record = record(TrustLevel::Anonymous);
        assert!(
            gate.authorise(
                &mut record,
                "write_page",
                &json!({ "route": "/guides/install", "markdown": "x" })
            )
            .is_ok(),
            "the trigger's own page must be writable"
        );
        let rejection = gate
            .authorise(
                &mut record,
                "write_page",
                &json!({ "route": "/pricing", "markdown": "x" }),
            )
            .expect_err("outside the scope");
        assert!(
            matches!(rejection, Rejection::OutOfScope { .. }),
            "{rejection:?}"
        );
    }

    #[test]
    fn a_move_is_checked_at_both_ends() {
        let config = config();
        let restrictions = restrictions(TrustLevel::Member, &[]);
        let gate = ToolGate::new(&restrictions, &config, layout());
        let mut record = record(TrustLevel::Member);
        // A member may move anything, so the interesting case needs a narrower
        // scope than `min_trust` allows the tool at. Build one directly.
        let narrow = Restrictions::for_run(
            TrustLevel::Member,
            &Trigger::new(TriggerKind::Prompt, TrustLevel::Member),
        );
        let gate_narrow = ToolGate::new(&narrow, &config, layout());
        assert!(
            gate_narrow
                .authorise(
                    &mut record,
                    "move_page",
                    &json!({ "from": "/a", "to": "/b" })
                )
                .is_ok()
        );
        // And an unnormalisable route is refused at either end, at any trust.
        let rejection = gate
            .authorise(
                &mut record,
                "move_page",
                &json!({ "from": "/a", "to": "/../b" }),
            )
            .expect_err("a traversal in the destination");
        assert!(
            matches!(rejection, Rejection::OutOfScope { .. }),
            "{rejection:?}"
        );
    }

    #[test]
    fn a_repo_read_outside_the_allow_list_is_rejected() {
        let config = config();
        let restrictions = restrictions(TrustLevel::Member, &[]);
        let gate = ToolGate::new(&restrictions, &config, layout());
        let mut record = record(TrustLevel::Member);
        assert!(
            gate.authorise(
                &mut record,
                "read_repo_file",
                &json!({ "repo": "acme/api", "path": "src/lib.rs" })
            )
            .is_ok()
        );
        for bad in [
            json!({ "repo": "acme/api", "path": "Makefile" }),
            json!({ "repo": "someone/else", "path": "src/lib.rs" }),
            json!({ "repo": "acme/api", "path": "../outside" }),
        ] {
            let rejection = gate
                .authorise(&mut record, "read_repo_file", &bad)
                .expect_err(&format!("{bad} was permitted"));
            assert!(matches!(rejection, Rejection::Repo { .. }), "{rejection:?}");
        }
    }

    #[test]
    fn web_fetch_is_refused_a_host_that_is_not_allow_listed() {
        let config = config();
        let restrictions = restrictions(TrustLevel::Member, &[]);
        let gate = ToolGate::new(&restrictions, &config, layout());
        let mut record = record(TrustLevel::Member);
        assert!(
            gate.authorise(
                &mut record,
                "web_fetch",
                &json!({ "url": "https://docs.example.com/a" })
            )
            .is_ok()
        );
        let rejection = gate
            .authorise(
                &mut record,
                "web_fetch",
                &json!({ "url": "https://evil.example/a" }),
            )
            .expect_err("not allow-listed");
        assert!(
            matches!(rejection, Rejection::HostNotAllowed { .. }),
            "{rejection:?}"
        );
    }

    #[test]
    fn a_non_http_url_never_reaches_the_allow_list() {
        // The schema's `^https?://` is what stops it, so the rejection is about
        // the input rather than about the host.
        let config = config();
        let restrictions = restrictions(TrustLevel::Operator, &[]);
        let gate = ToolGate::new(&restrictions, &config, layout());
        let mut record = record(TrustLevel::Operator);
        let rejection = gate
            .authorise(
                &mut record,
                "web_fetch",
                &json!({ "url": "file:///etc/passwd" }),
            )
            .expect_err("file:// is not an http url");
        assert!(
            matches!(rejection, Rejection::InvalidInput { .. }),
            "{rejection:?}"
        );
    }

    #[test]
    fn the_tool_call_budget_is_enforced_off_the_records_own_count() {
        // Not off a counter the gate holds: two gates over one record must not
        // each allow a full budget.
        let mut config = config();
        config.budget.max_tool_calls = 2;
        let restrictions = restrictions(TrustLevel::Member, &[]);
        let layout = Layout::default();
        let mut record = record(TrustLevel::Member);
        let input = json!({ "route": "/pricing" });
        for _ in 0..2 {
            let gate = ToolGate::new(&restrictions, &config, &layout);
            assert!(gate.authorise(&mut record, "read_page", &input).is_ok());
        }
        let gate = ToolGate::new(&restrictions, &config, &layout);
        let rejection = gate
            .authorise(&mut record, "read_page", &input)
            .expect_err("the budget is spent");
        assert_eq!(
            rejection,
            Rejection::BudgetExhausted {
                tool: "read_page".to_owned(),
                budget: "tool-call",
                limit: 2
            }
        );
        // The refused call is audited too, so the record shows three attempts.
        assert_eq!(record.call_count(), 3);
    }

    #[test]
    fn an_authorisation_cannot_happen_without_a_record() {
        // A compilation fact, stated where someone relaxing it would look:
        // `authorise` takes `&mut RunRecord`, so there is no call site that can
        // ask the question and drop the answer.
        let config = config();
        let restrictions = restrictions(TrustLevel::Member, &[]);
        let gate = ToolGate::new(&restrictions, &config, layout());
        let mut record = record(TrustLevel::Member);
        let before = record.call_count();
        let _ = gate.authorise(&mut record, "list_drift", &json!({}));
        assert_eq!(record.call_count(), before + 1);
    }

    #[test]
    fn every_offered_tool_can_be_authorised_with_a_valid_input() {
        // The surface a run is offered has to be usable: a tool offered and always
        // refused is worse than a tool not offered.
        let mut config = config();
        config.fetch_hosts = crate::hosts::KnownHosts::new(["docs.example.com"]);
        let restrictions = restrictions(TrustLevel::Operator, &[]);
        let gate = ToolGate::new(&restrictions, &config, layout());
        let mut record = record(TrustLevel::Operator);
        let inputs: Vec<(&str, Value)> = vec![
            ("search_docs", json!({ "query": "seats" })),
            ("read_page", json!({ "route": "/pricing" })),
            (
                "write_page",
                json!({ "route": "/pricing", "markdown": "x" }),
            ),
            ("move_page", json!({ "from": "/a", "to": "/b" })),
            (
                "edit_navigation",
                json!({ "operation": "insert", "route": "/pricing" }),
            ),
            (
                "read_repo_file",
                json!({ "repo": "acme/api", "path": "src/lib.rs" }),
            ),
            (
                "search_repo",
                json!({ "repo": "acme/api", "query": "seats" }),
            ),
            ("get_openapi", json!({ "operation": "GET /seats" })),
            ("get_fact", json!({ "fact": "plan.seats" })),
            (
                "propose_fact_update",
                json!({ "fact": "plan.seats", "value": 10, "evidence": "the API says so" }),
            ),
            ("run_validate", json!({})),
            ("run_verify", json!({ "changed_only": true })),
            ("render_preview", json!({ "route": "/pricing" })),
            ("list_drift", json!({ "open": true })),
            ("web_fetch", json!({ "url": "https://docs.example.com/a" })),
            ("ask_reviewer", json!({ "question": "which plan?" })),
        ];
        assert_eq!(inputs.len(), crate::tools::NAMES.len());
        for (name, input) in inputs {
            gate.authorise(&mut record, name, &input)
                .unwrap_or_else(|e| panic!("`{name}` refused a valid call: {e}"));
        }
    }

    #[test]
    fn search_repo_still_needs_the_repo_to_be_configured() {
        // It has no path, so the allow list is checked at the repository. An
        // unconfigured repository is still refused.
        let config = config();
        let restrictions = restrictions(TrustLevel::Member, &[]);
        let gate = ToolGate::new(&restrictions, &config, layout());
        let mut record = record(TrustLevel::Member);
        let rejection = gate
            .authorise(
                &mut record,
                "search_repo",
                &json!({ "repo": "someone/else", "query": "x" }),
            )
            .expect_err("not configured");
        assert!(matches!(rejection, Rejection::Repo { .. }), "{rejection:?}");
    }

    #[test]
    fn the_offered_surface_narrows_with_trust() {
        let config = config();
        for (trust, expected) in [
            (TrustLevel::Operator, 16),
            (TrustLevel::Member, 16),
            (TrustLevel::Anonymous, 10),
            (TrustLevel::External, 10),
        ] {
            let restrictions = restrictions(trust, &["/a"]);
            let gate = ToolGate::new(&restrictions, &config, layout());
            assert_eq!(gate.offered().len(), expected, "{trust:?}");
        }
    }
}
