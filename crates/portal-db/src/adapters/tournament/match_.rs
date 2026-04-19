//! `PostgreSQL` implementation of `TournamentMatchRepository`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::entities::tournament::TournamentMatchRow;
use crate::transaction::DbTransaction;
use crate::DbPool;
use portal_core::types::TournamentMatchStatus;
use portal_core::{
    DomainError, PlayerId, TournamentBracketId, TournamentId, TournamentMatchId,
    TournamentRegistrationId, TournamentStageId, UserId,
};
use portal_domain::entities::tournament::TournamentMatch;
use portal_domain::repositories::tournament::{
    CreateTournamentMatch, ParticipantSlot, TournamentMatchRepository, UpdateTournamentMatch,
};

/// `PostgreSQL` implementation of `TournamentMatchRepository`.
#[derive(Debug, Clone)]
pub struct PgTournamentMatchRepository {
    pool: DbPool,
}

impl PgTournamentMatchRepository {
    /// Create a new repository instance.
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TournamentMatchRepository for PgTournamentMatchRepository {
    async fn find_by_id(
        &self,
        id: TournamentMatchId,
    ) -> Result<Option<TournamentMatch>, DomainError> {
        let row = sqlx::query_as::<_, TournamentMatchRow>(
            "SELECT * FROM tournament_matches WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(TournamentMatch::from))
    }

    async fn find_by_position(
        &self,
        bracket_id: TournamentBracketId,
        position: &str,
    ) -> Result<Option<TournamentMatch>, DomainError> {
        let row = sqlx::query_as::<_, TournamentMatchRow>(
            r"
            SELECT * FROM tournament_matches
            WHERE bracket_id = $1 AND bracket_position = $2
            ",
        )
        .bind(bracket_id.as_uuid())
        .bind(position)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(TournamentMatch::from))
    }

    async fn create(&self, cmd: CreateTournamentMatch) -> Result<TournamentMatch, DomainError> {
        let id = uuid::Uuid::now_v7();
        let now = Utc::now();

        let participant1_source_json = cmd
            .participant1_source
            .as_ref()
            .and_then(|s| serde_json::to_value(s).ok());
        let participant2_source_json = cmd
            .participant2_source
            .as_ref()
            .and_then(|s| serde_json::to_value(s).ok());

        let row = sqlx::query_as::<_, TournamentMatchRow>(
            r"
            INSERT INTO tournament_matches (
                id, bracket_id, stage_id, tournament_id,
                round, match_number, bracket_position,
                participant1_registration_id, participant2_registration_id,
                participant1_name, participant1_logo_url, participant1_seed,
                participant2_name, participant2_logo_url, participant2_seed,
                participant1_source, participant2_source,
                match_format, maps_required,
                winner_progresses_to, loser_progresses_to,
                created_at, updated_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23
            )
            RETURNING *
            ",
        )
        .bind(id)
        .bind(cmd.bracket_id.as_uuid())
        .bind(cmd.stage_id.as_uuid())
        .bind(cmd.tournament_id.as_uuid())
        .bind(cmd.round)
        .bind(cmd.match_number)
        .bind(&cmd.bracket_position)
        .bind(cmd.participant1_registration_id.map(|id| id.as_uuid()))
        .bind(cmd.participant2_registration_id.map(|id| id.as_uuid()))
        .bind(&cmd.participant1_name)
        .bind(&cmd.participant1_logo_url)
        .bind(cmd.participant1_seed)
        .bind(&cmd.participant2_name)
        .bind(&cmd.participant2_logo_url)
        .bind(cmd.participant2_seed)
        .bind(participant1_source_json)
        .bind(participant2_source_json)
        .bind(cmd.match_format.to_string())
        .bind(cmd.maps_required)
        .bind(cmd.winner_progresses_to.map(|id| id.as_uuid()))
        .bind(cmd.loser_progresses_to.map(|id| id.as_uuid()))
        .bind(now)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentMatch::from(row))
    }

