//! PostgreSQL implementations of Demo repositories.

use crate::DbPool;
use crate::entities::{
    DemoMatchLinkRow, DemoPlayerRow, DemoRow, NewDemo, NewDemoMatchLink, NewDemoPlayer,
};
use async_trait::async_trait;
use portal_core::{
    DemoCategory, DemoId, DemoLinkType, DemoMatchLinkId, DemoPlayerId, DemoStatus, DomainError,
    GameId, LeagueId, PlayerId, TournamentId, TournamentMatchId, UserId,
};
use portal_domain::entities::demo::{
    Demo, DemoFilter, DemoListResult, DemoMatchLink, DemoPlayer, DemoPlayerStats,
    ParsedDemoMetadata,
};
use portal_domain::repositories::demo::{
    CreateDemo, CreateDemoMatchLink, CreateDemoPlayer, DemoMatchLinkRepository,
    DemoMatchLinkWithData, DemoPlayerRepository, DemoRepository,
};

// =============================================================================
// DEMO REPOSITORY
// =============================================================================

/// PostgreSQL implementation of DemoRepository.
#[derive(Debug, Clone)]
pub struct PgDemoRepository {
    pool: DbPool,
}

impl PgDemoRepository {
    /// Create a new repository instance.
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DemoRepository for PgDemoRepository {
    async fn find_by_id(&self, id: DemoId) -> Result<Option<Demo>, DomainError> {
        let row = sqlx::query_as::<_, DemoRow>(r"SELECT * FROM demos WHERE id = $1")
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::internal(format!("Failed to find demo: {e}")))?;

        row.map(demo_row_to_domain).transpose()
    }

    async fn find_by_s3_key(&self, bucket: &str, key: &str) -> Result<Option<Demo>, DomainError> {
        let row = sqlx::query_as::<_, DemoRow>(
            r"SELECT * FROM demos WHERE s3_bucket = $1 AND s3_key = $2",
        )
        .bind(bucket)
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to find demo by S3 key: {e}")))?;

        row.map(demo_row_to_domain).transpose()
    }

