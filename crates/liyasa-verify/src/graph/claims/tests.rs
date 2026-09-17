use std::time::Duration;

use super::*;

fn at(seconds: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
}

fn candidate(page: &str, block: &str, fact: &str, confidence: f32) -> ClaimCandidate {
    ClaimCandidate {
        block: BlockId::explicit(block),
        page: Route::new(page),
        fact: FactId::new(fact),
        confidence,
        excerpt: "Pro costs $20 a month.".to_owned(),
    }
}

fn key(page: &str, block: &str, fact: &str) -> ClaimKey {
    ClaimKey {
        page: Route::new(page),
        block: BlockId::explicit(block),
        fact: FactId::new(fact),
    }
}

#[test]
fn a_scan_proposes_and_a_reviewer_decides() {
    let claims = MemoryClaims::new();
    let found = candidate("/pricing", "intro", "plan.pro.price", 0.8);
    claims.observe(&found, at(1)).expect("observe");

    let key = ClaimKey::of(&found);
    let row = claims.get(&key).expect("get").expect("the claim");
    assert_eq!(row.status, ClaimStatus::Candidate);
    assert_eq!(row.decided_by, None);

    claims.confirm(&key, "ana", at(2)).expect("confirm");

    let row = claims.get(&key).expect("get").expect("the claim");
    assert_eq!(row.status, ClaimStatus::Confirmed);
    assert_eq!(row.decided_by.as_deref(), Some("ana"));
    assert_eq!(row.decided_at, Some(at(2)));
}

#[test]
fn a_later_scan_refreshes_the_candidate_and_leaves_the_decision_alone() {
    let claims = MemoryClaims::new();
    let found = candidate("/pricing", "intro", "plan.pro.price", 0.6);
    claims.observe(&found, at(1)).expect("observe");
    let key = ClaimKey::of(&found);
    claims.confirm(&key, "ana", at(2)).expect("confirm");

    let again = ClaimCandidate {
        confidence: 0.95,
        excerpt: "Pro costs $25 a month.".to_owned(),
        ..found
    };
    claims.observe(&again, at(3)).expect("observe");

    let row = claims.get(&key).expect("get").expect("the claim");
    assert_eq!(
        row.status,
        ClaimStatus::Confirmed,
        "the reviewer outranks the scanner"
    );
    assert_eq!(row.decided_by.as_deref(), Some("ana"));
    assert_eq!(row.confidence, 0.95);
    assert_eq!(row.excerpt, "Pro costs $25 a month.");
    assert_eq!(row.first_seen, at(1));
    assert_eq!(row.last_seen, at(3));
}

#[test]
fn a_rejected_claim_stays_rejected_when_the_scanner_finds_it_again() {
    let claims = MemoryClaims::new();
    let found = candidate("/pricing", "intro", "plan.pro.price", 0.6);
    claims.observe(&found, at(1)).expect("observe");
    let key = ClaimKey::of(&found);
    claims.reject(&key, "ana", at(2)).expect("reject");
    claims.observe(&found, at(3)).expect("observe");

    assert_eq!(
        claims.get(&key).expect("get").expect("the claim").status,
        ClaimStatus::Rejected
    );
}

#[test]
fn deciding_a_claim_the_table_does_not_hold_is_not_a_silent_insert() {
    let claims = MemoryClaims::new();

    assert_eq!(
        claims.confirm(&key("/pricing", "intro", "plan.pro.price"), "ana", at(1)),
        Err(StoreError::NotFound)
    );
    assert!(claims.rows().expect("rows").is_empty());
}

#[test]
fn a_fact_names_every_claim_on_it_in_page_order() {
    let claims = MemoryClaims::new();
    for page in ["/pricing", "/plans", "/index"] {
        claims
            .observe(&candidate(page, "intro", "plan.pro.price", 0.7), at(1))
            .expect("observe");
    }
    claims
        .observe(
            &candidate("/pricing", "intro", "plan.free.price", 0.7),
            at(1),
        )
        .expect("observe");

    let pages: Vec<Route> = claims
        .for_fact(&FactId::new("plan.pro.price"))
        .expect("for_fact")
        .into_iter()
        .map(|row| row.key.page)
        .collect();

    assert_eq!(
        pages,
        vec![
            Route::new("/index"),
            Route::new("/plans"),
            Route::new("/pricing")
        ]
    );
}

#[test]
fn a_page_names_every_claim_on_it() {
    let claims = MemoryClaims::new();
    claims
        .observe(
            &candidate("/pricing", "intro", "plan.pro.price", 0.7),
            at(1),
        )
        .expect("observe");
    claims
        .observe(&candidate("/plans", "intro", "plan.pro.price", 0.7), at(1))
        .expect("observe");

    let rows = claims.for_page(&Route::new("/pricing")).expect("for_page");

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].key.page, Route::new("/pricing"));
}

#[test]
fn only_confirmed_pages_are_what_the_registry_learned() {
    let claims = MemoryClaims::new();
    let fact = FactId::new("plan.pro.price");
    for page in ["/pricing", "/plans", "/index"] {
        claims
            .observe(&candidate(page, "intro", fact.as_str(), 0.7), at(1))
            .expect("observe");
    }
    claims
        .confirm(&key("/pricing", "intro", fact.as_str()), "ana", at(2))
        .expect("confirm");
    claims
        .reject(&key("/plans", "intro", fact.as_str()), "ana", at(2))
        .expect("reject");

    assert_eq!(
        claims.confirmed_pages(&fact).expect("confirmed"),
        vec![Route::new("/pricing")]
    );
}

#[test]
fn an_excerpt_is_capped_where_every_other_excerpt_is() {
    let claims = MemoryClaims::new();
    let long = ClaimCandidate {
        excerpt: "x".repeat(crate::core::EXCERPT_LIMIT * 2),
        ..candidate("/pricing", "intro", "plan.pro.price", 0.7)
    };
    claims.observe(&long, at(1)).expect("observe");

    let row = claims
        .get(&ClaimKey::of(&long))
        .expect("get")
        .expect("the claim");
    assert!(row.excerpt.len() <= crate::core::EXCERPT_LIMIT);
}

#[test]
fn two_blocks_on_one_page_stating_one_fact_are_two_claims() {
    let claims = MemoryClaims::new();
    claims
        .observe(
            &candidate("/pricing", "intro", "plan.pro.price", 0.7),
            at(1),
        )
        .expect("observe");
    claims
        .observe(
            &candidate("/pricing", "table", "plan.pro.price", 0.7),
            at(1),
        )
        .expect("observe");

    assert_eq!(claims.rows().expect("rows").len(), 2);
}
