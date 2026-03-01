//! Demo and DemoMatchLink builders for tests.

use chrono::Utc;
use portal_db::entities::{DemoMatchLinkRow, DemoRow};
use portal_db::DbPool;
use uuid::Uuid;

// =============================================================================
// DEMO BUILDER
// =============================================================================

/// Builder for creating test demos.
#[derive(Debug, Clone)]
pub struct DemoBuilder {
    id: Option<Uuid>,
    game_id: Option<Uuid>,
    file_name: Option<String>,
    s3_bucket: String,
    s3_key: Option<String>,
    file_size_bytes: Option<i64>,
    category: String,
    is_hidden: bool,
    league_id: Option<Uuid>,
    tournament_id: Option<Uuid>,
    metadata: Option<serde_json::Value>,
    stats_json: Option<serde_json::Value>,
    status: String,
}

impl Default for DemoBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl DemoBuilder {
    /// Create a new demo builder with sensible defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: None,
            game_id: None,
            file_name: None,
            s3_bucket: "test-demos".to_string(),
            s3_key: None,
            file_size_bytes: Some(50_000_000), // 50MB default
            category: "uncategorized".to_string(),
            is_hidden: false,
            league_id: None,
            tournament_id: None,
            metadata: None,
            stats_json: None,
            status: "ready".to_string(),
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

    /// Set the file name.
    #[must_use]
    pub fn file_name(mut self, name: impl Into<String>) -> Self {
        self.file_name = Some(name.into());
        self
    }

    /// Set the S3 bucket.
    #[must_use]
    pub fn s3_bucket(mut self, bucket: impl Into<String>) -> Self {
        self.s3_bucket = bucket.into();
        self
    }

    /// Set the S3 key.
    #[must_use]
    pub fn s3_key(mut self, key: impl Into<String>) -> Self {
        self.s3_key = Some(key.into());
        self
    }

    /// Set the file size.
    #[must_use]
    pub const fn file_size_bytes(mut self, size: i64) -> Self {
        self.file_size_bytes = Some(size);
        self
    }

    /// Set category to league.
    #[must_use]
    pub fn league_category(mut self) -> Self {
        self.category = "league".to_string();
        self
    }

    /// Set category to pug.
    #[must_use]
    pub fn pug_category(mut self) -> Self {
        self.category = "pug".to_string();
        self
    }

    /// Set category to scrim.
    #[must_use]
    pub fn scrim_category(mut self) -> Self {
        self.category = "scrim".to_string();
        self
    }

    /// Set category to ignored.
    #[must_use]
    pub fn ignored_category(mut self) -> Self {
        self.category = "ignored".to_string();
        self
    }

    /// Set the demo as hidden.
    #[must_use]
    pub const fn hidden(mut self) -> Self {
        self.is_hidden = true;
        self
    }

    /// Associate with a league.
    #[must_use]
    pub const fn league_id(mut self, league_id: Uuid) -> Self {
        self.league_id = Some(league_id);
        self
    }

    /// Associate with a tournament.
    #[must_use]
    pub const fn tournament_id(mut self, tournament_id: Uuid) -> Self {
        self.tournament_id = Some(tournament_id);
        self
    }

    /// Set the metadata.
    #[must_use]
    pub fn metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Set CS2-like metadata for a match.
    #[must_use]
    pub fn cs2_metadata(self, map_name: &str, team1: &str, team2: &str, score1: i32, score2: i32) -> Self {
        self.metadata(serde_json::json!({
            "map_name": map_name,
            "team1_name": team1,
            "team2_name": team2,
            "team1_score": score1,
            "team2_score": score2,
            "total_rounds": score1 + score2,
            "duration_seconds": 2400,
            "match_date": Utc::now().to_rfc3339()
        }))
    }

    /// Set the stats JSON.
    #[must_use]
    pub fn stats_json(mut self, stats: serde_json::Value) -> Self {
        self.stats_json = Some(stats);
        self
    }

    /// Set status to pending.
    #[must_use]
    pub fn pending(mut self) -> Self {
        self.status = "pending".to_string();
        self
    }

    /// Set status to processing.
    #[must_use]
    pub fn processing(mut self) -> Self {
        self.status = "processing".to_string();
        self
    }

    /// Set status to ready (default).
    #[must_use]
    pub fn ready(mut self) -> Self {
        self.status = "ready".to_string();
        self
    }

    /// Set status to failed.
    #[must_use]
    pub fn failed(mut self) -> Self {
        self.status = "failed".to_string();
        self
    }