    async fn list(&self, filter: DemoFilter) -> Result<DemoListResult, DomainError> {
        // Build WHERE clause
        let mut conditions = vec!["TRUE".to_string()];
        let mut param_index = 1;

        if filter.game_id.is_some() {
            conditions.push(format!("game_id = ${param_index}"));
            param_index += 1;
        }
        if filter.category.is_some() {
            conditions.push(format!("category = ${param_index}"));
            param_index += 1;
        }
        if filter.status.is_some() {
            conditions.push(format!("status = ${param_index}"));
            param_index += 1;
        }
        if filter.league_id.is_some() {
            conditions.push(format!("league_id = ${param_index}"));
            param_index += 1;
        }
        if filter.tournament_id.is_some() {
            conditions.push(format!("tournament_id = ${param_index}"));
            param_index += 1;
        }
        if filter.map_name.is_some() {
            conditions.push(format!("metadata->>'map_name' ILIKE ${param_index}"));
            param_index += 1;
        }
        if filter.team_name_contains.is_some() {
            conditions.push(format!(
                "(metadata->>'team1_name' ILIKE ${param_index} OR metadata->>'team2_name' ILIKE ${param_index})"
            ));
            param_index += 1;
        }
        if filter.steam_id.is_some() {
            conditions.push(format!(
                "id IN (SELECT demo_id FROM demo_players WHERE steam_id = ${param_index})"
            ));
            param_index += 1;
        }
        if filter.match_date_from.is_some() {
            conditions.push(format!(
                "(metadata->>'match_date')::timestamptz >= ${param_index}"
            ));
            param_index += 1;
        }
        if filter.match_date_to.is_some() {
            conditions.push(format!(
                "(metadata->>'match_date')::timestamptz <= ${param_index}"
            ));
            param_index += 1;
        }
        if !filter.include_hidden {
            conditions.push("is_hidden = false".to_string());
            conditions.push("category != 'ignored'".to_string());
        }

        let where_clause = conditions.join(" AND ");

        // Count query
        let count_query = format!("SELECT COUNT(*) FROM demos WHERE {where_clause}");
        let list_query = format!(
            "SELECT * FROM demos WHERE {where_clause} ORDER BY discovered_at DESC LIMIT ${param_index} OFFSET ${}",
            param_index + 1
        );

        // Execute count with bound parameters
        let mut count_builder = sqlx::query_scalar::<_, i64>(&count_query);
        let mut list_builder = sqlx::query_as::<_, DemoRow>(&list_query);

        // Bind parameters in the same order
        if let Some(game_id) = filter.game_id {
            count_builder = count_builder.bind(game_id.as_uuid());
            list_builder = list_builder.bind(game_id.as_uuid());
        }
        if let Some(category) = &filter.category {
            count_builder = count_builder.bind(category.to_string());
            list_builder = list_builder.bind(category.to_string());
        }
        if let Some(status) = &filter.status {
            count_builder = count_builder.bind(status.to_string());
            list_builder = list_builder.bind(status.to_string());
        }
        if let Some(league_id) = filter.league_id {
            count_builder = count_builder.bind(league_id.as_uuid());
            list_builder = list_builder.bind(league_id.as_uuid());
        }
        if let Some(tournament_id) = filter.tournament_id {
            count_builder = count_builder.bind(tournament_id.as_uuid());
            list_builder = list_builder.bind(tournament_id.as_uuid());
        }
        if let Some(map_name) = &filter.map_name {
            let pattern = format!("%{map_name}%");
            count_builder = count_builder.bind(pattern.clone());
            list_builder = list_builder.bind(pattern);
        }
        if let Some(team_name) = &filter.team_name_contains {
            let pattern = format!("%{team_name}%");
            count_builder = count_builder.bind(pattern.clone());
            list_builder = list_builder.bind(pattern);
        }
        if let Some(steam_id) = &filter.steam_id {
            count_builder = count_builder.bind(steam_id);
            list_builder = list_builder.bind(steam_id);
        }
        if let Some(from) = filter.match_date_from {
            count_builder = count_builder.bind(from);
            list_builder = list_builder.bind(from);
        }
        if let Some(to) = filter.match_date_to {
            count_builder = count_builder.bind(to);
            list_builder = list_builder.bind(to);
        }

        // Bind limit and offset for list query
        let limit = filter.limit.unwrap_or(50);
        let offset = filter.offset.unwrap_or(0);
        list_builder = list_builder.bind(limit).bind(offset);

        let total = count_builder
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::internal(format!("Failed to count demos: {e}")))?;

