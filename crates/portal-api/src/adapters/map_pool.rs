//! Adapter resolving a tournament's legal map IDs for the domain layer.

use std::sync::Arc;

use async_trait::async_trait;
use portal_core::{DomainError, TournamentId, TournamentStageId};
use portal_db::{GameRepository, PgTournamentMapPoolRepository, PgTournamentRepository};
use portal_domain::repositories::tournament::{TournamentMapPoolRepository, TournamentRepository};
use portal_domain::services::tournament::MapPoolProvider;
use portal_plugins::PluginManager;

/// Resolves valid map IDs through the same chain the veto bootstrap uses:
/// the tournament/stage map pool, then the game's default pool, then the
/// game's full map catalog (DB rows, else plugin defaults).
///
/// Returning an empty vec means "cannot determine" — the domain treats that
/// as "skip validation" rather than rejecting every map.
pub struct DbMapPoolProvider {
    map_pool_repo: Arc<PgTournamentMapPoolRepository>,
    tournament_repo: Arc<PgTournamentRepository>,
    game_repo: GameRepository,
    plugin_manager: Arc<PluginManager>,
}

impl DbMapPoolProvider {
    /// Create a new provider.
    #[must_use]
    pub const fn new(
        map_pool_repo: Arc<PgTournamentMapPoolRepository>,
        tournament_repo: Arc<PgTournamentRepository>,
        game_repo: GameRepository,
        plugin_manager: Arc<PluginManager>,
    ) -> Self {
        Self {
            map_pool_repo,
            tournament_repo,
            game_repo,
            plugin_manager,
        }
    }
}

#[async_trait]
impl MapPoolProvider for DbMapPoolProvider {
    async fn valid_map_ids(
        &self,
        tournament_id: TournamentId,
        stage_id: Option<TournamentStageId>,
    ) -> Result<Vec<String>, DomainError> {
        // 1. Tournament/stage map pool override.
        if let Ok(Some(pool)) = self
            .map_pool_repo
            .get_effective(tournament_id, stage_id)
            .await
            && !pool.maps.is_empty()
        {
            return Ok(pool.maps);
        }

        // 2. Fall back to the game the tournament belongs to.
        let Some(tournament) = self.tournament_repo.find_by_id(tournament_id).await? else {
            return Ok(vec![]);
        };

        let Ok(Some(game)) = self
            .game_repo
            .find_by_id(tournament.game_id.as_uuid())
            .await
        else {
            return Ok(vec![]);
        };

        let default_pool = crate::handlers::games::extract_map_pool(&game);
        if !default_pool.is_empty() {
            return Ok(default_pool);
        }

        // 3. Last resort: the game's full map catalog, so a legacy game with no
        //    configured pool still rejects made-up map IDs.
        let plugin = self.plugin_manager.get(&game.plugin_id);
        Ok(crate::handlers::games::load_available_maps(&game, &plugin)
            .into_iter()
            .map(|m| m.id)
            .collect())
    }
}
