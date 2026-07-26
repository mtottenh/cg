//! League season service.

use crate::entities::league_team::{
    CreateLeagueSeasonCommand, LeagueSeason, UpdateLeagueSeasonCommand,
};
use crate::repositories::LeagueRepository;
use crate::repositories::league_team::{
    CreateLeagueSeason, LeagueSeasonRepository, UpdateLeagueSeason,
};
use portal_core::types::{RosterLockStatus, SeasonStatus};
use portal_core::{DomainError, LeagueId, LeagueSeasonId, UserId};
use std::sync::Arc;
use tracing::{info, instrument};

/// Service for league season-related business logic.
pub struct LeagueSeasonService<SR, LR>
where
    SR: LeagueSeasonRepository,
    LR: LeagueRepository,
{
    season_repo: Arc<SR>,
    league_repo: Arc<LR>,
}

impl<SR, LR> LeagueSeasonService<SR, LR>
where
    SR: LeagueSeasonRepository,
    LR: LeagueRepository,
{
    /// Create a new league season service.
    pub const fn new(season_repo: Arc<SR>, league_repo: Arc<LR>) -> Self {
        Self {
            season_repo,
            league_repo,
        }
    }

    /// Get a season by ID.
    #[instrument(skip(self))]
    pub async fn get_season(&self, id: LeagueSeasonId) -> Result<LeagueSeason, DomainError> {
        self.season_repo
            .find_by_id(id)
            .await?
            .ok_or(DomainError::LeagueSeasonNotFound(id))
    }

    /// Get a season by league and slug.
    #[instrument(skip(self))]
    pub async fn get_season_by_slug(
        &self,
        league_id: LeagueId,
        slug: &str,
    ) -> Result<LeagueSeason, DomainError> {
        self.season_repo
            .find_by_slug(league_id, slug)
            .await?
            .ok_or_else(|| DomainError::LookupFailed {
                resource: "league season",
                query: slug.to_string(),
            })
    }

    /// Get the current season for a league.
    #[instrument(skip(self))]
    pub async fn get_current_season(
        &self,
        league_id: LeagueId,
    ) -> Result<Option<LeagueSeason>, DomainError> {
        self.season_repo.find_current_by_league(league_id).await
    }

    /// Create a new season for a league.
    #[instrument(skip(self))]
    pub async fn create_season(
        &self,
        creator_id: UserId,
        cmd: CreateLeagueSeasonCommand,
    ) -> Result<LeagueSeason, DomainError> {
        // Verify the league exists
        let _league = self
            .league_repo
            .find_by_id(cmd.league_id)
            .await?
            .ok_or(DomainError::LeagueNotFound(cmd.league_id))?;

        // Check slug uniqueness within the league
        if self
            .season_repo
            .slug_exists(cmd.league_id, &cmd.slug)
            .await?
        {
            return Err(DomainError::Conflict(format!(
                "season slug '{}' is already taken in this league",
                cmd.slug
            )));
        }

        let season = self
            .season_repo
            .create(CreateLeagueSeason {
                league_id: cmd.league_id,
                name: cmd.name,
                slug: cmd.slug,
                description: cmd.description,
                registration_start: cmd.registration_start,
                registration_end: cmd.registration_end,
                season_start: cmd.season_start,
                season_end: cmd.season_end,
                team_size_min: cmd.team_size_min,
                team_size_max: cmd.team_size_max,
                max_substitutes: cmd.max_substitutes,
                max_teams: cmd.max_teams,
                created_by: creator_id,
            })
            .await?;

        info!(
            season_id = %season.id,
            league_id = %cmd.league_id,
            creator_id = %creator_id,
            "League season created"
        );

        Ok(season)
    }

    /// Update a season.
    ///
    /// # P-14
    ///
    /// `UpdateLeagueSeasonCommand::roster_lock_status` used to be dropped on
    /// the floor here: the DTO accepted and validated it, the command carried
    /// it, and nothing ever read it — so the roster lock could not be set by
    /// any caller, and every lock check downstream was unreachable code.
    ///
    /// It is now applied by delegating to [`Self::update_roster_lock`] rather
    /// than by adding a column to `UpdateLeagueSeason`. That keeps **one**
    /// implementation of "change the lock", which is the one that also stamps
    /// the `roster_locked_by` / `roster_locked_at` audit columns and enforces
    /// the season-state guard — a plain `COALESCE` column on the generic update
    /// would have silently skipped both.
    ///
    /// The lock is validated against the status the request is moving *to*, not
    /// the one it is leaving, so `{status: "registration", roster_lock_status:
    /// "hard_lock"}` in one PATCH behaves the way an admin means it. It is
    /// validated **before** the generic field update so a rejected lock does not
    /// leave the other fields half-applied.
    #[instrument(skip(self))]
    pub async fn update_season(
        &self,
        id: LeagueSeasonId,
        cmd: UpdateLeagueSeasonCommand,
        actor: UserId,
    ) -> Result<LeagueSeason, DomainError> {
        let season = self.get_season(id).await?;

        // Check slug uniqueness if changing
        if let Some(ref slug) = cmd.slug
            && slug != &season.slug
            && self.season_repo.slug_exists(season.league_id, slug).await?
        {
            return Err(DomainError::Conflict(format!(
                "season slug '{slug}' is already taken in this league"
            )));
        }

        // P-199: `status` used to be written here as a plain field with no
        // validation, while update_status enforced the transition chain —
        // two mechanisms disagreeing about the same rule, and a PATCH could
        // jump a season straight from draft to completed. One rule now;
        // echoing the current status back is allowed (idempotent PATCH).
        if let Some(new_status) = cmd.status
            && new_status != season.status
        {
            Self::ensure_status_transition_allowed(season.status, new_status)?;
        }

        let roster_lock_status = cmd.roster_lock_status;
        if let Some(lock) = roster_lock_status {
            Self::ensure_lock_change_allowed(cmd.status.unwrap_or(season.status), lock)?;
        }

        // P-198: one repo call, one UPDATE statement. This used to be the
        // generic field write followed by update_roster_lock — two
        // non-transactional writes, so a DB error between them half-applied
        // the PATCH.
        let updated = self
            .season_repo
            .update(
                id,
                UpdateLeagueSeason {
                    name: cmd.name,
                    slug: cmd.slug,
                    description: cmd.description,
                    registration_start: cmd.registration_start,
                    registration_end: cmd.registration_end,
                    season_start: cmd.season_start,
                    season_end: cmd.season_end,
                    team_size_min: cmd.team_size_min,
                    team_size_max: cmd.team_size_max,
                    max_substitutes: cmd.max_substitutes,
                    max_teams: cmd.max_teams,
                    status: cmd.status,
                    settings: cmd.settings,
                    roster_lock_status,
                    roster_locked_by: roster_lock_status.map(|_| actor),
                },
            )
            .await?;

        info!(season_id = %id, "League season updated");

        if let Some(lock) = roster_lock_status {
            info!(
                season_id = %id,
                roster_lock = %lock,
                locked_by = %actor,
                "Roster lock status updated"
            );
        }

        Ok(updated)
    }

    /// List seasons for a league.
    #[instrument(skip(self))]
    pub async fn list_seasons(
        &self,
        league_id: LeagueId,
    ) -> Result<Vec<LeagueSeason>, DomainError> {
        self.season_repo.list_by_league(league_id).await
    }

    /// List active seasons for a league.
    #[instrument(skip(self))]
    pub async fn list_active_seasons(
        &self,
        league_id: LeagueId,
    ) -> Result<Vec<LeagueSeason>, DomainError> {
        self.season_repo.list_active_by_league(league_id).await
    }

    /// Whether a season in `season_status` may move its roster lock to `lock`.
    ///
    /// Extracted so [`Self::update_season`] can pre-validate a combined
    /// status + lock PATCH without keeping a second copy of the rule — the same
    /// mistake that produced P-15 one layer down.
    ///
    /// Lifting the lock (`open`) is always allowed; tightening it only makes
    /// sense while the season can still take roster changes at all.
    ///
    /// # P-148
    ///
    /// "Can still take roster changes at all" used to mean
    /// `SeasonStatus::allows_roster_changes()` — `draft | registration`. Under
    /// the owner's ruling that the lock is an optional, per-season decision,
    /// that reading made the control unusable for its only real purpose: an
    /// operator could not tighten the lock on a season that had actually
    /// started, which is when a league decides its rosters are final. It now
    /// means "not terminal": `draft`, `registration`, `active` and `playoffs`
    /// may all set any lock value; a `completed` or `cancelled` season may not
    /// be re-locked, because there is nothing left to lock — its roster is
    /// frozen by `refusal_reason` regardless of what this column says. Lifting
    /// to `open` stays unconditional so a mis-set lock is always recoverable.
    fn ensure_lock_change_allowed(
        season_status: SeasonStatus,
        lock: RosterLockStatus,
    ) -> Result<(), DomainError> {
        if season_status.is_terminal() && lock != RosterLockStatus::Open {
            return Err(DomainError::InvalidState(
                "cannot modify roster lock in current season state".to_string(),
            ));
        }
        Ok(())
    }

    /// The season status transition chain, in one place.
    ///
    /// Both writers consult this — the dedicated transition endpoint
    /// (`update_status`) and the generic season PATCH (`update_season`).
    /// P-199: the PATCH used to write `status` as a plain field with no
    /// validation, so it bypassed the chain the other endpoint enforced —
    /// two mechanisms disagreeing about the same rule, the P-15/P-168
    /// shape one more time.
    fn ensure_status_transition_allowed(
        from: SeasonStatus,
        to: SeasonStatus,
    ) -> Result<(), DomainError> {
        let valid = match (&from, &to) {
            (SeasonStatus::Draft, SeasonStatus::Registration) => true,
            (SeasonStatus::Registration, SeasonStatus::Active) => true,
            (SeasonStatus::Active, SeasonStatus::Playoffs) => true,
            (SeasonStatus::Playoffs, SeasonStatus::Completed) => true,
            // Direct completion without playoffs
            (SeasonStatus::Active, SeasonStatus::Completed) => true,
            (_, SeasonStatus::Cancelled) => !from.is_terminal(),
            _ => false,
        };

        if valid {
            Ok(())
        } else {
            Err(DomainError::InvalidState(format!(
                "cannot transition season from {from} to {to}"
            )))
        }
    }

    /// Update roster lock status.
    #[instrument(skip(self))]
    pub async fn update_roster_lock(
        &self,
        id: LeagueSeasonId,
        status: RosterLockStatus,
        locked_by: UserId,
    ) -> Result<LeagueSeason, DomainError> {
        let season = self.get_season(id).await?;

        // The lock can be set on any season that is not already finished.
        Self::ensure_lock_change_allowed(season.status, status)?;

        let updated = self
            .season_repo
            .update_roster_lock(id, status, Some(locked_by))
            .await?;

        info!(
            season_id = %id,
            roster_lock = %status,
            locked_by = %locked_by,
            "Roster lock status updated"
        );

        Ok(updated)
    }

    /// Update season status.
    #[instrument(skip(self))]
    pub async fn update_status(
        &self,
        id: LeagueSeasonId,
        status: SeasonStatus,
    ) -> Result<LeagueSeason, DomainError> {
        let season = self.get_season(id).await?;

        Self::ensure_status_transition_allowed(season.status, status)?;

        let updated = self.season_repo.update_status(id, status).await?;

        info!(
            season_id = %id,
            from_status = %season.status,
            to_status = %status,
            "Season status updated"
        );

        Ok(updated)
    }

    /// Count teams registered in a season.
    pub async fn count_teams(&self, season_id: LeagueSeasonId) -> Result<i64, DomainError> {
        self.season_repo.count_teams(season_id).await
    }

    /// Count participants (individual format) in a season.
    pub async fn count_participants(&self, season_id: LeagueSeasonId) -> Result<i64, DomainError> {
        self.season_repo.count_participants(season_id).await
    }
}

impl<SR, LR> Clone for LeagueSeasonService<SR, LR>
where
    SR: LeagueSeasonRepository,
    LR: LeagueRepository,
{
    fn clone(&self) -> Self {
        Self {
            season_repo: Arc::clone(&self.season_repo),
            league_repo: Arc::clone(&self.league_repo),
        }
    }
}
