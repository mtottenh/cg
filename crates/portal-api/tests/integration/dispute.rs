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

    // match_id filter narrows to this match's disputes; without an explicit
    // status the default queue view applies (pending + under_review only).
    let response = app
        .get_auth(&format!(
            "/v1/admin/disputes?match_id={match_id}&page=1&page_size=50"
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let default_ids: Vec<&str> = body["data"]["disputes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["id"].as_str().unwrap())
        .collect();
    assert!(default_ids.contains(&pending_id.to_string().as_str()));
    assert!(
        !default_ids.contains(&resolved_id.to_string().as_str()),
        "default queue view must not include resolved disputes"
    );

    // With status=resolved + match_id both filters compose.
    let response = app
        .get_auth(&format!(
            "/v1/admin/disputes?match_id={match_id}&status=resolved&page=1&page_size=50"
        ))
        .await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["data"]["disputes"].as_array().unwrap().len(), 1);

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

// ============================================================================
// DISPUTE AUTHORIZATION TESTS
// ============================================================================

/// Force a match into `completed` status directly (the full result-claim
/// flow is not what these tests exercise).
async fn complete_match_row(app: &TestApp, match_id: &str) {
    sqlx::query("UPDATE tournament_matches SET status = 'completed' WHERE id = $1")
        .bind(uuid::Uuid::parse_str(match_id).unwrap())
        .execute(app.pool())
        .await
        .expect("complete match row");
}

fn dispute_test_token(user_id: uuid::Uuid, username: &str) -> String {
    portal_test::helpers::create_test_token(
        user_id,
        user_id,
        username,
        portal_test::helpers::TEST_JWT_SECRET,
    )
}

/// Only a member of the disputing registration (or an admin) may raise a
/// dispute for it.
#[tokio::test]
async fn test_raise_dispute_registration_binding() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, reg1, reg2, player2_token) =
        crate::tournaments::create_tournament_with_matches_and_opponent(&app, "dispute-authz")
            .await;
    complete_match_row(&app, &match_id).await;

    let outsider = portal_test::builders::UserBuilder::new()
        .username("dispute_outsider")
        .build_persisted(app.pool())
        .await;
    let outsider_token = dispute_test_token(outsider.id, "dispute_outsider");

    // An outsider cannot dispute on behalf of a participant registration.
    let response = app
        .post_json_with_token(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/dispute"),
            &json!({
                "registration_id": reg2,
                "reason": "wrong_score",
                "description": "Fabricated dispute from a non-participant account.",
                "evidence_ids": []
            }),
            &outsider_token,
        )
        .await;
    // Accept 401 or 403 while the NotAuthorized status-code mapping is in flight.
    assert!(
        response.status == StatusCode::FORBIDDEN || response.status == StatusCode::UNAUTHORIZED,
        "Outsider dispute should be rejected, got {}",
        response.status
    );

    // A participant cannot dispute pretending to be the OTHER registration.
    let response = app
        .post_json_with_token(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/dispute"),
            &json!({
                "registration_id": reg1,
                "reason": "wrong_score",
                "description": "Impersonating the opposing registration for a dispute.",
                "evidence_ids": []
            }),
            &player2_token,
        )
        .await;
    assert!(
        response.status == StatusCode::FORBIDDEN || response.status == StatusCode::UNAUTHORIZED,
        "Cross-registration dispute should be rejected, got {}",
        response.status
    );

    // The participant CAN dispute as their own registration.
    let response = app
        .post_json_with_token(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/dispute"),
            &json!({
                "registration_id": reg2,
                "reason": "wrong_score",
                "description": "The score reported was incorrect. We won 2-1, not 1-2.",
                "evidence_ids": []
            }),
            &player2_token,
        )
        .await;
    response.assert_status(StatusCode::CREATED);
}

