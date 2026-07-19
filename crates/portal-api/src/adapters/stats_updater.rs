//! Stats updater adapter for the match completion saga.
//!
//! Wraps the PlayerGameProfileService + PluginManager + demo repos to implement
//! the MatchStatsUpdater trait. When demo data is available for a match, it builds
//! a `DemoData` (game-agnostic), delegates to the plugin's `build_match_data_from_demo`
//! to produce a `MatchData`, then calls `calculate_player_stats` for each player.
//! When no demo data exists, it falls back to updating only win/loss/match counters.

use async_trait::async_trait;
use portal_core::{DomainError, GameId, PlayerId, TournamentMatchId, TournamentRegistrationId};
use portal_db::GameRepository;
use portal_domain::entities::demo::{Demo, DemoPlayer, ParsedDemoMetadata};
use portal_domain::repositories::demo::DemoMatchLinkRepository;
use portal_domain::repositories::tournament::{
    TournamentMatchRepository, TournamentRegistrationRepository, TournamentRepository,
};
use portal_domain::services::tournament::MatchStatsUpdater;
use portal_plugins::PluginManager;
use portal_plugins::types::{DemoData, DemoPlayerData, MatchData};
use serde_json::Value;
use std::sync::Arc;
use tracing::{debug, warn};

use crate::state::AppPlayerGameProfileService;

/// Adapter that wraps PlayerGameProfileService + tournament repos + demo repos + PluginManager
/// to implement MatchStatsUpdater.
pub struct StatsUpdaterAdapter<TMR, TR, TRR, DMLR> {
    match_repo: Arc<TMR>,
    tournament_repo: Arc<TR>,
    registration_repo: Arc<TRR>,
    demo_link_repo: Arc<DMLR>,
    game_repo: GameRepository,
    profile_service: AppPlayerGameProfileService,
    plugin_manager: Arc<PluginManager>,
}

impl<TMR, TR, TRR, DMLR> StatsUpdaterAdapter<TMR, TR, TRR, DMLR> {
    /// Create a new adapter.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        match_repo: Arc<TMR>,
        tournament_repo: Arc<TR>,
        registration_repo: Arc<TRR>,
        demo_link_repo: Arc<DMLR>,
        game_repo: GameRepository,
        profile_service: AppPlayerGameProfileService,
        plugin_manager: Arc<PluginManager>,
    ) -> Self {
        Self {
            match_repo,
            tournament_repo,
            registration_repo,
            demo_link_repo,
            game_repo,
            profile_service,
            plugin_manager,
        }
    }
}

/// Build a game-agnostic `DemoData` from domain entities.
///
/// This maps the domain's `Demo`/`DemoPlayer`/`ParsedDemoMetadata` into the plugin
/// system's `DemoData`. Player stats are serialized as raw JSON so the plugin can
/// interpret the fields according to its own schema.
fn build_demo_data(
    match_id: TournamentMatchId,
    game_id: GameId,
    demo: &Demo,
    metadata: &ParsedDemoMetadata,
    demo_players: &[DemoPlayer],
) -> DemoData {
    let players: Vec<DemoPlayerData> = demo_players
        .iter()
        .map(|dp| {
            // Serialize the domain DemoPlayerStats into raw JSON for the plugin
            let stats = serde_json::to_value(&dp.stats).unwrap_or_default();
            DemoPlayerData {
                player_id: dp.player_id.map(|id| id.as_uuid()),
                player_name: dp.player_name.clone(),
                team_name: dp.team_name.clone(),
                stats,
            }
        })
        .collect();

    DemoData {
        match_id: match_id.as_uuid(),
        game_id: game_id.to_string(),
        map_name: metadata.map_name.clone(),
        duration_seconds: metadata.duration_seconds.unwrap_or(0) as u64,
        team1_name: metadata.team1_name.clone(),
        team2_name: metadata.team2_name.clone(),
        team1_score: metadata.team1_score,
        team2_score: metadata.team2_score,
        players,
        raw_stats: demo.stats_json.clone().unwrap_or_default(),
    }
}