    /// Build and persist the demo to the database.
    ///
    /// If `game_id` is not set, uses or creates a test game.
    pub async fn build_persisted(self, pool: &DbPool) -> DemoRow {
        let now = Utc::now();

        // Get or create game
        let game_id = if let Some(id) = self.game_id {
            id
        } else {
            create_test_game(pool).await
        };

        let id = self.id.unwrap_or_else(Uuid::now_v7);
        let file_name = self.file_name.unwrap_or_else(|| format!("test_demo_{}.dem", &id.to_string()[..8]));
        let s3_key = self.s3_key.unwrap_or_else(|| format!("demos/{game_id}/{file_name}"));

        sqlx::query_as::<_, DemoRow>(
            r"
            INSERT INTO demos (
                id, game_id, file_name, s3_bucket, s3_key, file_size_bytes,
                category, is_hidden, league_id, tournament_id,
                metadata, stats_json, status, discovered_at, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
            RETURNING *
            ",
        )
        .bind(id)
        .bind(game_id)
        .bind(&file_name)
        .bind(&self.s3_bucket)
        .bind(&s3_key)
        .bind(self.file_size_bytes)
        .bind(&self.category)
        .bind(self.is_hidden)
        .bind(self.league_id)
        .bind(self.tournament_id)
        .bind(&self.metadata)
        .bind(&self.stats_json)
        .bind(&self.status)
        .bind(now) // discovered_at
        .bind(now) // created_at
        .bind(now) // updated_at
        .fetch_one(pool)
        .await
        .expect("Failed to create test demo")
    }
}

// =============================================================================
// DEMO MATCH LINK BUILDER
// =============================================================================

/// Builder for creating demo-match links.
#[derive(Debug, Clone)]
pub struct DemoMatchLinkBuilder {
    id: Option<Uuid>,
    demo_id: Option<Uuid>,
    match_id: Option<Uuid>,
    game_number: Option<i32>,
    link_type: String,
    confidence_score: Option<f32>,
    validated: bool,
    validation_result: Option<serde_json::Value>,
    linked_by_user_id: Option<Uuid>,
}

impl Default for DemoMatchLinkBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl DemoMatchLinkBuilder {
    /// Create a new demo match link builder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: None,
            demo_id: None,
            match_id: None,
            game_number: None,
            link_type: "manual".to_string(),
            confidence_score: None,
            validated: false,
            validation_result: None,
            linked_by_user_id: None,
        }
    }

    /// Set a specific ID.
    #[must_use]
    pub const fn id(mut self, id: Uuid) -> Self {
        self.id = Some(id);
        self
    }

    /// Set the demo ID.
    #[must_use]
    pub const fn demo_id(mut self, demo_id: Uuid) -> Self {
        self.demo_id = Some(demo_id);
        self
    }

    /// Set the match ID.
    #[must_use]
    pub const fn match_id(mut self, match_id: Uuid) -> Self {
        self.match_id = Some(match_id);
        self
    }

    /// Set the game number in the series.
    #[must_use]
    pub const fn game_number(mut self, game_number: i32) -> Self {
        self.game_number = Some(game_number);
        self
    }

    /// Set link type to manual (default).
    #[must_use]
    pub fn manual(mut self) -> Self {
        self.link_type = "manual".to_string();
        self.confidence_score = None;
        self
    }

    /// Set link type to auto-matched with confidence score.
    #[must_use]
    pub fn auto_matched(mut self, confidence: f32) -> Self {
        self.link_type = "auto_matched".to_string();
        self.confidence_score = Some(confidence);
        self
    }

    /// Set link type to evidence.
    #[must_use]
    pub fn evidence(mut self) -> Self {
        self.link_type = "evidence".to_string();
        self
    }

    /// Mark as validated with a result.
    #[must_use]
    pub fn validated_with(mut self, result: serde_json::Value) -> Self {
        self.validated = true;
        self.validation_result = Some(result);
        self
    }

    /// Set the user who created this link.
    #[must_use]
    pub const fn linked_by(mut self, user_id: Uuid) -> Self {
        self.linked_by_user_id = Some(user_id);
        self
    }

    /// Build and persist the demo match link to the database.
    ///
    /// Requires `demo_id` and `match_id` to be set.
    pub async fn build_persisted(self, pool: &DbPool) -> DemoMatchLinkRow {
        let now = Utc::now();

        let demo_id = self.demo_id.expect("demo_id is required for DemoMatchLinkBuilder");
        let match_id = self.match_id.expect("match_id is required for DemoMatchLinkBuilder");
        let id = self.id.unwrap_or_else(Uuid::now_v7);

        sqlx::query_as::<_, DemoMatchLinkRow>(
            r"
            INSERT INTO demo_match_links (
                id, demo_id, match_id, game_number, link_type, confidence_score,
                validated, validated_at, validation_result, linked_by_user_id,
                linked_at, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            RETURNING *
            ",
        )
        .bind(id)
        .bind(demo_id)
        .bind(match_id)
        .bind(self.game_number)
        .bind(&self.link_type)
        .bind(self.confidence_score)
        .bind(self.validated)
        .bind(if self.validated { Some(now) } else { None })
        .bind(&self.validation_result)
        .bind(self.linked_by_user_id)
        .bind(now) // linked_at
        .bind(now) // created_at
        .fetch_one(pool)
        .await
        .expect("Failed to create test demo match link")
    }
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Creates a test game for demo tests.
async fn create_test_game(pool: &DbPool) -> Uuid {
    use portal_db::entities::GameRow;

    // Use random UUID v4 for uniqueness in parallel tests
    let id = Uuid::new_v4();
    let slug = format!("demo-game-{}", &id.to_string()[..12]);

    sqlx::query_as::<_, GameRow>(
        r"
        INSERT INTO games (id, slug, display_name, plugin_id, plugin_version, team_size_min, team_size_max, team_size_default, status)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING *
        ",
    )
    .bind(id)
    .bind(&slug)
    .bind("Demo Test Game")
    .bind("cs2")
    .bind("1.0.0")
    .bind(5)
    .bind(5)
    .bind(5)
    .bind("active")
    .fetch_one(pool)
    .await
    .expect("Failed to create test game for demo")
    .id
}
