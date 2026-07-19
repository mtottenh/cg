//! Demo catalog service.
//!
//! Handles demo cataloging, categorization, stats processing, and match linking.

use std::sync::Arc;

use chrono::Utc;
use portal_core::{
    DemoCategory, DemoId, DemoLinkType, DemoStatus, DomainError, GameId, LeagueId, PlayerId,
    TournamentId, TournamentMatchId, UserId,
};
use tracing::{info, instrument, warn};

use crate::entities::demo::{
    Demo, DemoFilter, DemoListResult, DemoMatchLink, DemoPlayer, DemoPlayerStats,
    ParsedDemoMetadata,
};
use crate::entities::evidence::{
    DiscoveredEvidence, EvidenceStorage, EvidenceType, MatchEvidenceContext,
};
use crate::repositories::demo::{
    CreateDemo, CreateDemoMatchLink, CreateDemoPlayer, DemoMatchLinkRepository,
    DemoMatchLinkWithData, DemoPlayerRepository, DemoRepository,
};
use crate::repositories::tournament::{MatchLinkCandidate, TournamentMatchRepository};

/// Minimum steam-ID-overlap confidence required to auto-link a demo.
const AUTO_LINK_MIN_CONFIDENCE: f32 = 0.6;
/// Time window (hours) around the demo's match date for candidate matches.
const AUTO_LINK_WINDOW_HOURS: i64 = 24;
/// Cap on candidate matches considered per demo.
const AUTO_LINK_CANDIDATE_LIMIT: i64 = 100;

/// Service for managing the demo catalog.
#[derive(Clone)]
pub struct DemoService<DR, DMLR, DPR, TMR>
where
    DR: DemoRepository,
    DMLR: DemoMatchLinkRepository,
    DPR: DemoPlayerRepository,
    TMR: TournamentMatchRepository,
{
    demo_repo: Arc<DR>,
    link_repo: Arc<DMLR>,
    player_repo: Arc<DPR>,
    match_repo: Arc<TMR>,
}

