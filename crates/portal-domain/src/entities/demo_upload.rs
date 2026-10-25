//! MatchZy demo upload attempts (docs/demo-ingestion-admin-design.md §4).
//!
//! One [`DemoUploadAttempt`] per request to `POST /v1/gameserver/demos`,
//! whatever its outcome. MatchZy never retries and reports nothing, so this
//! is the only record a refused or failed upload leaves outside the logs.

use chrono::{DateTime, Utc};
use portal_core::ids::{
    DemoId, DemoUploadAttemptId, GameServerId, ServerReservationId, TournamentId, TournamentMatchId,
};
use portal_core::types::{DemoStatus, DemoUploadLinkNote, DemoUploadOutcome};

/// Minutes after a reservation completes before its demo counts as missing.
pub const DEMO_MISSING_AFTER_MINUTES: i64 = 30;
/// How far back the missing-demo check looks. Older gaps are history, not
/// something an operator can still act on (the server may have rotated the
/// file away).
pub const DEMO_MISSING_LOOKBACK_DAYS: i64 = 14;
/// Minutes an uploaded demo may sit pending/processing before it is stuck.
pub const DEMO_STUCK_AFTER_MINUTES: i64 = 60;
/// Attempts older than this are pruned by the lifecycle pass.
pub const DEMO_UPLOAD_RETENTION_DAYS: i64 = 90;

/// One recorded upload request.
#[derive(Debug, Clone)]
pub struct DemoUploadAttempt {
    pub id: DemoUploadAttemptId,
    pub received_at: DateTime<Utc>,
    /// `None` when the token was missing or unknown.
    pub server_id: Option<GameServerId>,
    pub outcome: DemoUploadOutcome,
    pub http_status: i32,
    /// Sanitised file name; `None` when the header was missing.
    pub file_name: Option<String>,
    pub matchzy_id: Option<i64>,
    pub map_number: Option<i32>,
    pub bytes: i64,
    pub duration_ms: i32,
    pub demo_id: Option<DemoId>,
    pub linked_match_id: Option<TournamentMatchId>,
    /// Stored outcomes only: why the demo was or wasn't linked.
    pub link_note: Option<DemoUploadLinkNote>,
    pub error: Option<String>,
}

/// An attempt with the names an operator reads it by (admin list).
#[derive(Debug, Clone)]
pub struct DemoUploadAttemptDetail {
    pub attempt: DemoUploadAttempt,
    pub server_name: Option<String>,
    /// Current catalog status of the stored demo.
    pub demo_status: Option<DemoStatus>,
    pub linked_tournament_id: Option<TournamentId>,
    pub linked_tournament_slug: Option<String>,
}

/// The server-upload stage of the pipeline overview.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerUploadsSummary {
    pub uploaded_24h: i64,
    /// `rejected_auth` + `rejected_request`.
    pub refused_24h: i64,
    /// `store_failed` + `catalog_failed`.
    pub failed_24h: i64,
    /// Completed reservations with no demo past the threshold.
    pub missing: i64,
    /// Uploaded demos pending/processing past the threshold.
    pub stuck_pending: i64,
    /// Latest `stored` attempt.
    pub last_upload_at: Option<DateTime<Utc>>,
}

/// A finished server match whose demo never arrived.
#[derive(Debug, Clone)]
pub struct MissingDemo {
    pub reservation_id: ServerReservationId,
    pub match_id: TournamentMatchId,
    pub tournament_id: TournamentId,
    pub tournament_slug: Option<String>,
    pub tournament_name: Option<String>,
    pub matchzy_id: i64,
    pub server_id: GameServerId,
    pub server_name: Option<String>,
    pub completed_at: DateTime<Utc>,
}

/// A server's most recent attempt of any outcome (Game Servers column).
#[derive(Debug, Clone, Copy)]
pub struct LatestServerUpload {
    pub server_id: GameServerId,
    pub received_at: DateTime<Utc>,
    pub outcome: DemoUploadOutcome,
}
