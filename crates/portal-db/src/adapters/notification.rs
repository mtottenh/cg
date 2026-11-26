//! PostgreSQL implementation of the notification repository.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use portal_core::errors::DomainError;
use portal_core::ids::{
    NotificationId, TournamentId, TournamentMatchId, TournamentRegistrationId, UserId,
};
use portal_domain::entities::notification::{
    Audience, CheckInReminderCandidate, MatchNotificationContext, NewNotification, Notification,
    TournamentNotificationSettings,
};
use portal_domain::repositories::NotificationRepository;
use uuid::Uuid;

use crate::DbPool;
use crate::entities::notification::{
    CheckInReminderRow, MatchNotificationContextRow, NotificationRow,
    TournamentNotificationSettingsRow,
};

/// PostgreSQL notification repository.
#[derive(Clone)]
pub struct PgNotificationRepository {
    pool: DbPool,
}

impl PgNotificationRepository {
    /// Create the repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

fn db(e: &sqlx::Error) -> DomainError {
    DomainError::Internal(e.to_string())
}

impl From<NotificationRow> for Notification {
    fn from(r: NotificationRow) -> Self {
        Self {
            id: NotificationId::from_uuid(r.id),
            user_id: UserId::from_uuid(r.user_id),
            kind: r.kind,
            title: r.title,
            body: r.body,
            link: r.link,
            tournament_id: r.tournament_id.map(TournamentId::from_uuid),
            match_id: r.match_id.map(TournamentMatchId::from_uuid),
            created_at: r.created_at,
            read_at: r.read_at,
        }
    }
}

impl From<TournamentNotificationSettingsRow> for TournamentNotificationSettings {
    fn from(r: TournamentNotificationSettingsRow) -> Self {
        Self {
            tournament_id: TournamentId::from_uuid(r.tournament_id),
            discord_webhook_url: r.discord_webhook_url,
            discord_enabled: r.discord_enabled,
            updated_at: r.updated_at,
            updated_by: r.updated_by.map(UserId::from_uuid),
        }
    }
}

#[async_trait]
impl NotificationRepository for PgNotificationRepository {
    async fn match_context(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Option<MatchNotificationContext>, DomainError> {
        let row = sqlx::query_as::<_, MatchNotificationContextRow>(
            r"
            SELECT tm.id AS match_id, tm.tournament_id, t.name AS tournament_name,
                   t.slug AS tournament_slug, tm.round,
                   tm.participant1_registration_id, tm.participant1_name,
                   tm.participant2_registration_id, tm.participant2_name,
                   tm.participant1_score, tm.participant2_score, tm.winner_registration_id,
                   tm.status
            FROM tournament_matches tm
            JOIN tournaments t ON t.id = tm.tournament_id
            WHERE tm.id = $1
            ",
        )
        .bind(match_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| db(&e))?;
        Ok(row.map(|r| MatchNotificationContext {
            match_id: TournamentMatchId::from_uuid(r.match_id),
            tournament_id: TournamentId::from_uuid(r.tournament_id),
            tournament_name: r.tournament_name,
            tournament_slug: r.tournament_slug,
            round: r.round,
            participant1_registration_id: r
                .participant1_registration_id
                .map(TournamentRegistrationId::from_uuid),
            participant1_name: r.participant1_name,
            participant2_registration_id: r
                .participant2_registration_id
                .map(TournamentRegistrationId::from_uuid),
            participant2_name: r.participant2_name,
            participant1_score: r.participant1_score,
            participant2_score: r.participant2_score,
            winner_registration_id: r
                .winner_registration_id
                .map(TournamentRegistrationId::from_uuid),
            status: r.status,
        }))
    }

    async fn recipients(
        &self,
        registration_ids: &[TournamentRegistrationId],
        audience: Audience,
    ) -> Result<Vec<UserId>, DomainError> {
        if registration_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<Uuid> = registration_ids
            .iter()
            .map(TournamentRegistrationId::as_uuid)
            .collect();
        let schedulers = audience == Audience::Schedulers;
        // Schedulers are the people who may act for the registration (the
        // same rule as match check-in and scheduling): the registered player,
        // or the team's captain, owner and active delegates. The roster is
        // the registered player, or every active member of the team.
        let users = sqlx::query_scalar::<_, Uuid>(
            r"
            WITH regs AS (
                SELECT tr.id, tr.tournament_id, tr.player_id, tr.team_season_id
                FROM tournament_registrations tr
                WHERE tr.id = ANY($1)
            ),
            people AS (
                SELECT r.player_id AS player_id FROM regs r WHERE r.player_id IS NOT NULL
                UNION
                SELECT ltm.player_id
                FROM regs r
                JOIN league_team_members ltm
                  ON ltm.team_season_id = r.team_season_id AND ltm.left_at IS NULL
                WHERE NOT $2 OR ltm.role = 'captain'
                UNION
                SELECT lt.owner_player_id
                FROM regs r
                JOIN league_team_seasons lts ON lts.id = r.team_season_id
                JOIN league_teams lt ON lt.id = lts.team_id
                WHERE $2 AND lt.owner_player_id IS NOT NULL
                UNION
                SELECT vd.player_id
                FROM regs r
                JOIN veto_delegates vd
                  ON vd.team_season_id = r.team_season_id
                 AND vd.revoked_at IS NULL
                 AND (vd.tournament_id IS NULL OR vd.tournament_id = r.tournament_id)
                WHERE $2
            )
            SELECT DISTINCT p.user_id
            FROM people pe
            JOIN players p ON p.id = pe.player_id
            ",
        )
        .bind(&ids)
        .bind(schedulers)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| db(&e))?;
        Ok(users.into_iter().map(UserId::from_uuid).collect())
    }

