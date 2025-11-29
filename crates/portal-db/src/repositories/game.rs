//! Game repository.

use crate::entities::{GameRow, NewGame, UpdateGame};
use crate::error::RepositoryError;
use crate::DbPool;
use uuid::Uuid;

/// Repository for game operations.
#[derive(Clone)]
pub struct GameRepository {
    pool: DbPool,
}

impl GameRepository {
    /// Create a new game repository.
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Find a game by slug (e.g., "cs2", "aoe4").
    pub async fn find_by_slug(&self, slug: &str) -> Result<Option<GameRow>, RepositoryError> {
        let game = sqlx::query_as::<_, GameRow>("SELECT * FROM games WHERE slug = $1")
            .bind(slug)
            .fetch_optional(&self.pool)
            .await?;

        Ok(game)
    }

    /// Find a game by UUID.
    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<GameRow>, RepositoryError> {
        let game = sqlx::query_as::<_, GameRow>("SELECT * FROM games WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(game)
    }

    /// List all games.
    pub async fn list(&self) -> Result<Vec<GameRow>, RepositoryError> {
        let games = sqlx::query_as::<_, GameRow>(
            "SELECT * FROM games ORDER BY sort_order, display_name",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(games)
    }

    /// List active games only.
    pub async fn list_active(&self) -> Result<Vec<GameRow>, RepositoryError> {
        let games = sqlx::query_as::<_, GameRow>(
            "SELECT * FROM games WHERE status = 'active' ORDER BY sort_order, display_name",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(games)
    }

    /// Create a new game.
    pub async fn create(&self, new_game: NewGame) -> Result<GameRow, RepositoryError> {
        let game = sqlx::query_as::<_, GameRow>(
            r#"
            INSERT INTO games (slug, display_name, short_name, description, plugin_id, plugin_version,
                              team_size_min, team_size_max, team_size_default)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING *
            "#,
        )
        .bind(&new_game.slug)
        .bind(&new_game.display_name)
        .bind(&new_game.short_name)
        .bind(&new_game.description)
        .bind(&new_game.plugin_id)
        .bind(&new_game.plugin_version)
        .bind(new_game.team_size_min)
        .bind(new_game.team_size_max)
        .bind(new_game.team_size_default)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RepositoryError::from_sqlx_error(e, &new_game.slug))?;

        Ok(game)
    }

    /// Update a game by slug.
    pub async fn update(&self, slug: &str, update: UpdateGame) -> Result<GameRow, RepositoryError> {
        let game = sqlx::query_as::<_, GameRow>(
            r#"
            UPDATE games SET
                display_name = COALESCE($2, display_name),
                short_name = COALESCE($3, short_name),
                description = COALESCE($4, description),
                icon_url = COALESCE($5, icon_url),
                logo_url = COALESCE($6, logo_url),
                banner_url = COALESCE($7, banner_url),
                config = COALESCE($8, config),
                available_maps = COALESCE($9, available_maps),
                default_map_pool = COALESCE($10, default_map_pool),
                rank_tiers = COALESCE($11, rank_tiers),
                status = COALESCE($12, status),
                is_featured = COALESCE($13, is_featured),
                sort_order = COALESCE($14, sort_order),
                updated_at = NOW()
            WHERE slug = $1
            RETURNING *
            "#,
        )
        .bind(slug)
        .bind(update.display_name)
        .bind(update.short_name)
        .bind(update.description)
        .bind(update.icon_url)
        .bind(update.logo_url)
        .bind(update.banner_url)
        .bind(update.config)
        .bind(update.available_maps)
        .bind(update.default_map_pool)
        .bind(update.rank_tiers)
        .bind(update.status)
        .bind(update.is_featured)
        .bind(update.sort_order)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| RepositoryError::not_found("Game", slug.to_string()))?;

        Ok(game)
    }

    /// Enable a game by slug.
    pub async fn enable(&self, slug: &str) -> Result<GameRow, RepositoryError> {
        let game = sqlx::query_as::<_, GameRow>(
            r#"
            UPDATE games SET
                status = 'active',
                updated_at = NOW()
            WHERE slug = $1
            RETURNING *
            "#,
        )
        .bind(slug)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| RepositoryError::not_found("Game", slug.to_string()))?;

        Ok(game)
    }

    /// Disable a game (set to maintenance) by slug.
    pub async fn disable(&self, slug: &str) -> Result<GameRow, RepositoryError> {
        let game = sqlx::query_as::<_, GameRow>(
            r#"
            UPDATE games SET
                status = 'maintenance',
                updated_at = NOW()
            WHERE slug = $1
            RETURNING *
            "#,
        )
        .bind(slug)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| RepositoryError::not_found("Game", slug.to_string()))?;

