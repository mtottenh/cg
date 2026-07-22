//! Tournament registration builder for tests.

use fake::Fake;
use fake::faker::company::en::CompanyName;
use portal_core::types::TournamentRegistrationStatus;
use portal_core::{LeagueTeamSeasonId, PlayerId, TournamentId, UserId};
use portal_db::DbPool;
use portal_db::adapters::PgTournamentRegistrationRepository;
use portal_domain::entities::tournament::TournamentRegistration;
use portal_domain::repositories::tournament::{
    CreateTournamentRegistration, TournamentRegistrationRepository,
};
use uuid::Uuid;

/// Builder for creating test tournament registrations.
#[derive(Debug, Clone)]
pub struct TournamentRegistrationBuilder {
    tournament_id: Option<TournamentId>,
    team_season_id: Option<LeagueTeamSeasonId>,
    player_id: Option<PlayerId>,
    adhoc_team_id: Option<Uuid>,
    participant_name: Option<String>,
    participant_logo_url: Option<String>,
    registered_by: Option<UserId>,
    seed_rating: Option<i32>,
    status: Option<TournamentRegistrationStatus>,
}

impl Default for TournamentRegistrationBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TournamentRegistrationBuilder {
    /// Create a new tournament registration builder with sensible defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tournament_id: None,
            team_season_id: None,
            player_id: None,
            adhoc_team_id: None,
            participant_name: None,
            participant_logo_url: None,
            registered_by: None,
            seed_rating: None,
            status: None,
        }
    }

    /// Set the tournament ID (required).
    #[must_use]
    pub fn tournament_id(mut self, id: TournamentId) -> Self {
        self.tournament_id = Some(id);
        self
    }

    /// Set the tournament ID from a raw UUID.
    #[must_use]
    pub fn tournament_id_from_uuid(mut self, id: Uuid) -> Self {
        self.tournament_id = Some(TournamentId::from(id));
        self
    }

    /// Set the team season ID (for team tournaments).
    #[must_use]
    pub fn team_season_id(mut self, id: LeagueTeamSeasonId) -> Self {
        self.team_season_id = Some(id);
        self
    }

    /// Set the team season ID from a raw UUID.
    #[must_use]
    pub fn team_season_id_from_uuid(mut self, id: Uuid) -> Self {
        self.team_season_id = Some(LeagueTeamSeasonId::from(id));
        self
    }

    /// Set the player ID (for individual tournaments).
    #[must_use]
    pub fn player_id(mut self, id: PlayerId) -> Self {
        self.player_id = Some(id);
        self
    }

    /// Set the player ID from a raw UUID.
    #[must_use]
    pub fn player_id_from_uuid(mut self, id: Uuid) -> Self {
        self.player_id = Some(PlayerId::from(id));
        self
    }

    /// Set the ad-hoc team ID (for pickup teams).
    #[must_use]
    pub fn adhoc_team_id(mut self, id: Uuid) -> Self {
        self.adhoc_team_id = Some(id);
        self
    }

    /// Set the participant name.
    #[must_use]
    pub fn participant_name(mut self, name: impl Into<String>) -> Self {
        self.participant_name = Some(name.into());
        self
    }

    /// Set the participant logo URL.
    #[must_use]
    pub fn participant_logo_url(mut self, url: impl Into<String>) -> Self {
        self.participant_logo_url = Some(url.into());
        self
    }

    /// Set who registered this participant (required).
    #[must_use]
    pub fn registered_by(mut self, user_id: UserId) -> Self {
        self.registered_by = Some(user_id);
        self
    }

    /// Set who registered this participant from a raw UUID.
    #[must_use]
    pub fn registered_by_uuid(mut self, user_id: Uuid) -> Self {
        self.registered_by = Some(UserId::from(user_id));
        self
    }

    /// Set the seed rating for this participant.
    #[must_use]
    pub const fn seed_rating(mut self, rating: i32) -> Self {
        self.seed_rating = Some(rating);
        self
    }

    /// Set the registration status (e.g., Approved, CheckedIn).
    #[must_use]
    pub const fn status(mut self, status: TournamentRegistrationStatus) -> Self {
        self.status = Some(status);
        self
    }

    /// Mark the registration as approved (shorthand for `.status(TournamentRegistrationStatus::Approved)`).
    #[must_use]
    pub fn approved(self) -> Self {
        self.status(TournamentRegistrationStatus::Approved)
    }

    /// Mark the registration as checked in.
    #[must_use]
    pub fn checked_in(self) -> Self {
        self.status(TournamentRegistrationStatus::CheckedIn)
    }

    /// Build and persist the registration to the database using repository.
    ///
    /// # Panics
    ///
    /// Panics if `tournament_id` or `registered_by` is not set.
    pub async fn build_persisted(self, pool: &DbPool) -> TournamentRegistration {
        let tournament_id = self
            .tournament_id
            .expect("tournament_id must be set before building");
        let registered_by = self
            .registered_by
            .expect("registered_by must be set before building");

        let participant_name = self.participant_name.unwrap_or_else(|| {
            let company: String = CompanyName().fake();
            company
        });

        let repo = PgTournamentRegistrationRepository::new(pool.clone());

        let create = CreateTournamentRegistration {
            tournament_id,
            team_season_id: self.team_season_id,
            player_id: self.player_id,
            adhoc_team_id: self.adhoc_team_id,
            participant_name,
            participant_logo_url: self.participant_logo_url,
            registered_by,
            seed_rating: self.seed_rating,
        };

        let registration = repo
            .create(create)
            .await
            .expect("Failed to create test tournament registration");

        // If a status was specified (e.g., Approved), update it after creation
        if let Some(status) = self.status {
            repo.update_status(registration.id, status)
                .await
                .expect("Failed to update test tournament registration status")
        } else {
            registration
        }
    }
}
