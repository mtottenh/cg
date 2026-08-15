//! Steam tracking repository adapter.

use crate::DbPool;
use crate::entities::SteamTrackingRow;
use async_trait::async_trait;
use portal_core::{DomainError, GameId, PlayerId, SteamTrackingId};
use portal_domain::entities::steam_tracking::{
    PollOutcome, SteamTracking, UpdatePollResultCommand,
};
use portal_domain::repositories::discovered_match::BackoffPolicy;
use portal_domain::repositories::steam_tracking::{
    CreateSteamTracking, SteamTrackingRepository, TrackingHealthEntry, TrackingHealthSummary,
};

/// Column list with `poll_state` cast to `TEXT` for sqlx compatibility.
///
/// Replaces the `SELECT *` these queries used before the enum column existed.
const COLUMNS: &str = r"
    id, player_id, game_id, steam_id_64, game_auth_code, last_known_code,
    is_active, poll_errors, last_poll_at, last_error,
    next_poll_at, poll_state::TEXT as poll_state, paused_at,
    created_at, updated_at
";

/// Exponential backoff with equal jitter, as a SQL interval expression.
///
/// Mirrors the discovered-match schedule: `min(base * 2^(n-1), cap)` scaled
/// into `[50%, 100%]`, computed against the row's own counter so it stays
/// atomic with the status write.
fn backoff_interval(attempts_col: &str, base_param: &str, cap_param: &str) -> String {
    format!(
        "(LEAST({base_param}::double precision * POWER(2, GREATEST({attempts_col} - 1, 0)), \
         {cap_param}::double precision) * (0.5 + random() * 0.5)) * INTERVAL '1 second'"
    )
}

// =============================================================================
// Type Conversions
// =============================================================================

impl From<SteamTrackingRow> for SteamTracking {
    fn from(row: SteamTrackingRow) -> Self {
        Self {
            id: SteamTrackingId::from(row.id),
            player_id: PlayerId::from(row.player_id),
            game_id: GameId::from(row.game_id),
            steam_id_64: row.steam_id_64,
            game_auth_code: row.game_auth_code,
            last_known_code: row.last_known_code,
            is_active: row.is_active,
            poll_errors: row.poll_errors,
            last_poll_at: row.last_poll_at,
            last_error: row.last_error,
            next_poll_at: row.next_poll_at,
            poll_state: row.poll_state,
            paused_at: row.paused_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

// =============================================================================
// Steam Tracking Repository Adapter
// =============================================================================

/// PostgreSQL implementation of the domain `SteamTrackingRepository` trait.
#[derive(Clone)]
pub struct PgSteamTrackingRepository {
    pool: DbPool,
}

impl PgSteamTrackingRepository {
    /// Create a new PostgreSQL steam tracking repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SteamTrackingRepository for PgSteamTrackingRepository {
    async fn find_by_id(&self, id: SteamTrackingId) -> Result<Option<SteamTracking>, DomainError> {
        let sql = format!("SELECT {COLUMNS} FROM steam_tracking WHERE id = $1");
        let row = sqlx::query_as::<_, SteamTrackingRow>(&sql)
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(SteamTracking::from))
    }

    async fn find_by_player_and_game(
        &self,
        player_id: PlayerId,
        game_id: GameId,
    ) -> Result<Option<SteamTracking>, DomainError> {
        let sql =
            format!("SELECT {COLUMNS} FROM steam_tracking WHERE player_id = $1 AND game_id = $2");
        let row = sqlx::query_as::<_, SteamTrackingRow>(&sql)
            .bind(player_id.as_uuid())
            .bind(game_id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(SteamTracking::from))
    }

    async fn create(&self, cmd: CreateSteamTracking) -> Result<SteamTracking, DomainError> {
        let sql = format!(
            r"
            INSERT INTO steam_tracking (player_id, game_id, steam_id_64, game_auth_code, last_known_code)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING {COLUMNS}
            "
        );
        let row = sqlx::query_as::<_, SteamTrackingRow>(&sql)
            .bind(cmd.player_id.as_uuid())
            .bind(cmd.game_id.as_uuid())
            .bind(cmd.steam_id_64)
            .bind(&cmd.game_auth_code)
            .bind(&cmd.initial_share_code)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                if e.to_string().contains("uq_steam_tracking_player_game") {
                    DomainError::Conflict("Player already has tracking for this game".into())
                } else if e.to_string().contains("uq_steam_tracking_steam_id_game") {
                    DomainError::Conflict(
                        "This Steam ID is already being tracked for this game".into(),
                    )
                } else {
                    DomainError::Internal(e.to_string())
                }
            })?;

        Ok(SteamTracking::from(row))
    }

