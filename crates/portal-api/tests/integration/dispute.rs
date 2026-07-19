//! Dispute API integration tests.

use crate::common::TestApp;
use axum::http::StatusCode;
use serde_json::json;

// ============================================================================
// ENDPOINT ROUTING TESTS
// ============================================================================

#[tokio::test]
async fn test_dispute_endpoints_exist() {
    let app = TestApp::new().await;
    let tournament_id = "00000000-0000-0000-0000-000000000000";
    let match_id = "00000000-0000-0000-0000-000000000001";
    let dispute_id = "00000000-0000-0000-0000-000000000002";

    // Verify raise dispute endpoint exists (authenticated)
    let response = app
        .post_json(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/dispute"),
            &json!({
                "registration_id": "00000000-0000-0000-0000-000000000003",
                "reason": "wrong_score",
                "description": "The score reported was incorrect. We won 2-1, not 1-2.",
                "evidence_ids": []
            }),
        )
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /dispute endpoint should exist"
    );

    // Verify get dispute endpoint exists
    let response = app.get(&format!("/v1/disputes/{dispute_id}")).await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "GET /disputes/{{id}} endpoint should exist"
    );

    // Verify add message endpoint exists (authenticated)
    let response = app
        .post_json(
            &format!("/v1/disputes/{dispute_id}/messages"),
            &json!({
                "message": "Additional evidence provided",
                "evidence_ids": []
            }),
        )
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /disputes/{{id}}/messages endpoint should exist"
    );

    // Verify admin list disputes endpoint exists
    let response = app.get_auth("/v1/admin/disputes").await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "GET /admin/disputes endpoint should exist"
    );

    // Verify admin assign endpoint exists
    let response = app
        .post_auth(&format!("/v1/admin/disputes/{dispute_id}/assign"))
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /admin/disputes/{{id}}/assign endpoint should exist"
    );
}

#[tokio::test]
async fn test_dispute_resolve_endpoints_exist() {
    let app = TestApp::new().await;
    let dispute_id = "00000000-0000-0000-0000-000000000001";

    // Verify resolve uphold endpoint exists
    let response = app
        .post_json(
            &format!("/v1/admin/disputes/{dispute_id}/resolve/uphold"),
            &json!({
                "notes": "After reviewing the evidence, the original result stands."
            }),
        )
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /admin/disputes/{{id}}/resolve/uphold endpoint should exist"
    );

    // Verify resolve overturn endpoint exists
    let response = app
        .post_json(
            &format!("/v1/admin/disputes/{dispute_id}/resolve/overturn"),
            &json!({
                "notes": "Evidence clearly shows the reported result was incorrect.",
                "new_winner_registration_id": "00000000-0000-0000-0000-000000000002",
                "new_participant1_score": 2,
                "new_participant2_score": 1
            }),
        )
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /admin/disputes/{{id}}/resolve/overturn endpoint should exist"
    );

    // Verify resolve rematch endpoint exists
    let response = app
        .post_json(
            &format!("/v1/admin/disputes/{dispute_id}/resolve/rematch"),
            &json!({
                "notes": "Due to technical issues, a rematch is required."
            }),
        )
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /admin/disputes/{{id}}/resolve/rematch endpoint should exist"
    );

    // Verify resolve adjusted endpoint exists
    let response = app
        .post_json(
            &format!("/v1/admin/disputes/{dispute_id}/resolve/adjusted"),
            &json!({
                "notes": "Scores have been adjusted based on the evidence.",
                "new_participant1_score": 2,
                "new_participant2_score": 0
            }),
        )
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /admin/disputes/{{id}}/resolve/adjusted endpoint should exist"
    );

    // Verify resolve double-dq endpoint exists
    let response = app
        .post_json(
            &format!("/v1/admin/disputes/{dispute_id}/resolve/double-dq"),
            &json!({
                "notes": "Both teams violated the rules. Both are disqualified."
            }),
        )
        .await;
    assert_ne!(
        response.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /admin/disputes/{{id}}/resolve/double-dq endpoint should exist"
    );
}

// ============================================================================
// RAISE DISPUTE TESTS
// ============================================================================

