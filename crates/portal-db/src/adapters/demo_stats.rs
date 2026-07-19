//! Demo stat-fact (EAV) repository adapter.
//!
//! Backs `demo_player_stats` — per-`(demo, steam_id, stat_key)` fact rows —
//! and the leaderboard aggregation over them.

use crate::DbPool;
use async_trait::async_trait;
use portal_core::{DemoId, DomainError};
use portal_domain::entities::award::{MinQualifierType, StatAggregation, StatDirection};
use portal_domain::repositories::demo_stats::{
    DemoPlayerStatsRepository, DemoStatFact, LeaderboardEntry, LeaderboardQuery, LeaderboardScope,
};
use sqlx::FromRow;
use uuid::Uuid;

/// One aggregated leaderboard row (query-shaped, not a table row).
#[derive(Debug, FromRow)]
struct LeaderboardRow {
    player_id: Uuid,
    display_name: String,
    avatar_url: Option<String>,
    value: f64,
    demos_counted: i64,
}

/// Postgres implementation of [`DemoPlayerStatsRepository`].
#[derive(Clone)]
pub struct PgDemoPlayerStatsRepository {
    pool: DbPool,
}

impl PgDemoPlayerStatsRepository {
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DemoPlayerStatsRepository for PgDemoPlayerStatsRepository {
    async fn replace_for_demo(
        &self,
        demo_id: DemoId,
        extractor_version: i32,
        facts: Vec<DemoStatFact>,
    ) -> Result<u64, DomainError> {
        // Defensive dedupe on the unique key (demo, steam_id, stat_key):
        // a duplicate pair in one INSERT would abort the whole statement.
        let mut seen = std::collections::HashSet::with_capacity(facts.len());
        let mut deduped = Vec::with_capacity(facts.len());
        for fact in facts.into_iter().rev() {
            if seen.insert((fact.steam_id.clone(), fact.stat_key.clone())) {
                deduped.push(fact);
            }
        }

        let mut tx = self.pool.begin().await.map_err(|e| {
            DomainError::internal(format!("Failed to begin stat-fact transaction: {e}"))
        })?;

        sqlx::query("DELETE FROM demo_player_stats WHERE demo_id = $1")
            .bind(demo_id.as_uuid())
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::internal(format!("Failed to delete stat facts: {e}")))?;

        let inserted = if deduped.is_empty() {
            0
        } else {
            let steam_ids: Vec<String> = deduped.iter().map(|f| f.steam_id.clone()).collect();
            let player_ids: Vec<Option<Uuid>> = deduped
                .iter()
                .map(|f| f.player_id.map(|id| id.as_uuid()))
                .collect();
            let stat_keys: Vec<String> = deduped.iter().map(|f| f.stat_key.clone()).collect();
            let values: Vec<f64> = deduped.iter().map(|f| f.value).collect();

            let result = sqlx::query(
                r"
                INSERT INTO demo_player_stats
                    (demo_id, steam_id, player_id, stat_key, value, extractor_version)
                SELECT $1, u.steam_id, u.player_id, u.stat_key, u.value, $2
                FROM UNNEST($3::text[], $4::uuid[], $5::text[], $6::float8[])
                    AS u(steam_id, player_id, stat_key, value)
                ",
            )
            .bind(demo_id.as_uuid())
            .bind(extractor_version)
            .bind(&steam_ids)
            .bind(&player_ids)
            .bind(&stat_keys)
            .bind(&values)
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::internal(format!("Failed to insert stat facts: {e}")))?;
            result.rows_affected()
        };

        // Resolve steam_id -> players.steam_id_64 (same guard as
        // demo_players resolution: numeric check keeps the ::bigint cast
        // safe and the index usable).
        sqlx::query(
            r"
            UPDATE demo_player_stats s
            SET player_id = p.id
            FROM players p
            WHERE s.demo_id = $1
              AND s.player_id IS NULL
              AND s.steam_id ~ '^[0-9]{1,19}$'
              AND p.steam_id_64 = s.steam_id::bigint
            ",
        )
        .bind(demo_id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::internal(format!("Failed to resolve stat-fact players: {e}")))?;

        tx.commit().await.map_err(|e| {
            DomainError::internal(format!("Failed to commit stat-fact transaction: {e}"))
        })?;

        Ok(inserted)
    }

    async fn leaderboard(
        &self,
        query: &LeaderboardQuery,
    ) -> Result<Vec<LeaderboardEntry>, DomainError> {
        // The SQL is assembled from fixed fragments selected by enums; every
        // user-influenced value (scope id, stat key, thresholds, limit) is a
        // bound parameter.
        let scope_predicate = match query.scope {
            LeaderboardScope::Tournament(_) => "WHERE tm.tournament_id = $1",
            LeaderboardScope::Season(_) => {
                "JOIN tournaments t ON t.id = tm.tournament_id WHERE t.season_id = $1"
            }
        };
        let scope_id = match query.scope {
            LeaderboardScope::Tournament(id) => id.as_uuid(),
            LeaderboardScope::Season(id) => id.as_uuid(),
        };
        let aggregate = match query.aggregation {
            StatAggregation::Sum => "SUM(s.value)",
            StatAggregation::MaxSingleDemo => "MAX(s.value)",
            StatAggregation::AvgPerDemo => "AVG(s.value)",
        };
        let order = match query.direction {
            StatDirection::Desc => "DESC",
            StatDirection::Asc => "ASC",
        };
        let (min_demos, min_rounds) = match query.min_qualifier {
            Some(q) if q.qualifier_type == MinQualifierType::Matches => (i64::from(q.value), 0.0),
            Some(q) => (1_i64, f64::from(q.value)),
            None => (1_i64, 0.0),
        };

        // A demo can link to several matches; `scoped_demos` collapses the
        // links to DISTINCT demo ids so facts are never double-counted.
        let sql = format!(
            r"
            WITH scoped_demos AS (
                SELECT DISTINCT dml.demo_id AS id
                FROM demo_match_links dml
                JOIN tournament_matches tm ON tm.id = dml.match_id
                {scope_predicate}
            )
            SELECT s.player_id,
                   p.display_name,
                   p.avatar_url,
                   ({aggregate})::float8 AS value,
                   COUNT(DISTINCT s.demo_id) AS demos_counted
            FROM demo_player_stats s
            JOIN players p ON p.id = s.player_id
            WHERE s.stat_key = $2
              AND s.player_id IS NOT NULL
              AND s.demo_id IN (SELECT id FROM scoped_demos)
            GROUP BY s.player_id, p.display_name, p.avatar_url
            HAVING COUNT(DISTINCT s.demo_id) >= $3
               AND COALESCE((
                    SELECT SUM(r.value)
                    FROM demo_player_stats r
                    WHERE r.stat_key = 'rounds_played'
                      AND r.player_id = s.player_id
                      AND r.demo_id IN (SELECT id FROM scoped_demos)
                   ), 0) >= $4
            ORDER BY value {order}, p.display_name ASC
            LIMIT $5
            "
        );

        let rows = sqlx::query_as::<_, LeaderboardRow>(&sql)
            .bind(scope_id)
            .bind(&query.stat_key)
            .bind(min_demos)
            .bind(min_rounds)
            .bind(query.limit)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::internal(format!("Leaderboard query failed: {e}")))?;

        Ok(rows
            .into_iter()
            .map(|row| LeaderboardEntry {
                player_id: row.player_id.into(),
                display_name: row.display_name,
                avatar_url: row.avatar_url,
                value: row.value,
                demos_counted: row.demos_counted,
            })
            .collect())
    }
}
