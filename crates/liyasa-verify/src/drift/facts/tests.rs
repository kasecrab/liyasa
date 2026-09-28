use std::collections::BTreeMap;

use liyasa_core::document::{DepTarget, Edge, EdgeKind, EdgeOrigin};
use liyasa_core::ids::{BlockId, FactId, PageId, Route};
use liyasa_core::verify::{ChangeKind, CheckOutcome, FactChange, FactValue, Impact, StoreError};

use crate::core::Scrubber;
use crate::drift::engine::Routes;
use crate::drift::record::{DriftKey, DriftKind};

use super::{candidates, checks, show};

/// The pairing a graph would answer with, stated instead of walked.
struct Known(BTreeMap<PageId, Route>);

impl Routes for Known {
    fn routes(&self, blocks: &[(EdgeOrigin, Vec<Edge>)]) -> Result<Vec<Route>, StoreError> {
        let mut out: Vec<Route> = blocks
            .iter()
            .filter_map(|(origin, _)| {
                let (EdgeOrigin::Block(page, _) | EdgeOrigin::Page(page)) = origin;
                self.0.get(page).cloned()
            })
            .collect();
        out.sort();
        out.dedup();
        Ok(out)
    }
}

fn page(byte: u8) -> PageId {
    PageId(ulid::Ulid::from_bytes([byte; 16]))
}

fn known() -> Known {
    Known(BTreeMap::from([
        (page(1), Route::new("/pricing")),
        (page(2), Route::new("/plans")),
    ]))
}

fn edge(fact: &str) -> Edge {
    Edge {
        from: EdgeOrigin::Block(page(1), BlockId([1; 12])),
        to: DepTarget::Fact(FactId::new(fact)),
        kind: EdgeKind::Reads,
    }
}

fn impact(fact: &str, origins: &[(u8, u8)]) -> Impact {
    Impact {
        change: FactChange {
            fact: FactId::new(fact),
            old: Some(FactValue::Num(20.0)),
            new: Some(FactValue::Num(25.0)),
            kind: ChangeKind::Changed,
        },
        blocks: origins
            .iter()
            .map(|(p, b)| {
                (
                    EdgeOrigin::Block(page(*p), BlockId([*b; 12])),
                    vec![edge(fact)],
                )
            })
            .collect(),
    }
}

#[test]
fn a_candidate_carries_the_pages_the_change_reached_and_the_evidence() {
    let impacts = [impact("plan.pro.price", &[(1, 1), (2, 2)])];
    let found = candidates(&impacts, &known()).expect("the pairing answers");
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].key(),
        DriftKey::Fact(FactId::new("plan.pro.price"))
    );
    assert_eq!(
        found[0].pages,
        vec![Route::new("/plans"), Route::new("/pricing")]
    );
    assert_eq!(found[0].blocks.len(), 2, "the evidence travels with it");
    assert!(matches!(
        &found[0].kind,
        DriftKind::Fact { old: Some(FactValue::Num(old)), new: Some(FactValue::Num(new)), .. }
            if *old == 20.0 && *new == 25.0
    ));
}

#[test]
fn the_facts_class_reports_one_finding_per_block_and_names_the_two_values() {
    let impacts = [impact("plan.pro.price", &[(1, 1), (2, 2)])];
    let results = checks(&impacts, &known(), &Scrubber::new()).expect("the pairing answers");

    assert_eq!(results.len(), 2);
    for result in &results {
        match &result.outcome {
            CheckOutcome::Fail { excerpt } => {
                assert!(excerpt.contains("plan.pro.price"), "{excerpt}");
                assert!(excerpt.contains("20"), "{excerpt}");
                assert!(excerpt.contains("25"), "{excerpt}");
            }
            other => panic!("a reached block is stale, not {other:?}"),
        }
    }

    // The id is the route of the block that reads it, not the impact's set.
    let ids: Vec<&str> = results.iter().map(|r| r.id.as_str()).collect();
    assert!(ids.iter().any(|id| id.starts_with("/pricing#")), "{ids:?}");
    assert!(ids.iter().any(|id| id.starts_with("/plans#")), "{ids:?}");
}

#[test]
fn a_block_whose_page_the_pairing_does_not_know_is_skipped_rather_than_guessed() {
    let impacts = [impact("plan.pro.price", &[(1, 1), (9, 9)])];
    let results = checks(&impacts, &known(), &Scrubber::new()).expect("the pairing answers");
    assert_eq!(results.len(), 1);
    assert!(results[0].id.as_str().starts_with("/pricing#"));
}

#[test]
fn the_digest_follows_the_value_so_the_same_staleness_is_the_same_result() {
    let first = checks(
        &[impact("plan.pro.price", &[(1, 1)])],
        &known(),
        &Scrubber::new(),
    )
    .expect("the pairing answers");
    let again = checks(
        &[impact("plan.pro.price", &[(1, 1)])],
        &known(),
        &Scrubber::new(),
    )
    .expect("the pairing answers");
    assert_eq!(first[0].digest, again[0].digest);

    let mut moved = impact("plan.pro.price", &[(1, 1)]);
    moved.change.new = Some(FactValue::Num(30.0));
    let moved = checks(&[moved], &known(), &Scrubber::new()).expect("the pairing answers");
    assert_ne!(first[0].digest, moved[0].digest);
    assert_eq!(first[0].id, moved[0].id, "and it is the same check");
}

#[test]
fn a_secret_that_reached_a_fact_value_does_not_reach_the_excerpt() {
    const TOKEN: &str = "sk-live-4f8a2c9e1b7d3a6f";
    let mut leaky = impact("deploy.token", &[(1, 1)]);
    leaky.change.new = Some(FactValue::Str(TOKEN.to_owned()));
    let results =
        checks(&[leaky], &known(), &Scrubber::with_secrets([TOKEN])).expect("the pairing answers");
    match &results[0].outcome {
        CheckOutcome::Fail { excerpt } => {
            assert!(!excerpt.contains(TOKEN), "{excerpt}");
            assert!(excerpt.contains("deploy.token"), "{excerpt}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_value_is_shown_the_way_a_reader_of_the_finding_needs_it() {
    assert_eq!(show(None), "absent");
    assert_eq!(show(Some(&FactValue::Num(25.0))), "25");
    assert_eq!(show(Some(&FactValue::Percent(99.9))), "99.9%");
    assert_eq!(show(Some(&FactValue::Bool(true))), "true");
    assert_eq!(show(Some(&FactValue::Str("pro".to_owned()))), "pro");
    assert_eq!(
        show(Some(&FactValue::Currency {
            amount: 2500,
            minor: 2,
            code: "USD".to_owned()
        })),
        "25.00 USD"
    );
    // A zero-minor currency has no point to place.
    assert_eq!(
        show(Some(&FactValue::Currency {
            amount: 2500,
            minor: 0,
            code: "JPY".to_owned()
        })),
        "2500 JPY"
    );
    assert_eq!(
        show(Some(&FactValue::List(vec![
            FactValue::Num(1.0),
            FactValue::Str("two".to_owned())
        ]))),
        "[1, two]"
    );
    // An object is JSON rather than nothing.
    let object = FactValue::Object(std::collections::BTreeMap::from([(
        "seats".to_owned(),
        FactValue::Num(5.0),
    )]));
    assert!(show(Some(&object)).contains("seats"));
}