impl<DR, DMLR, DPR, TMR> DemoService<DR, DMLR, DPR, TMR>
where
    DR: DemoRepository,
    DMLR: DemoMatchLinkRepository,
    DPR: DemoPlayerRepository,
    TMR: TournamentMatchRepository,
{
    /// Create a new demo service.
    pub fn new(
        demo_repo: Arc<DR>,
        link_repo: Arc<DMLR>,
        player_repo: Arc<DPR>,
        match_repo: Arc<TMR>,
    ) -> Self {
        Self {
            demo_repo,
            link_repo,
            player_repo,
            match_repo,
        }
    }

    // =========================================================================
    // Demo Catalog Operations
    // =========================================================================

    /// Get a demo by ID.
    #[instrument(skip(self))]
    pub async fn get_demo(&self, id: DemoId) -> Result<Demo, DomainError> {
        self.demo_repo
            .find_by_id(id)
            .await?
            .ok_or(DomainError::DemoNotFound(id))
    }

    /// List demos with filtering.
    #[instrument(skip(self))]
    pub async fn list_demos(&self, filter: DemoFilter) -> Result<DemoListResult, DomainError> {
        self.demo_repo.list(filter).await
    }

    /// Catalog a new demo discovered in S3.
    ///
    /// Returns [`CatalogResult::Created`] if the demo was newly cataloged,
    /// or [`CatalogResult::AlreadyExists`] if it was already in the catalog.
    #[instrument(skip(self))]
    pub async fn catalog_demo(
        &self,
        game_id: GameId,
        file_name: String,
        s3_bucket: String,
        s3_key: String,
        file_size_bytes: Option<i64>,
    ) -> Result<CatalogResult, DomainError> {
        // Check if demo already exists
        if let Some(existing) = self.demo_repo.find_by_s3_key(&s3_bucket, &s3_key).await? {
            info!(demo_id = %existing.id, "Demo already cataloged");
            return Ok(CatalogResult::AlreadyExists(existing));
        }

        let demo = self
            .demo_repo
            .create(CreateDemo {
                game_id,
                file_name,
                s3_bucket,
                s3_key,
                file_size_bytes,
                discovered_at: Utc::now(),
            })
            .await?;

        info!(demo_id = %demo.id, "Cataloged new demo");
        Ok(CatalogResult::Created(demo))
    }

    /// Get demos pending stats processing.
    #[instrument(skip(self))]
    pub async fn get_pending_demos(&self, limit: i64) -> Result<Vec<Demo>, DomainError> {
        self.demo_repo.find_pending_processing(limit).await
    }

    /// Update demo status to processing.
    #[instrument(skip(self))]
    pub async fn mark_processing(&self, id: DemoId) -> Result<Demo, DomainError> {
        self.demo_repo
            .update_status(id, DemoStatus::Processing)
            .await
    }

    /// Save parsed demo stats.
    ///
    /// Idempotent: deletes existing player entries before re-inserting.
    #[instrument(skip(self, metadata, stats_json, players))]
    pub async fn save_demo_stats(
        &self,
        id: DemoId,
        metadata: ParsedDemoMetadata,
        stats_json: serde_json::Value,
        players: Vec<DemoPlayerInput>,
    ) -> Result<Demo, DomainError> {
        // Update demo with stats
        let demo = self
            .demo_repo
            .update_stats(id, metadata, stats_json)
            .await?;

        // Delete existing players for idempotent re-submission
        self.player_repo.delete_by_demo(id).await?;

        // Create player entries
        let player_creates: Vec<CreateDemoPlayer> = players
            .into_iter()
            .map(|p| CreateDemoPlayer {
                steam_id: p.steam_id,
                player_name: p.player_name,
                team_name: p.team_name,
                stats: p.stats,
            })
            .collect();

        if !player_creates.is_empty() {
            self.player_repo.create_batch(id, player_creates).await?;
        }

        // Resolve demo player identities (steam_id -> players.steam_id_64).
        // Non-fatal: stats are already persisted.
        match self.player_repo.resolve_player_links(id).await {
            Ok(resolved) if resolved > 0 => {
                info!(demo_id = %id, resolved, "Resolved demo player identities");
            }
            Ok(_) => {}
            Err(e) => warn!(demo_id = %id, error = %e, "Failed to resolve demo player identities"),
        }

        // Attempt to auto-link the demo to a tournament match. Non-fatal:
        // a failed pass must never fail stats ingestion.
        let demo = match self.try_auto_link(&demo).await {
            Ok(Some(updated)) => updated,
            Ok(None) => demo,
            Err(e) => {
                warn!(demo_id = %id, error = %e, "Demo auto-link pass failed");
                demo
            }
        };

        info!(demo_id = %id, "Saved demo stats");
        Ok(demo)
    }

    /// Mark demo stats fetch as failed.
    #[instrument(skip(self))]
    pub async fn mark_stats_failed(&self, id: DemoId, error: &str) -> Result<Demo, DomainError> {
        warn!(demo_id = %id, error = %error, "Demo stats fetch failed");
        self.demo_repo.mark_stats_failed(id, error).await
    }

    // =========================================================================
    // Categorization & Visibility
    // =========================================================================

    /// Categorize a demo (PUG, League, Scrim, Ignored).
    #[instrument(skip(self))]
    pub async fn categorize_demo(
        &self,
        id: DemoId,
        category: DemoCategory,
        by_user_id: UserId,
    ) -> Result<Demo, DomainError> {
        let demo = self.demo_repo.categorize(id, category, by_user_id).await?;
        info!(demo_id = %id, category = %category, by_user = %by_user_id, "Categorized demo");
        Ok(demo)
    }

    /// Hide or unhide a demo.
    #[instrument(skip(self))]
    pub async fn set_demo_visibility(
        &self,
        id: DemoId,
        is_hidden: bool,
        by_user_id: UserId,
    ) -> Result<Demo, DomainError> {
        let demo = self
            .demo_repo
            .set_visibility(id, is_hidden, by_user_id)
            .await?;
        info!(demo_id = %id, hidden = is_hidden, by_user = %by_user_id, "Updated demo visibility");
        Ok(demo)
    }

    /// Associate a demo with a league or tournament.
    #[instrument(skip(self))]
    pub async fn associate_demo(
        &self,
        id: DemoId,
        league_id: Option<LeagueId>,
        tournament_id: Option<TournamentId>,
    ) -> Result<Demo, DomainError> {
        self.demo_repo.associate(id, league_id, tournament_id).await
    }

    /// Set admin notes on a demo.
    #[instrument(skip(self))]
    pub async fn set_admin_notes(
        &self,
        id: DemoId,
        notes: Option<String>,
    ) -> Result<Demo, DomainError> {
        self.demo_repo.set_admin_notes(id, notes).await
    }

    // =========================================================================
    // Match Linking
    // =========================================================================

    /// Link a demo to a tournament match.
    #[instrument(skip(self))]
    pub async fn link_to_match(
        &self,
        demo_id: DemoId,
        match_id: TournamentMatchId,
        game_number: Option<i32>,
        link_type: DemoLinkType,
        linked_by: Option<UserId>,
    ) -> Result<DemoMatchLink, DomainError> {
        // Check if link already exists
        if self
            .link_repo
            .find_by_demo_and_match(demo_id, match_id)
            .await?
            .is_some()
        {
            return Err(DomainError::conflict(format!(
                "Demo {demo_id} is already linked to match {match_id}"
            )));
        }

        let link = self
            .link_repo
            .create(CreateDemoMatchLink {
                demo_id,
                match_id,
                game_number,
                link_type,
                confidence_score: None,
                linked_by_user_id: linked_by,
            })
            .await?;

        info!(demo_id = %demo_id, match_id = %match_id, "Linked demo to match");
        Ok(link)
    }

    /// Unlink a demo from a match.
    #[instrument(skip(self))]
    pub async fn unlink_from_match(
        &self,
        demo_id: DemoId,
        match_id: TournamentMatchId,
    ) -> Result<(), DomainError> {
        let link = self
            .link_repo
            .find_by_demo_and_match(demo_id, match_id)
            .await?
            .ok_or_else(|| DomainError::LookupFailed {
                resource: "demo match link",
                query: format!("demo={demo_id},match={match_id}"),
            })?;

        self.link_repo.delete(link.id).await?;
        info!(demo_id = %demo_id, match_id = %match_id, "Unlinked demo from match");
        Ok(())
    }

    /// Get all demos linked to a match.
    #[instrument(skip(self))]
    pub async fn get_match_demos(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<DemoMatchLink>, DomainError> {
        self.link_repo.find_by_match(match_id).await
    }

    /// Get all demos linked to a match with full demo and player data.
    ///
    /// This is useful for displaying match demos with all their details.
    #[instrument(skip(self))]
    pub async fn get_match_demos_with_data(
        &self,
        match_id: TournamentMatchId,
        include_stats: bool,
        game_number: Option<i32>,
    ) -> Result<Vec<DemoMatchLinkWithData>, DomainError> {
        let mut results = self.link_repo.find_by_match_with_demos(match_id).await?;

        // Filter by game number if specified
        if let Some(gn) = game_number {
            results.retain(|r| r.link.game_number == Some(gn));
        }

        // Optionally strip players if not needed
        if !include_stats {
            for result in &mut results {
                result.players.clear();
            }
        }

        Ok(results)
    }

    /// Get all matches linked to a demo.
    #[instrument(skip(self))]
    pub async fn get_demo_links(&self, demo_id: DemoId) -> Result<Vec<DemoMatchLink>, DomainError> {
        self.link_repo.find_by_demo(demo_id).await
    }

    // =========================================================================
    // Auto-Linking
    // =========================================================================

    /// Attempt to auto-link a demo to a tournament match.
    ///
    /// Extracts the demo's player Steam IDs (top-level `player_summaries`
    /// keys of the stats JSON) and match date, fetches candidate matches for
    /// the same game within [`AUTO_LINK_WINDOW_HOURS`] of the match date, and
    /// scores each candidate by Steam-ID overlap:
    ///
    /// ```text
    /// confidence = overlapping_steam_ids / total_demo_players  (capped at 1.0)
    /// ```
    ///
    /// The best candidate with confidence >= [`AUTO_LINK_MIN_CONFIDENCE`] is
    /// linked (`link_type = auto_matched`), idempotently, and the demo is
    /// stamped with the match's tournament (and its league, if any).
    ///
    /// Returns the updated demo when a link was made, `None` otherwise.
    #[instrument(skip(self, demo), fields(demo_id = %demo.id))]
    async fn try_auto_link(&self, demo: &Demo) -> Result<Option<Demo>, DomainError> {
        let Some(stats) = demo.stats_json.as_ref() else {
            return Ok(None);
        };

        let demo_steam_ids: std::collections::HashSet<&str> = stats
            .get("player_summaries")
            .and_then(serde_json::Value::as_object)
            .map(|summaries| summaries.keys().map(String::as_str).collect())
            .unwrap_or_default();

        if demo_steam_ids.is_empty() {
            return Ok(None);
        }

        let match_date = stats
            .get("match_date")
            .and_then(serde_json::Value::as_str)
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.to_utc())
            .or_else(|| demo.metadata.as_ref().and_then(|m| m.match_date));

        let Some(match_date) = match_date else {
            return Ok(None);
        };

        let candidates = self
            .match_repo
            .list_auto_link_candidates(
                demo.game_id,
                match_date,
                AUTO_LINK_WINDOW_HOURS,
                AUTO_LINK_CANDIDATE_LIMIT,
            )
            .await?;

        let mut best: Option<(&MatchLinkCandidate, f32)> = None;
        for candidate in &candidates {
            let overlap = candidate
                .steam_ids
                .iter()
                .filter(|sid| demo_steam_ids.contains(sid.as_str()))
                .count();
            let confidence = (overlap as f32 / demo_steam_ids.len() as f32).min(1.0);
            if confidence >= AUTO_LINK_MIN_CONFIDENCE
                && best.is_none_or(|(_, best_confidence)| confidence > best_confidence)
            {
                best = Some((candidate, confidence));
            }
        }

        let Some((candidate, confidence)) = best else {
            return Ok(None);
        };

        // Idempotent: tolerate an existing link (manual or from a prior pass).
        if self
            .link_repo
            .find_by_demo_and_match(demo.id, candidate.match_id)
            .await?
            .is_none()
        {
            self.link_repo
                .create(CreateDemoMatchLink {
                    demo_id: demo.id,
                    match_id: candidate.match_id,
                    game_number: None,
                    link_type: DemoLinkType::AutoMatched,
                    confidence_score: Some(confidence),
                    linked_by_user_id: None,
                })
                .await?;
        }

        // Stamp the demo's organization: the match's tournament, and its
        // league when set (keep any pre-existing league association).
        let league_id = candidate.league_id.or(demo.league_id);
        let updated = self
            .demo_repo
            .associate(demo.id, league_id, Some(candidate.tournament_id))
            .await?;

        info!(
            demo_id = %demo.id,
            match_id = %candidate.match_id,
            tournament_id = %candidate.tournament_id,
            confidence,
            "Auto-linked demo to tournament match"
        );

        Ok(Some(updated))
    }

    /// Run the auto-link pass over ready demos with stats but no match links.
    ///
    /// Bounded batch backfill for demos ingested before auto-linking existed
    /// (or whose match was scheduled after stats came in). Also resolves demo
    /// player identities as part of the pass. Per-demo failures are logged
    /// and counted as skipped.
    #[instrument(skip(self))]
    pub async fn process_unlinked_demos(
        &self,
        limit: i64,
    ) -> Result<ProcessUnlinkedResult, DomainError> {
        let demos = self.demo_repo.find_ready_unlinked(limit).await?;

        let mut result = ProcessUnlinkedResult {
            examined: 0,
            linked: 0,
            skipped: 0,
        };

        for demo in demos {
            result.examined += 1;

            if let Err(e) = self.player_repo.resolve_player_links(demo.id).await {
                warn!(demo_id = %demo.id, error = %e, "Failed to resolve demo player identities");
            }

            match self.try_auto_link(&demo).await {
                Ok(Some(_)) => result.linked += 1,
                Ok(None) => result.skipped += 1,
                Err(e) => {
                    warn!(demo_id = %demo.id, error = %e, "Demo auto-link pass failed");
                    result.skipped += 1;
                }
            }
        }

        info!(
            examined = result.examined,
            linked = result.linked,
            skipped = result.skipped,
            "Processed unlinked demos"
        );

        Ok(result)
    }

    // =========================================================================
    // Player Queries
    // =========================================================================

    /// Get players from a demo.
    #[instrument(skip(self))]
    pub async fn get_demo_players(&self, demo_id: DemoId) -> Result<Vec<DemoPlayer>, DomainError> {
        self.player_repo.find_by_demo(demo_id).await
    }

    /// Find all demos featuring a player by Steam ID.
    #[instrument(skip(self))]
    pub async fn find_demos_by_steam_id(&self, steam_id: &str) -> Result<Vec<DemoId>, DomainError> {
        self.player_repo.find_demos_by_steam_id(steam_id).await
    }

    /// Link a demo player entry to a portal player account.
    #[instrument(skip(self))]
    pub async fn link_player_account(
        &self,
        demo_player_id: portal_core::DemoPlayerId,
        player_id: PlayerId,
    ) -> Result<DemoPlayer, DomainError> {
        self.player_repo
            .link_to_player(demo_player_id, player_id)
            .await
    }

    // =========================================================================
    // Evidence Discovery
    // =========================================================================

    /// Discover demos in the catalog that match a match's evidence context.
    ///
    /// Queries the catalog for ready demos with matching Steam IDs within
    /// a time window around the match, then scores them by relevance.
    #[instrument(skip(self, context))]
    pub async fn discover_for_match(
        &self,
        context: &MatchEvidenceContext,
    ) -> Result<Vec<DiscoveredEvidence>, DomainError> {
        // Collect all steam_ids from participants
        let steam_ids: Vec<String> = context
            .participants
            .iter()
            .flat_map(|p| p.steam_ids.clone())
            .collect();

        if steam_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Build time window: reference_time ± 6 hours
        let reference_time = context.started_at.or(context.scheduled_at);

        let (time_from, time_to) = match reference_time {
            Some(t) => (
                Some(t - chrono::Duration::hours(6)),
                Some(t + chrono::Duration::hours(6)),
            ),
            None => (None, None),
        };

        // Query the catalog
        let game_id = context
            .game_id
            .parse::<uuid::Uuid>()
            .map(portal_core::GameId::from_uuid)
            .map_err(|_| {
                portal_core::DomainError::Internal(format!(
                    "Invalid game_id UUID in evidence context: {}",
                    context.game_id
                ))
            })?;
        let match_id = portal_core::TournamentMatchId::from_uuid(context.match_id);

        let matching_demos = self
            .demo_repo
            .find_matching_for_context(game_id, &steam_ids, time_from, time_to, Some(match_id), 50)
            .await?;

        // For each demo, fetch players and compute relevance
        let mut results = Vec::with_capacity(matching_demos.len());
        for demo in matching_demos {
            let demo_players = self.player_repo.find_by_demo(demo.id).await?;
            let relevance = compute_relevance(&demo, &demo_players, context, reference_time);

            let metadata_json = serde_json::json!({
                "map_name": demo.metadata.as_ref().map(|m| &m.map_name),
                "team1_name": demo.metadata.as_ref().map(|m| &m.team1_name),
                "team2_name": demo.metadata.as_ref().map(|m| &m.team2_name),
                "team1_score": demo.metadata.as_ref().map(|m| m.team1_score),
                "team2_score": demo.metadata.as_ref().map(|m| m.team2_score),
                "total_rounds": demo.metadata.as_ref().map(|m| m.total_rounds),
            });

            results.push(DiscoveredEvidence {
                external_id: format!("catalog:{}", demo.id),
                evidence_type: EvidenceType::Demo,
                name: demo.file_name.clone(),
                storage: EvidenceStorage::S3 {
                    bucket: demo.s3_bucket.clone(),
                    key: demo.s3_key.clone(),
                },
                file_size_bytes: demo.file_size_bytes,
                metadata: metadata_json,
                discovered_at: Utc::now(),
                relevance_score: relevance,
            });
        }

        // Sort by relevance descending
        results.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(results)
    }

    // =========================================================================
    // Admin Operations
    // =========================================================================

    /// Get demo count by status (for admin dashboard).
    #[instrument(skip(self))]
    pub async fn get_status_counts(&self) -> Result<Vec<(DemoStatus, i64)>, DomainError> {
        self.demo_repo.count_by_status().await
    }

    /// Delete a demo and all associated data.
    #[instrument(skip(self))]
    pub async fn delete_demo(&self, id: DemoId) -> Result<(), DomainError> {
        // Delete player entries first
        self.player_repo.delete_by_demo(id).await?;

        // Delete match links
        self.link_repo.delete_by_demo(id).await?;

        // Delete the demo
        self.demo_repo.delete(id).await?;

        info!(demo_id = %id, "Deleted demo and all associated data");
        Ok(())
    }
}