/// Only dispute participants (or admins) may post participant messages.
#[tokio::test]
async fn test_add_dispute_message_requires_participant() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _reg1, reg2, player2_token) =
        crate::tournaments::create_tournament_with_matches_and_opponent(&app, "dispute-msg-authz")
            .await;
    complete_match_row(&app, &match_id).await;

    // Player 2 raises a dispute.
    let response = app
        .post_json_with_token(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/dispute"),
            &json!({
                "registration_id": reg2,
                "reason": "wrong_score",
                "description": "The score reported was incorrect. We won 2-1, not 1-2.",
                "evidence_ids": []
            }),
            &player2_token,
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    let dispute_id = body["data"]["id"].as_str().unwrap().to_string();

    // An outsider cannot post to the thread.
    let outsider = portal_test::builders::UserBuilder::new()
        .username("dispute_msg_outsider")
        .build_persisted(app.pool())
        .await;
    let outsider_token = dispute_test_token(outsider.id, "dispute_msg_outsider");
    let response = app
        .post_json_with_token(
            &format!("/v1/disputes/{dispute_id}/messages"),
            &json!({ "message": "Interloper message", "evidence_ids": [] }),
            &outsider_token,
        )
        .await;
    assert!(
        response.status == StatusCode::FORBIDDEN || response.status == StatusCode::UNAUTHORIZED,
        "Outsider dispute message should be rejected, got {}",
        response.status
    );

    // The dispute participant can post.
    let response = app
        .post_json_with_token(
            &format!("/v1/disputes/{dispute_id}/messages"),
            &json!({ "message": "Here is our additional evidence.", "evidence_ids": [] }),
            &player2_token,
        )
        .await;
    response.assert_status(StatusCode::CREATED);

    // An admin (dev token) can also post via the participant endpoint.
    let response = app
        .post_json(
            &format!("/v1/disputes/{dispute_id}/messages"),
            &json!({ "message": "Admin note on the thread.", "evidence_ids": [] }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
}

// ============================================================================
// P-77 / P-78 — RESOLUTION ACTUALLY WRITES THE MATCH RESULT
// ============================================================================

/// Drive a match to `awaiting_result` with a submitted claim, then dispute that
/// claim. Returns `(tournament_id, match_id, dispute_id, winner_reg)`.
///
/// This is the CLAIM path — the one P-77 broke. It matters that the claim is
/// never confirmed: that is exactly the state in which the match row carries no
/// winner and 0-0 scores, so anything reading "the original result" off the
/// match instead of the claim gets nothing.
async fn raise_dispute_on_a_claim(app: &TestApp, slug: &str) -> (String, String, String, String) {
    let (tournament_id, match_id, _reg1, _reg2, player2_token) =
        crate::tournaments::create_tournament_with_matches_and_opponent(app, slug).await;

    let scheduled_time = chrono::Utc::now() + chrono::Duration::minutes(5);
    app.post_json(
        &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/schedule"),
        &json!({ "scheduled_at": scheduled_time.to_rfc3339(), "reason": "p77 setup" }),
    )
    .await
    .assert_status(StatusCode::OK);
    app.post_json(
        &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/transition"),
        &json!({ "to_status": "in_progress", "override_reason": "p77 setup" }),
    )
    .await
    .assert_status(StatusCode::OK);

    // `reg1` from the builder is not necessarily participant ONE on the match —
    // read the slot from the match row, or the winner/score consistency check
    // rejects the claim.
    let p1_reg = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}"
        ))
        .await
        .json::<serde_json::Value>()["data"]["participant1_registration_id"]
        .as_str()
        .unwrap()
        .to_string();

    // p1 claims a 2-0 win. Non-zero and asymmetric on purpose: a 0-0 claim
    // would be indistinguishable from the empty match row this test is about.
    // (2 games minimum — the seeded format rejects a single-game total.)
    let response = app
        .post_json(
            &format!("/v1/matches/{match_id}/result"),
            &json!({
                "claimed_winner_registration_id": p1_reg,
                "participant1_score": 2,
                "participant2_score": 0,
                "game_results": [],
                "evidence_ids": []
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let claim_id = response.json::<serde_json::Value>()["data"]["claim"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // The opponent disputes it (a player may not dispute their own claim).
    let response = app
        .post_json_with_token(
            &format!("/v1/matches/{match_id}/result/{claim_id}/dispute"),
            &json!({
                "reason": "wrong_score",
                "description": "That score is not what happened in the server logs.",
                "evidence_ids": []
            }),
            &player2_token,
        )
        .await;
    response.assert_status(StatusCode::OK);

    // `ResultDisputeResponse` carries the claim and match status but not the
    // dispute id (the claim path returns the claim's view of the world), so
    // read it back from the row the handler just created.
    let dispute_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM disputes WHERE match_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(uuid::Uuid::parse_str(&match_id).unwrap())
    .fetch_one(app.pool())
    .await
    .expect("the claim dispute should have created a disputes row");

    (tournament_id, match_id, dispute_id.to_string(), p1_reg)
}

async fn match_row(app: &TestApp, match_id: &str) -> serde_json::Value {
    let row = sqlx::query_as::<
        _,
        (
            Option<uuid::Uuid>,
            Option<uuid::Uuid>,
            i32,
            i32,
            Option<chrono::DateTime<chrono::Utc>>,
            String,
        ),
    >(
        "SELECT winner_registration_id, loser_registration_id, participant1_score,
                participant2_score, completed_at, status
         FROM tournament_matches WHERE id = $1",
    )
    .bind(uuid::Uuid::parse_str(match_id).unwrap())
    .fetch_one(app.pool())
    .await
    .expect("match row");
    json!({
        "winner": row.0.map(|u| u.to_string()),
        "loser": row.1.map(|u| u.to_string()),
        "p1": row.2,
        "p2": row.3,
        "completed_at": row.4.map(|t| t.to_rfc3339()),
        "status": row.5,
    })
}

/// P-77: upholding a dispute raised against a CLAIM must leave the claimed
/// result on the match.
///
/// Before the fix this produced a `completed` match with **no winner and 0-0**,
/// which then fed bracket progression: `raise_dispute` snapshotted `original_*`
/// from the match row (empty on the claim path) and `resolve_uphold` only
/// flipped the status without writing any result at all.
#[tokio::test]
async fn test_uphold_on_claim_path_leaves_the_claimed_result_on_the_match() {
    let app = TestApp::new().await;
    let (_tournament_id, match_id, dispute_id, winner_reg) =
        raise_dispute_on_a_claim(&app, "p77-uphold").await;

    let response = app
        .post_json(
            &format!("/v1/admin/disputes/{dispute_id}/resolve/uphold"),
            &json!({ "notes": "Server logs match the submitted score." }),
        )
        .await;
    response.assert_status(StatusCode::OK);

    let m = match_row(&app, &match_id).await;
    assert_eq!(m["status"], "completed", "uphold completes the match");
    assert_eq!(
        m["winner"].as_str(),
        Some(winner_reg.as_str()),
        "the UPHELD claim's winner must be on the match — a completed match with \
         no winner is what P-77 produced, and progression consumes it"
    );
    assert_eq!(m["p1"], 2, "the upheld claim's score must be on the match");
    assert_eq!(m["p2"], 0);
}

/// The dispute an organiser opens from the queue must name the teams, not
/// hand over registration ids: the list attached the match context but the
/// detail endpoint did not, so the modal showed four UUIDs.
#[tokio::test]
async fn test_dispute_detail_names_the_match_and_the_sides() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, dispute_id, winner_reg) =
        raise_dispute_on_a_claim(&app, "dispute-names").await;
    let m: serde_json::Value = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}"
        ))
        .await
        .json();

    let response = app.get(&format!("/v1/disputes/{dispute_id}")).await;
    response.assert_status(StatusCode::OK);
    let body: serde_json::Value = response.json();
    let dispute = &body["data"]["dispute"];
    assert!(
        dispute["participant1_name"].is_string() && dispute["participant2_name"].is_string(),
        "both sides are named: {dispute}"
    );
    assert!(
        dispute["disputed_by_name"].is_string(),
        "the raiser is named"
    );
    assert_eq!(
        dispute["original_winner_registration_id"].as_str(),
        Some(winner_reg.as_str())
    );
    let winner_name =
        if m["data"]["participant1_registration_id"].as_str() == Some(winner_reg.as_str()) {
            dispute["participant1_name"].clone()
        } else {
            dispute["participant2_name"].clone()
        };
    assert_eq!(
        dispute["original_winner_name"], winner_name,
        "the claimed winner is named as one of the sides"
    );
    assert!(
        dispute["tournament_slug"].is_string(),
        "links back to the tournament"
    );
}

