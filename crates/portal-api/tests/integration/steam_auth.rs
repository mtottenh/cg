//! Steam OpenID sign-in integration tests.
//!
//! The outbound `check_authentication` call is stubbed via
//! [`MockSteamVerifier`] injected through
//! `AppState::with_steam_verifier`, so no test touches the network.

use crate::common::TestApp;
use axum::http::StatusCode;
use portal_api::steam_openid::SteamOpenIdVerifier;
use portal_core::DomainError;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

const TEST_STEAM_ID: i64 = 76_561_197_960_287_930;

/// Test double for the Steam OpenID verifier seam.
struct MockSteamVerifier {
    /// What `check_authentication` should answer.
    valid: bool,
    /// Persona name returned by the (keyless in tests, so normally
    /// unused) Steam Web API stub.
    persona: Option<String>,
    /// Number of `check_authentication` calls observed.
    calls: AtomicUsize,
}

impl MockSteamVerifier {
    fn valid() -> Arc<Self> {
        Arc::new(Self {
            valid: true,
            persona: None,
            calls: AtomicUsize::new(0),
        })
    }

    fn invalid() -> Arc<Self> {
        Arc::new(Self {
            valid: false,
            persona: None,
            calls: AtomicUsize::new(0),
        })
    }
}

#[async_trait::async_trait]
impl SteamOpenIdVerifier for MockSteamVerifier {
    async fn check_authentication(&self, params: &[(String, String)]) -> Result<bool, DomainError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        // The handler must echo the openid params through to verification.
        assert!(
            params.iter().any(|(k, _)| k == "openid.claimed_id"),
            "verification params should include openid.claimed_id"
        );
        Ok(self.valid)
    }

    async fn fetch_persona_name(&self, _api_key: &str, _steam_id_64: i64) -> Option<String> {
        self.persona.clone()
    }
}

/// Build the callback URI (path + query) for a given SteamID64 /
/// return_to, URL-encoding the OpenID parameters the way Steam does.
fn callback_uri(steam_id_64: i64, return_to: &str) -> String {
    callback_uri_with_claimed_id(
        &format!("https://steamcommunity.com/openid/id/{steam_id_64}"),
        return_to,
    )
}

/// Same as [`callback_uri`] but with an explicit claimed_id URL.
fn callback_uri_with_claimed_id(claimed_id: &str, return_to: &str) -> String {
    let url = reqwest::Url::parse_with_params(
        "http://localhost:3000/v1/auth/steam/callback",
        &[
            ("openid.ns", "http://specs.openid.net/auth/2.0"),
            ("openid.mode", "id_res"),
            (
                "openid.op_endpoint",
                "https://steamcommunity.com/openid/login",
            ),
            ("openid.claimed_id", claimed_id),
            ("openid.identity", claimed_id),
            ("openid.return_to", return_to),
            ("openid.response_nonce", "2026-07-19T00:00:00Znonce"),
            ("openid.assoc_handle", "1234567890"),
            (
                "openid.signed",
                "signed,op_endpoint,claimed_id,identity,return_to,response_nonce,assoc_handle",
            ),
            ("openid.sig", "dGVzdHNpZw=="),
        ],
    )
    .expect("valid callback url");
    format!("{}?{}", url.path(), url.query().unwrap())
}

fn default_return_to() -> String {
    "http://localhost:3000/v1/auth/steam/callback".to_string()
}

/// Extract `#access_token=...&refresh_token=...` from a redirect Location.
fn tokens_from_fragment(location: &str) -> (String, String) {
    let (_, fragment) = location
        .split_once('#')
        .expect("redirect location should carry a fragment");
    let mut access = None;
    let mut refresh = None;
    for pair in fragment.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            match k {
                "access_token" => access = Some(v.to_string()),
                "refresh_token" => refresh = Some(v.to_string()),
                _ => {}
            }
        }
    }
    (
        access.expect("access_token in fragment"),
        refresh.expect("refresh_token in fragment"),
    )
}

// ===========================================
// Login redirect
// ===========================================

