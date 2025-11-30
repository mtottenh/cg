//! Tournament builder for tests.

use chrono::{DateTime, Utc};
use fake::faker::company::en::CompanyName;
use fake::Fake;
use portal_db::entities::TournamentRow;
use portal_db::DbPool;
use uuid::Uuid;

/// Builder for creating test tournaments.
#[derive(Debug, Clone)]
pub struct TournamentBuilder {
    id: Option<Uuid>,
    game_id: Option<Uuid>,
    league_id: Option<Uuid>,
    season_id: Option<Uuid>,
    name: Option<String>,
    slug: Option<String>,
    description: Option<String>,
    format: String,
    format_settings: serde_json::Value,
    participant_type: String,
    team_size: Option<i32>,
    min_participants: i32,
    max_participants: i32,
    registration_type: String,
    registration_start: Option<DateTime<Utc>>,
    registration_end: Option<DateTime<Utc>>,
    check_in_required: bool,
    check_in_start: Option<DateTime<Utc>>,
    check_in_end: Option<DateTime<Utc>>,
    scheduling_mode: String,
    starts_at: Option<DateTime<Utc>>,
    ends_at: Option<DateTime<Utc>>,
    default_match_format: String,
    default_map_veto_format: Option<String>,
    withdrawal_policy: String,
    rules_url: Option<String>,
    settings: serde_json::Value,
    status: String,
    created_by: Option<Uuid>,
}

