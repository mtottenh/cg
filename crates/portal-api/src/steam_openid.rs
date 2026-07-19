//! Steam OpenID 2.0 sign-in support.
//!
//! Steam's login flow is plain OpenID 2.0 with `identifier_select` — no
//! OAuth, no client secret. The portal redirects the browser to
//! `steamcommunity.com/openid/login` (`checkid_setup`), Steam sends the
//! user back to our callback with a signed assertion, and we verify that
//! assertion *directly with Steam* by POSTing it back with
//! `openid.mode=check_authentication` (stateless verification).
//!
//! The outbound verification call is behind [`SteamOpenIdVerifier`] so
//! integration tests can drive the callback handler with a test double
//! and never touch the network. The real implementation is
//! [`HttpSteamOpenIdVerifier`].

use async_trait::async_trait;
use portal_core::DomainError;
use std::time::Duration;

/// Steam community OpenID 2.0 endpoint (login redirect + verification).
pub const STEAM_OPENID_ENDPOINT: &str = "https://steamcommunity.com/openid/login";

/// OpenID 2.0 namespace value.
pub const OPENID_NS: &str = "http://specs.openid.net/auth/2.0";

/// OpenID 2.0 identifier-select value for identity/claimed_id.
pub const OPENID_IDENTIFIER_SELECT: &str = "http://specs.openid.net/auth/2.0/identifier_select";

/// Prefix of the `openid.claimed_id` URL Steam returns; the SteamID64
/// is the path segment after it.
pub const STEAM_CLAIMED_ID_PREFIX: &str = "https://steamcommunity.com/openid/id/";

/// Configuration for the Steam sign-in flow, sourced from environment.
#[derive(Debug, Clone)]
pub struct SteamAuthConfig {
    /// Public base URL of this API (for `openid.return_to` / `openid.realm`).
    pub public_url: String,
    /// Frontend base URL to redirect back to with the issued tokens.
    pub frontend_url: String,
    /// Optional Steam Web API key for persona-name enrichment
    /// (`ISteamUser.GetPlayerSummaries`). Sign-in works without it.
    pub api_key: Option<String>,
}

impl SteamAuthConfig {
    /// Load from `PORTAL_PUBLIC_URL`, `PORTAL_FRONTEND_URL` and
    /// `STEAM_API_KEY`, with dev-friendly defaults for the URLs.
    #[must_use]
    pub fn from_env() -> Self {
        let trim = |s: String| s.trim_end_matches('/').to_string();
        Self {
            public_url: std::env::var("PORTAL_PUBLIC_URL")
                .map_or_else(|_| "http://localhost:3000".to_string(), trim),
            frontend_url: std::env::var("PORTAL_FRONTEND_URL")
                .map_or_else(|_| "http://localhost:5173".to_string(), trim),
            api_key: std::env::var("STEAM_API_KEY")
                .ok()
                .filter(|k| !k.trim().is_empty()),
        }
    }

    /// The `openid.return_to` URL for this deployment.
    #[must_use]
    pub fn return_to_url(&self) -> String {
        format!("{}/v1/auth/steam/callback", self.public_url)
    }
}

/// Seam for the outbound calls the Steam sign-in flow makes.
///
/// Injected via `AppState` so integration tests can substitute a double
/// (see `AppState::with_steam_verifier`).
#[async_trait]
pub trait SteamOpenIdVerifier: Send + Sync {
    /// Verify a received OpenID assertion directly with Steam.
    ///
    /// Implementations POST `params` (with `openid.mode` replaced by
    /// `check_authentication`) to the Steam OpenID endpoint and return
    /// whether Steam answered `is_valid:true`.
    async fn check_authentication(&self, params: &[(String, String)]) -> Result<bool, DomainError>;

    /// Fetch the persona (display) name for a SteamID64.
    ///
    /// With an API key, uses `ISteamUser.GetPlayerSummaries`; without
    /// one, falls back to the public community profile XML endpoint
    /// (`steamcommunity.com/profiles/<id64>?xml=1`), which needs no
    /// credentials. Best-effort enrichment: any failure returns `None`.
    async fn fetch_persona_name(&self, api_key: Option<&str>, steam_id_64: i64) -> Option<String>;
}

/// Production [`SteamOpenIdVerifier`] backed by `reqwest`.
pub struct HttpSteamOpenIdVerifier {
    client: reqwest::Client,
}

impl HttpSteamOpenIdVerifier {
    /// Create a verifier with a short-timeout HTTP client.
    #[must_use]
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();
        Self { client }
    }
}

