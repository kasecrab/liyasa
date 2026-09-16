//! Projects, builds, deployments, domains, feedback, and webhooks.

use liyasa_core::ids::{BuildId, Fingerprint};
use liyasa_core::store::{BuildStatus, Page, StoreError};
use liyasa_store::records::{
    BuildRecord, DeliveryStatus, FeedbackKind, FeedbackRecord, FeedbackStatus,
};
use liyasa_store::repos::{
    Builds, Deployments, DomainRecord, Domains, Feedback, FeedbackQuery, Projects, Webhooks,
};

use crate::support::app_db;

fn build_id(seed: &str) -> BuildId {
    BuildId(Fingerprint::of(seed))
}

#[tokio::test]
async fn a_projects_slug_is_unique_and_a_stale_write_is_a_conflict() {
    let (_dir, pool) = app_db("repos-project").await;
    let projects = Projects::new(pool);
    let project = projects
        .create("acme-docs", "Acme")
        .await
        .expect("a project");
    assert!(matches!(
        projects.create("acme-docs", "Other").await,
        Err(StoreError::Conflict)
    ));
    assert_eq!(
        projects
            .by_slug("acme-docs")
            .await
            .expect("a read")
            .expect("the project")
            .id,
        project.id
    );

    let renamed = projects
        .put(&liyasa_store::records::ProjectRecord {
            name: "Acme Docs".to_owned(),
            ..project.clone()
        })
        .await
        .expect("a write");
    assert_eq!(renamed.version, project.version + 1);
    assert!(
        matches!(projects.put(&project).await, Err(StoreError::Conflict)),
        "the second writer saw an older version"
    );

    assert_eq!(
        projects
            .list(None, &Page::default())
            .await
            .expect("a listing")
            .len(),
        1
    );
    projects.delete(&project.id).await.expect("a delete");
    assert!(projects.get(&project.id).await.expect("a read").is_none());
}

#[tokio::test]
async fn an_environment_points_at_a_build_and_rolls_back_to_the_previous_one() {
    let (_dir, pool) = app_db("repos-deploy").await;
    let projects = Projects::new(pool.clone());
    let builds = Builds::new(pool.clone());
    let deployments = Deployments::new(pool);
    let project = projects
        .create("acme-docs", "Acme")
        .await
        .expect("a project");

    let mut ids = Vec::new();
    for seed in ["one", "two", "three"] {
        let id = build_id(seed);
        builds
            .put(&BuildRecord {
                id,
                project: project.id,
                env: "production".to_owned(),
                status: BuildStatus::Succeeded,
                dist: format!("bundles/{seed}"),
                created_at: liyasa_store::now_ms() + ids.len() as i64,
                updated_at: 0,
                version: 1,
            })
            .await
            .expect("a build");
        ids.push(id);
    }

    assert_eq!(
        builds
            .latest_for(&project.id, "production")
            .await
            .expect("a read")
            .expect("a build")
            .id,
        ids[2]
    );
    assert!(
        builds
            .latest_for(&project.id, "preview")
            .await
            .expect("a read")
            .is_none()
    );

    for id in &ids {
        deployments
            .point(&project.id, "production", id)
            .await
            .expect("a pointer swap");
    }
    assert_eq!(
        deployments
            .current(&project.id, "production")
            .await
            .expect("a read")
            .expect("a pointer")
            .build,
        ids[2]
    );
    assert_eq!(
        deployments
            .previous(&project.id, "production")
            .await
            .expect("a read")
            .expect("a rollback target"),
        ids[1]
    );
    assert_eq!(
        deployments
            .history(&project.id, "production", &Page::default())
            .await
            .expect("a history")
            .len(),
        3
    );

    let listed = builds
        .list(
            Some(&project.id),
            Some("production"),
            Some(BuildStatus::Succeeded),
            &Page::default(),
        )
        .await
        .expect("a listing");
    assert_eq!(listed.len(), 3);
}

#[tokio::test]
async fn a_renamed_subdomain_keeps_redirecting_until_the_window_closes() {
    let (_dir, pool) = app_db("repos-domain").await;
    let projects = Projects::new(pool.clone());
    let domains = Domains::new(pool);
    let project = projects
        .create("acme-docs", "Acme")
        .await
        .expect("a project");
    domains
        .put(&DomainRecord {
            host: "old.liyasa.site".to_owned(),
            project: project.id,
            base_path: "/docs".to_owned(),
            env: "production".to_owned(),
        })
        .await
        .expect("a domain");

    let thirty_days = liyasa_store::now_ms() + 30 * 86_400_000;
    domains
        .rename("old.liyasa.site", "new.liyasa.site", thirty_days)
        .await
        .expect("a rename");

    assert!(
        domains
            .get("old.liyasa.site")
            .await
            .expect("a read")
            .is_none()
    );
    let moved = domains
        .get("NEW.liyasa.site")
        .await
        .expect("a read")
        .expect("the domain, matched case-insensitively");
    assert_eq!(moved.base_path, "/docs");
    assert_eq!(
        domains
            .redirect_for("old.liyasa.site")
            .await
            .expect("a read")
            .as_deref(),
        Some("new.liyasa.site")
    );

    // A redirect whose window has closed is gone.
    domains
        .rename(
            "new.liyasa.site",
            "newer.liyasa.site",
            liyasa_store::now_ms() - 1,
        )
        .await
        .expect("a rename");
    assert!(
        domains
            .redirect_for("new.liyasa.site")
            .await
            .expect("a read")
            .is_none()
    );
    assert!(domains.rename("absent.liyasa.site", "x", 0).await.is_err());
}