#[async_trait]
impl<TMR, TR, TRR, DMLR> MatchStatsUpdater for StatsUpdaterAdapter<TMR, TR, TRR, DMLR>
where
    TMR: TournamentMatchRepository + 'static,
    TR: TournamentRepository + 'static,
    TRR: TournamentRegistrationRepository + 'static,
    DMLR: DemoMatchLinkRepository + 'static,
{
    async fn update_player_stats(
        &self,
        match_id: TournamentMatchId,
        winner_registration_id: TournamentRegistrationId,
        loser_registration_id: TournamentRegistrationId,
        _is_forfeit: bool,
    ) -> Result<(), DomainError> {
        // 1. Get the match to find tournament_id
        let match_ = self
            .match_repo
            .find_by_id(match_id)
            .await?
            .ok_or_else(|| DomainError::TournamentMatchNotFound(match_id))?;

        // 2. Get the tournament to find game_id
        let tournament = self
            .tournament_repo
            .find_by_id(match_.tournament_id)
            .await?
            .ok_or_else(|| DomainError::TournamentNotFound(match_.tournament_id))?;
        let game_id = tournament.game_id;

        // 3. Look up the plugin for this game
        let plugin = self
            .game_repo
            .find_by_id(game_id.as_uuid())
            .await
            .ok()
            .flatten()
            .and_then(|game| self.plugin_manager.get(&game.plugin_id));

        // 4. Try to build MatchData from demo data via the plugin
        let match_data = if let Some(ref plugin) = plugin {
            self.try_build_match_data(match_id, game_id, plugin.as_ref())
                .await
        } else {
            None
        };

        if match_data.is_some() {
            debug!(match_id = %match_id, "Using demo data for plugin-based stats calculation");
        } else {
            debug!(match_id = %match_id, "No demo data available — updating counters only");
        }

        // 5. Resolve player IDs from registrations
        let winner_reg = self
            .registration_repo
            .find_by_id(winner_registration_id)
            .await?
            .ok_or_else(|| DomainError::TournamentRegistrationNotFound(winner_registration_id))?;
        let loser_reg = self
            .registration_repo
            .find_by_id(loser_registration_id)
            .await?
            .ok_or_else(|| DomainError::TournamentRegistrationNotFound(loser_registration_id))?;

        // For now, only handle individual player registrations.
        // Team stats aggregation would require looking up team members.
        let winner_player_id = winner_reg.player_id;
        let loser_player_id = loser_reg.player_id;

        // 6. Update winner stats
        if let Some(player_id) = winner_player_id {
            let new_stats = self
                .calculate_new_stats(player_id, game_id, &plugin, &match_data)
                .await;
            self.profile_service
                .update_stats_after_match(player_id, game_id, new_stats, true, false, false)
                .await?;
            debug!(player_id = %player_id, game_id = %game_id, "Updated winner stats");
        }

        // 7. Update loser stats
        if let Some(player_id) = loser_player_id {
            let new_stats = self
                .calculate_new_stats(player_id, game_id, &plugin, &match_data)
                .await;
            self.profile_service
                .update_stats_after_match(player_id, game_id, new_stats, false, true, false)
                .await?;
            debug!(player_id = %player_id, game_id = %game_id, "Updated loser stats");
        }

        if winner_player_id.is_none() && loser_player_id.is_none() {
            warn!(
                match_id = %match_id,
                "Neither registration has a player_id — team stats not yet supported"
            );
        }

        Ok(())
    }
}

impl<TMR, TR, TRR, DMLR> StatsUpdaterAdapter<TMR, TR, TRR, DMLR>
where
    DMLR: DemoMatchLinkRepository,
{
    /// Try to fetch demo data and have the plugin build a `MatchData` from it.
    ///
    /// Returns `None` if no suitable demo data exists or if the plugin can't parse it.
    async fn try_build_match_data(
        &self,
        match_id: TournamentMatchId,
        game_id: GameId,
        plugin: &dyn portal_plugins::traits::GamePlugin,
    ) -> Option<MatchData> {
        let links = match self.demo_link_repo.find_by_match_with_demos(match_id).await {
            Ok(links) => links,
            Err(e) => {
                warn!(
                    match_id = %match_id,
                    error = %e,
                    "Failed to fetch demo data for stats — falling back to counters only"
                );
                return None;
            }
        };

        // Use the first demo that has parsed metadata and players
        for link_data in links {
            let Some(metadata) = link_data.demo.metadata.as_ref() else {
                continue;
            };
            if link_data.players.is_empty() {
                continue;
            }

            let demo_data = build_demo_data(
                match_id,
                game_id,
                &link_data.demo,
                metadata,
                &link_data.players,
            );

            match plugin.build_match_data_from_demo(&demo_data) {
                Ok(match_data) => return Some(match_data),
                Err(e) => {
                    warn!(
                        match_id = %match_id,
                        error = %e,
                        "Plugin failed to build MatchData from demo — trying next demo"
                    );
                    continue;
                }
            }
        }

        None
    }

    /// Calculate updated game_specific_stats for a player.
    ///
    /// If demo-derived `MatchData` + plugin are available, delegates to the plugin's
    /// `calculate_player_stats`. Otherwise returns existing stats unchanged (only the
    /// SQL counters get bumped).
    async fn calculate_new_stats(
        &self,
        player_id: PlayerId,
        game_id: GameId,
        plugin: &Option<Arc<dyn portal_plugins::traits::GamePlugin>>,
        match_data: &Option<MatchData>,
    ) -> Value {
        let existing = self
            .profile_service
            .get_profile(player_id, game_id)
            .await
            .ok()
            .flatten()
            .map(|p| p.game_specific_stats)
            .unwrap_or_default();

        // Try plugin-based calculation if we have both plugin and demo-derived MatchData
        if let (Some(plugin), Some(data)) = (plugin, match_data) {
            match plugin.calculate_player_stats(data, player_id.as_uuid(), &existing) {
                Ok(new_stats) => return new_stats,
                Err(e) => {
                    // Player might not be in the demo (e.g. sub who didn't play).
                    // Fall through to return existing stats unchanged.
                    debug!(
                        player_id = %player_id,
                        error = %e,
                        "Plugin stats calculation failed — preserving existing stats"
                    );
                }
            }
        }

        existing
    }
}
