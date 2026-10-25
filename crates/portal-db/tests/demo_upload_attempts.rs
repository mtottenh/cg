//! Repository-level tests for `demo_upload_attempts`
//! (docs/demo-ingestion-admin-design.md §4): record, filtered list, keyset
//! paging, latest-per-server, the 24h summary counts, and prune.

use chrono::{Duration, Utc};
use portal_core::ids::{GameId, GameServerId};
use portal_core::types::{DemoUploadLinkNote, DemoUploadOutcome};
use portal_db::DbPool;
use portal_db::adapters::{PgDemoUploadAttemptRepository, PgGameServerRepository};
use portal_domain::repositories::{
    CreateDemoUploadAttempt, CreateGameServer, DemoUploadAttemptFilter,
    DemoUploadAttemptRepository, GameServerRepository,
};
use portal_test::database::TestDb;

fn attempt(server_id: Option<GameServerId>, outcome: DemoUploadOutcome) -> CreateDemoUploadAttempt {
    CreateDemoUploadAttempt {
        server_id,
        outcome,
        http_status: match outcome {
            DemoUploadOutcome::Stored => 200,
            DemoUploadOutcome::RejectedAuth => 401,
            DemoUploadOutcome::RejectedRequest => 400,
            DemoUploadOutcome::StoreFailed | DemoUploadOutcome::CatalogFailed => 500,
        },
        file_name: Some("map1.dem".to_string()),
        matchzy_id: Some(42),
        map_number: Some(0),
        bytes: 1234,
        duration_ms: 7,
        demo_id: None,
        linked_match_id: None,
        link_note: (outcome == DemoUploadOutcome::Stored)
            .then_some(DemoUploadLinkNote::NoReservation),
        error: None,
    }
}

async fn create_server(pool: &DbPool, name: &str) -> GameServerId {
    let game_id = GameId::from(portal_test::helpers::get_game_id(pool, "cs2").await);
    PgGameServerRepository::new(pool.clone())
        .create(CreateGameServer {
            id: GameServerId::new(),
            name: name.to_string(),
            game_id,
            ip_address: "203.0.113.9".parse().unwrap(),
            port: 27015,
            gotv_port: None,
            region: "test".to_string(),
        })
        .await
        .unwrap()
        .id
}

fn filter(limit: i64) -> DemoUploadAttemptFilter {
    DemoUploadAttemptFilter {
        limit,
        ..Default::default()
    }
}

#[tokio::test]
async fn test_record_roundtrips_every_field() {
    let db = TestDb::new().await;
    let repo = PgDemoUploadAttemptRepository::new(db.pool().clone());
    let server = create_server(db.pool(), "Repo box").await;

    let mut create = attempt(Some(server), DemoUploadOutcome::Stored);
    create.error = Some("note".to_string());
    let row = repo.record(create).await.unwrap();
    assert_eq!(row.server_id, Some(server));
    assert_eq!(row.outcome, DemoUploadOutcome::Stored);
    assert_eq!(row.http_status, 200);
    assert_eq!(row.file_name.as_deref(), Some("map1.dem"));
    assert_eq!(row.matchzy_id, Some(42));
    assert_eq!(row.map_number, Some(0));
    assert_eq!(row.bytes, 1234);
    assert_eq!(row.duration_ms, 7);
    assert_eq!(row.link_note, Some(DemoUploadLinkNote::NoReservation));
    assert_eq!(row.error.as_deref(), Some("note"));

    // An auth refusal has no server.
    let refused = repo
        .record(attempt(None, DemoUploadOutcome::RejectedAuth))
        .await
        .unwrap();
    assert_eq!(refused.server_id, None);
    assert_eq!(refused.link_note, None);

    let listed = repo.list(filter(10)).await.unwrap();
    assert_eq!(listed.len(), 2);
    // Newest first; the joined server name is filled where there is a server.
    assert_eq!(listed[0].attempt.id, refused.id);
    assert_eq!(listed[0].server_name, None);
    assert_eq!(listed[1].server_name.as_deref(), Some("Repo box"));
}