    async fn insert_for_users(
        &self,
        user_ids: &[UserId],
        n: &NewNotification,
    ) -> Result<Vec<UserId>, DomainError> {
        if user_ids.is_empty() {
            return Ok(Vec::new());
        }
        let users: Vec<Uuid> = user_ids.iter().map(UserId::as_uuid).collect();
        let ids: Vec<Uuid> = users.iter().map(|_| Uuid::now_v7()).collect();
        let written = sqlx::query_scalar::<_, Uuid>(
            r"
            INSERT INTO notifications
                (id, user_id, kind, title, body, link, tournament_id, match_id, dedupe_key)
            SELECT x.id, x.user_id, $3, $4, $5, $6, $7, $8, $9
            FROM UNNEST($1::uuid[], $2::uuid[]) AS x(id, user_id)
            ON CONFLICT (user_id, dedupe_key) WHERE dedupe_key IS NOT NULL DO NOTHING
            RETURNING user_id
            ",
        )
        .bind(&ids)
        .bind(&users)
        .bind(n.kind.as_str())
        .bind(&n.title)
        .bind(&n.body)
        .bind(&n.link)
        .bind(n.tournament_id.map(|t| t.as_uuid()))
        .bind(n.match_id.map(|m| m.as_uuid()))
        .bind(&n.dedupe_key)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| db(&e))?;
        Ok(written.into_iter().map(UserId::from_uuid).collect())
    }

    async fn list_for_user(
        &self,
        user_id: UserId,
        unread_only: bool,
        limit: i64,
        before: Option<NotificationId>,
    ) -> Result<Vec<Notification>, DomainError> {
        // `before` is a cursor into the user's own list; an id that is not
        // theirs matches nothing, so the page is empty rather than another
        // user's rows.
        let rows = sqlx::query_as::<_, NotificationRow>(
            r"
            SELECT * FROM notifications n
            WHERE n.user_id = $1
              AND (NOT $2 OR n.read_at IS NULL)
              AND (
                  $4::uuid IS NULL
                  OR (n.created_at, n.id) < (
                      SELECT c.created_at, c.id FROM notifications c
                      WHERE c.id = $4 AND c.user_id = $1
                  )
              )
            ORDER BY n.created_at DESC, n.id DESC
            LIMIT $3
            ",
        )
        .bind(user_id.as_uuid())
        .bind(unread_only)
        .bind(limit)
        .bind(before.map(|b| b.as_uuid()))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| db(&e))?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn unread_count(
        &self,
        user_id: UserId,
    ) -> Result<(i64, Option<NotificationId>), DomainError> {
        let (count, latest): (i64, Option<Uuid>) = sqlx::query_as(
            r"
            SELECT
                (SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND read_at IS NULL),
                (SELECT id FROM notifications WHERE user_id = $1 AND read_at IS NULL
                 ORDER BY created_at DESC, id DESC LIMIT 1)
            ",
        )
        .bind(user_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| db(&e))?;
        Ok((count, latest.map(NotificationId::from_uuid)))
    }