    async fn update(
        &self,
        id: TournamentMatchId,
        update: UpdateTournamentMatch,
    ) -> Result<TournamentMatch, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentMatchRow>(
            r"
            UPDATE tournament_matches SET
                scheduled_at = COALESCE($2, scheduled_at),
                schedule_deadline = COALESCE($3, schedule_deadline),
                stream_url = COALESCE($4, stream_url),
                vod_url = COALESCE($5, vod_url),
                updated_at = $6
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(update.scheduled_at)
        .bind(update.schedule_deadline)
        .bind(&update.stream_url)
        .bind(&update.vod_url)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentMatch::from(row))
    }

    async fn update_status(
        &self,
        id: TournamentMatchId,
        status: TournamentMatchStatus,
    ) -> Result<TournamentMatch, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentMatchRow>(
            r"
            UPDATE tournament_matches SET status = $2, updated_at = $3
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(status.to_string())
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentMatch::from(row))
    }

    async fn mark_pending_as_ready_bulk(
        &self,
        ids: &[TournamentMatchId],
    ) -> Result<u64, DomainError> {
        if ids.is_empty() {
            return Ok(0);
        }

        // Single `WHERE id = ANY($1) AND status = 'pending'` so the
        // full set either transitions or not. Prior code looped
        // `update_status` per id; a mid-loop failure left the bracket
        // half-Ready. Matches already beyond Pending (advanced, DQ'd,
        // forfeited) are silently skipped by the WHERE clause — we
        // don't want to accidentally un-progress them.
        let now = Utc::now();
        let uuids: Vec<uuid::Uuid> = ids.iter().map(|id| id.as_uuid()).collect();

        let result = sqlx::query(
            r"
            UPDATE tournament_matches
            SET status = 'ready', updated_at = $2
            WHERE id = ANY($1) AND status = 'pending'
            ",
        )
        .bind(&uuids)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(result.rows_affected())
    }

    async fn assign_participant(
        &self,
        id: TournamentMatchId,
        slot: ParticipantSlot,
        registration_id: TournamentRegistrationId,
        name: String,
        logo_url: Option<String>,
        seed: Option<i32>,
    ) -> Result<TournamentMatch, DomainError> {
        let now = Utc::now();

        let row = match slot {
            ParticipantSlot::One => {
                sqlx::query_as::<_, TournamentMatchRow>(
                    r"
                    UPDATE tournament_matches SET
                        participant1_registration_id = $2,
                        participant1_name = $3,
                        participant1_logo_url = $4,
                        participant1_seed = $5,
                        updated_at = $6
                    WHERE id = $1
                    RETURNING *
                    ",
                )
                .bind(id.as_uuid())
                .bind(registration_id.as_uuid())
                .bind(&name)
                .bind(&logo_url)
                .bind(seed)
                .bind(now)
                .fetch_one(&self.pool)
                .await
            }
            ParticipantSlot::Two => {
                sqlx::query_as::<_, TournamentMatchRow>(
                    r"
                    UPDATE tournament_matches SET
                        participant2_registration_id = $2,
                        participant2_name = $3,
                        participant2_logo_url = $4,
                        participant2_seed = $5,
                        updated_at = $6
                    WHERE id = $1
                    RETURNING *
                    ",
                )
                .bind(id.as_uuid())
                .bind(registration_id.as_uuid())
                .bind(&name)
                .bind(&logo_url)
                .bind(seed)
                .bind(now)
                .fetch_one(&self.pool)
                .await
            }
        }
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentMatch::from(row))
    }

    async fn submit_result(
        &self,
        id: TournamentMatchId,
        participant1_score: i32,
        participant2_score: i32,
        winner_id: TournamentRegistrationId,
        loser_id: TournamentRegistrationId,
    ) -> Result<TournamentMatch, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentMatchRow>(
            r"
            UPDATE tournament_matches SET
                participant1_score = $2,
                participant2_score = $3,
                winner_registration_id = $4,
                loser_registration_id = $5,
                completed_at = $6,
                status = 'completed',
                updated_at = $6
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(participant1_score)
        .bind(participant2_score)
        .bind(winner_id.as_uuid())
        .bind(loser_id.as_uuid())
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentMatch::from(row))
    }

    async fn schedule(
        &self,
        id: TournamentMatchId,
        scheduled_at: DateTime<Utc>,
    ) -> Result<TournamentMatch, DomainError> {
        let now = Utc::now();

        // Note: Only update scheduled_at. The status transition is handled
        // by the service layer through the transition() method.
        let row = sqlx::query_as::<_, TournamentMatchRow>(
            r"
            UPDATE tournament_matches SET
                scheduled_at = $2,
                updated_at = $3
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(scheduled_at)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentMatch::from(row))
    }

    async fn start(&self, id: TournamentMatchId) -> Result<TournamentMatch, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentMatchRow>(
            r"
            UPDATE tournament_matches SET
                started_at = $2,
                status = 'in_progress',
                updated_at = $2
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentMatch::from(row))
    }

    async fn check_in_participant(
        &self,
        id: TournamentMatchId,
        slot: ParticipantSlot,
        checked_in_by: UserId,
    ) -> Result<TournamentMatch, DomainError> {
        let now = Utc::now();

        let row = match slot {
            ParticipantSlot::One => {
                sqlx::query_as::<_, TournamentMatchRow>(
                    r"
                    UPDATE tournament_matches SET
                        participant1_checked_in_at = $2,
                        participant1_checked_in_by = $3,
                        updated_at = $2
                    WHERE id = $1
                    RETURNING *
                    ",
                )
                .bind(id.as_uuid())
                .bind(now)
                .bind(checked_in_by.as_uuid())
                .fetch_one(&self.pool)
                .await
            }
            ParticipantSlot::Two => {
                sqlx::query_as::<_, TournamentMatchRow>(
                    r"
                    UPDATE tournament_matches SET
                        participant2_checked_in_at = $2,
                        participant2_checked_in_by = $3,
                        updated_at = $2
                    WHERE id = $1
                    RETURNING *
                    ",
                )
                .bind(id.as_uuid())
                .bind(now)
                .bind(checked_in_by.as_uuid())
                .fetch_one(&self.pool)
                .await
            }
        }
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentMatch::from(row))
    }

    async fn complete(&self, id: TournamentMatchId) -> Result<TournamentMatch, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentMatchRow>(
            r"
            UPDATE tournament_matches SET
                completed_at = $2,
                status = 'completed',
                updated_at = $2
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentMatch::from(row))
    }

    async fn forfeit(
        &self,
        id: TournamentMatchId,
        winner_id: TournamentRegistrationId,
        loser_id: TournamentRegistrationId,
    ) -> Result<TournamentMatch, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentMatchRow>(
            r"
            UPDATE tournament_matches SET
                winner_registration_id = $2,
                loser_registration_id = $3,
                completed_at = $4,
                status = 'forfeit',
                updated_at = $4
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(winner_id.as_uuid())
        .bind(loser_id.as_uuid())
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentMatch::from(row))
    }

    async fn file_dispute(
        &self,
        id: TournamentMatchId,
        reason: String,
    ) -> Result<TournamentMatch, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentMatchRow>(
            r"
            UPDATE tournament_matches SET
                disputed = true,
                dispute_reason = $2,
                status = 'disputed',
                updated_at = $3
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(&reason)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentMatch::from(row))
    }

    async fn resolve_dispute(
        &self,
        id: TournamentMatchId,
        resolved_by: UserId,
        resolution: String,
    ) -> Result<TournamentMatch, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentMatchRow>(
            r"
            UPDATE tournament_matches SET
                dispute_resolved_by = $2,
                dispute_resolution = $3,
                dispute_resolved_at = $4,
                updated_at = $4
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(resolved_by.as_uuid())
        .bind(&resolution)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentMatch::from(row))
    }

    async fn list_by_bracket(
        &self,
        bracket_id: TournamentBracketId,
    ) -> Result<Vec<TournamentMatch>, DomainError> {
        let rows = sqlx::query_as::<_, TournamentMatchRow>(
            r"
            SELECT * FROM tournament_matches
            WHERE bracket_id = $1
            ORDER BY round ASC, match_number ASC
            ",
        )
        .bind(bracket_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(TournamentMatch::from).collect())
    }

    async fn list_by_stage(
        &self,
        stage_id: TournamentStageId,
    ) -> Result<Vec<TournamentMatch>, DomainError> {
        let rows = sqlx::query_as::<_, TournamentMatchRow>(
            r"
            SELECT * FROM tournament_matches
            WHERE stage_id = $1
            ORDER BY round ASC, match_number ASC
            ",
        )
        .bind(stage_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(TournamentMatch::from).collect())
    }

    async fn list_by_tournament(
        &self,
        tournament_id: TournamentId,
    ) -> Result<Vec<TournamentMatch>, DomainError> {
        let rows = sqlx::query_as::<_, TournamentMatchRow>(
            r"
            SELECT * FROM tournament_matches
            WHERE tournament_id = $1
            ORDER BY round ASC, match_number ASC
            ",
        )
        .bind(tournament_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(TournamentMatch::from).collect())
    }

    async fn list_by_round(
        &self,
        bracket_id: TournamentBracketId,
        round: i32,
    ) -> Result<Vec<TournamentMatch>, DomainError> {
        let rows = sqlx::query_as::<_, TournamentMatchRow>(
            r"
            SELECT * FROM tournament_matches
            WHERE bracket_id = $1 AND round = $2
            ORDER BY match_number ASC
            ",
        )
        .bind(bracket_id.as_uuid())
        .bind(round)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(TournamentMatch::from).collect())
    }

    async fn list_by_status(
        &self,
        tournament_id: TournamentId,
        status: TournamentMatchStatus,
    ) -> Result<Vec<TournamentMatch>, DomainError> {
        let rows = sqlx::query_as::<_, TournamentMatchRow>(
            r"
            SELECT * FROM tournament_matches
            WHERE tournament_id = $1 AND status = $2
            ORDER BY round ASC, match_number ASC
            ",
        )
        .bind(tournament_id.as_uuid())
        .bind(status.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(TournamentMatch::from).collect())
    }

    async fn list_by_participant(
        &self,
        registration_id: TournamentRegistrationId,
    ) -> Result<Vec<TournamentMatch>, DomainError> {
        let rows = sqlx::query_as::<_, TournamentMatchRow>(
            r"
            SELECT * FROM tournament_matches
            WHERE participant1_registration_id = $1 OR participant2_registration_id = $1
            ORDER BY round ASC, match_number ASC
            ",
        )
        .bind(registration_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(TournamentMatch::from).collect())
    }

    async fn list_by_player(
        &self,
        player_id: PlayerId,
        status: Option<TournamentMatchStatus>,
        tournament_id: Option<TournamentId>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TournamentMatch>, DomainError> {
        let status_str = status.map(|s| s.to_string());
        let tournament_uuid = tournament_id.map(|id| id.as_uuid());

        let rows = sqlx::query_as::<_, TournamentMatchRow>(
            r"
            SELECT DISTINCT tm.*
            FROM tournament_matches tm
            JOIN tournament_registrations tr
              ON tm.participant1_registration_id = tr.id
              OR tm.participant2_registration_id = tr.id
            LEFT JOIN league_team_members ltm
              ON tr.team_season_id = ltm.team_season_id
              AND ltm.player_id = $1
            WHERE (tr.player_id = $1 OR ltm.player_id IS NOT NULL)
              AND ($2::text IS NULL OR tm.status::text = $2)
              AND ($3::uuid IS NULL OR tm.tournament_id = $3)
            ORDER BY
              CASE tm.status::text
                WHEN 'in_progress' THEN 1
                WHEN 'pick_ban' THEN 2
                WHEN 'checking_in' THEN 3
                WHEN 'scheduled' THEN 4
                WHEN 'ready' THEN 5
                WHEN 'awaiting_result' THEN 6
                WHEN 'disputed' THEN 7
                WHEN 'pending' THEN 8
                WHEN 'completed' THEN 9
                WHEN 'forfeit' THEN 10
                WHEN 'cancelled' THEN 11
              END,
              tm.scheduled_at ASC NULLS LAST,
              tm.created_at DESC
            LIMIT $4 OFFSET $5
            ",
        )
        .bind(player_id.as_uuid())
        .bind(status_str.as_deref())
        .bind(tournament_uuid)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(TournamentMatch::from).collect())
    }

    async fn list_upcoming(
        &self,
        tournament_id: TournamentId,
        limit: i64,
    ) -> Result<Vec<TournamentMatch>, DomainError> {
        let rows = sqlx::query_as::<_, TournamentMatchRow>(
            r"
            SELECT * FROM tournament_matches
            WHERE tournament_id = $1
              AND scheduled_at > NOW()
              AND status IN ('ready', 'scheduled')
            ORDER BY scheduled_at ASC
            LIMIT $2
            ",
        )
        .bind(tournament_id.as_uuid())
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(TournamentMatch::from).collect())
    }

    async fn bulk_create(
        &self,
        matches: Vec<CreateTournamentMatch>,
    ) -> Result<Vec<TournamentMatch>, DomainError> {
        if matches.is_empty() {
            return Ok(Vec::new());
        }

        // Previously this looped `self.create(cmd).await?` outside any
        // transaction, so a mid-generation failure committed the earlier
        // INSERTs and left the bracket half-populated. 23 columns per row
        // (including JSON-encoded participant sources) makes a single
        // UNNEST statement awkward, so we instead run every INSERT inside
        // one transaction that commits or rolls back atomically.
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let mut results = Vec::with_capacity(matches.len());
        let now = Utc::now();

        for cmd in matches {
            let id = uuid::Uuid::now_v7();
            let participant1_source_json = cmd
                .participant1_source
                .as_ref()
                .and_then(|s| serde_json::to_value(s).ok());
            let participant2_source_json = cmd
                .participant2_source
                .as_ref()
                .and_then(|s| serde_json::to_value(s).ok());

            let row = sqlx::query_as::<_, TournamentMatchRow>(
                r"
                INSERT INTO tournament_matches (
                    id, bracket_id, stage_id, tournament_id,
                    round, match_number, bracket_position,
                    participant1_registration_id, participant2_registration_id,
                    participant1_name, participant1_logo_url, participant1_seed,
                    participant2_name, participant2_logo_url, participant2_seed,
                    participant1_source, participant2_source,
                    match_format, maps_required,
                    winner_progresses_to, loser_progresses_to,
                    created_at, updated_at
                )
                VALUES (
                    $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                    $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23
                )
                RETURNING *
                ",
            )
            .bind(id)
            .bind(cmd.bracket_id.as_uuid())
            .bind(cmd.stage_id.as_uuid())
            .bind(cmd.tournament_id.as_uuid())
            .bind(cmd.round)
            .bind(cmd.match_number)
            .bind(&cmd.bracket_position)
            .bind(cmd.participant1_registration_id.map(|id| id.as_uuid()))
            .bind(cmd.participant2_registration_id.map(|id| id.as_uuid()))
            .bind(&cmd.participant1_name)
            .bind(&cmd.participant1_logo_url)
            .bind(cmd.participant1_seed)
            .bind(&cmd.participant2_name)
            .bind(&cmd.participant2_logo_url)
            .bind(cmd.participant2_seed)
            .bind(participant1_source_json)
            .bind(participant2_source_json)
            .bind(cmd.match_format.to_string())
            .bind(cmd.maps_required)
            .bind(cmd.winner_progresses_to.map(|id| id.as_uuid()))
            .bind(cmd.loser_progresses_to.map(|id| id.as_uuid()))
            .bind(now)
            .bind(now)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

            results.push(TournamentMatch::from(row));
        }

        tx.commit()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(results)
    }

    async fn delete(&self, id: TournamentMatchId) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM tournament_matches WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn delete_by_bracket(&self, bracket_id: TournamentBracketId) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM tournament_matches WHERE bracket_id = $1")
            .bind(bracket_id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn set_progression_links(
        &self,
        id: TournamentMatchId,
        winner_progresses_to: Option<TournamentMatchId>,
        loser_progresses_to: Option<TournamentMatchId>,
    ) -> Result<(), DomainError> {
        sqlx::query(
            r"
            UPDATE tournament_matches
            SET winner_progresses_to = $2,
                loser_progresses_to = $3
            WHERE id = $1
            ",
        )
        .bind(id.as_uuid())
        .bind(winner_progresses_to.map(|id| id.as_uuid()))
        .bind(loser_progresses_to.map(|id| id.as_uuid()))
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }
}

