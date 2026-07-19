//! Veto session builder for tests.

use portal_core::TournamentMatchId;
use portal_db::DbPool;
use portal_db::adapters::PgVetoSessionRepository;
use portal_domain::entities::veto::VetoSession;
use portal_domain::repositories::tournament::{CreateVetoSession, VetoSessionRepository};

/// Default CS2 map pool used in tests.
pub const DEFAULT_CS2_MAP_POOL: &[&str] = &[
    "de_ancient",
    "de_anubis",
    "de_dust2",
    "de_inferno",
    "de_mirage",
    "de_nuke",
    "de_vertigo",
];

/// Builder for creating test veto sessions.
#[derive(Debug, Clone)]
pub struct VetoSessionBuilder {
    match_id: Option<TournamentMatchId>,
    veto_format_id: String,
    map_pool: Vec<String>,
    timeout_seconds: u32,
    side_selection_mode: portal_core::SideSelectionMode,
}

impl Default for VetoSessionBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl VetoSessionBuilder {
    /// Create a new veto session builder with sensible defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            match_id: None,
            veto_format_id: "bo3_veto".to_string(),
            map_pool: DEFAULT_CS2_MAP_POOL
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            timeout_seconds: 30,
            side_selection_mode: portal_core::SideSelectionMode::Knife,
        }
    }

    /// Set the side selection mode (default: knife).
    #[must_use]
    pub const fn side_selection_mode(mut self, mode: portal_core::SideSelectionMode) -> Self {
        self.side_selection_mode = mode;
        self
    }

    /// Set the match ID (required).
    #[must_use]
    pub fn match_id(mut self, id: TournamentMatchId) -> Self {
        self.match_id = Some(id);
        self
    }

    /// Set the match ID from a raw UUID.
    #[must_use]
    pub fn match_id_from_uuid(mut self, id: uuid::Uuid) -> Self {
        self.match_id = Some(TournamentMatchId::from(id));
        self
    }

    /// Set the veto format ID.
    #[must_use]
    pub fn veto_format_id(mut self, format_id: impl Into<String>) -> Self {
        self.veto_format_id = format_id.into();
        self
    }

    /// Set to Bo1 veto format.
    #[must_use]
    pub fn bo1(mut self) -> Self {
        self.veto_format_id = "bo1_veto".to_string();
        self
    }

    /// Set to Bo3 veto format (default).
    #[must_use]
    pub fn bo3(mut self) -> Self {
        self.veto_format_id = "bo3_veto".to_string();
        self
    }

    /// Set to Bo5 veto format.
    #[must_use]
    pub fn bo5(mut self) -> Self {
        self.veto_format_id = "bo5_veto".to_string();
        self
    }

    /// Set the map pool.
    #[must_use]
    pub fn map_pool(mut self, maps: Vec<String>) -> Self {
        self.map_pool = maps;
        self
    }

    /// Set the map pool from string slices.
    #[must_use]
    pub fn map_pool_from_slices(mut self, maps: &[&str]) -> Self {
        self.map_pool = maps.iter().map(|s| (*s).to_string()).collect();
        self
    }

    /// Set the timeout per action in seconds.
    #[must_use]
    pub const fn timeout_seconds(mut self, seconds: u32) -> Self {
        self.timeout_seconds = seconds;
        self
    }

    /// Build and persist the veto session to the database using repository.
    ///
    /// # Panics
    ///
    /// Panics if `match_id` is not set.
    pub async fn build_persisted(self, pool: &DbPool) -> VetoSession {
        let match_id = self.match_id.expect("match_id must be set before building");

        let repo = PgVetoSessionRepository::new(pool.clone());

        let create = CreateVetoSession {
            match_id,
            veto_format_id: self.veto_format_id,
            map_pool: self.map_pool,
            timeout_seconds: self.timeout_seconds,
            side_selection_mode: self.side_selection_mode,
        };

        repo.create(create)
            .await
            .expect("Failed to create test veto session")
    }
}