impl Default for TournamentBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TournamentBuilder {
    /// Create a new tournament builder with sensible defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: None,
            game_id: None,
            league_id: None,
            season_id: None,
            name: None,
            slug: None,
            description: None,
            format: "single_elimination".to_string(),
            format_settings: serde_json::json!({}),
            participant_type: "team".to_string(),
            team_size: Some(5),
            min_participants: 4,
            max_participants: 16,
            registration_type: "open".to_string(),
            registration_start: None,
            registration_end: None,
            check_in_required: false,
            check_in_start: None,
            check_in_end: None,
            scheduling_mode: "live".to_string(),
            starts_at: None,
            ends_at: None,
            default_match_format: "bo3".to_string(),
            default_map_veto_format: None,
            withdrawal_policy: "forfeit".to_string(),
            rules_url: None,
            settings: serde_json::json!({}),
            status: "draft".to_string(),
            created_by: None,
        }
    }

    /// Set a specific ID.
    #[must_use]
    pub const fn id(mut self, id: Uuid) -> Self {
        self.id = Some(id);
        self
    }

    /// Set the game ID.
    #[must_use]
    pub const fn game_id(mut self, game_id: Uuid) -> Self {
        self.game_id = Some(game_id);
        self
    }

    /// Set the league ID (for league-associated tournaments).
    #[must_use]
    pub const fn league_id(mut self, league_id: Uuid) -> Self {
        self.league_id = Some(league_id);
        self
    }

    /// Set the season ID (for season-associated tournaments).
    #[must_use]
    pub const fn season_id(mut self, season_id: Uuid) -> Self {
        self.season_id = Some(season_id);
        self
    }

    /// Set the tournament name.
    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the tournament slug.
    #[must_use]
    pub fn slug(mut self, slug: impl Into<String>) -> Self {
        self.slug = Some(slug.into());
        self
    }

    /// Set the description.
    #[must_use]
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set format to single elimination (default).
    #[must_use]
    pub fn single_elimination(mut self) -> Self {
        self.format = "single_elimination".to_string();
        self
    }

    /// Set format to double elimination.
    #[must_use]
    pub fn double_elimination(mut self) -> Self {
        self.format = "double_elimination".to_string();
        self
    }

    /// Set format to round robin.
    #[must_use]
    pub fn round_robin(mut self) -> Self {
        self.format = "round_robin".to_string();
        self
    }

    /// Set format to swiss.
    #[must_use]
    pub fn swiss(mut self) -> Self {
        self.format = "swiss".to_string();
        self
    }

    /// Set format settings.
    #[must_use]
    pub fn format_settings(mut self, settings: serde_json::Value) -> Self {
        self.format_settings = settings;
        self
    }

    /// Set to team tournament (default).
    #[must_use]
    pub fn team(mut self) -> Self {
        self.participant_type = "team".to_string();
        self.team_size = Some(5);
        self
    }

    /// Set to individual (1v1) tournament.
    #[must_use]
    pub fn individual(mut self) -> Self {
        self.participant_type = "individual".to_string();
        self.team_size = None;
        self
    }

    /// Set team size requirement.
    #[must_use]
    pub const fn team_size(mut self, size: i32) -> Self {
        self.team_size = Some(size);
        self
    }

    /// Set participant limits.
    #[must_use]
    pub const fn participants(mut self, min: i32, max: i32) -> Self {
        self.min_participants = min;
        self.max_participants = max;
        self
    }

    /// Set registration type to open (default).
    #[must_use]
    pub fn open_registration(mut self) -> Self {
        self.registration_type = "open".to_string();
        self
    }

    /// Set registration type to invite only.
    #[must_use]
    pub fn invite_only_registration(mut self) -> Self {
        self.registration_type = "invite_only".to_string();
        self
    }

    /// Set registration type to approval required.
    #[must_use]
    pub fn approval_registration(mut self) -> Self {
        self.registration_type = "approval".to_string();
        self
    }

    /// Set registration window.
    #[must_use]
    pub const fn registration_window(mut self, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        self.registration_start = Some(start);
        self.registration_end = Some(end);
        self
    }

    /// Enable check-in with times.
    #[must_use]
    pub const fn with_check_in(mut self, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        self.check_in_required = true;
        self.check_in_start = Some(start);
        self.check_in_end = Some(end);
        self
    }

    /// Set scheduling mode to live (synchronous, default).
    #[must_use]
    pub fn live_scheduling(mut self) -> Self {
        self.scheduling_mode = "live".to_string();
        self
    }

    /// Set scheduling mode to self-scheduled.
    #[must_use]
    pub fn self_scheduled(mut self) -> Self {
        self.scheduling_mode = "self_scheduled".to_string();
        self
    }

    /// Set the tournament start time.
    #[must_use]
    pub const fn starts_at(mut self, starts_at: DateTime<Utc>) -> Self {
        self.starts_at = Some(starts_at);
        self
    }

    /// Set the tournament end time.
    #[must_use]
    pub const fn ends_at(mut self, ends_at: DateTime<Utc>) -> Self {
        self.ends_at = Some(ends_at);
        self
    }

    /// Set default match format.
    #[must_use]
    pub fn match_format(mut self, format: impl Into<String>) -> Self {
        self.default_match_format = format.into();
        self
    }

    /// Set status to draft (default).
    #[must_use]
    pub fn draft(mut self) -> Self {
        self.status = "draft".to_string();
        self
    }

    /// Set status to published.
    #[must_use]
    pub fn published(mut self) -> Self {
        self.status = "published".to_string();
        self
    }

    /// Set status to registration open.
    #[must_use]
    pub fn registration_open(mut self) -> Self {
        self.status = "registration".to_string();
        self
    }

    /// Set status to in progress.
    #[must_use]
    pub fn in_progress(mut self) -> Self {
        self.status = "in_progress".to_string();
        self
    }

    /// Set custom settings.
    #[must_use]
    pub fn settings(mut self, settings: serde_json::Value) -> Self {
        self.settings = settings;
        self
    }

    /// Set the creator user ID.
    #[must_use]
    pub const fn created_by(mut self, user_id: Uuid) -> Self {
        self.created_by = Some(user_id);
        self
    }

    /// Build an in-memory tournament (not persisted).
    #[must_use]
    pub fn build(self, game_id: Uuid, created_by: Uuid) -> TournamentRow {
        let now = Utc::now();
        let name = self.name.unwrap_or_else(|| {
            let company: String = CompanyName().fake();
            format!("{company} Tournament")
        });
        let slug = self.slug.unwrap_or_else(|| slug::slugify(&name));

        TournamentRow {
            id: self.id.unwrap_or_else(Uuid::now_v7),
            game_id,
            league_id: self.league_id,
            season_id: self.season_id,
            name,
            slug,
            description: self.description,
            logo_url: None,
            banner_url: None,
            format: self.format,
            format_settings: self.format_settings,
            participant_type: self.participant_type,
            team_size: self.team_size,
            min_participants: self.min_participants,
            max_participants: self.max_participants,
            registration_type: self.registration_type,
            registration_start: self.registration_start,
            registration_end: self.registration_end,
            check_in_start: self.check_in_start,
            check_in_end: self.check_in_end,
            check_in_required: self.check_in_required,
            scheduling_mode: self.scheduling_mode,
            starts_at: self.starts_at,
            ends_at: self.ends_at,
            timezone_hint: None,
            default_match_format: self.default_match_format,
            default_map_veto_format: self.default_map_veto_format,
            prize_pool: None,
            rules_url: self.rules_url,
            settings: self.settings,
            withdrawal_policy: self.withdrawal_policy,
            status: self.status,
            created_by,
            organization_id: None,
            created_at: now,
            updated_at: now,
            published_at: None,
            started_at: None,
            completed_at: None,
        }
    }

    /// Build and persist the tournament to the database.
    ///
    /// If `game_id` is not set, uses or creates a test game.
    /// If `created_by` is not set, creates a test user.
    pub async fn build_persisted(self, pool: &DbPool) -> TournamentRow {
        use super::UserBuilder;

        // Get or create game
        let game_id = if let Some(id) = self.game_id {
            id
        } else {
            create_test_game(pool).await
        };

        // Get or create user
        let created_by = if let Some(id) = self.created_by {
            id
        } else {
            let user = UserBuilder::new().build_persisted(pool).await;
            user.id
        };

        let tournament = self.build(game_id, created_by);

        sqlx::query_as::<_, TournamentRow>(
            r"
            INSERT INTO tournaments (
                id, game_id, league_id, season_id, name, slug, description,
                format, format_settings, participant_type, team_size,
                min_participants, max_participants, registration_type,
                registration_start, registration_end, check_in_required,
                check_in_start, check_in_end, scheduling_mode, starts_at, ends_at,
                default_match_format, default_map_veto_format,
                withdrawal_policy, rules_url, settings, status, created_by,
                created_at, updated_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28, $29, $30, $31
            )
            RETURNING *
            ",
        )
        .bind(tournament.id)
        .bind(tournament.game_id)
        .bind(tournament.league_id)
        .bind(tournament.season_id)
        .bind(&tournament.name)
        .bind(&tournament.slug)
        .bind(&tournament.description)
        .bind(&tournament.format)
        .bind(&tournament.format_settings)
        .bind(&tournament.participant_type)
        .bind(tournament.team_size)
        .bind(tournament.min_participants)
        .bind(tournament.max_participants)
        .bind(&tournament.registration_type)
        .bind(tournament.registration_start)
        .bind(tournament.registration_end)
        .bind(tournament.check_in_required)
        .bind(tournament.check_in_start)
        .bind(tournament.check_in_end)
        .bind(&tournament.scheduling_mode)
        .bind(tournament.starts_at)
        .bind(tournament.ends_at)
        .bind(&tournament.default_match_format)
        .bind(&tournament.default_map_veto_format)
        .bind(&tournament.withdrawal_policy)
        .bind(&tournament.rules_url)
        .bind(&tournament.settings)
        .bind(&tournament.status)
        .bind(tournament.created_by)
        .bind(tournament.created_at)
        .bind(tournament.updated_at)
        .fetch_one(pool)
        .await
        .expect("Failed to create test tournament")
    }
}

/// Creates a test game for tournament tests.
async fn create_test_game(pool: &DbPool) -> Uuid {
    use portal_db::entities::GameRow;

    // Use random UUID v4 for uniqueness in parallel tests
    let id = Uuid::new_v4();
    let slug = format!("game-{}", &id.to_string()[..12]);

    sqlx::query_as::<_, GameRow>(
        r"
        INSERT INTO games (id, slug, display_name, plugin_id, plugin_version, team_size_min, team_size_max, team_size_default, status)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING *
        ",
    )
    .bind(id)
    .bind(&slug)
    .bind("Test Game")
    .bind("test-plugin")
    .bind("1.0.0")
    .bind(1)
    .bind(5)
    .bind(5)
    .bind("active")
    .fetch_one(pool)
    .await
    .expect("Failed to create test game")
    .id
}