    async fn update_auth_code(
        &self,
        id: SteamTrackingId,
        auth_code: &str,
    ) -> Result<SteamTracking, DomainError> {
        // A new auth code IS the remedy for an `auth_expired` pause, so this
        // resumes the entry rather than only swapping the credential. Leaving
        // the pause in place — as this used to — meant the one self-service
        // fix available to a player changed nothing observable.
        //
        // A `cursor_invalid` pause is deliberately NOT cleared here: a fresh
        // auth code does nothing about a cursor Steam is rejecting, and
        // resuming would just reproduce the 412 on the next poll.
        let sql = format!(
            r"
            UPDATE steam_tracking
            SET game_auth_code = $2,
                poll_state = CASE
                    WHEN poll_state = 'cursor_invalid' THEN poll_state
                    ELSE 'ok'::steam_tracking_poll_state
                END,
                poll_errors  = CASE WHEN poll_state = 'cursor_invalid' THEN poll_errors ELSE 0 END,
                last_error   = CASE WHEN poll_state = 'cursor_invalid' THEN last_error ELSE NULL END,
                paused_at    = CASE WHEN poll_state = 'cursor_invalid' THEN paused_at ELSE NULL END,
                next_poll_at = CASE WHEN poll_state = 'cursor_invalid' THEN next_poll_at ELSE NOW() END,
                updated_at   = NOW()
            WHERE id = $1
            RETURNING {COLUMNS}
            "
        );
        let row = sqlx::query_as::<_, SteamTrackingRow>(&sql)
            .bind(id.as_uuid())
            .bind(auth_code)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?
            .ok_or_else(|| DomainError::Internal("Steam tracking entry not found".into()))?;

        Ok(SteamTracking::from(row))
    }

    async fn deactivate(&self, id: SteamTrackingId) -> Result<(), DomainError> {
        sqlx::query(
            "UPDATE steam_tracking SET is_active = FALSE, updated_at = NOW() WHERE id = $1",
        )
        .bind(id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn delete(&self, id: SteamTrackingId) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM steam_tracking WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn find_active_by_game(
        &self,
        game_id: GameId,
    ) -> Result<Vec<SteamTracking>, DomainError> {
        let sql = format!(
            r"
            SELECT {COLUMNS} FROM steam_tracking
            WHERE is_active = TRUE AND game_id = $1
            ORDER BY last_poll_at ASC NULLS FIRST
            "
        );
        let rows = sqlx::query_as::<_, SteamTrackingRow>(&sql)
            .bind(game_id.as_uuid())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(SteamTracking::from).collect())
    }

    async fn find_due_for_poll(
        &self,
        game_id: GameId,
        limit: i64,
    ) -> Result<Vec<SteamTracking>, DomainError> {
        // Paused entries are excluded by state, not by an error count. A
        // backing-off entry is still in the list — it is simply not due yet,
        // and `next_poll_at` says when it will be.
        //
        // Ordered by due time so the entry that has waited longest past its
        // schedule goes first; with a limit, a large fleet still drains fairly
        // instead of the same head-of-queue entries winning every cycle.
        let sql = format!(
            r"
            SELECT {COLUMNS} FROM steam_tracking
            WHERE game_id = $1
              AND is_active = TRUE
              AND poll_state IN ('ok', 'backoff')
              AND next_poll_at <= NOW()
            ORDER BY next_poll_at ASC, last_poll_at ASC NULLS FIRST
            LIMIT $2
            "
        );
        let rows = sqlx::query_as::<_, SteamTrackingRow>(&sql)
            .bind(game_id.as_uuid())
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(SteamTracking::from).collect())
    }

