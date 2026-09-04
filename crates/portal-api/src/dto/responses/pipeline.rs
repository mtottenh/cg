//! Ingestion-pipeline operator response DTOs (P-73).
//!
//! Everything upstream of the demo catalog — Steam tracking tokens, the
//! discovered-match queue, enrichment — runs through the `X-API-Key`
//! `/v1/internal` routes, which are a service-to-service contract and are
//! deliberately absent from the public spec. These types are the
//! **admin-authenticated read projection** over the same tables, so an
//! operator can see that ingestion has stopped without shelling into the
//! database.
//!
//! Deliberately excluded: `steam_tracking.game_auth_code`. It is a live
//! Steam credential, and an operator needs to know a token is failing, not
//! what the token is.

use crate::dto::responses::demo::DemoStatusCountsResponse;
use portal_domain::entities::discovered_match::DiscoveredMatch;
use portal_domain::repositories::steam_tracking::{TrackingHealthEntry, TrackingHealthSummary};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Hours without a poll after which a tracking token is reported stale.
///
/// The CS2 poller's normal cadence is minutes; six hours of silence means the
/// poller is down, the token was revoked, or Valve is refusing it.
pub const TRACKING_STALE_AFTER_HOURS: i64 = 6;

/// Aggregate health of the Steam tracking tokens that feed the pipeline.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrackingHealthSummaryResponse {
    /// All tracking entries in scope, active or not.
    pub total: i64,
    /// Entries the poller is currently working.
    pub active: i64,
    /// Entries switched off (a deactivated token stops that player's feed).
    pub inactive: i64,
    /// Active entries whose last poll failed (`poll_errors > 0`).
    pub with_errors: i64,
    /// Active entries the poller has never touched.
    pub never_polled: i64,
    /// Active entries not polled within [`TRACKING_STALE_AFTER_HOURS`].
    pub stale: i64,
    /// Hours of silence after which an entry counts as stale.
    pub stale_after_hours: i64,
    /// Active entries the poller has stopped working pending a person.
    ///
    /// Counted apart from `with_errors` because the required response
    /// differs: an entry that is backing off recovers on its own, whereas
    /// these stay stopped until someone acts. This is the number that used to
    /// be invisible — parked entries looked identical to healthy ones except
    /// for a counter nothing explained.
    pub paused: i64,
    /// Of `paused`, those needing the player to supply a new auth code.
    pub paused_auth_expired: i64,
    /// Of `paused`, those needing the share-code cursor reset.
    pub paused_cursor_invalid: i64,
    /// Most recent poll across all entries in scope (ISO 8601).
    pub last_poll_at: Option<String>,
}

impl TrackingHealthSummaryResponse {
    #[must_use]
    pub fn from_summary(summary: TrackingHealthSummary, stale_after_hours: i64) -> Self {
        Self {
            total: summary.total,
            active: summary.active,
            inactive: summary.inactive,
            with_errors: summary.with_errors,
            never_polled: summary.never_polled,
            stale: summary.stale,
            stale_after_hours,
            paused: summary.paused,
            paused_auth_expired: summary.paused_auth_expired,
            paused_cursor_invalid: summary.paused_cursor_invalid,
            last_poll_at: summary.last_poll_at.map(|t| t.to_rfc3339()),
        }
    }
}

/// One tracking token, with the player it belongs to.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TrackingHealthEntryResponse {
    pub id: String,
    pub player_id: String,
    /// Named, not a truncated UUID (P-115).
    pub player_display_name: String,
    pub game_id: String,
    pub game_slug: String,
    /// The tracked SteamID64. Not a secret — it is public on the profile.
    pub steam_id_64: String,
    pub is_active: bool,
    pub poll_errors: i32,
    pub last_poll_at: Option<String>,
    pub last_error: Option<String>,
    /// `ok` · `backoff` · `auth_expired` · `cursor_invalid`.
    pub poll_state: String,
    /// True for the two paused states — the poller has stopped working this
    /// entry and will not resume without a person.
    pub is_paused: bool,
    /// What a person has to do, when `is_paused`. Null otherwise.
    pub required_action: Option<String>,
    /// When the poller will next try this entry (ISO 8601). Meaningless while
    /// paused, since nothing is scheduled.
    pub next_poll_at: String,
    pub paused_at: Option<String>,
    /// Whether a share-code cursor has been recorded yet.
    pub has_share_code: bool,
    pub created_at: String,
}

