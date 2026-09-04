//! `PostgreSQL` implementation of `LeagueTeamRepository`.

use async_trait::async_trait;
use chrono::Utc;

use crate::DbPool;
use crate::entities::league_team::{LeagueTeamRow, LeagueTeamSeasonRow};
use portal_core::types::{LeagueTeamRole, LeagueTeamStatus};
use portal_core::{DomainError, LeagueId, LeagueSeasonId, LeagueTeamId, PlayerId, UserId};
use portal_domain::entities::league_team::{LeagueTeam, LeagueTeamSeason};
use portal_domain::repositories::league_team::{
    CreateLeagueTeam, LeagueTeamListFilter, LeagueTeamRepository, UpdateLeagueTeam,
};

/// `PostgreSQL` implementation of `LeagueTeamRepository`.
#[derive(Debug, Clone)]
pub struct PgLeagueTeamRepository {
    pool: DbPool,
}

impl PgLeagueTeamRepository {
    /// Create a new repository instance.
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    fn normalize_name(name: &str) -> String {
        name.to_lowercase().trim().to_string()
    }
}

#[async_trait]
impl LeagueTeamRepository for PgLeagueTeamRepository {
    async fn find_by_id(&self, id: LeagueTeamId) -> Result<Option<LeagueTeam>, DomainError> {
        let row = sqlx::query_as::<_, LeagueTeamRow>("SELECT * FROM league_teams WHERE id = $1")
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(LeagueTeam::from))
    }

    async fn find_by_name(
        &self,
        league_id: LeagueId,
        name: &str,
    ) -> Result<Option<LeagueTeam>, DomainError> {
        let normalized = Self::normalize_name(name);
        let row = sqlx::query_as::<_, LeagueTeamRow>(
            "SELECT * FROM league_teams WHERE league_id = $1 AND name_normalized = $2",
        )
        .bind(league_id.as_uuid())
        .bind(&normalized)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(LeagueTeam::from))
    }

    async fn find_by_tag(
        &self,
        league_id: LeagueId,
        tag: &str,
    ) -> Result<Option<LeagueTeam>, DomainError> {
        let normalized = Self::normalize_name(tag);
        let row = sqlx::query_as::<_, LeagueTeamRow>(
            "SELECT * FROM league_teams WHERE league_id = $1 AND tag_normalized = $2",
        )
        .bind(league_id.as_uuid())
        .bind(&normalized)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(LeagueTeam::from))
    }

    async fn create(&self, cmd: CreateLeagueTeam) -> Result<LeagueTeam, DomainError> {
        let id = uuid::Uuid::now_v7();
        let now = Utc::now();

        // Note: name_normalized and tag_normalized are GENERATED columns
        let row = sqlx::query_as::<_, LeagueTeamRow>(
            r"
            INSERT INTO league_teams (
                id, league_id, name, tag,
                description, logo_url, primary_color, secondary_color,
                owner_player_id, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING *
            ",
        )
        .bind(id)
        .bind(cmd.league_id.as_uuid())
        .bind(&cmd.name)
        .bind(&cmd.tag)
        .bind(&cmd.description)
        .bind(&cmd.logo_url)
        .bind(&cmd.primary_color)
        .bind(&cmd.secondary_color)
        .bind(cmd.owner_player_id.as_uuid())
        .bind(now)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(LeagueTeam::from(row))
    }

    async fn update(
        &self,
        id: LeagueTeamId,
        update: UpdateLeagueTeam,
    ) -> Result<LeagueTeam, DomainError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, LeagueTeamRow>(
            r"
            UPDATE league_teams SET
                name = COALESCE($2, name),
                tag = COALESCE($3, tag),
                description = COALESCE($4, description),
                logo_url = COALESCE($5, logo_url),
                banner_url = COALESCE($6, banner_url),
                primary_color = COALESCE($7, primary_color),
                secondary_color = COALESCE($8, secondary_color),
                updated_at = $9
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(&update.name)
        .bind(&update.tag)
        .bind(&update.description)
        .bind(&update.logo_url)
        .bind(&update.banner_url)
        .bind(&update.primary_color)
        .bind(&update.secondary_color)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(LeagueTeam::from(row))
    }

    async fn list_by_league(
        &self,
        league_id: LeagueId,
        filter: LeagueTeamListFilter,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<LeagueTeam>, i64), DomainError> {
        // One statement with nullable filters, not a branch per combination.
        // The four-way branch this replaces had a count that applied none of
        // the filters, so a filtered page reported the whole league's total
        // and paginated against a length it did not have.
        const WHERE: &str = r"
            WHERE league_id = $1
              AND ($2::text IS NULL OR status = $2)
              AND ($3::text IS NULL OR name ILIKE $3 OR tag ILIKE $3)
              AND ($4::bool IS TRUE OR archived_at IS NULL)
        ";

        let search_pattern = filter.search.as_ref().map(|s| format!("%{s}%"));
        let status = filter.status.map(|s| s.to_string());

        let rows = sqlx::query_as::<_, LeagueTeamRow>(&format!(
            "SELECT * FROM league_teams {WHERE} ORDER BY name ASC LIMIT $5 OFFSET $6"
        ))
        .bind(league_id.as_uuid())
        .bind(&status)
        .bind(&search_pattern)
        .bind(filter.include_archived)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let count: (i64,) = sqlx::query_as(&format!("SELECT COUNT(*) FROM league_teams {WHERE}"))
            .bind(league_id.as_uuid())
            .bind(&status)
            .bind(&search_pattern)
            .bind(filter.include_archived)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok((rows.into_iter().map(LeagueTeam::from).collect(), count.0))
    }

    async fn set_archived(
        &self,
        id: LeagueTeamId,
        archived_by: Option<UserId>,
    ) -> Result<LeagueTeam, DomainError> {
        let row = sqlx::query_as::<_, LeagueTeamRow>(
            r"
            UPDATE league_teams
            SET archived_at = CASE WHEN $2::uuid IS NULL THEN NULL ELSE NOW() END,
                archived_by = $2::uuid,
                updated_at  = NOW()
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(archived_by.map(|u| u.as_uuid()))
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?
        .ok_or(DomainError::LeagueTeamNotFound(id))?;

        Ok(LeagueTeam::from(row))
    }

    async fn move_to_league(
        &self,
        id: LeagueTeamId,
        target_league_id: LeagueId,
        target_season_id: LeagueSeasonId,
    ) -> Result<(LeagueTeam, LeagueTeamSeason), DomainError> {
        // One transaction: a half-moved team — new league, old season
        // registrations — is a team registered in a league it does not
        // belong to, which no listing would show and no admin could fix.
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let now = Utc::now();

        // `tournament_registrations.team_season_id` cascades on delete, so
        // dropping the old registrations below would take a tournament entry
        // with them without a word. Checked inside the transaction, because a
        // check outside it could be raced by a registration landing between
        // the look and the delete.
        let (tournament_entries,): (i64,) = sqlx::query_as(
            r"
            SELECT COUNT(*)
            FROM tournament_registrations tr
            JOIN league_team_seasons lts ON lts.id = tr.team_season_id
            WHERE lts.team_id = $1
            ",
        )
        .bind(id.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        if tournament_entries > 0 {
            return Err(DomainError::InvalidState(
                "team is registered for a tournament in its current league; move refused rather \
                 than deleting that entry"
                    .to_string(),
            ));
        }

        let team_row = sqlx::query_as::<_, LeagueTeamRow>(
            r"
            UPDATE league_teams
            SET league_id = $2, updated_at = $3
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(target_league_id.as_uuid())
        .bind(now)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?
        .ok_or(DomainError::LeagueTeamNotFound(id))?;

        // The roster to carry: the team's most recent registration. Older
        // ones are historical and are being dropped with the old league.
        let source_team_season: Option<(uuid::Uuid,)> = sqlx::query_as(
            r"
            SELECT id FROM league_team_seasons
            WHERE team_id = $1
            ORDER BY created_at DESC
            LIMIT 1
            ",
        )
        .bind(id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let team_season_row = sqlx::query_as::<_, LeagueTeamSeasonRow>(
            r"
            INSERT INTO league_team_seasons (team_id, season_id, status, registered_at)
            VALUES ($1, $2, 'registered', $3)
            ON CONFLICT (team_id, season_id) DO UPDATE SET updated_at = NOW()
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(target_season_id.as_uuid())
        .bind(now)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        if let Some((source_id,)) = source_team_season
            && source_id != team_season_row.id
        {
            // Insert rather than re-point: `league_team_members.season_id` is
            // filled by an INSERT trigger, so an UPDATE of team_season_id
            // would leave the denormalised season behind and quietly break
            // the one-team-per-season unique index.
            sqlx::query(
                r"
                INSERT INTO league_team_members (
                    team_season_id, player_id, role, position, jersey_number,
                    status, joined_at, added_by
                )
                SELECT $1, player_id, role, position, jersey_number,
                       status, joined_at, added_by
                FROM league_team_members
                WHERE team_season_id = $2 AND status = 'active'
                ON CONFLICT (team_season_id, player_id) DO NOTHING
                ",
            )
            .bind(team_season_row.id)
            .bind(source_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        }

        // The old registrations point at seasons of a league this team has
        // left. Members and invitations cascade with them.
        sqlx::query("DELETE FROM league_team_seasons WHERE team_id = $1 AND id <> $2")
            .bind(id.as_uuid())
            .bind(team_season_row.id)
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok((
            LeagueTeam::from(team_row),
            LeagueTeamSeason::from(team_season_row),
        ))
    }

    async fn list_by_owner(
        &self,
        league_id: LeagueId,
        owner_player_id: PlayerId,
    ) -> Result<Vec<LeagueTeam>, DomainError> {
        let rows = sqlx::query_as::<_, LeagueTeamRow>(
            r"
            SELECT * FROM league_teams
            WHERE league_id = $1 AND owner_player_id = $2 AND status != 'disbanded'
            ORDER BY name ASC
            ",
        )
        .bind(league_id.as_uuid())
        .bind(owner_player_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(LeagueTeam::from).collect())
    }

    async fn update_status(
        &self,
        id: LeagueTeamId,
        status: LeagueTeamStatus,
    ) -> Result<LeagueTeam, DomainError> {
        let now = Utc::now();
        let disbanded_at = if status == LeagueTeamStatus::Disbanded {
            Some(now)
        } else {
            None
        };

        let row = sqlx::query_as::<_, LeagueTeamRow>(
            r"
            UPDATE league_teams SET
                status = $2,
                disbanded_at = COALESCE($3, disbanded_at),
                updated_at = $4
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(status.to_string())
        .bind(disbanded_at)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(LeagueTeam::from(row))
    }

    async fn transfer_ownership(
        &self,
        id: LeagueTeamId,
        new_owner_player_id: PlayerId,
    ) -> Result<LeagueTeam, DomainError> {
        // P-113. `owner_player_id` is only half of what ownership *is*. The
        // other half is the team-scoped `team_captain` RBAC grant written by
        // `create_team_with_season_and_captain` below — that grant is what
        // `require_team_settings_manage` (update_team / disband_team /
        // register_team_for_season) actually checks.
        //
        // This method used to update the column and nothing else, which split
        // ownership in two: the NEW owner 403'd on every owner-gated endpoint
        // while the OLD owner kept the power to disband a team they no longer
        // owned. Both halves therefore move here, in ONE transaction — a
        // transfer that moves one and not the other is precisely the split
        // state the finding describes.
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let now = Utc::now();

        // Read the outgoing owner under a row lock, inside the same
        // transaction that will overwrite it. Taking it from an earlier
        // `find_by_id` on another connection would let two concurrent
        // transfers revoke the same (already-stale) player twice and leave
        // the loser's grant behind.
        let previous_owner = sqlx::query_as::<_, (uuid::Uuid,)>(
            "SELECT owner_player_id FROM league_teams WHERE id = $1 FOR UPDATE",
        )
        .bind(id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let Some((previous_owner_player_id,)) = previous_owner else {
            return Err(DomainError::LeagueTeamNotFound(id));
        };

        let row = sqlx::query_as::<_, LeagueTeamRow>(
            r"
            UPDATE league_teams SET
                owner_player_id = $2,
                updated_at = $3
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(new_owner_player_id.as_uuid())
        .bind(now)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        // Revoke BEFORE granting: a transfer to the current owner (a no-op the
        // service does not forbid) would otherwise revoke the row it had just
        // written and leave the owner with no grant at all.
        //
        // Scoped to the `team_captain` role only, not every role in the team
        // scope — a player may hold other team-scoped roles for reasons that
        // have nothing to do with owning it.
        sqlx::query(
            r"
            UPDATE user_roles ur SET
                revoked_at = $1
            FROM players p, roles r
            WHERE ur.user_id = p.user_id
              AND ur.role_id = r.id
              AND r.name = 'team_captain'
              AND ur.scope_type = 'team'
              AND ur.scope_id = $2
              AND ur.revoked_at IS NULL
              AND p.id = $3
            ",
        )
        .bind(now)
        .bind(id.as_uuid())
        .bind(previous_owner_player_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        // Same INSERT...SELECT shape as team creation: a player with no linked
        // user account selects zero rows and grants nothing, which is correct
        // — there is no user to authorise.
        sqlx::query(
            r"
            INSERT INTO user_roles (user_id, role_id, scope_type, scope_id, granted_at)
            SELECT p.user_id, r.id, 'team', $1, $2
            FROM players p
            JOIN roles r ON r.name = 'team_captain'
            WHERE p.id = $3 AND p.user_id IS NOT NULL
            ON CONFLICT DO NOTHING
            ",
        )
        .bind(id.as_uuid())
        .bind(now)
        .bind(new_owner_player_id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(LeagueTeam::from(row))
    }

    async fn name_exists(&self, league_id: LeagueId, name: &str) -> Result<bool, DomainError> {
        let normalized = Self::normalize_name(name);
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM league_teams WHERE league_id = $1 AND name_normalized = $2",
        )
        .bind(league_id.as_uuid())
        .bind(&normalized)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(count.0 > 0)
    }

    async fn tag_exists(&self, league_id: LeagueId, tag: &str) -> Result<bool, DomainError> {
        let normalized = Self::normalize_name(tag);
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM league_teams WHERE league_id = $1 AND tag_normalized = $2",
        )
        .bind(league_id.as_uuid())
        .bind(&normalized)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(count.0 > 0)
    }

    async fn delete(&self, id: LeagueTeamId) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM league_teams WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn create_team_with_season_and_captain(
        &self,
        cmd: CreateLeagueTeam,
        season_id: LeagueSeasonId,
        captain_player_id: PlayerId,
    ) -> Result<(LeagueTeam, LeagueTeamSeason), DomainError> {
        // All three writes share a single transaction. Any error below
        // causes the transaction to roll back on drop, so a failed
        // team_season insert doesn't orphan the league_teams row, and a
        // failed member insert doesn't leave an empty team_season.
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let now = Utc::now();
        let team_id = uuid::Uuid::now_v7();
        let team_row = sqlx::query_as::<_, LeagueTeamRow>(
            r"
            INSERT INTO league_teams (
                id, league_id, name, tag,
                description, logo_url, primary_color, secondary_color,
                owner_player_id, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING *
            ",
        )
        .bind(team_id)
        .bind(cmd.league_id.as_uuid())
        .bind(&cmd.name)
        .bind(&cmd.tag)
        .bind(&cmd.description)
        .bind(&cmd.logo_url)
        .bind(&cmd.primary_color)
        .bind(&cmd.secondary_color)
        .bind(cmd.owner_player_id.as_uuid())
        .bind(now)
        .bind(now)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let team_season_id = uuid::Uuid::now_v7();
        let team_season_row = sqlx::query_as::<_, LeagueTeamSeasonRow>(
            r"
            INSERT INTO league_team_seasons (
                id, team_id, season_id, status, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            ",
        )
        .bind(team_season_id)
        .bind(team_id)
        .bind(season_id.as_uuid())
        .bind("forming")
        .bind(now)
        .bind(now)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        let member_id = uuid::Uuid::now_v7();
        sqlx::query(
            r"
            INSERT INTO league_team_members (
                id, team_season_id, player_id, role, position, jersey_number, added_by, joined_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ",
        )
        .bind(member_id)
        .bind(team_season_id)
        .bind(captain_player_id.as_uuid())
        .bind(LeagueTeamRole::Captain.to_string())
        .bind(None::<String>)
        .bind(None::<i32>)
        .bind(None::<uuid::Uuid>)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        // Grant the team-scoped `team_captain` RBAC role to the captain's
        // user. This is what makes `PermissionChecker::require_team_permission`
        // succeed for the owner. Without this row the owner can only be
        // authorised by the ad-hoc `team.is_owner(...)` check in the
        // handler layer — see audit item I4.
        //
        // The lookup is done as a single INSERT...SELECT so we can roll
        // back with the same transaction if the owner has no linked
        // user_id (edge case: bot/player-only rows) — in that case the
        // SELECT returns zero rows and nothing is inserted, which is
        // correct: there is no user account to grant the role to.
        sqlx::query(
            r"
            INSERT INTO user_roles (user_id, role_id, scope_type, scope_id, granted_at)
            SELECT p.user_id, r.id, 'team', $1, $2
            FROM players p
            JOIN roles r ON r.name = 'team_captain'
            WHERE p.id = $3 AND p.user_id IS NOT NULL
            ON CONFLICT DO NOTHING
            ",
        )
        .bind(team_id)
        .bind(now)
        .bind(captain_player_id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok((
            LeagueTeam::from(team_row),
            LeagueTeamSeason::from(team_season_row),
        ))
    }
}