    async fn update_poll_result(
        &self,
        id: SteamTrackingId,
        cmd: UpdatePollResultCommand,
        backoff: BackoffPolicy,
        rate_limit_cooldown_secs: i64,
    ) -> Result<SteamTracking, DomainError> {
        // One statement for every outcome, because the cursor advance has to
        // apply regardless of how the poll ended. A walk that found three
        // codes and then hit a network error must still bank those three:
        // otherwise a player whose walk reliably breaks partway can never make
        // forward progress, and re-walks the same prefix every cycle forever.
        //
        // The backoff exponent is the POST-increment error count, so the first
        // transient failure waits `base` and each subsequent one doubles.
        let backoff_sql = backoff_interval("poll_errors + 1", "$5", "$6");
        let sql = format!(
            r"
            UPDATE steam_tracking
            SET last_known_code = COALESCE($2, last_known_code),
                poll_state      = $3::steam_tracking_poll_state,
                last_error      = $4::text,
                last_poll_at    = NOW(),

                -- Only transient failures accumulate. Rate limiting is our own
                -- request volume, not this token's health, so it must not
                -- drive this entry's backoff or its reported error state.
                poll_errors = CASE
                    WHEN $3::text = 'backoff' THEN poll_errors + 1
                    WHEN $3::text = 'ok'      THEN 0
                    ELSE poll_errors
                END,

                paused_at = CASE
                    WHEN $3::text IN ('auth_expired', 'cursor_invalid')
                        THEN COALESCE(paused_at, NOW())
                    ELSE NULL
                END,

                next_poll_at = CASE
                    -- Paused: nothing is scheduled. The entry re-enters the
                    -- queue when a human supplies a new auth code or resumes
                    -- it, both of which set next_poll_at themselves.
                    WHEN $3::text IN ('auth_expired', 'cursor_invalid') THEN next_poll_at
                    -- Rate limited: a flat cooldown, not an escalating one.
                    -- The condition is global and short-lived; escalating per
                    -- entry would punish tokens that did nothing wrong.
                    WHEN $7::boolean THEN NOW() + ($8::double precision * INTERVAL '1 second')
                    WHEN $3::text = 'backoff' THEN NOW() + {backoff_sql}
                    ELSE NOW()
                END,

                updated_at = NOW()
            WHERE id = $1
            RETURNING {COLUMNS}
            "
        );

        let state = cmd.outcome.resulting_state();
        let rate_limited = cmd.outcome == PollOutcome::RateLimited;

        let row = sqlx::query_as::<_, SteamTrackingRow>(&sql)
            .bind(id.as_uuid())
            .bind(&cmd.last_known_code)
            .bind(state)
            .bind(cmd.error.as_deref())
            .bind(backoff.base_secs)
            .bind(backoff.cap_secs)
            .bind(rate_limited)
            .bind(rate_limit_cooldown_secs as f64)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?
            .ok_or_else(|| DomainError::Internal("Steam tracking entry not found".into()))?;

        Ok(SteamTracking::from(row))
    }

    async fn resume(
        &self,
        id: SteamTrackingId,
        reset_cursor: bool,
    ) -> Result<SteamTracking, DomainError> {
        let sql = format!(
            r"
            UPDATE steam_tracking
            SET poll_state      = 'ok',
                poll_errors     = 0,
                last_error      = NULL,
                paused_at       = NULL,
                next_poll_at    = NOW(),
                last_known_code = CASE WHEN $2::boolean THEN NULL ELSE last_known_code END,
                updated_at      = NOW()
            WHERE id = $1
            RETURNING {COLUMNS}
            "
        );
        let row = sqlx::query_as::<_, SteamTrackingRow>(&sql)
            .bind(id.as_uuid())
            .bind(reset_cursor)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?
            .ok_or_else(|| DomainError::Internal("Steam tracking entry not found".into()))?;

        Ok(SteamTracking::from(row))
    }