#[tokio::test]
async fn test_steam_login_redirects_to_steam() {
    let app = TestApp::new_with_steam_verifier(MockSteamVerifier::valid()).await;

    let response = app.get("/v1/auth/steam/login").await;
    response.assert_status(StatusCode::FOUND);

    let location = response
        .header("location")
        .expect("302 must carry a Location header");
    assert!(
        location.starts_with("https://steamcommunity.com/openid/login?"),
        "should redirect to Steam OpenID endpoint, got {location}"
    );
    let url = reqwest::Url::parse(&location).expect("valid redirect URL");
    let params: std::collections::HashMap<String, String> = url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();

    assert_eq!(
        params.get("openid.ns").map(String::as_str),
        Some("http://specs.openid.net/auth/2.0")
    );
    assert_eq!(
        params.get("openid.mode").map(String::as_str),
        Some("checkid_setup")
    );
    assert_eq!(
        params.get("openid.identity").map(String::as_str),
        Some("http://specs.openid.net/auth/2.0/identifier_select")
    );
    assert_eq!(
        params.get("openid.claimed_id").map(String::as_str),
        Some("http://specs.openid.net/auth/2.0/identifier_select")
    );
    assert_eq!(
        params.get("openid.return_to").map(String::as_str),
        Some("http://localhost:3000/v1/auth/steam/callback")
    );
    assert_eq!(
        params.get("openid.realm").map(String::as_str),
        Some("http://localhost:3000")
    );
}

// ===========================================
// Callback: provisioning + sign-in
// ===========================================

#[tokio::test]
async fn test_steam_callback_creates_account_and_redirects_with_tokens() {
    let app = TestApp::new_with_steam_verifier(MockSteamVerifier::valid()).await;

    let response = app
        .get(&callback_uri(TEST_STEAM_ID, &default_return_to()))
        .await;
    response.assert_status(StatusCode::FOUND);

    let location = response.header("location").expect("Location header");
    assert!(
        location.starts_with("http://localhost:5173/auth/steam/complete#"),
        "should redirect to the frontend completion route, got {location}"
    );
    // Tokens must be in the fragment, not the query string.
    assert!(!location.contains("?access_token"));

    let (access_token, refresh_token) = tokens_from_fragment(&location);

    // The access token works like any password-login token.
    let me = app.get_with_token("/v1/users/me", &access_token).await;
    me.assert_status(StatusCode::OK);
    let body: serde_json::Value = me.json();
    assert_eq!(body["data"]["auth_provider"], "steam");
    let username = body["data"]["username"].as_str().unwrap().to_string();
    assert_eq!(username, format!("steam_{TEST_STEAM_ID}"));

    // The player row carries the SteamID64.
    let row: (Option<i64>, Option<String>) = sqlx::query_as(
        "SELECT p.steam_id_64, p.steam_id FROM players p
         JOIN users u ON u.id = p.user_id WHERE u.username = $1",
    )
    .bind(&username)
    .fetch_one(app.pool())
    .await
    .expect("player row");
    assert_eq!(row.0, Some(TEST_STEAM_ID));
    assert_eq!(row.1, Some(TEST_STEAM_ID.to_string()));

    // The refresh token is a real, rotating credential.
    let refreshed = app
        .post_json_no_auth(
            "/v1/auth/refresh",
            &json!({ "refresh_token": refresh_token }),
        )
        .await;
    refreshed.assert_status(StatusCode::OK);
}

#[tokio::test]
async fn test_steam_callback_second_login_reuses_account() {
    let app = TestApp::new_with_steam_verifier(MockSteamVerifier::valid()).await;

    let first = app
        .get(&callback_uri(TEST_STEAM_ID, &default_return_to()))
        .await;
    first.assert_status(StatusCode::FOUND);

    let second = app
        .get(&callback_uri(TEST_STEAM_ID, &default_return_to()))
        .await;
    second.assert_status(StatusCode::FOUND);
    let (access_token, _) =
        tokens_from_fragment(&second.header("location").expect("Location header"));

    // Exactly one user row exists for this SteamID64.
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM players WHERE steam_id_64 = $1")
        .bind(TEST_STEAM_ID)
        .fetch_one(app.pool())
        .await
        .expect("count");
    assert_eq!(count, 1, "second Steam login must not create a duplicate");

    let me = app.get_with_token("/v1/users/me", &access_token).await;
    me.assert_status(StatusCode::OK);
}

// ===========================================
// Callback: rejection paths
// ===========================================

#[tokio::test]
async fn test_steam_callback_verification_failure_rejected() {
    let app = TestApp::new_with_steam_verifier(MockSteamVerifier::invalid()).await;

    let response = app
        .get(&callback_uri(TEST_STEAM_ID, &default_return_to()))
        .await;
    response.assert_status(StatusCode::UNAUTHORIZED);

    // No account was created.
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM players WHERE steam_id_64 = $1")
        .bind(TEST_STEAM_ID)
        .fetch_one(app.pool())
        .await
        .expect("count");
    assert_eq!(count, 0, "failed verification must not create an account");
}