impl Default for HttpSteamOpenIdVerifier {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SteamOpenIdVerifier for HttpSteamOpenIdVerifier {
    async fn check_authentication(&self, params: &[(String, String)]) -> Result<bool, DomainError> {
        // Echo every openid.* param back, with mode swapped to
        // check_authentication, per the OpenID 2.0 direct-verification
        // protocol.
        let form: Vec<(&str, &str)> = params
            .iter()
            .map(|(k, v)| {
                if k == "openid.mode" {
                    (k.as_str(), "check_authentication")
                } else {
                    (k.as_str(), v.as_str())
                }
            })
            .collect();

        let response = self
            .client
            .post(STEAM_OPENID_ENDPOINT)
            .form(&form)
            .send()
            .await
            .map_err(|e| DomainError::Internal(format!("steam openid verification: {e}")))?;

        let body = response
            .text()
            .await
            .map_err(|e| DomainError::Internal(format!("steam openid verification: {e}")))?;

        // Response is a key-value document; a valid assertion contains
        // the line `is_valid:true`.
        Ok(body.lines().any(|line| line.trim() == "is_valid:true"))
    }

    async fn fetch_persona_name(&self, api_key: Option<&str>, steam_id_64: i64) -> Option<String> {
        // Preferred: the Steam Web API (stable JSON contract).
        if let Some(key) = api_key
            && let Some(name) = self.fetch_persona_via_web_api(key, steam_id_64).await
        {
            return Some(name);
        }
        // Keyless fallback: the public community profile XML document.
        self.fetch_persona_via_profile_xml(steam_id_64).await
    }
}

impl HttpSteamOpenIdVerifier {
    /// `ISteamUser.GetPlayerSummaries` (requires an API key).
    async fn fetch_persona_via_web_api(&self, api_key: &str, steam_id_64: i64) -> Option<String> {
        let url = reqwest::Url::parse_with_params(
            "https://api.steampowered.com/ISteamUser/GetPlayerSummaries/v0002/",
            &[("key", api_key), ("steamids", &steam_id_64.to_string())],
        )
        .ok()?;

        let response = self.client.get(url).send().await.ok()?;
        let body: serde_json::Value = response.json().await.ok()?;
        body.get("response")?
            .get("players")?
            .get(0)?
            .get("personaname")?
            .as_str()
            .map(std::string::ToString::to_string)
    }

    /// Public, keyless persona lookup via the community profile XML
    /// (`steamcommunity.com/profiles/<id64>?xml=1`). Works for any
    /// profile that isn't fully hidden.
    async fn fetch_persona_via_profile_xml(&self, steam_id_64: i64) -> Option<String> {
        let url = format!("https://steamcommunity.com/profiles/{steam_id_64}?xml=1");
        let response = self.client.get(url).send().await.ok()?;
        let body = response.text().await.ok()?;
        extract_persona_from_profile_xml(&body)
    }
}

/// Pull the persona name out of a community profile XML document.
///
/// The document carries it as `<steamID><![CDATA[Name]]></steamID>`
/// (CDATA optional). A tiny string extraction beats pulling in an XML
/// dependency for one field of one endpoint.
#[must_use]
pub fn extract_persona_from_profile_xml(xml: &str) -> Option<String> {
    let start = xml.find("<steamID>")? + "<steamID>".len();
    let end = xml[start..].find("</steamID>")? + start;
    let raw = xml[start..end].trim();
    let name = raw
        .strip_prefix("<![CDATA[")
        .and_then(|s| s.strip_suffix("]]>"))
        .unwrap_or(raw)
        .trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// Extract the SteamID64 from an `openid.claimed_id` URL of the form
/// `https://steamcommunity.com/openid/id/<id64>`.
#[must_use]
pub fn parse_steam_id_from_claimed_id(claimed_id: &str) -> Option<i64> {
    let rest = claimed_id.strip_prefix(STEAM_CLAIMED_ID_PREFIX)?;
    let digits = rest.trim_end_matches('/');
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_claimed_id() {
        assert_eq!(
            parse_steam_id_from_claimed_id(
                "https://steamcommunity.com/openid/id/76561197960287930"
            ),
            Some(76_561_197_960_287_930)
        );
    }

    #[test]
    fn extracts_persona_from_profile_xml() {
        let xml = r"<?xml version=\?><profile>
            <steamID64>76561197971721556</steamID64>
            <steamID><![CDATA[Murphy]]></steamID>
            <onlineState>offline</onlineState></profile>";
        assert_eq!(
            extract_persona_from_profile_xml(xml),
            Some("Murphy".to_string())
        );
        // Without CDATA wrapping
        assert_eq!(
            extract_persona_from_profile_xml("<profile><steamID>Plain Name</steamID></profile>"),
            Some("Plain Name".to_string())
        );
        // Missing / empty element
        assert_eq!(
            extract_persona_from_profile_xml("<profile></profile>"),
            None
        );
        assert_eq!(
            extract_persona_from_profile_xml("<steamID><![CDATA[]]></steamID>"),
            None
        );
    }

    #[test]
    fn rejects_foreign_or_malformed_claimed_ids() {
        assert_eq!(
            parse_steam_id_from_claimed_id("https://evil.example.com/openid/id/765611"),
            None
        );
        assert_eq!(
            parse_steam_id_from_claimed_id("https://steamcommunity.com/openid/id/not-a-number"),
            None
        );
        assert_eq!(parse_steam_id_from_claimed_id(""), None);
    }
}