// =============================================================================
// TRANSACTIONAL METHODS
// =============================================================================

impl PgTournamentMatchRepository {
    /// Find a match by ID within a transaction.
    pub async fn find_by_id_in_tx(
        tx: &mut DbTransaction<'_>,
        id: TournamentMatchId,
    ) -> Result<Option<TournamentMatch>, DomainError> {
        let row = sqlx::query_as::<_, TournamentMatchRow>(
            "SELECT * FROM tournament_matches WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(TournamentMatch::from))
    }

    /// Update match status within a transaction.
    pub async fn update_status_in_tx(
        tx: &mut DbTransaction<'_>,
        id: TournamentMatchId,
        status: TournamentMatchStatus,
    ) -> Result<TournamentMatch, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentMatchRow>(
            r"
            UPDATE tournament_matches SET status = $2, updated_at = $3
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(status.to_string())
        .bind(now)
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentMatch::from(row))
    }

    /// Submit match result within a transaction.
    pub async fn submit_result_in_tx(
        tx: &mut DbTransaction<'_>,
        id: TournamentMatchId,
        participant1_score: i32,
        participant2_score: i32,
        winner_id: TournamentRegistrationId,
        loser_id: TournamentRegistrationId,
    ) -> Result<TournamentMatch, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, TournamentMatchRow>(
            r"
            UPDATE tournament_matches SET
                participant1_score = $2,
                participant2_score = $3,
                winner_registration_id = $4,
                loser_registration_id = $5,
                completed_at = $6,
                status = 'completed',
                updated_at = $6
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(participant1_score)
        .bind(participant2_score)
        .bind(winner_id.as_uuid())
        .bind(loser_id.as_uuid())
        .bind(now)
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentMatch::from(row))
    }

    /// Assign a participant to a match slot within a transaction.
    pub async fn assign_participant_in_tx(
        tx: &mut DbTransaction<'_>,
        id: TournamentMatchId,
        slot: ParticipantSlot,
        registration_id: TournamentRegistrationId,
        name: String,
        logo_url: Option<String>,
        seed: Option<i32>,
    ) -> Result<TournamentMatch, DomainError> {
        let now = Utc::now();

        let row = match slot {
            ParticipantSlot::One => {
                sqlx::query_as::<_, TournamentMatchRow>(
                    r"
                    UPDATE tournament_matches SET
                        participant1_registration_id = $2,
                        participant1_name = $3,
                        participant1_logo_url = $4,
                        participant1_seed = $5,
                        updated_at = $6
                    WHERE id = $1
                    RETURNING *
                    ",
                )
                .bind(id.as_uuid())
                .bind(registration_id.as_uuid())
                .bind(&name)
                .bind(&logo_url)
                .bind(seed)
                .bind(now)
                .fetch_one(&mut **tx)
                .await
            }
            ParticipantSlot::Two => {
                sqlx::query_as::<_, TournamentMatchRow>(
                    r"
                    UPDATE tournament_matches SET
                        participant2_registration_id = $2,
                        participant2_name = $3,
                        participant2_logo_url = $4,
                        participant2_seed = $5,
                        updated_at = $6
                    WHERE id = $1
                    RETURNING *
                    ",
                )
                .bind(id.as_uuid())
                .bind(registration_id.as_uuid())
                .bind(&name)
                .bind(&logo_url)
                .bind(seed)
                .bind(now)
                .fetch_one(&mut **tx)
                .await
            }
        }
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(TournamentMatch::from(row))
    }

    /// List matches by bracket within a transaction.
    pub async fn list_by_bracket_in_tx(
        tx: &mut DbTransaction<'_>,
        bracket_id: TournamentBracketId,
    ) -> Result<Vec<TournamentMatch>, DomainError> {
        let rows = sqlx::query_as::<_, TournamentMatchRow>(
            r"
            SELECT * FROM tournament_matches
            WHERE bracket_id = $1
            ORDER BY round ASC, match_number ASC
            ",
        )
        .bind(bracket_id.as_uuid())
        .fetch_all(&mut **tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(TournamentMatch::from).collect())
    }
}