#[tokio::test]
async fn test_steam_callback_mismatched_return_to_rejected() {
    let verifier = MockSteamVerifier::valid();
    let app = TestApp::new_with_steam_verifier(Arc::<MockSteamVerifier>::clone(&verifier)).await;

    let response = app
        .get(&callback_uri(
            TEST_STEAM_ID,
            "https://evil.example.com/v1/auth/steam/callback",
        ))
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
    assert_eq!(
        verifier.calls.load(Ordering::SeqCst),
        0,
        "mismatched return_to must be rejected before contacting Steam"
    );
}

#[tokio::test]
async fn test_steam_callback_bad_claimed_id_rejected() {
    let app = TestApp::new_with_steam_verifier(MockSteamVerifier::valid()).await;

    // Claimed id on a non-Steam host.
    let uri = callback_uri_with_claimed_id(
        &format!("https://evil.example.com/openid/id/{TEST_STEAM_ID}"),
        &default_return_to(),
    );
    let response = app.get(&uri).await;
    response.assert_status(StatusCode::BAD_REQUEST);

    // Claimed id that is not numeric.
    let uri = callback_uri_with_claimed_id(
        "https://steamcommunity.com/openid/id/not-a-number",
        &default_return_to(),
    );
    let response = app.get(&uri).await;
    response.assert_status(StatusCode::BAD_REQUEST);
}

// ===========================================
// Password login against a Steam account
// ===========================================

#[tokio::test]
async fn test_password_login_against_steam_account_rejected() {
    let app = TestApp::new_with_steam_verifier(MockSteamVerifier::valid()).await;

    // Provision a steam account.
    let response = app
        .get(&callback_uri(TEST_STEAM_ID, &default_return_to()))
        .await;
    response.assert_status(StatusCode::FOUND);

    // Password login with the generated username must fail loudly.
    let response = app
        .post_json_no_auth(
            "/v1/auth/login",
            &json!({
                "username_or_email": format!("steam_{TEST_STEAM_ID}"),
                "password": "SomePassword123!"
            }),
        )
        .await;
    response.assert_status(StatusCode::BAD_REQUEST);
    let body = response.text().to_lowercase();
    assert!(
        body.contains("steam"),
        "error should tell the user to sign in through Steam, got: {body}"
    );
}

// ===========================================
// Existing local account with linked SteamID64
// ===========================================

#[tokio::test]
async fn test_steam_callback_maps_to_existing_player_with_steam_id() {
    let app = TestApp::new_with_steam_verifier(MockSteamVerifier::valid()).await;

    // Register a normal (local) account.
    let response = app
        .post_json_no_auth(
            "/v1/auth/register",
            &json!({
                "username": "linkedsteam",
                "email": "linkedsteam@example.com",
                "password": "SecurePass123!",
                "display_name": "Linked Steam"
            }),
        )
        .await;
    response.assert_status(StatusCode::CREATED);
    let body: serde_json::Value = response.json();
    let token = body["data"]["access_token"].as_str().unwrap().to_string();
    let user_id = body["data"]["user"]["id"].as_str().unwrap().to_string();

    // Link a SteamID64 to the player profile.
    let response = app
        .patch_json_with_token(
            "/v1/players/me",
            &json!({ "steam_id": TEST_STEAM_ID.to_string() }),
            &token,
        )
        .await;
    response.assert_status(StatusCode::OK);

    // Steam sign-in with that SteamID64 maps to the existing account.
    let response = app
        .get(&callback_uri(TEST_STEAM_ID, &default_return_to()))
        .await;
    response.assert_status(StatusCode::FOUND);
    let (access_token, _) =
        tokens_from_fragment(&response.header("location").expect("Location header"));

    let me = app.get_with_token("/v1/users/me", &access_token).await;
    me.assert_status(StatusCode::OK);
    let me_body: serde_json::Value = me.json();
    assert_eq!(me_body["data"]["id"], user_id.as_str());
    assert_eq!(me_body["data"]["username"], "linkedsteam");
    // Still a local account — Steam login mapped, not converted.
    assert_eq!(me_body["data"]["auth_provider"], "local");

    // No duplicate account appeared.
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM players WHERE steam_id_64 = $1")
        .bind(TEST_STEAM_ID)
        .fetch_one(app.pool())
        .await
        .expect("count");
    assert_eq!(count, 1);
}