#[tokio::test]
async fn test_list_filters_and_keyset() {
    let db = TestDb::new().await;
    let repo = PgDemoUploadAttemptRepository::new(db.pool().clone());
    let a = create_server(db.pool(), "A").await;
    let b = create_server(db.pool(), "B").await;

    for _ in 0..3 {
        repo.record(attempt(Some(a), DemoUploadOutcome::Stored))
            .await
            .unwrap();
    }
    repo.record(attempt(Some(b), DemoUploadOutcome::StoreFailed))
        .await
        .unwrap();
    repo.record(attempt(None, DemoUploadOutcome::RejectedAuth))
        .await
        .unwrap();

    let by_server = repo
        .list(DemoUploadAttemptFilter {
            server_id: Some(a),
            ..filter(50)
        })
        .await
        .unwrap();
    assert_eq!(by_server.len(), 3);
    assert!(by_server.iter().all(|d| d.attempt.server_id == Some(a)));

    let by_outcome = repo
        .list(DemoUploadAttemptFilter {
            outcome: Some(DemoUploadOutcome::StoreFailed),
            ..filter(50)
        })
        .await
        .unwrap();
    assert_eq!(by_outcome.len(), 1);
    assert_eq!(by_outcome[0].attempt.server_id, Some(b));

    // Keyset: pages of two walk all five rows exactly once, newest first.
    let mut seen = Vec::new();
    let mut before = None;
    loop {
        let page = repo
            .list(DemoUploadAttemptFilter {
                before,
                ..filter(2)
            })
            .await
            .unwrap();
        if page.is_empty() {
            break;
        }
        before = page.last().map(|d| d.attempt.received_at);
        seen.extend(page.into_iter().map(|d| d.attempt));
    }
    assert_eq!(seen.len(), 5);
    assert!(
        seen.windows(2).all(|w| w[0].received_at > w[1].received_at),
        "strictly newest first, no repeats"
    );
}

#[tokio::test]
async fn test_latest_per_server_and_summary() {
    let db = TestDb::new().await;
    let repo = PgDemoUploadAttemptRepository::new(db.pool().clone());
    let a = create_server(db.pool(), "A").await;
    let b = create_server(db.pool(), "B").await;
    let never = create_server(db.pool(), "Never").await;

    repo.record(attempt(Some(a), DemoUploadOutcome::Stored))
        .await
        .unwrap();
    let last_a = repo
        .record(attempt(Some(a), DemoUploadOutcome::CatalogFailed))
        .await
        .unwrap();
    let last_b = repo
        .record(attempt(Some(b), DemoUploadOutcome::RejectedRequest))
        .await
        .unwrap();
    repo.record(attempt(None, DemoUploadOutcome::RejectedAuth))
        .await
        .unwrap();

    let latest = repo.latest_per_server(&[a, b, never]).await.unwrap();
    assert_eq!(latest.len(), 2, "a server that never uploaded is absent");
    let la = latest.iter().find(|l| l.server_id == a).unwrap();
    assert_eq!(la.outcome, DemoUploadOutcome::CatalogFailed);
    assert_eq!(la.received_at, last_a.received_at);
    let lb = latest.iter().find(|l| l.server_id == b).unwrap();
    assert_eq!(lb.outcome, DemoUploadOutcome::RejectedRequest);
    assert_eq!(lb.received_at, last_b.received_at);
    assert!(repo.latest_per_server(&[]).await.unwrap().is_empty());

    let summary = repo.summary(Utc::now()).await.unwrap();
    assert_eq!(summary.uploaded_24h, 1);
    assert_eq!(summary.refused_24h, 2);
    assert_eq!(summary.failed_24h, 1);
    assert_eq!(summary.missing, 0);
    assert_eq!(summary.stuck_pending, 0);
    assert!(summary.last_upload_at.is_some());

    // Outside the 24h window nothing counts, but last_upload_at still shows.
    let later = repo.summary(Utc::now() + Duration::days(2)).await.unwrap();
    assert_eq!(
        (later.uploaded_24h, later.refused_24h, later.failed_24h),
        (0, 0, 0)
    );
    assert_eq!(later.last_upload_at, summary.last_upload_at);
}

#[tokio::test]
async fn test_prune_deletes_only_old_rows() {
    let db = TestDb::new().await;
    let repo = PgDemoUploadAttemptRepository::new(db.pool().clone());

    let old = repo
        .record(attempt(None, DemoUploadOutcome::RejectedAuth))
        .await
        .unwrap();
    let fresh = repo
        .record(attempt(None, DemoUploadOutcome::RejectedAuth))
        .await
        .unwrap();
    sqlx::query(
        "UPDATE demo_upload_attempts SET received_at = NOW() - INTERVAL '91 days' WHERE id = $1",
    )
    .bind(old.id.as_uuid())
    .execute(db.pool())
    .await
    .unwrap();

    let pruned = repo.prune(Utc::now() - Duration::days(90)).await.unwrap();
    assert_eq!(pruned, 1);
    let left = repo.list(filter(10)).await.unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].attempt.id, fresh.id);
    assert_eq!(
        repo.prune(Utc::now() - Duration::days(90)).await.unwrap(),
        0
    );
}