    async fn list_health(
        &self,
        game_id: Option<GameId>,
        limit: i64,
    ) -> Result<Vec<TrackingHealthEntry>, DomainError> {
        // Worst first, and "worst" now means paused before merely erroring: a
        // backing-off entry recovers by itself, whereas a paused one is
        // waiting on a person and will sit there indefinitely until someone
        // sees it. Within each group, most errors then longest since a poll.
        let rows = sqlx::query_as::<_, TrackingHealthRow>(
            r"
            SELECT st.id,
                   st.player_id,
                   p.display_name AS player_display_name,
                   st.game_id,
                   g.slug AS game_slug,
                   st.steam_id_64,
                   st.is_active,
                   st.poll_errors,
                   st.last_poll_at,
                   st.last_error,
                   st.poll_state::TEXT AS poll_state,
                   st.next_poll_at,
                   st.paused_at,
                   (st.last_known_code IS NOT NULL) AS has_share_code,
                   st.created_at
            FROM steam_tracking st
            JOIN players p ON p.id = st.player_id
            JOIN games g ON g.id = st.game_id
            WHERE ($1::uuid IS NULL OR st.game_id = $1)
            ORDER BY (st.poll_state IN ('auth_expired', 'cursor_invalid')) DESC,
                     st.poll_errors DESC,
                     st.last_poll_at ASC NULLS FIRST,
                     st.created_at DESC
            LIMIT $2
            ",
        )
        .bind(game_id.map(|g| g.as_uuid()))
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(TrackingHealthEntry::from).collect())
    }

    async fn tracking_health_summary(
        &self,
        game_id: Option<GameId>,
        stale_after_hours: i64,
    ) -> Result<TrackingHealthSummary, DomainError> {
        let row = sqlx::query_as::<_, TrackingSummaryRow>(
            r"
            SELECT COUNT(*)                                                    AS total,
                   COUNT(*) FILTER (WHERE is_active)                           AS active,
                   COUNT(*) FILTER (WHERE NOT is_active)                       AS inactive,
                   COUNT(*) FILTER (WHERE is_active AND poll_errors > 0)       AS with_errors,
                   COUNT(*) FILTER (WHERE is_active AND last_poll_at IS NULL)  AS never_polled,
                   COUNT(*) FILTER (
                       WHERE is_active
                         AND last_poll_at IS NOT NULL
                         AND last_poll_at < NOW() - $2 * INTERVAL '1 hour'
                   )                                                           AS stale,
                   COUNT(*) FILTER (
                       WHERE is_active
                         AND poll_state IN ('auth_expired', 'cursor_invalid')
                   )                                                           AS paused,
                   COUNT(*) FILTER (
                       WHERE is_active AND poll_state = 'auth_expired'
                   )                                                           AS paused_auth_expired,
                   COUNT(*) FILTER (
                       WHERE is_active AND poll_state = 'cursor_invalid'
                   )                                                           AS paused_cursor_invalid,
                   MAX(last_poll_at)                                           AS last_poll_at
            FROM steam_tracking
            WHERE ($1::uuid IS NULL OR game_id = $1)
            ",
        )
        .bind(game_id.map(|g| g.as_uuid()))
        .bind(stale_after_hours as f64)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TrackingHealthSummary {
            total: row.total.unwrap_or(0),
            active: row.active.unwrap_or(0),
            inactive: row.inactive.unwrap_or(0),
            with_errors: row.with_errors.unwrap_or(0),
            never_polled: row.never_polled.unwrap_or(0),
            stale: row.stale.unwrap_or(0),
            paused: row.paused.unwrap_or(0),
            paused_auth_expired: row.paused_auth_expired.unwrap_or(0),
            paused_cursor_invalid: row.paused_cursor_invalid.unwrap_or(0),
            last_poll_at: row.last_poll_at,
        })
    }
}

/// Row shape for [`PgSteamTrackingRepository::list_health`].
#[derive(sqlx::FromRow)]
struct TrackingHealthRow {
    id: uuid::Uuid,
    player_id: uuid::Uuid,
    player_display_name: String,
    game_id: uuid::Uuid,
    game_slug: String,
    steam_id_64: i64,
    is_active: bool,
    poll_errors: i32,
    last_poll_at: Option<chrono::DateTime<chrono::Utc>>,
    last_error: Option<String>,
    poll_state: String,
    next_poll_at: chrono::DateTime<chrono::Utc>,
    paused_at: Option<chrono::DateTime<chrono::Utc>>,
    has_share_code: bool,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl From<TrackingHealthRow> for TrackingHealthEntry {
    fn from(row: TrackingHealthRow) -> Self {
        Self {
            id: SteamTrackingId::from(row.id),
            player_id: PlayerId::from(row.player_id),
            player_display_name: row.player_display_name,
            game_id: GameId::from(row.game_id),
            game_slug: row.game_slug,
            steam_id_64: row.steam_id_64,
            is_active: row.is_active,
            poll_errors: row.poll_errors,
            last_poll_at: row.last_poll_at,
            last_error: row.last_error,
            poll_state: row.poll_state,
            next_poll_at: row.next_poll_at,
            paused_at: row.paused_at,
            has_share_code: row.has_share_code,
            created_at: row.created_at,
        }
    }
}

/// Row shape for [`PgSteamTrackingRepository::tracking_health_summary`].
#[derive(sqlx::FromRow)]
struct TrackingSummaryRow {
    total: Option<i64>,
    active: Option<i64>,
    inactive: Option<i64>,
    with_errors: Option<i64>,
    never_polled: Option<i64>,
    stale: Option<i64>,
    paused: Option<i64>,
    paused_auth_expired: Option<i64>,
    paused_cursor_invalid: Option<i64>,
    last_poll_at: Option<chrono::DateTime<chrono::Utc>>,
}
