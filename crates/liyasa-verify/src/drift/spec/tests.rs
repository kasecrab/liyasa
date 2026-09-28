use std::collections::BTreeMap;

use liyasa_core::document::{DepTarget, Edge, EdgeKind, EdgeOrigin};
use liyasa_core::ids::{BlockId, PageId, Route};
use liyasa_core::verify::{CheckOutcome, StoreError};

use crate::core::Scrubber;
use crate::drift::engine::Routes;
use crate::drift::record::DriftKey;
use crate::sources::{OperationChange, OperationImpact};

use super::{candidates, checks};

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
    Known(BTreeMap::from([(page(1), Route::new("/api/pets"))]))
}

fn impact(diff: &[&str]) -> OperationImpact {
    let change = OperationChange {
        spec: "petstore".to_owned(),
        op: "listPets".to_owned(),
        diff: diff.iter().copied().map(ToOwned::to_owned).collect(),
    };
    let origin = EdgeOrigin::Block(page(1), BlockId([1; 12]));
    OperationImpact {
        blocks: vec![(
            origin.clone(),
            vec![Edge {
                from: origin,
                to: DepTarget::Operation {
                    spec: change.spec.clone(),
                    op: change.op.clone(),
                },
                kind: EdgeKind::Documents,
            }],
        )],
        change,
    }
}

#[test]
fn an_operation_that_moved_is_one_candidate_naming_the_spec_and_the_operation() {
    let found = candidates(&[impact(&["responses", "auth"])], &known()).expect("the pairing");
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].key(),
        DriftKey::Operation {
            spec: "petstore".to_owned(),
            op: "listPets".to_owned(),
        }
    );
    assert_eq!(found[0].pages, vec![Route::new("/api/pets")]);
    assert_eq!(found[0].blocks.len(), 1);
}

#[test]
fn the_finding_carries_the_diff_because_that_is_what_ver_12_flags_a_page_with() {
    let results = checks(
        &[impact(&["responses", "auth"])],
        &known(),
        &Scrubber::new(),
    )
    .expect("the pairing");
    assert_eq!(results.len(), 1);
    match &results[0].outcome {
        CheckOutcome::Fail { excerpt } => {
            assert!(excerpt.contains("listPets"), "{excerpt}");
            assert!(excerpt.contains("petstore"), "{excerpt}");
            assert!(excerpt.contains("responses"), "{excerpt}");
            assert!(excerpt.contains("auth"), "{excerpt}");
        }
        other => panic!("{other:?}"),
    }
    assert!(results[0].id.as_str().starts_with("/api/pets#"));
}

#[test]
fn a_different_diff_for_the_same_operation_is_a_different_result_on_the_same_check() {
    let one = checks(&[impact(&["responses"])], &known(), &Scrubber::new()).expect("the pairing");
    let two = checks(&[impact(&["auth"])], &known(), &Scrubber::new()).expect("the pairing");
    assert_eq!(one[0].id, two[0].id);
    assert_ne!(one[0].digest, two[0].digest);

    // And the same diff twice is the same result, so a sweep that changes
    // nothing reports nothing new.
    let again = checks(&[impact(&["responses"])], &known(), &Scrubber::new()).expect("the pairing");
    assert_eq!(one[0].digest, again[0].digest);
}

#[test]
fn a_page_the_pairing_does_not_know_is_skipped_and_the_candidate_still_has_none() {
    let mut orphan = impact(&["responses"]);
    orphan.blocks = vec![(EdgeOrigin::Block(page(9), BlockId([9; 12])), Vec::new())];
    assert!(
        checks(&[orphan.clone()], &known(), &Scrubber::new())
            .expect("the pairing")
            .is_empty()
    );
    let found = candidates(&[orphan], &known()).expect("the pairing");
    assert!(
        found[0].pages.is_empty(),
        "and the engine drops it rather than this module"
    );
}
