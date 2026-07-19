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
            .ok_or_else(|| DomainError::LeagueSeasonNotFound(id))
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
            .ok_or_else(|| DomainError::LeagueNotFound(cmd.league_id))?;

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
    #[instrument(skip(self))]
    pub async fn update_season(
        &self,
        id: LeagueSeasonId,
        cmd: UpdateLeagueSeasonCommand,
    ) -> Result<LeagueSeason, DomainError> {
        let season = self.get_season(id).await?;

        // Check slug uniqueness if changing
        if let Some(ref slug) = cmd.slug {
            if slug != &season.slug && self.season_repo.slug_exists(season.league_id, slug).await? {
                return Err(DomainError::Conflict(format!(
                    "season slug '{slug}' is already taken in this league"
                )));
            }
        }

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
                },
            )
            .await?;

        info!(season_id = %id, "League season updated");

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

    /// Update roster lock status.
    #[instrument(skip(self))]
    pub async fn update_roster_lock(
        &self,
        id: LeagueSeasonId,
        status: RosterLockStatus,
        locked_by: UserId,
    ) -> Result<LeagueSeason, DomainError> {
        let season = self.get_season(id).await?;

        // Can only lock rosters during registration or active season
        if !season.status.allows_roster_changes() && status != RosterLockStatus::Open {
            return Err(DomainError::InvalidState(
                "cannot modify roster lock in current season state".to_string(),
            ));
        }

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

        // Validate status transitions
        let valid_transition = match (&season.status, &status) {
            (SeasonStatus::Draft, SeasonStatus::Registration) => true,
            (SeasonStatus::Registration, SeasonStatus::Active) => true,
            (SeasonStatus::Active, SeasonStatus::Playoffs) => true,
            (SeasonStatus::Playoffs, SeasonStatus::Completed) => true,
            (SeasonStatus::Active, SeasonStatus::Completed) => true, // Direct completion without playoffs
            (_, SeasonStatus::Cancelled) => !season.status.is_terminal(),
            _ => false,
        };

        if !valid_transition {
            return Err(DomainError::InvalidState(format!(
                "cannot transition season from {} to {}",
                season.status, status
            )));
        }

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