impl From<TrackingHealthEntry> for TrackingHealthEntryResponse {
    fn from(entry: TrackingHealthEntry) -> Self {
        Self {
            id: entry.id.to_string(),
            player_id: entry.player_id.to_string(),
            player_display_name: entry.player_display_name,
            game_id: entry.game_id.to_string(),
            game_slug: entry.game_slug,
            // SteamID64 exceeds JS's safe integer range; send it as a string
            // so the browser cannot silently round it.
            steam_id_64: entry.steam_id_64.to_string(),
            is_active: entry.is_active,
            poll_errors: entry.poll_errors,
            last_poll_at: entry.last_poll_at.map(|t| t.to_rfc3339()),
            last_error: entry.last_error,
            is_paused: matches!(entry.poll_state.as_str(), "auth_expired" | "cursor_invalid"),
            required_action: match entry.poll_state.as_str() {
                "auth_expired" => {
                    Some("The player must supply a new CS2 match-sharing auth code.".to_string())
                }
                "cursor_invalid" => Some(
                    "Steam rejected the stored share-code cursor. Resume with \
                     reset_cursor=true, then have the player supply a recent share code."
                        .to_string(),
                ),
                _ => None,
            },
            poll_state: entry.poll_state,
            next_poll_at: entry.next_poll_at.to_rfc3339(),
            paused_at: entry.paused_at.map(|t| t.to_rfc3339()),
            has_share_code: entry.has_share_code,
            created_at: entry.created_at.to_rfc3339(),
        }
    }
}

/// Depth of the discovered-match queue, by status.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DiscoveredMatchQueueResponse {
    /// Discovered, awaiting the enricher.
    pub pending: i64,
    /// Claimed by an enricher and in flight.
    pub enriching: i64,
    /// Enrichment succeeded.
    pub enriched: i64,
    /// Enrichment failed at least once.
    pub failed: i64,
    /// Failed with the retry budget spent — the enricher will not retry these.
    pub retry_exhausted: i64,
}

/// Depth of the demo-extraction stage, by `demo_status`.
///
/// Enrichment succeeding does not mean the match is complete: the demo carries
/// the rank updates and usually the map name, and it is fetched separately with
/// its own retry budget. A healthy `enriched` count next to a growing `pending`
/// here localises the stall to the Valve CDN rather than the GC.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DemoExtractionQueueResponse {
    /// Awaiting a first or subsequent attempt.
    pub pending: i64,
    /// Parsed, rank updates extracted.
    pub succeeded: i64,
    /// Parsed cleanly with no rank updates — casual and deathmatch demos are
    /// legitimately empty. A success, not a failure.
    pub empty: i64,
    /// Never published, or past Valve's retention window. Terminal.
    pub unavailable: i64,
    /// Downloaded but could not be decompressed or parsed. Terminal.
    pub failed: i64,
    /// GC returned no demo URL; there was never anything to fetch.
    pub not_applicable: i64,
}

/// Result of a bulk enrichment requeue.
#[derive(Debug, Serialize, ToSchema)]
pub struct RequeueDiscoveredMatchesResponse {
    /// How many rows were returned to the enrichment queue.
    pub requeued: u64,
}

/// One discovered match, as the operator needs to see it.
///
/// `gc_data` and the raw `demo_url` are omitted: they are large, and the
/// operator question is "did this get through, and if not why".
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DiscoveredMatchAdminResponse {
    pub id: String,
    pub share_code: String,
    pub status: String,
    pub error: Option<String>,
    pub retry_count: i32,
    pub max_retries: i32,
    /// True once the retry budget is spent — the enricher stops retrying.
    pub retry_exhausted: bool,
    /// Whether enrichment produced a demo URL for the scanner to fetch.
    pub has_demo_url: bool,
    pub discovered_at: String,
    pub enriched_at: Option<String>,
}

impl From<DiscoveredMatch> for DiscoveredMatchAdminResponse {
    fn from(m: DiscoveredMatch) -> Self {
        Self {
            id: m.id.to_string(),
            share_code: m.share_code,
            retry_exhausted: m.status == "failed" && m.retry_count >= m.max_retries,
            status: m.status,
            error: m.error,
            retry_count: m.retry_count,
            max_retries: m.max_retries,
            has_demo_url: m.demo_url.is_some(),
            discovered_at: m.discovered_at.to_rfc3339(),
            enriched_at: m.enriched_at.map(|t| t.to_rfc3339()),
        }
    }
}

/// The whole ingestion pipeline in one read: tokens → discovered matches →
/// demos. Each stage feeds the next, so a zero downstream with a healthy
/// upstream localises the stoppage.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PipelineOverviewResponse {
    /// Game slug this overview is scoped to, or null for all games.
    pub game_slug: Option<String>,
    /// Stage 1 — Steam tracking tokens (the poller's work list).
    pub tracking: TrackingHealthSummaryResponse,
    /// Stage 2 — the discovered-match queue (poller → enricher).
    pub discovered_matches: DiscoveredMatchQueueResponse,
    /// Stage 2b — demo fetch + rank extraction, retried independently of the
    /// GC call that produced the URL.
    pub demo_extraction: DemoExtractionQueueResponse,
    /// Stage 3 — the demo catalog (scanner → stats service).
    pub demos: DemoStatusCountsResponse,
    /// Whether the demo→match auto-linker is enabled. The backfill refuses to
    /// run while it is off, so the operator must see it next to the button.
    pub auto_link_enabled: bool,
}