fn feedback(route: &str, kind: FeedbackKind, rating: Option<i32>) -> FeedbackRecord {
    FeedbackRecord {
        id: format!("fb_{}", liyasa_store::new_ulid()),
        project: None,
        route: route.to_owned(),
        kind,
        rating,
        category: None,
        text: None,
        block_id: None,
        task: None,
        status: FeedbackStatus::Open,
        notes: String::new(),
        created_at: liyasa_store::now_ms(),
        updated_at: liyasa_store::now_ms(),
    }
}

#[tokio::test]
async fn feedback_carries_a_status_workflow_and_a_per_page_ratio() {
    let (_dir, pool) = app_db("repos-feedback").await;
    let store = Feedback::new(pool);
    for rating in [Some(1), Some(1), Some(-1)] {
        store
            .insert(&feedback("/install", FeedbackKind::Page, rating))
            .await
            .expect("a rating");
    }
    let agent = feedback("/install", FeedbackKind::Agent, None);
    store.insert(&agent).await.expect("an agent report");

    assert_eq!(store.ratio("/install").await.expect("a ratio"), (2, 1));

    store
        .set_status(&agent.id, FeedbackStatus::Triaged, Some("assigned"))
        .await
        .expect("a triage");
    let triaged = store
        .get(&agent.id)
        .await
        .expect("a read")
        .expect("the row");
    assert_eq!(triaged.status, FeedbackStatus::Triaged);
    assert!(triaged.notes.contains("assigned"));

    let open = store
        .list(
            &FeedbackQuery {
                status: Some(FeedbackStatus::Open),
                ..FeedbackQuery::default()
            },
            &Page::default(),
        )
        .await
        .expect("a listing");
    assert_eq!(open.len(), 3);
    let agents = store
        .list(
            &FeedbackQuery {
                kind: Some(FeedbackKind::Agent),
                ..FeedbackQuery::default()
            },
            &Page::default(),
        )
        .await
        .expect("a listing");
    assert_eq!(agents.len(), 1, "agent feedback is a separate stream");
}

#[tokio::test]
async fn a_delivery_is_queued_per_interested_subscription_and_pauses_after_twenty_failures() {
    let (_dir, pool) = app_db("repos-webhooks").await;
    let hooks = Webhooks::new(pool);
    let all = hooks
        .subscribe(None, "https://example.com/all", "s1", &[])
        .await
        .expect("a subscription");
    let deploys = hooks
        .subscribe(
            None,
            "https://example.com/deploys",
            "s2",
            &["deployment.succeeded".to_owned()],
        )
        .await
        .expect("a subscription");

    let queued = hooks
        .queue("evt_1", "drift.created", "{}")
        .await
        .expect("a fan-out");
    assert_eq!(queued.len(), 1, "only the catch-all wants a drift event");
    assert_eq!(queued[0].subscription, all.id);

    let queued = hooks
        .queue("evt_2", "deployment.succeeded", "{}")
        .await
        .expect("a fan-out");
    assert_eq!(queued.len(), 2);
    assert_eq!(hooks.due(10).await.expect("the due list").len(), 3);

    let delivery = &queued[0];
    hooks
        .record_attempt(
            &delivery.id,
            &delivery.subscription,
            Some(200),
            DeliveryStatus::Delivered,
            0,
        )
        .await
        .expect("an attempt");
    let delivered = hooks
        .deliveries(&delivery.subscription)
        .await
        .expect("a history")
        .into_iter()
        .find(|d| d.id == delivery.id)
        .expect("the delivery");
    assert_eq!(delivered.status, DeliveryStatus::Delivered);
    assert_eq!(delivered.attempt, 1);

    for _ in 0..20 {
        let one = hooks
            .queue("evt_n", "deployment.succeeded", "{}")
            .await
            .expect("a fan-out")
            .into_iter()
            .find(|d| d.subscription == deploys.id)
            .expect("a delivery");
        hooks
            .record_attempt(
                &one.id,
                &one.subscription,
                Some(500),
                DeliveryStatus::Failed,
                0,
            )
            .await
            .expect("an attempt");
    }
    let paused = hooks
        .get_subscription(&deploys.id)
        .await
        .expect("a read")
        .expect("the subscription");
    assert!(!paused.active, "twenty consecutive failures pause it");

    hooks.unsubscribe(&all.id).await.expect("a removal");
    assert!(
        hooks
            .get_subscription(&all.id)
            .await
            .expect("a read")
            .is_none()
    );
}