        let rows = list_builder
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::internal(format!("Failed to list demos: {e}")))?;

        let demos: Result<Vec<Demo>, DomainError> =
            rows.into_iter().map(demo_row_to_domain).collect();

        Ok(DemoListResult {
            demos: demos?,
            total,
        })
    }

    async fn create(&self, demo: CreateDemo) -> Result<Demo, DomainError> {
        let new_demo = NewDemo {
            game_id: demo.game_id.as_uuid(),
            file_name: demo.file_name,
            s3_bucket: demo.s3_bucket,
            s3_key: demo.s3_key,
            file_size_bytes: demo.file_size_bytes,
            discovered_at: demo.discovered_at,
        };

        let row = sqlx::query_as::<_, DemoRow>(
            r"
            INSERT INTO demos (game_id, file_name, s3_bucket, s3_key, file_size_bytes, discovered_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            ",
        )
        .bind(new_demo.game_id)
        .bind(&new_demo.file_name)
        .bind(&new_demo.s3_bucket)
        .bind(&new_demo.s3_key)
        .bind(new_demo.file_size_bytes)
        .bind(new_demo.discovered_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to create demo: {e}")))?;

        demo_row_to_domain(row)
    }

    async fn update_stats(
        &self,
        id: DemoId,
        metadata: ParsedDemoMetadata,
        stats_json: serde_json::Value,
    ) -> Result<Demo, DomainError> {
        let metadata_json = serde_json::to_value(&metadata)
            .map_err(|e| DomainError::internal(format!("Failed to serialize metadata: {e}")))?;

        let row = sqlx::query_as::<_, DemoRow>(
            r"
            UPDATE demos
            SET metadata = $2, stats_json = $3, status = 'ready', stats_fetched_at = NOW(), updated_at = NOW()
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(&metadata_json)
        .bind(&stats_json)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to update demo stats: {e}")))?;

        demo_row_to_domain(row)
    }

    async fn update_status(&self, id: DemoId, status: DemoStatus) -> Result<Demo, DomainError> {
        let row = sqlx::query_as::<_, DemoRow>(
            r"UPDATE demos SET status = $2, updated_at = NOW() WHERE id = $1 RETURNING *",
        )
        .bind(id.as_uuid())
        .bind(status.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to update demo status: {e}")))?;

        demo_row_to_domain(row)
    }

    async fn mark_stats_failed(&self, id: DemoId, error: &str) -> Result<Demo, DomainError> {
        let row = sqlx::query_as::<_, DemoRow>(
            r"
            UPDATE demos
            SET status = 'failed', stats_fetch_error = $2, updated_at = NOW()
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(error)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to mark demo stats failed: {e}")))?;

        demo_row_to_domain(row)
    }

    async fn categorize(
        &self,
        id: DemoId,
        category: DemoCategory,
        by_user_id: UserId,
    ) -> Result<Demo, DomainError> {
        let row = sqlx::query_as::<_, DemoRow>(
            r"
            UPDATE demos
            SET category = $2, categorized_by_user_id = $3, categorized_at = NOW(), updated_at = NOW()
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(category.to_string())
        .bind(by_user_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to categorize demo: {e}")))?;

        demo_row_to_domain(row)
    }

    async fn set_visibility(
        &self,
        id: DemoId,
        is_hidden: bool,
        by_user_id: UserId,
    ) -> Result<Demo, DomainError> {
        let row = sqlx::query_as::<_, DemoRow>(
            r"
            UPDATE demos
            SET is_hidden = $2, hidden_by_user_id = $3, hidden_at = CASE WHEN $2 THEN NOW() ELSE NULL END, updated_at = NOW()
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(is_hidden)
        .bind(by_user_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to set demo visibility: {e}")))?;

        demo_row_to_domain(row)
    }

    async fn associate(
        &self,
        id: DemoId,
        league_id: Option<LeagueId>,
        tournament_id: Option<TournamentId>,
    ) -> Result<Demo, DomainError> {
        let row = sqlx::query_as::<_, DemoRow>(
            r"
            UPDATE demos
            SET league_id = $2, tournament_id = $3, updated_at = NOW()
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(league_id.map(|id| id.as_uuid()))
        .bind(tournament_id.map(|id| id.as_uuid()))
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to associate demo: {e}")))?;

        demo_row_to_domain(row)
    }

    async fn set_admin_notes(
        &self,
        id: DemoId,
        notes: Option<String>,
    ) -> Result<Demo, DomainError> {
        let row = sqlx::query_as::<_, DemoRow>(
            r"UPDATE demos SET admin_notes = $2, updated_at = NOW() WHERE id = $1 RETURNING *",
        )
        .bind(id.as_uuid())
        .bind(&notes)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to set admin notes: {e}")))?;

        demo_row_to_domain(row)
    }

    async fn find_pending_processing(&self, limit: i64) -> Result<Vec<Demo>, DomainError> {
        let rows = sqlx::query_as::<_, DemoRow>(
            r"SELECT * FROM demos WHERE status = 'pending' ORDER BY discovered_at ASC LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to find pending demos: {e}")))?;

        rows.into_iter().map(demo_row_to_domain).collect()
    }

    async fn count_by_status(&self) -> Result<Vec<(DemoStatus, i64)>, DomainError> {
        let rows = sqlx::query_as::<_, (String, i64)>(
            r"SELECT status, COUNT(*) FROM demos GROUP BY status",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to count demos by status: {e}")))?;

        rows.into_iter()
            .map(|(status_str, count)| {
                let status: DemoStatus = status_str
                    .parse()
                    .map_err(|e: String| DomainError::internal(e))?;
                Ok((status, count))
            })
            .collect()
    }

    async fn delete(&self, id: DemoId) -> Result<(), DomainError> {
        sqlx::query(r"DELETE FROM demos WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::internal(format!("Failed to delete demo: {e}")))?;

        Ok(())
    }

    async fn find_matching_for_context(
        &self,
        game_id: GameId,
        steam_ids: &[String],
        time_from: Option<chrono::DateTime<chrono::Utc>>,
        time_to: Option<chrono::DateTime<chrono::Utc>>,
        exclude_match_id: Option<TournamentMatchId>,
        limit: i64,
    ) -> Result<Vec<portal_domain::entities::demo::Demo>, DomainError> {
        if steam_ids.is_empty() {
            return Ok(Vec::new());
        }

        let rows = sqlx::query_as::<_, DemoRow>(
            r"
            SELECT DISTINCT d.*
            FROM demos d
            JOIN demo_players dp ON dp.demo_id = d.id
            WHERE d.game_id = $1
              AND d.status = 'ready'
              AND d.is_hidden = false
              AND dp.steam_id = ANY($2)
              AND ($3::timestamptz IS NULL OR (d.metadata->>'match_date')::timestamptz >= $3)
              AND ($4::timestamptz IS NULL OR (d.metadata->>'match_date')::timestamptz <= $4)
              AND ($5::uuid IS NULL OR NOT EXISTS (
                  SELECT 1 FROM demo_match_links dml WHERE dml.demo_id = d.id AND dml.match_id = $5
              ))
            ORDER BY d.discovered_at DESC
            LIMIT $6
            ",
        )
        .bind(game_id.as_uuid())
        .bind(steam_ids)
        .bind(time_from)
        .bind(time_to)
        .bind(exclude_match_id.map(|id| id.as_uuid()))
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to find matching demos: {e}")))?;

        rows.into_iter().map(demo_row_to_domain).collect()
    }
}

// =============================================================================
// DEMO MATCH LINK REPOSITORY
// =============================================================================

/// PostgreSQL implementation of DemoMatchLinkRepository.
#[derive(Debug, Clone)]
pub struct PgDemoMatchLinkRepository {
    pool: DbPool,
}

impl PgDemoMatchLinkRepository {
    /// Create a new repository instance.
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DemoMatchLinkRepository for PgDemoMatchLinkRepository {
    async fn find_by_id(&self, id: DemoMatchLinkId) -> Result<Option<DemoMatchLink>, DomainError> {
        let row =
            sqlx::query_as::<_, DemoMatchLinkRow>(r"SELECT * FROM demo_match_links WHERE id = $1")
                .bind(id.as_uuid())
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| {
                    DomainError::internal(format!("Failed to find demo match link: {e}"))
                })?;

        row.map(link_row_to_domain).transpose()
    }

    async fn find_by_ids(
        &self,
        ids: &[DemoMatchLinkId],
    ) -> Result<Vec<DemoMatchLink>, DomainError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let uuids: Vec<uuid::Uuid> = ids
            .iter()
            .map(portal_core::DemoMatchLinkId::as_uuid)
            .collect();
        let rows = sqlx::query_as::<_, DemoMatchLinkRow>(
            r"SELECT * FROM demo_match_links WHERE id = ANY($1)",
        )
        .bind(&uuids)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to find demo match links: {e}")))?;

        rows.into_iter().map(link_row_to_domain).collect()
    }

    async fn find_by_demo(&self, demo_id: DemoId) -> Result<Vec<DemoMatchLink>, DomainError> {
        let rows = sqlx::query_as::<_, DemoMatchLinkRow>(
            r"SELECT * FROM demo_match_links WHERE demo_id = $1 ORDER BY linked_at DESC",
        )
        .bind(demo_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to find demo links: {e}")))?;

        rows.into_iter().map(link_row_to_domain).collect()
    }

    async fn find_by_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<DemoMatchLink>, DomainError> {
        let rows = sqlx::query_as::<_, DemoMatchLinkRow>(
            r"SELECT * FROM demo_match_links WHERE match_id = $1 ORDER BY game_number, linked_at",
        )
        .bind(match_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to find match links: {e}")))?;

        rows.into_iter().map(link_row_to_domain).collect()
    }

    async fn find_by_match_with_demos(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<DemoMatchLinkWithData>, DomainError> {
        // First get all links for this match
        let link_rows = sqlx::query_as::<_, DemoMatchLinkRow>(
            r"SELECT * FROM demo_match_links WHERE match_id = $1 ORDER BY game_number, linked_at",
        )
        .bind(match_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to find match links: {e}")))?;

        if link_rows.is_empty() {
            return Ok(Vec::new());
        }

        // Get demo IDs from links
        let demo_ids: Vec<uuid::Uuid> = link_rows.iter().map(|l| l.demo_id).collect();

        // Fetch all demos
        let demo_rows = sqlx::query_as::<_, DemoRow>(r"SELECT * FROM demos WHERE id = ANY($1)")
            .bind(&demo_ids)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::internal(format!("Failed to find demos: {e}")))?;

        // Fetch all players for these demos
        let player_rows = sqlx::query_as::<_, DemoPlayerRow>(
            r"SELECT * FROM demo_players WHERE demo_id = ANY($1) ORDER BY team_name, kills DESC",
        )
        .bind(&demo_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to find demo players: {e}")))?;

        // Build lookup maps
        let demos_map: std::collections::HashMap<uuid::Uuid, Demo> = demo_rows
            .into_iter()
            .filter_map(|row| {
                let id = row.id;
                match demo_row_to_domain(row) {
                    Ok(d) => Some((id, d)),
                    Err(e) => {
                        tracing::warn!(demo_id = %id, error = %e, "demo_row_to_domain failed");
                        None
                    }
                }
            })
            .collect();

        let mut players_map: std::collections::HashMap<uuid::Uuid, Vec<DemoPlayer>> =
            std::collections::HashMap::new();
        for row in player_rows {
            let demo_id = row.demo_id;
            let player = player_row_to_domain(row);
            players_map.entry(demo_id).or_default().push(player);
        }

        // Build result
        let mut results = Vec::with_capacity(link_rows.len());
        for link_row in link_rows {
            let link = link_row_to_domain(link_row.clone())?;
            if let Some(demo) = demos_map.get(&link_row.demo_id).cloned() {
                let players = players_map.remove(&link_row.demo_id).unwrap_or_default();
                results.push(DemoMatchLinkWithData {
                    link,
                    demo,
                    players,
                });
            }
        }

        Ok(results)
    }

    async fn find_by_demo_and_match(
        &self,
        demo_id: DemoId,
        match_id: TournamentMatchId,
    ) -> Result<Option<DemoMatchLink>, DomainError> {
        let row = sqlx::query_as::<_, DemoMatchLinkRow>(
            r"SELECT * FROM demo_match_links WHERE demo_id = $1 AND match_id = $2",
        )
        .bind(demo_id.as_uuid())
        .bind(match_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to find link: {e}")))?;

        row.map(link_row_to_domain).transpose()
    }

    async fn create(&self, link: CreateDemoMatchLink) -> Result<DemoMatchLink, DomainError> {
        let new_link = NewDemoMatchLink {
            demo_id: link.demo_id.as_uuid(),
            match_id: link.match_id.as_uuid(),
            game_number: link.game_number,
            link_type: link.link_type.to_string(),
            confidence_score: link.confidence_score,
            linked_by_user_id: link.linked_by_user_id.map(|id| id.as_uuid()),
        };

        let row = sqlx::query_as::<_, DemoMatchLinkRow>(
            r"
            INSERT INTO demo_match_links (demo_id, match_id, game_number, link_type, confidence_score, linked_by_user_id)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            ",
        )
        .bind(new_link.demo_id)
        .bind(new_link.match_id)
        .bind(new_link.game_number)
        .bind(&new_link.link_type)
        .bind(new_link.confidence_score)
        .bind(new_link.linked_by_user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to create demo match link: {e}")))?;

        link_row_to_domain(row)
    }

    async fn mark_validated(
        &self,
        id: DemoMatchLinkId,
        validation_result: serde_json::Value,
    ) -> Result<DemoMatchLink, DomainError> {
        let row = sqlx::query_as::<_, DemoMatchLinkRow>(
            r"
            UPDATE demo_match_links
            SET validated = true, validated_at = NOW(), validation_result = $2
            WHERE id = $1
            RETURNING *
            ",
        )
        .bind(id.as_uuid())
        .bind(&validation_result)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to mark link validated: {e}")))?;

        link_row_to_domain(row)
    }

    async fn delete(&self, id: DemoMatchLinkId) -> Result<(), DomainError> {
        sqlx::query(r"DELETE FROM demo_match_links WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::internal(format!("Failed to delete demo match link: {e}")))?;

        Ok(())
    }

    async fn delete_by_demo(&self, demo_id: DemoId) -> Result<(), DomainError> {
        sqlx::query(r"DELETE FROM demo_match_links WHERE demo_id = $1")
            .bind(demo_id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::internal(format!("Failed to delete demo links: {e}")))?;

        Ok(())
    }
}

// =============================================================================
// DEMO PLAYER REPOSITORY
// =============================================================================

/// PostgreSQL implementation of DemoPlayerRepository.
#[derive(Debug, Clone)]
pub struct PgDemoPlayerRepository {
    pool: DbPool,
}

impl PgDemoPlayerRepository {
    /// Create a new repository instance.
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DemoPlayerRepository for PgDemoPlayerRepository {
    async fn find_by_id(&self, id: DemoPlayerId) -> Result<Option<DemoPlayer>, DomainError> {
        let row = sqlx::query_as::<_, DemoPlayerRow>(r"SELECT * FROM demo_players WHERE id = $1")
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DomainError::internal(format!("Failed to find demo player: {e}")))?;

        Ok(row.map(player_row_to_domain))
    }

    async fn find_by_demo(&self, demo_id: DemoId) -> Result<Vec<DemoPlayer>, DomainError> {
        let rows = sqlx::query_as::<_, DemoPlayerRow>(
            r"SELECT * FROM demo_players WHERE demo_id = $1 ORDER BY team_name, kills DESC",
        )
        .bind(demo_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to find demo players: {e}")))?;

        Ok(rows.into_iter().map(player_row_to_domain).collect())
    }

    async fn find_demos_by_steam_id(&self, steam_id: &str) -> Result<Vec<DemoId>, DomainError> {
        let rows = sqlx::query_scalar::<_, uuid::Uuid>(
            r"SELECT DISTINCT demo_id FROM demo_players WHERE steam_id = $1 ORDER BY demo_id",
        )
        .bind(steam_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to find demos by steam_id: {e}")))?;

        Ok(rows.into_iter().map(DemoId::from_uuid).collect())
    }

    async fn find_by_steam_id(&self, steam_id: &str) -> Result<Vec<DemoPlayer>, DomainError> {
        let rows = sqlx::query_as::<_, DemoPlayerRow>(
            r"SELECT * FROM demo_players WHERE steam_id = $1 ORDER BY created_at DESC",
        )
        .bind(steam_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to find players by steam_id: {e}")))?;

        Ok(rows.into_iter().map(player_row_to_domain).collect())
    }

    async fn create_batch(
        &self,
        demo_id: DemoId,
        players: Vec<CreateDemoPlayer>,
    ) -> Result<Vec<DemoPlayer>, DomainError> {
        let mut results = Vec::new();

        for player in players {
            let new_player = NewDemoPlayer {
                demo_id: demo_id.as_uuid(),
                steam_id: player.steam_id,
                player_name: player.player_name,
                team_name: player.team_name,
                kills: player.stats.kills,
                deaths: player.stats.deaths,
                assists: player.stats.assists,
                damage: player.stats.damage,
                adr: player.stats.adr,
                headshot_kills: player.stats.headshot_kills,
                hs_percentage: player.stats.hs_percentage,
            };

            let row = sqlx::query_as::<_, DemoPlayerRow>(
                r"
                INSERT INTO demo_players (
                    demo_id, steam_id, player_name, team_name,
                    kills, deaths, assists, damage, adr, headshot_kills, hs_percentage
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                ON CONFLICT (demo_id, steam_id) DO UPDATE SET
                    player_name = EXCLUDED.player_name,
                    team_name = EXCLUDED.team_name,
                    kills = EXCLUDED.kills,
                    deaths = EXCLUDED.deaths,
                    assists = EXCLUDED.assists,
                    damage = EXCLUDED.damage,
                    adr = EXCLUDED.adr,
                    headshot_kills = EXCLUDED.headshot_kills,
                    hs_percentage = EXCLUDED.hs_percentage
                RETURNING *
                ",
            )
            .bind(new_player.demo_id)
            .bind(&new_player.steam_id)
            .bind(&new_player.player_name)
            .bind(&new_player.team_name)
            .bind(new_player.kills)
            .bind(new_player.deaths)
            .bind(new_player.assists)
            .bind(new_player.damage)
            .bind(new_player.adr)
            .bind(new_player.headshot_kills)
            .bind(new_player.hs_percentage)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::internal(format!("Failed to create demo player: {e}")))?;

            results.push(player_row_to_domain(row));
        }

        Ok(results)
    }

    async fn link_to_player(
        &self,
        id: DemoPlayerId,
        player_id: PlayerId,
    ) -> Result<DemoPlayer, DomainError> {
        let row = sqlx::query_as::<_, DemoPlayerRow>(
            r"UPDATE demo_players SET player_id = $2 WHERE id = $1 RETURNING *",
        )
        .bind(id.as_uuid())
        .bind(player_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to link demo player: {e}")))?;

        Ok(player_row_to_domain(row))
    }

    async fn delete_by_demo(&self, demo_id: DemoId) -> Result<(), DomainError> {
        sqlx::query(r"DELETE FROM demo_players WHERE demo_id = $1")
            .bind(demo_id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::internal(format!("Failed to delete demo players: {e}")))?;

        Ok(())
    }
}

// =============================================================================
// CONVERSION FUNCTIONS
// =============================================================================

/// Convert a DemoRow to a domain Demo.
fn demo_row_to_domain(row: DemoRow) -> Result<Demo, DomainError> {
    let category: DemoCategory = row
        .category
        .parse()
        .map_err(|e: String| DomainError::internal(e))?;

    let status: DemoStatus = row
        .status
        .parse()
        .map_err(|e: String| DomainError::internal(e))?;

    let metadata: Option<ParsedDemoMetadata> = row
        .metadata
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| DomainError::internal(format!("Failed to parse metadata: {e}")))?;

    Ok(Demo {
        id: DemoId::from_uuid(row.id),
        game_id: GameId::from_uuid(row.game_id),
        file_name: row.file_name,
        s3_bucket: row.s3_bucket,
        s3_key: row.s3_key,
        file_size_bytes: row.file_size_bytes,
        category,
        is_hidden: row.is_hidden,
        league_id: row.league_id.map(LeagueId::from_uuid),
        tournament_id: row.tournament_id.map(TournamentId::from_uuid),
        metadata,
        stats_json: row.stats_json,
        status,
        stats_fetched_at: row.stats_fetched_at,
        stats_fetch_error: row.stats_fetch_error,
        categorized_by_user_id: row.categorized_by_user_id.map(UserId::from_uuid),
        categorized_at: row.categorized_at,
        hidden_by_user_id: row.hidden_by_user_id.map(UserId::from_uuid),
        hidden_at: row.hidden_at,
        admin_notes: row.admin_notes,
        discovered_at: row.discovered_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

/// Convert a DemoMatchLinkRow to a domain DemoMatchLink.
fn link_row_to_domain(row: DemoMatchLinkRow) -> Result<DemoMatchLink, DomainError> {
    let link_type: DemoLinkType = row
        .link_type
        .parse()
        .map_err(|e: String| DomainError::internal(e))?;

    Ok(DemoMatchLink {
        id: DemoMatchLinkId::from_uuid(row.id),
        demo_id: DemoId::from_uuid(row.demo_id),
        match_id: TournamentMatchId::from_uuid(row.match_id),
        game_number: row.game_number,
        link_type,
        confidence_score: row.confidence_score,
        validated: row.validated,
        validated_at: row.validated_at,
        validation_result: row.validation_result,
        linked_by_user_id: row.linked_by_user_id.map(UserId::from_uuid),
        linked_at: row.linked_at,
        created_at: row.created_at,
    })
}

/// Convert a DemoPlayerRow to a domain DemoPlayer.
fn player_row_to_domain(row: DemoPlayerRow) -> DemoPlayer {
    DemoPlayer {
        id: DemoPlayerId::from_uuid(row.id),
        demo_id: DemoId::from_uuid(row.demo_id),
        steam_id: row.steam_id,
        player_name: row.player_name,
        team_name: row.team_name,
        player_id: row.player_id.map(PlayerId::from_uuid),
        stats: DemoPlayerStats {
            kills: row.kills,
            deaths: row.deaths,
            assists: row.assists,
            damage: row.damage,
            adr: row.adr,
            headshot_kills: row.headshot_kills,
            hs_percentage: row.hs_percentage,
        },
        created_at: row.created_at,
    }
}