/// Counters from a backfill pass over unlinked demos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessUnlinkedResult {
    /// Demos examined in this batch.
    pub examined: i64,
    /// Demos that were auto-linked to a match.
    pub linked: i64,
    /// Demos examined but not linked (no confident candidate, or errored).
    pub skipped: i64,
}

/// Result of cataloging a demo.
#[derive(Debug, Clone)]
pub enum CatalogResult {
    /// The demo was newly created.
    Created(Demo),
    /// The demo already existed in the catalog.
    AlreadyExists(Demo),
}

impl CatalogResult {
    /// Get the demo, regardless of whether it was created or already existed.
    #[must_use]
    pub fn into_demo(self) -> Demo {
        match self {
            Self::Created(d) | Self::AlreadyExists(d) => d,
        }
    }

    /// Check if this was a newly created demo.
    #[must_use]
    pub fn is_created(&self) -> bool {
        matches!(self, Self::Created(_))
    }
}

/// Input for creating demo player entries.
#[derive(Debug, Clone)]
pub struct DemoPlayerInput {
    pub steam_id: String,
    pub player_name: String,
    pub team_name: Option<String>,
    pub stats: DemoPlayerStats,
}

/// Compute relevance score for a demo against a match context.
///
/// | Factor               | Weight | Logic                                          |
/// |----------------------|--------|------------------------------------------------|
/// | Steam ID overlap     | 0.50   | matching_ids / total_context_ids               |
/// | Time proximity       | 0.30   | (1 - hours_diff / 24).max(0)                   |
/// | Both teams present   | 0.15   | Players from both participants found            |
/// | Base                 | 0.05   | Always present (unlinked to this match)         |
fn compute_relevance(
    demo: &Demo,
    demo_players: &[DemoPlayer],
    context: &MatchEvidenceContext,
    reference_time: Option<chrono::DateTime<Utc>>,
) -> f32 {
    let all_steam_ids: Vec<&str> = context
        .participants
        .iter()
        .flat_map(|p| p.steam_ids.iter().map(String::as_str))
        .collect();

    if all_steam_ids.is_empty() {
        return 0.05;
    }

    let demo_steam_ids: std::collections::HashSet<&str> =
        demo_players.iter().map(|p| p.steam_id.as_str()).collect();

    // Steam ID overlap
    let matching_count = all_steam_ids
        .iter()
        .filter(|id| demo_steam_ids.contains(**id))
        .count();
    let steam_overlap = matching_count as f32 / all_steam_ids.len() as f32;

    // Time proximity
    let time_score = match (
        reference_time,
        demo.metadata.as_ref().and_then(|m| m.match_date),
    ) {
        (Some(ref_time), Some(demo_time)) => {
            let hours_diff = (ref_time - demo_time).num_hours().unsigned_abs() as f32;
            (1.0 - hours_diff / 24.0).max(0.0)
        }
        _ => 0.5, // Unknown — neutral score
    };

    // Both teams present
    let both_teams_present = context.participants.len() >= 2
        && context.participants.iter().all(|p| {
            p.steam_ids
                .iter()
                .any(|sid| demo_steam_ids.contains(sid.as_str()))
        });

    let both_teams_score = if both_teams_present { 1.0 } else { 0.0 };

    // Weighted sum
    0.15f32.mul_add(
        both_teams_score,
        0.50f32.mul_add(steam_overlap, 0.30 * time_score),
    ) + 0.05
}