/// P-78: a rematch un-completes the match, so the previous winner and score
/// must not survive it.
///
/// Before the fix `resolve_with_status_change` updated `status` alone, so a
/// match sitting in `ready` to be replayed still reported a winner, a score and
/// a `completed_at` — and progression had already advanced that winner.
///
/// This uses the CONFIRMED path deliberately: the claim is confirmed first so
/// the match genuinely carries a result, and the precondition asserts that.
/// Ordering a rematch on an unconfirmed claim would clear a match that had
/// nothing on it — an assertion that passes without the fix.
#[tokio::test]
async fn test_rematch_clears_the_previous_result_from_the_match() {
    let app = TestApp::new().await;
    let (tournament_id, match_id, _reg1, reg2, player2_token) =
        crate::tournaments::create_tournament_with_matches_and_opponent(&app, "p78-rematch").await;

    let scheduled_time = chrono::Utc::now() + chrono::Duration::minutes(5);
    app.post_json(
        &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/schedule"),
        &json!({ "scheduled_at": scheduled_time.to_rfc3339(), "reason": "p78 setup" }),
    )
    .await
    .assert_status(StatusCode::OK);
    app.post_json(
        &format!("/v1/admin/tournaments/{tournament_id}/matches/{match_id}/transition"),
        &json!({ "to_status": "in_progress", "override_reason": "p78 setup" }),
    )
    .await
    .assert_status(StatusCode::OK);

    let p1_reg = app
        .get(&format!(
            "/v1/tournaments/{tournament_id}/matches/{match_id}"
        ))
        .await
        .json::<serde_json::Value>()["data"]["participant1_registration_id"]
        .as_str()
        .unwrap()
        .to_string();

    let response = app
        .post_json(
            &format!("/v1/matches/{match_id}/result"),
            &json!({
                "claimed_winner_registration_id": p1_reg,
                "participant1_score": 2,
                "participant2_score": 1,
                "game_results": [],
                "evidence_ids": []
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let claim_id = response.json::<serde_json::Value>()["data"]["claim"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    // The opponent CONFIRMS, so the match really is completed with a result.
    app.post_json_with_token(
        &format!("/v1/matches/{match_id}/result/{claim_id}/confirm"),
        &json!({}),
        &player2_token,
    )
    .await
    .assert_status(StatusCode::OK);

    let before = match_row(&app, &match_id).await;
    assert!(
        before["winner"].as_str().is_some() && before["p1"] == 2,
        "precondition: the match must carry a real result before the rematch, got {before}"
    );

    // Dispute the confirmed result, then order the replay.
    let response = app
        .post_json_with_token(
            &format!("/v1/tournaments/{tournament_id}/matches/{match_id}/dispute"),
            &json!({
                "registration_id": reg2,
                "reason": "wrong_score",
                "description": "The recorded score does not match the server logs at all.",
                "evidence_ids": []
            }),
            &player2_token,
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let dispute_id = response.json::<serde_json::Value>()["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    app.post_json(
        &format!("/v1/admin/disputes/{dispute_id}/resolve/rematch"),
        &json!({ "notes": "Both sides agreed to replay the map." }),
    )
    .await
    .assert_status(StatusCode::OK);

    let after = match_row(&app, &match_id).await;
    assert_eq!(after["status"], "ready", "a rematch reopens the match");
    assert!(
        after["winner"].is_null(),
        "P-78: a match ready to be replayed must not still record a winner, got {after}"
    );
    assert!(after["loser"].is_null(), "P-78: nor a loser");
    assert_eq!(after["p1"], 0, "P-78: nor a score");
    assert_eq!(after["p2"], 0);
    assert!(
        after["completed_at"].is_null(),
        "P-78: nor a completion time"
    );
}
