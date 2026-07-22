//! Availability PostgreSQL repository implementations.

use async_trait::async_trait;
use chrono::NaiveDate;
use chrono::NaiveTime;
use portal_core::{
    AvailabilityExceptionId, AvailabilityWindowId, DomainError, PlayerId, SuggestedTimeId,
    TournamentMatchId, TournamentRegistrationId,
};
use portal_domain::entities::{
    AvailabilityOverride, AvailabilityWindow, CreateAvailabilityOverride, CreateAvailabilityWindow,
    CreateSuggestedTime, SuggestedTime, UpdateAvailabilityWindow,
};
use portal_domain::repositories::{
    AvailabilityOverrideRepository, AvailabilityWindowRepository, SuggestedTimeRepository,
};

use crate::DbPool;
use crate::entities::{AvailabilityOverrideRow, AvailabilityWindowRow, SuggestedTimeRow};

// =============================================================================
// AVAILABILITY WINDOW REPOSITORY
// =============================================================================

/// PostgreSQL implementation of `AvailabilityWindowRepository`.
#[derive(Clone)]
pub struct PgAvailabilityWindowRepository {
    pool: DbPool,
}

impl PgAvailabilityWindowRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AvailabilityWindowRepository for PgAvailabilityWindowRepository {
    async fn find_by_id(
        &self,
        id: AvailabilityWindowId,
    ) -> Result<Option<AvailabilityWindow>, DomainError> {
        let row = sqlx::query_as::<_, AvailabilityWindowRow>(
            "SELECT * FROM availability_windows WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Database error: {e}")))?;

        Ok(row.map(Into::into))
    }

    async fn find_by_player_id(
        &self,
        player_id: PlayerId,
    ) -> Result<Vec<AvailabilityWindow>, DomainError> {
        let rows = sqlx::query_as::<_, AvailabilityWindowRow>(
            "SELECT * FROM availability_windows WHERE player_id = $1 ORDER BY day_of_week, start_time",
        )
        .bind(player_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Database error: {e}")))?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn find_by_registration_id(
        &self,
        registration_id: TournamentRegistrationId,
    ) -> Result<Vec<AvailabilityWindow>, DomainError> {
        let rows = sqlx::query_as::<_, AvailabilityWindowRow>(
            "SELECT * FROM availability_windows WHERE registration_id = $1 ORDER BY day_of_week, start_time",
        )
        .bind(registration_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Database error: {e}")))?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn find_by_player_and_day(
        &self,
        player_id: PlayerId,
        day_of_week: u8,
    ) -> Result<Vec<AvailabilityWindow>, DomainError> {
        let rows = sqlx::query_as::<_, AvailabilityWindowRow>(
            "SELECT * FROM availability_windows WHERE player_id = $1 AND day_of_week = $2 ORDER BY start_time",
        )
        .bind(player_id.as_uuid())
        .bind(i16::from(day_of_week))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Database error: {e}")))?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn find_by_registration_and_day(
        &self,
        registration_id: TournamentRegistrationId,
        day_of_week: u8,
    ) -> Result<Vec<AvailabilityWindow>, DomainError> {
        let rows = sqlx::query_as::<_, AvailabilityWindowRow>(
            "SELECT * FROM availability_windows WHERE registration_id = $1 AND day_of_week = $2 ORDER BY start_time",
        )
        .bind(registration_id.as_uuid())
        .bind(i16::from(day_of_week))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Database error: {e}")))?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn create(
        &self,
        command: CreateAvailabilityWindow,
    ) -> Result<AvailabilityWindow, DomainError> {
        let row = sqlx::query_as::<_, AvailabilityWindowRow>(
            r"
            INSERT INTO availability_windows (
                player_id, registration_id, day_of_week, start_time, end_time,
                timezone, is_preferred, notes
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING *
            ",
        )
        .bind(command.player_id.map(|id| id.as_uuid()))
        .bind(command.registration_id.map(|id| id.as_uuid()))
        .bind(i16::from(command.day_of_week))
        .bind(command.start_time)
        .bind(command.end_time)
        .bind(&command.timezone)
        .bind(command.is_preferred)
        .bind(&command.notes)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Database error: {e}")))?;

        Ok(row.into())
    }

    async fn update(
        &self,
        id: AvailabilityWindowId,
        command: UpdateAvailabilityWindow,
    ) -> Result<AvailabilityWindow, DomainError> {
        let row = sqlx::query_as::<_, AvailabilityWindowRow>(
            r"
            UPDATE availability_windows
            SET day_of_week = COALESCE($2, day_of_week),
                start_time = COALESCE($3, start_time),
                end_time = COALESCE($4, end_time),
                timezone = COALESCE($5, timezone),
                is_preferred = COALESCE($6, is_preferred),
                notes = COALESCE($7, notes)
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(command.day_of_week.map(i16::from))
        .bind(command.start_time)
        .bind(command.end_time)
        .bind(command.timezone.flatten())
        .bind(command.is_preferred)
        .bind(command.notes.flatten())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Database error: {e}")))?;

        Ok(row.into())
    }

    async fn delete(&self, id: AvailabilityWindowId) -> Result<bool, DomainError> {
        let result = sqlx::query("DELETE FROM availability_windows WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(format!("Database error: {e}")))?;

        Ok(result.rows_affected() > 0)
    }

    async fn delete_by_player_id(&self, player_id: PlayerId) -> Result<u64, DomainError> {
        let result = sqlx::query("DELETE FROM availability_windows WHERE player_id = $1")
            .bind(player_id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(format!("Database error: {e}")))?;

        Ok(result.rows_affected())
    }

    async fn delete_by_registration_id(
        &self,
        registration_id: TournamentRegistrationId,
    ) -> Result<u64, DomainError> {
        let result = sqlx::query("DELETE FROM availability_windows WHERE registration_id = $1")
            .bind(registration_id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(format!("Database error: {e}")))?;

        Ok(result.rows_affected())
    }

    async fn exists(
        &self,
        player_id: Option<PlayerId>,
        registration_id: Option<TournamentRegistrationId>,
        day_of_week: u8,
        start_time: NaiveTime,
        end_time: NaiveTime,
    ) -> Result<bool, DomainError> {
        let exists: bool = sqlx::query_scalar(
            r"
            SELECT EXISTS(
                SELECT 1 FROM availability_windows
                WHERE (player_id = $1 OR registration_id = $2)
                    AND day_of_week = $3
                    AND start_time = $4
                    AND end_time = $5
            )
            ",
        )
        .bind(player_id.map(|id| id.as_uuid()))
        .bind(registration_id.map(|id| id.as_uuid()))
        .bind(i16::from(day_of_week))
        .bind(start_time)
        .bind(end_time)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Database error: {e}")))?;

        Ok(exists)
    }
}

// =============================================================================
// AVAILABILITY OVERRIDE REPOSITORY
// =============================================================================

/// PostgreSQL implementation of `AvailabilityOverrideRepository`.
#[derive(Clone)]
pub struct PgAvailabilityOverrideRepository {
    pool: DbPool,
}

impl PgAvailabilityOverrideRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AvailabilityOverrideRepository for PgAvailabilityOverrideRepository {
    async fn find_by_id(
        &self,
        id: AvailabilityExceptionId,
    ) -> Result<Option<AvailabilityOverride>, DomainError> {
        let row = sqlx::query_as::<_, AvailabilityOverrideRow>(
            "SELECT * FROM availability_overrides WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Database error: {e}")))?;

        row.map(TryInto::try_into)
            .transpose()
            .map_err(|e: String| DomainError::Internal(e))
    }

    async fn find_by_player_id(
        &self,
        player_id: PlayerId,
    ) -> Result<Vec<AvailabilityOverride>, DomainError> {
        let rows = sqlx::query_as::<_, AvailabilityOverrideRow>(
            "SELECT * FROM availability_overrides WHERE player_id = $1 ORDER BY override_date",
        )
        .bind(player_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Database error: {e}")))?;

        rows.into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e: String| DomainError::Internal(e))
    }

    async fn find_by_registration_id(
        &self,
        registration_id: TournamentRegistrationId,
    ) -> Result<Vec<AvailabilityOverride>, DomainError> {
        let rows = sqlx::query_as::<_, AvailabilityOverrideRow>(
            "SELECT * FROM availability_overrides WHERE registration_id = $1 ORDER BY override_date",
        )
        .bind(registration_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Database error: {e}")))?;

        rows.into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e: String| DomainError::Internal(e))
    }

    async fn find_by_player_id_and_date_range(
        &self,
        player_id: PlayerId,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<Vec<AvailabilityOverride>, DomainError> {
        let rows = sqlx::query_as::<_, AvailabilityOverrideRow>(
            r"
            SELECT * FROM availability_overrides
            WHERE player_id = $1 AND override_date BETWEEN $2 AND $3
            ORDER BY override_date
            ",
        )
        .bind(player_id.as_uuid())
        .bind(start_date)
        .bind(end_date)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Database error: {e}")))?;

        rows.into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e: String| DomainError::Internal(e))
    }

    async fn find_by_registration_id_and_date_range(
        &self,
        registration_id: TournamentRegistrationId,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Result<Vec<AvailabilityOverride>, DomainError> {
        let rows = sqlx::query_as::<_, AvailabilityOverrideRow>(
            r"
            SELECT * FROM availability_overrides
            WHERE registration_id = $1 AND override_date BETWEEN $2 AND $3
            ORDER BY override_date
            ",
        )
        .bind(registration_id.as_uuid())
        .bind(start_date)
        .bind(end_date)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Database error: {e}")))?;

        rows.into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e: String| DomainError::Internal(e))
    }

    async fn create(
        &self,
        command: CreateAvailabilityOverride,
    ) -> Result<AvailabilityOverride, DomainError> {
        let row = sqlx::query_as::<_, AvailabilityOverrideRow>(
            r"
            INSERT INTO availability_overrides (
                player_id, registration_id, override_date, start_time, end_time,
                override_type, reason
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
            ",
        )
        .bind(command.player_id.map(|id| id.as_uuid()))
        .bind(command.registration_id.map(|id| id.as_uuid()))
        .bind(command.override_date)
        .bind(command.start_time)
        .bind(command.end_time)
        .bind(command.override_type.to_string())
        .bind(&command.reason)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Database error: {e}")))?;

        row.try_into().map_err(|e: String| DomainError::Internal(e))
    }

    async fn delete(&self, id: AvailabilityExceptionId) -> Result<bool, DomainError> {
        let result = sqlx::query("DELETE FROM availability_overrides WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(format!("Database error: {e}")))?;

        Ok(result.rows_affected() > 0)
    }

    async fn delete_by_player_id(&self, player_id: PlayerId) -> Result<u64, DomainError> {
        let result = sqlx::query("DELETE FROM availability_overrides WHERE player_id = $1")
            .bind(player_id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(format!("Database error: {e}")))?;

        Ok(result.rows_affected())
    }

    async fn delete_by_registration_id(
        &self,
        registration_id: TournamentRegistrationId,
    ) -> Result<u64, DomainError> {
        let result = sqlx::query("DELETE FROM availability_overrides WHERE registration_id = $1")
            .bind(registration_id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(format!("Database error: {e}")))?;

        Ok(result.rows_affected())
    }

    async fn delete_expired(&self, before_date: NaiveDate) -> Result<u64, DomainError> {
        let result = sqlx::query("DELETE FROM availability_overrides WHERE override_date < $1")
            .bind(before_date)
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(format!("Database error: {e}")))?;

        Ok(result.rows_affected())
    }
}

// =============================================================================
// SUGGESTED TIME REPOSITORY
// =============================================================================

/// PostgreSQL implementation of `SuggestedTimeRepository`.
#[derive(Clone)]
pub struct PgSuggestedTimeRepository {
    pool: DbPool,
}

impl PgSuggestedTimeRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SuggestedTimeRepository for PgSuggestedTimeRepository {
    async fn find_by_id(&self, id: SuggestedTimeId) -> Result<Option<SuggestedTime>, DomainError> {
        let row =
            sqlx::query_as::<_, SuggestedTimeRow>("SELECT * FROM suggested_times WHERE id = $1")
                .bind(id.as_uuid())
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| DomainError::Internal(format!("Database error: {e}")))?;

        row.map(TryInto::try_into)
            .transpose()
            .map_err(|e: String| DomainError::Internal(e))
    }

    async fn find_by_match_id(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<SuggestedTime>, DomainError> {
        let rows = sqlx::query_as::<_, SuggestedTimeRow>(
            "SELECT * FROM suggested_times WHERE match_id = $1 ORDER BY confidence_score DESC",
        )
        .bind(match_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Database error: {e}")))?;

        rows.into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e: String| DomainError::Internal(e))
    }

    async fn find_active_by_match_id(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<SuggestedTime>, DomainError> {
        let rows = sqlx::query_as::<_, SuggestedTimeRow>(
            r"
            SELECT * FROM suggested_times
            WHERE match_id = $1 AND status = 'suggested'
            ORDER BY confidence_score DESC
            ",
        )
        .bind(match_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Database error: {e}")))?;

        rows.into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e: String| DomainError::Internal(e))
    }

    async fn create(&self, command: CreateSuggestedTime) -> Result<SuggestedTime, DomainError> {
        let row = sqlx::query_as::<_, SuggestedTimeRow>(
            r"
            INSERT INTO suggested_times (
                match_id, suggested_start, suggested_end, confidence_score,
                is_mutual_overlap, is_auto_generated
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            ",
        )
        .bind(command.match_id.as_uuid())
        .bind(command.suggested_start)
        .bind(command.suggested_end)
        .bind(command.confidence_score)
        .bind(command.is_mutual_overlap)
        .bind(command.is_auto_generated)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Database error: {e}")))?;

        row.try_into().map_err(|e: String| DomainError::Internal(e))
    }

    async fn accept(&self, id: SuggestedTimeId) -> Result<SuggestedTime, DomainError> {
        let row = sqlx::query_as::<_, SuggestedTimeRow>(
            "UPDATE suggested_times SET status = 'accepted' WHERE id = $1 RETURNING *",
        )
        .bind(id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Database error: {e}")))?;

        row.try_into().map_err(|e: String| DomainError::Internal(e))
    }

    async fn reject(&self, id: SuggestedTimeId) -> Result<SuggestedTime, DomainError> {
        let row = sqlx::query_as::<_, SuggestedTimeRow>(
            "UPDATE suggested_times SET status = 'rejected' WHERE id = $1 RETURNING *",
        )
        .bind(id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Database error: {e}")))?;

        row.try_into().map_err(|e: String| DomainError::Internal(e))
    }

    async fn expire(&self, id: SuggestedTimeId) -> Result<SuggestedTime, DomainError> {
        let row = sqlx::query_as::<_, SuggestedTimeRow>(
            "UPDATE suggested_times SET status = 'expired' WHERE id = $1 RETURNING *",
        )
        .bind(id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Database error: {e}")))?;

        row.try_into().map_err(|e: String| DomainError::Internal(e))
    }

    async fn delete_by_match_id(&self, match_id: TournamentMatchId) -> Result<u64, DomainError> {
        let result = sqlx::query("DELETE FROM suggested_times WHERE match_id = $1")
            .bind(match_id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(format!("Database error: {e}")))?;

        Ok(result.rows_affected())
    }

    async fn delete_auto_generated_pending(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<u64, DomainError> {
        let result = sqlx::query(
            r"
            DELETE FROM suggested_times
            WHERE match_id = $1
              AND is_auto_generated = true
              AND status = 'suggested'
            ",
        )
        .bind(match_id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Database error: {e}")))?;

        Ok(result.rows_affected())
    }

    async fn reject_all_pending(&self, match_id: TournamentMatchId) -> Result<u64, DomainError> {
        let result = sqlx::query(
            r"
            UPDATE suggested_times
            SET status = 'rejected'
            WHERE match_id = $1 AND status = 'suggested'
            ",
        )
        .bind(match_id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(format!("Database error: {e}")))?;

        Ok(result.rows_affected())
    }
}