#[tokio::test]
async fn test_raise_dispute_invalid_match_id() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/tournaments/00000000-0000-0000-0000-000000000000/matches/not-a-uuid/dispute",
            &json!({
                "registration_id": "00000000-0000-0000-0000-000000000001",
                "reason": "wrong_score",
                "description": "The score reported was incorrect. We won 2-1, not 1-2.",
                "evidence_ids": []
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_raise_dispute_invalid_registration_id() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/tournaments/00000000-0000-0000-0000-000000000000/matches/00000000-0000-0000-0000-000000000001/dispute",
            &json!({
                "registration_id": "not-a-uuid",
                "reason": "wrong_score",
                "description": "The score reported was incorrect. We won 2-1, not 1-2.",
                "evidence_ids": []
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_raise_dispute_invalid_reason() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/tournaments/00000000-0000-0000-0000-000000000000/matches/00000000-0000-0000-0000-000000000001/dispute",
            &json!({
                "registration_id": "00000000-0000-0000-0000-000000000002",
                "reason": "invalid_reason_type",
                "description": "The score reported was incorrect. We won 2-1, not 1-2.",
                "evidence_ids": []
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_raise_dispute_description_too_short() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/tournaments/00000000-0000-0000-0000-000000000000/matches/00000000-0000-0000-0000-000000000001/dispute",
            &json!({
                "registration_id": "00000000-0000-0000-0000-000000000002",
                "reason": "wrong_score",
                "description": "Short",  // Less than 20 characters
                "evidence_ids": []
            }),
        )
        .await;

    // Validation should fail
    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_raise_dispute_nonexistent_match() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/tournaments/00000000-0000-0000-0000-000000000000/matches/00000000-0000-0000-0000-000000000001/dispute",
            &json!({
                "registration_id": "00000000-0000-0000-0000-000000000002",
                "reason": "wrong_score",
                "description": "The score reported was incorrect. We won 2-1, not 1-2.",
                "evidence_ids": []
            }),
        )
        .await;

    // Should return not found for non-existent match
    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_raise_dispute_requires_auth() {
    let app = TestApp::new().await;

    let response = app
        .post_json_no_auth(
            "/v1/tournaments/00000000-0000-0000-0000-000000000000/matches/00000000-0000-0000-0000-000000000001/dispute",
            &json!({
                "registration_id": "00000000-0000-0000-0000-000000000002",
                "reason": "wrong_score",
                "description": "The score reported was incorrect. We won 2-1, not 1-2.",
                "evidence_ids": []
            }),
        )
        .await;

    response.assert_status(StatusCode::UNAUTHORIZED);
}

// ============================================================================
// GET DISPUTE TESTS
// ============================================================================

#[tokio::test]
async fn test_get_dispute_invalid_id() {
    let app = TestApp::new().await;

    // Auth required before any id validation — unauthenticated is 401.
    let response = app.get_auth("/v1/disputes/not-a-uuid").await;
    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_get_dispute_not_found() {
    let app = TestApp::new().await;

    let response = app
        .get_auth("/v1/disputes/00000000-0000-0000-0000-000000000001")
        .await;

    response.assert_status(StatusCode::NOT_FOUND);
}

// ============================================================================
// ADD MESSAGE TESTS
// ============================================================================

#[tokio::test]
async fn test_add_message_invalid_dispute_id() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/disputes/not-a-uuid/messages",
            &json!({
                "message": "Additional evidence provided",
                "evidence_ids": []
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_add_message_nonexistent_dispute() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/disputes/00000000-0000-0000-0000-000000000001/messages",
            &json!({
                "message": "Additional evidence provided",
                "evidence_ids": []
            }),
        )
        .await;

    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_add_message_empty_message() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/disputes/00000000-0000-0000-0000-000000000001/messages",
            &json!({
                "message": "",  // Empty message
                "evidence_ids": []
            }),
        )
        .await;

    // Validation should fail
    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_add_message_requires_auth() {
    let app = TestApp::new().await;

    let response = app
        .post_json_no_auth(
            "/v1/disputes/00000000-0000-0000-0000-000000000001/messages",
            &json!({
                "message": "Additional evidence provided",
                "evidence_ids": []
            }),
        )
        .await;

    response.assert_status(StatusCode::UNAUTHORIZED);
}

// ============================================================================
// ADMIN LIST DISPUTES TESTS
// ============================================================================

#[tokio::test]
async fn test_admin_list_disputes_success() {
    let app = TestApp::new().await;

    let response = app.get_auth("/v1/admin/disputes").await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert!(body["data"]["disputes"].is_array());
    assert!(body["data"]["total"].is_number());
    assert!(body["data"]["page"].is_number());
    assert!(body["data"]["page_size"].is_number());
}

#[tokio::test]
async fn test_admin_list_disputes_with_pagination() {
    let app = TestApp::new().await;

    let response = app.get_auth("/v1/admin/disputes?page=1&page_size=10").await;
    response.assert_status(StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["page"], 1);
    assert_eq!(body["data"]["page_size"], 10);
}

#[tokio::test]
async fn test_admin_list_disputes_requires_auth() {
    let app = TestApp::new().await;

    let response = app.get("/v1/admin/disputes").await;
    response.assert_status(StatusCode::UNAUTHORIZED);
}

// ============================================================================
// ADMIN ASSIGN DISPUTE TESTS
// ============================================================================

#[tokio::test]
async fn test_admin_assign_invalid_dispute_id() {
    let app = TestApp::new().await;

    let response = app.post_auth("/v1/admin/disputes/not-a-uuid/assign").await;
    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_admin_assign_nonexistent_dispute() {
    let app = TestApp::new().await;

    let response = app
        .post_auth("/v1/admin/disputes/00000000-0000-0000-0000-000000000001/assign")
        .await;

    response.assert_status(StatusCode::NOT_FOUND);
}

// ============================================================================
// ADMIN RESOLVE UPHOLD TESTS
// ============================================================================

#[tokio::test]
async fn test_resolve_uphold_invalid_dispute_id() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/admin/disputes/not-a-uuid/resolve/uphold",
            &json!({
                "notes": "After reviewing the evidence, the original result stands."
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_resolve_uphold_nonexistent_dispute() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/admin/disputes/00000000-0000-0000-0000-000000000001/resolve/uphold",
            &json!({
                "notes": "After reviewing the evidence, the original result stands."
            }),
        )
        .await;

    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_resolve_uphold_notes_too_short() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/admin/disputes/00000000-0000-0000-0000-000000000001/resolve/uphold",
            &json!({
                "notes": "Short"  // Less than 10 characters
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

// ============================================================================
// ADMIN RESOLVE OVERTURN TESTS
// ============================================================================

#[tokio::test]
async fn test_resolve_overturn_invalid_dispute_id() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/admin/disputes/not-a-uuid/resolve/overturn",
            &json!({
                "notes": "Evidence clearly shows the reported result was incorrect.",
                "new_winner_registration_id": "00000000-0000-0000-0000-000000000002",
                "new_participant1_score": 2,
                "new_participant2_score": 1
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_resolve_overturn_invalid_winner_id() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/admin/disputes/00000000-0000-0000-0000-000000000001/resolve/overturn",
            &json!({
                "notes": "Evidence clearly shows the reported result was incorrect.",
                "new_winner_registration_id": "not-a-uuid",
                "new_participant1_score": 2,
                "new_participant2_score": 1
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_resolve_overturn_missing_scores() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/admin/disputes/00000000-0000-0000-0000-000000000001/resolve/overturn",
            &json!({
                "notes": "Evidence clearly shows the reported result was incorrect.",
                "new_winner_registration_id": "00000000-0000-0000-0000-000000000002"
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

// ============================================================================
// ADMIN RESOLVE REMATCH TESTS
// ============================================================================

#[tokio::test]
async fn test_resolve_rematch_invalid_dispute_id() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/admin/disputes/not-a-uuid/resolve/rematch",
            &json!({
                "notes": "Due to technical issues, a rematch is required."
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_resolve_rematch_nonexistent_dispute() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/admin/disputes/00000000-0000-0000-0000-000000000001/resolve/rematch",
            &json!({
                "notes": "Due to technical issues, a rematch is required."
            }),
        )
        .await;

    response.assert_status(StatusCode::NOT_FOUND);
}

// ============================================================================
// ADMIN RESOLVE ADJUSTED TESTS
// ============================================================================

#[tokio::test]
async fn test_resolve_adjusted_invalid_dispute_id() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/admin/disputes/not-a-uuid/resolve/adjusted",
            &json!({
                "notes": "Scores have been adjusted based on the evidence.",
                "new_participant1_score": 2,
                "new_participant2_score": 0
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_resolve_adjusted_missing_scores() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/admin/disputes/00000000-0000-0000-0000-000000000001/resolve/adjusted",
            &json!({
                "notes": "Scores have been adjusted based on the evidence."
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

// ============================================================================
// ADMIN RESOLVE DOUBLE DQ TESTS
// ============================================================================

#[tokio::test]
async fn test_resolve_double_dq_invalid_dispute_id() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/admin/disputes/not-a-uuid/resolve/double-dq",
            &json!({
                "notes": "Both teams violated the rules. Both are disqualified."
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_resolve_double_dq_nonexistent_dispute() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/admin/disputes/00000000-0000-0000-0000-000000000001/resolve/double-dq",
            &json!({
                "notes": "Both teams violated the rules. Both are disqualified."
            }),
        )
        .await;

    response.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_resolve_double_dq_notes_too_short() {
    let app = TestApp::new().await;

    let response = app
        .post_json(
            "/v1/admin/disputes/00000000-0000-0000-0000-000000000001/resolve/double-dq",
            &json!({
                "notes": "Short"  // Less than 10 characters
            }),
        )
        .await;

    response.assert_status(StatusCode::BAD_REQUEST);
}

// ============================================================================
// ADMIN LIST FILTERS
// ============================================================================

/// Insert a dispute row directly (the filter tests need multiple statuses,
/// which the HTTP flow can't fabricate quickly).
async fn insert_dispute_row(
    app: &TestApp,
    match_id: &str,
    reg_id: &str,
    status: &str,
) -> uuid::Uuid {
    let id = uuid::Uuid::now_v7();
    let dev_user_id = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    // 'resolved' rows must carry a resolution_type (check constraint).
    sqlx::query(
        r"INSERT INTO disputes (id, match_id, disputed_by_registration_id, disputed_by_user_id,
                                reason, description, status, resolution_type, resolved_at,
                                resolved_by_user_id)
          VALUES ($1, $2, $3, $4, 'wrong_score', 'admin list filter test dispute', $5,
                  CASE WHEN $5 = 'resolved' THEN 'upheld' END,
                  CASE WHEN $5 = 'resolved' THEN NOW() END,
                  CASE WHEN $5 = 'resolved' THEN $4 END)",
    )
    .bind(id)
    .bind(uuid::Uuid::parse_str(match_id).unwrap())
    .bind(uuid::Uuid::parse_str(reg_id).unwrap())
    .bind(dev_user_id)
    .bind(status)
    .execute(app.pool())
    .await
    .expect("insert dispute row");
    id
}

/// The admin list previously ignored every filter it accepted — status,
/// match_id, tournament_id were server-side no-ops and resolved disputes
/// vanished from the queue forever. This locks in the fixed behavior.
#[tokio::test]
async fn test_admin_list_disputes_filters_work() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, reg1, _) =
        crate::tournaments::create_tournament_with_matches(&app, "dispute-filter-test").await;

    let resolved_id = insert_dispute_row(&app, &match_id, &reg1, "resolved").await;
    let pending_id = insert_dispute_row(&app, &match_id, &reg1, "pending").await;

    // status=resolved returns the resolved dispute (previously impossible).
    let response = app
        .get_auth("/v1/admin/disputes?status=resolved&page=1&page_size=50")
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let ids: Vec<&str> = body["data"]["disputes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&resolved_id.to_string().as_str()));
    assert!(!ids.contains(&pending_id.to_string().as_str()));

    // match_id filter narrows to this match's disputes only.
    let response = app
        .get_auth(&format!(
            "/v1/admin/disputes?match_id={match_id}&page=1&page_size=50"
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["disputes"].as_array().unwrap().len(), 2);

    // tournament_id filter also works.
    let response = app
        .get_auth(&format!(
            "/v1/admin/disputes?tournament_id={tournament_id}&status=pending&page=1&page_size=50"
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let ids: Vec<&str> = body["data"]["disputes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec![pending_id.to_string().as_str()]);

    // Garbage filter values are rejected, not ignored.
    let response = app.get_auth("/v1/admin/disputes?status=bogus").await;
    response.assert_status(StatusCode::BAD_REQUEST);
}