    async fn mark_read(
        &self,
        user_id: UserId,
        ids: Option<&[NotificationId]>,
        now: DateTime<Utc>,
    ) -> Result<u64, DomainError> {
        let ids: Option<Vec<Uuid>> =
            ids.map(|ids| ids.iter().map(NotificationId::as_uuid).collect());
        let done = sqlx::query(
            r"
            UPDATE notifications SET read_at = $3
            WHERE user_id = $1 AND read_at IS NULL
              AND ($2::uuid[] IS NULL OR id = ANY($2))
            ",
        )
        .bind(user_id.as_uuid())
        .bind(ids)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| db(&e))?;
        Ok(done.rows_affected())
    }

    async fn delete_expired(
        &self,
        read_before: DateTime<Utc>,
        any_before: DateTime<Utc>,
    ) -> Result<u64, DomainError> {
        let done = sqlx::query(
            r"
            DELETE FROM notifications
            WHERE (read_at IS NOT NULL AND created_at < $1) OR created_at < $2
            ",
        )
        .bind(read_before)
        .bind(any_before)
        .execute(&self.pool)
        .await
        .map_err(|e| db(&e))?;
        Ok(done.rows_affected())
    }

    async fn check_in_reminder_candidates(
        &self,
        now: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> Result<Vec<CheckInReminderCandidate>, DomainError> {
        let rows = sqlx::query_as::<_, CheckInReminderRow>(
            r"
            SELECT tm.id AS match_id, side.registration_id, tm.check_in_deadline AS deadline
            FROM tournament_matches tm
            CROSS JOIN LATERAL (VALUES
                (tm.participant1_registration_id, tm.participant1_checked_in_at),
                (tm.participant2_registration_id, tm.participant2_checked_in_at)
            ) AS side(registration_id, checked_in_at)
            WHERE tm.status = 'checking_in'
              AND tm.check_in_deadline > $1
              AND tm.check_in_deadline <= $2
              AND side.registration_id IS NOT NULL
              AND side.checked_in_at IS NULL
            ",
        )
        .bind(now)
        .bind(until)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| db(&e))?;
        Ok(rows
            .into_iter()
            .map(|r| CheckInReminderCandidate {
                match_id: TournamentMatchId::from_uuid(r.match_id),
                registration_id: TournamentRegistrationId::from_uuid(r.registration_id),
                deadline: r.deadline,
            })
            .collect())
    }

    async fn get_settings(
        &self,
        tournament_id: TournamentId,
    ) -> Result<Option<TournamentNotificationSettings>, DomainError> {
        let row = sqlx::query_as::<_, TournamentNotificationSettingsRow>(
            "SELECT * FROM tournament_notification_settings WHERE tournament_id = $1",
        )
        .bind(tournament_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| db(&e))?;
        Ok(row.map(Into::into))
    }

    async fn upsert_settings(
        &self,
        tournament_id: TournamentId,
        discord_enabled: bool,
        webhook: Option<Option<String>>,
        updated_by: UserId,
    ) -> Result<TournamentNotificationSettings, DomainError> {
        let set_url = webhook.is_some();
        let url = webhook.flatten();
        let row = sqlx::query_as::<_, TournamentNotificationSettingsRow>(
            r"
            INSERT INTO tournament_notification_settings
                (tournament_id, discord_webhook_url, discord_enabled, updated_at, updated_by)
            VALUES ($1, $2, $3, NOW(), $4)
            ON CONFLICT (tournament_id) DO UPDATE SET
                discord_enabled = EXCLUDED.discord_enabled,
                discord_webhook_url = CASE WHEN $5
                    THEN EXCLUDED.discord_webhook_url
                    ELSE tournament_notification_settings.discord_webhook_url END,
                updated_at = NOW(),
                updated_by = EXCLUDED.updated_by
            RETURNING *
            ",
        )
        .bind(tournament_id.as_uuid())
        .bind(url)
        .bind(discord_enabled)
        .bind(updated_by.as_uuid())
        .bind(set_url)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| db(&e))?;
        Ok(row.into())
    }
}