        Ok(game)
    }

    /// Check if a game slug exists.
    pub async fn exists(&self, slug: &str) -> Result<bool, RepositoryError> {
        let row = sqlx::query("SELECT 1 FROM games WHERE slug = $1")
            .bind(slug)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::UpdateGame;
    use portal_test::database::TestDb;

    fn new_test_game(id: &str) -> NewGame {
        NewGame {
            id: id.to_string(),
            display_name: format!("{} Game", id),
            short_name: Some(id.to_string()),
            description: Some("A test game".to_string()),
            plugin_id: format!("{}_plugin", id),
            plugin_version: "1.0.0".to_string(),
            team_size_min: 1,
            team_size_max: 5,
            team_size_default: 5,
        }
    }

    #[tokio::test]
    async fn test_create_game() {
        let db = TestDb::new().await;
        let repo = GameRepository::new(db.pool.clone());

        let game = repo.create(new_test_game("testgame")).await.unwrap();

        assert_eq!(game.id, "testgame");
        assert_eq!(game.display_name, "testgame Game");
        assert_eq!(game.status, "active");
        assert_eq!(game.team_size_default, 5);
    }

    #[tokio::test]
    async fn test_find_game_by_id() {
        let db = TestDb::new().await;
        let repo = GameRepository::new(db.pool.clone());

        repo.create(new_test_game("findgame")).await.unwrap();

        let found = repo.find_by_id("findgame").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().display_name, "findgame Game");

        let not_found = repo.find_by_id("nonexistent").await.unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_list_games() {
        let db = TestDb::new().await;
        let repo = GameRepository::new(db.pool.clone());

        // Migration inserts 2 default games (cs2, aoe4)
        let initial_count = repo.list().await.unwrap().len();

        for i in 1..=3 {
            repo.create(new_test_game(&format!("listgame{}", i))).await.unwrap();
        }

        let games = repo.list().await.unwrap();
        assert_eq!(games.len(), initial_count + 3);
    }

    #[tokio::test]
    async fn test_list_active_games() {
        let db = TestDb::new().await;
        let repo = GameRepository::new(db.pool.clone());

        // Migration inserts 2 default games (cs2, aoe4) which are active
        let initial_active = repo.list_active().await.unwrap().len();

        // Create games
        repo.create(new_test_game("active1")).await.unwrap();
        repo.create(new_test_game("active2")).await.unwrap();
        repo.create(new_test_game("inactive")).await.unwrap();

        // Disable one game
        repo.disable("inactive").await.unwrap();

        let active_games = repo.list_active().await.unwrap();
        assert_eq!(active_games.len(), initial_active + 2);

        // Verify the disabled game is not in the list
        assert!(active_games.iter().all(|g| g.id != "inactive"));
    }

    #[tokio::test]
    async fn test_enable_disable_game() {
        let db = TestDb::new().await;
        let repo = GameRepository::new(db.pool.clone());

        repo.create(new_test_game("togglegame")).await.unwrap();

        // Disable
        let disabled = repo.disable("togglegame").await.unwrap();
        assert_eq!(disabled.status, "maintenance");

        // Enable
        let enabled = repo.enable("togglegame").await.unwrap();
        assert_eq!(enabled.status, "active");
    }

    #[tokio::test]
    async fn test_update_game() {
        let db = TestDb::new().await;
        let repo = GameRepository::new(db.pool.clone());

        repo.create(new_test_game("updategame")).await.unwrap();

        let update = UpdateGame {
            display_name: Some("Updated Game Name".to_string()),
            description: Some("Updated description".to_string()),
            is_featured: Some(true),
            sort_order: Some(10),
            ..Default::default()
        };

        let updated = repo.update("updategame", update).await.unwrap();
        assert_eq!(updated.display_name, "Updated Game Name");
        assert_eq!(updated.description, Some("Updated description".to_string()));
        assert!(updated.is_featured);
        assert_eq!(updated.sort_order, 10);
    }

    #[tokio::test]
    async fn test_game_exists() {
        let db = TestDb::new().await;
        let repo = GameRepository::new(db.pool.clone());

        repo.create(new_test_game("existsgame")).await.unwrap();

        assert!(repo.exists("existsgame").await.unwrap());
        assert!(!repo.exists("doesnotexist").await.unwrap());
    }
}
