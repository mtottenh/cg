//! Pick-Up Game (PUG) lobby service.
//!
//! Owns the social/gathering phase: join codes, roster building, team
//! assignment, wheel nominations, and the weighted wheel draw. Everything
//! that spans other subsystems (materializing the container tournament,
//! recording spins into the veto session, server assignment) lives in the
//! API layer's `pug_flow`, mirroring how `game_server_flow` composes
//! domain services.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{Duration, Utc};
use portal_core::types::PugMapSelectionMode;
use portal_core::{DomainError, GameId, MatchFormat, PlayerId, PugId, SideSelectionMode, UserId};
use rand::Rng;
use rand::seq::SliceRandom;
use tracing::{info, instrument};

use crate::entities::pug::{Pug, PugPlayer, PugWheelEntry};
use crate::repositories::pug::{CreatePug, PugRepository};

/// How long a gathering lobby lives before the sweeper expires it.
pub const GATHERING_TTL_HOURS: i64 = 2;

/// Maximum non-terminal PUGs one user may have created at a time.
pub const MAX_ACTIVE_CREATED: i64 = 2;

/// Bench slots beyond the two team rosters.
pub const BENCH_SLOTS: i64 = 4;

/// Join-code alphabet: unambiguous upper-case letters and digits.
const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTVWXYZ23456789";
const CODE_LENGTH: usize = 10;

/// Everything a lock needs, computed and validated up front.
#[derive(Debug, Clone)]
pub struct LockPlan {
    pub pug: Pug,
    pub team1: Vec<PugPlayer>,
    pub team2: Vec<PugPlayer>,
    /// The veto map pool: the configured/custom pool for veto mode, the
    /// deduped nominations (padded by the caller if short) for wheel mode.
    pub map_pool_hint: Vec<String>,
    pub veto_format_id: String,
}

/// A wheel segment: one unique map with its aggregated weight.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WheelSegment {
    pub map_id: String,
    /// Number of nominations (duplicate nominations weight the wheel).
    pub weight: u32,
    /// Display names of the nominating players.
    pub nominated_by: Vec<String>,
}

/// Result of a wheel draw.
#[derive(Debug, Clone)]
pub struct WheelDraw {
    pub segments: Vec<WheelSegment>,
    pub winner_map_id: String,
    /// Seed for the deterministic client animation.
    pub spin_seed: i64,
}

/// Service for PUG lobby management.
#[derive(Clone)]
pub struct PugService {
    pug_repo: Arc<dyn PugRepository>,
}

impl PugService {
    pub fn new(pug_repo: Arc<dyn PugRepository>) -> Self {
        Self { pug_repo }
    }

    /// Direct repository access for the API-layer flow (materializer,
    /// sweeper, event hooks).
    #[must_use]
    pub fn repo(&self) -> &Arc<dyn PugRepository> {
        &self.pug_repo
    }

    // =========================================================================
    // CREATE / READ
    // =========================================================================

    /// Create a lobby. The creator becomes a captain on team 1.
    #[instrument(skip(self))]
    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        &self,
        game_id: GameId,
        creator_user: UserId,
        creator_player: PlayerId,
        match_format: MatchFormat,
        map_selection_mode: PugMapSelectionMode,
        side_selection_mode: SideSelectionMode,
        team_size: i32,
        region: Option<String>,
        map_pool: Option<Vec<String>>,
        listed: bool,
    ) -> Result<Pug, DomainError> {
        if !(1..=16).contains(&team_size) {
            return Err(DomainError::InvalidState(
                "Team size must be between 1 and 16".to_string(),
            ));
        }

        // The wheel has no picker, so "picker chooses" is meaningless.
        if map_selection_mode == PugMapSelectionMode::Wheel
            && side_selection_mode == SideSelectionMode::PickerChoice
        {
            return Err(DomainError::InvalidState(
                "Wheel mode supports knife or coin-flip side selection only".to_string(),
            ));
        }

        if let Some(pool) = &map_pool {
            let needed = usize::try_from(match_format.game_count()).unwrap_or(1);
            if pool.len() < needed.max(2) {
                return Err(DomainError::InvalidState(format!(
                    "Custom map pool needs at least {} maps for {match_format}",
                    needed.max(2)
                )));
            }
        }

        let active = self.pug_repo.count_active_created_by(creator_user).await?;
        if active >= MAX_ACTIVE_CREATED {
            return Err(DomainError::Conflict(format!(
                "You already have {active} active PUGs — finish or cancel one first"
            )));
        }

        let pug = self
            .pug_repo
            .create(CreatePug {
                game_id,
                created_by_user_id: creator_user,
                creator_player_id: creator_player,
                join_code: generate_join_code(),
                match_format,
                map_selection_mode,
                side_selection_mode,
                team_size,
                region,
                map_pool,
                listed,
                expires_at: Utc::now() + Duration::hours(GATHERING_TTL_HOURS),
            })
            .await?;

        info!(pug_id = %pug.id, "PUG lobby created");
        Ok(pug)
    }

    pub async fn get(&self, id: PugId) -> Result<Pug, DomainError> {
        self.pug_repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| DomainError::LookupFailed {
                resource: "pug",
                query: format!("id {id}"),
            })
    }

    pub async fn get_by_code(&self, code: &str) -> Result<Pug, DomainError> {
        self.pug_repo
            .find_by_join_code(code)
            .await?
            .ok_or_else(|| DomainError::LookupFailed {
                resource: "pug",
                query: "join code".to_string(),
            })
    }

    pub async fn players(&self, id: PugId) -> Result<Vec<PugPlayer>, DomainError> {
        self.pug_repo.list_players(id).await
    }

    pub async fn wheel_entries(&self, id: PugId) -> Result<Vec<PugWheelEntry>, DomainError> {
        self.pug_repo.list_wheel_entries(id).await
    }

    /// Lobby state a viewer may see. Participants always may; non-participants
    /// only with the join code (share-link viewers) or when terminal.
    pub async fn authorize_view(
        &self,
        pug: &Pug,
        viewer_player: Option<PlayerId>,
        provided_code: Option<&str>,
    ) -> Result<(), DomainError> {
        if let Some(player) = viewer_player
            && self.pug_repo.is_participant(pug.id, player).await?
        {
            return Ok(());
        }
        // Plain equality on purpose: the code is an invite (49 bits of
        // entropy behind a rate limit), not a bearer secret, and the DB
        // lookup in join_by_code compares it non-constant-time anyway — a
        // hand-rolled constant-time compare here was theater (review nit).
        // Plain equality on purpose: the code is an invite (49 bits of
        // entropy behind a rate limit), not a bearer secret, and the DB
        // lookup in join_by_code compares it non-constant-time anyway — a
        // hand-rolled constant-time compare here was theater (review nit).
        if provided_code == Some(pug.join_code.as_str()) {
            return Ok(());
        }
        if pug.listed || pug.status.is_terminal() {
            return Ok(());
        }
        Err(DomainError::NotAuthorized(
            "This PUG lobby is private — use the invite link".to_string(),
        ))
    }

    // =========================================================================
    // MEMBERSHIP
    // =========================================================================

    /// Join via invite code. Idempotent for existing participants.
    #[instrument(skip(self, code))]
    pub async fn join_by_code(&self, code: &str, player: PlayerId) -> Result<Pug, DomainError> {
        let pug = self.get_by_code(code).await?;

        if self.pug_repo.is_participant(pug.id, player).await? {
            return Ok(pug);
        }

        if !pug.is_open() {
            return Err(DomainError::InvalidState(
                "This PUG is no longer accepting players".to_string(),
            ));
        }

        let max_players = i64::from(pug.team_size) * 2 + BENCH_SLOTS;
        if !self
            .pug_repo
            .add_player(pug.id, player, max_players)
            .await?
        {
            return Err(DomainError::Conflict("This PUG lobby is full".to_string()));
        }
        info!(pug_id = %pug.id, %player, "Player joined PUG");
        Ok(pug)
    }

    #[instrument(skip(self))]
    pub async fn leave(
        &self,
        pug_id: PugId,
        player: PlayerId,
        user: UserId,
    ) -> Result<(), DomainError> {
        let pug = self.get(pug_id).await?;
        if !pug.is_open() {
            return Err(DomainError::InvalidState(
                "Cannot leave after the lobby has locked".to_string(),
            ));
        }
        if pug.created_by_user_id == user {
            return Err(DomainError::InvalidState(
                "The creator cannot leave — cancel the PUG instead".to_string(),
            ));
        }
        self.pug_repo.remove_player(pug_id, player).await
    }

    #[instrument(skip(self))]
    pub async fn kick(
        &self,
        pug_id: PugId,
        actor_user: UserId,
        target: PlayerId,
        creator_player: Option<PlayerId>,
    ) -> Result<(), DomainError> {
        let pug = self.get(pug_id).await?;
        self.require_creator(&pug, actor_user)?;
        if !pug.is_open() {
            return Err(DomainError::InvalidState(
                "Cannot kick after the lobby has locked".to_string(),
            ));
        }
        if creator_player == Some(target) {
            return Err(DomainError::InvalidState(
                "The creator cannot kick themself".to_string(),
            ));
        }
        self.pug_repo.remove_player(pug_id, target).await
    }

    // =========================================================================
    // TEAMS
    // =========================================================================

    /// Assign a player to a team (or the bench). Players move themselves;
    /// the creator may move anyone.
    #[instrument(skip(self))]
    pub async fn set_team(
        &self,
        pug_id: PugId,
        actor_user: UserId,
        actor_player: PlayerId,
        target: PlayerId,
        team: Option<i16>,
    ) -> Result<(), DomainError> {
        let pug = self.get(pug_id).await?;
        if !pug.is_open() {
            return Err(DomainError::InvalidState(
                "Teams are locked once the lobby locks".to_string(),
            ));
        }
        if target != actor_player {
            self.require_creator(&pug, actor_user)?;
        }
        if let Some(t) = team {
            if !(t == 1 || t == 2) {
                return Err(DomainError::InvalidState("Team must be 1 or 2".to_string()));
            }
            if !self.pug_repo.is_participant(pug_id, target).await? {
                return Err(DomainError::NotAuthorized(
                    "Player is not in this lobby".to_string(),
                ));
            }
        }
        // Capacity is enforced inside the UPDATE (review m5).
        let moved = self
            .pug_repo
            .set_player_team(pug_id, target, team, i64::from(pug.team_size))
            .await?;
        if !moved {
            return Err(DomainError::Conflict(
                "That team is full (or the player left)".to_string(),
            ));
        }
        Ok(())
    }

    /// Toggle captain status (creator only).
    #[instrument(skip(self))]
    pub async fn set_captain(
        &self,
        pug_id: PugId,
        actor_user: UserId,
        target: PlayerId,
        is_captain: bool,
    ) -> Result<(), DomainError> {
        let pug = self.get(pug_id).await?;
        self.require_creator(&pug, actor_user)?;
        if !pug.is_open() {
            return Err(DomainError::InvalidState(
                "Captains are fixed once the lobby locks".to_string(),
            ));
        }
        self.pug_repo
            .set_player_captain(pug_id, target, is_captain)
            .await
    }

    /// Randomize balanced teams from everyone in the lobby (creator only).
    #[instrument(skip(self))]
    pub async fn shuffle(&self, pug_id: PugId, actor_user: UserId) -> Result<(), DomainError> {
        let pug = self.get(pug_id).await?;
        self.require_creator(&pug, actor_user)?;
        if !pug.is_open() {
            return Err(DomainError::InvalidState(
                "Teams are locked once the lobby locks".to_string(),
            ));
        }

        let players = self.pug_repo.list_players(pug_id).await?;
        let mut ids: Vec<PlayerId> = players.iter().map(|p| p.player_id).collect();
        ids.shuffle(&mut rand::rng());

        let team_size = usize::try_from(pug.team_size).unwrap_or(usize::MAX);
        let assignments: Vec<(PlayerId, Option<i16>)> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| {
                // Deal alternately so uneven lobbies split as evenly as possible.
                let team = if i / 2 < team_size {
                    Some(i16::try_from(i % 2 + 1).unwrap_or(1))
                } else {
                    None // overflow to the bench
                };
                (*id, team)
            })
            .collect();

        self.pug_repo.assign_teams(pug_id, &assignments).await
    }

    /// Mirror the two rosters (creator only).
    #[instrument(skip(self))]
    pub async fn swap_teams(&self, pug_id: PugId, actor_user: UserId) -> Result<(), DomainError> {
        let pug = self.get(pug_id).await?;
        self.require_creator(&pug, actor_user)?;
        if !pug.is_open() {
            return Err(DomainError::InvalidState(
                "Teams are locked once the lobby locks".to_string(),
            ));
        }
        self.pug_repo.swap_teams(pug_id).await
    }

    /// Captains draft: the captain of the team with FEWER players (tie:
    /// team 1) picks the next player off the bench. Stateless by design —
    /// the turn derives from roster sizes, so there is no draft cursor to
    /// desync. The creator may draft on the picking team's behalf.
    #[instrument(skip(self))]
    pub async fn draft_pick(
        &self,
        pug_id: PugId,
        actor_user: UserId,
        actor_player: PlayerId,
        target: PlayerId,
    ) -> Result<i16, DomainError> {
        let pug = self.get(pug_id).await?;
        if !pug.is_open() {
            return Err(DomainError::InvalidState(
                "Drafting ends when the lobby locks".to_string(),
            ));
        }

        let players = self.pug_repo.list_players(pug_id).await?;
        let on_bench = players
            .iter()
            .find(|p| p.player_id == target)
            .ok_or_else(|| DomainError::NotAuthorized("Player is not in this lobby".to_string()))?;
        if on_bench.team.is_some() {
            return Err(DomainError::InvalidState(
                "That player is already on a team".to_string(),
            ));
        }

        let Some(picking) = Self::picking_team(&players, pug.team_size) else {
            return Err(DomainError::Conflict("Both teams are full".to_string()));
        };

        let is_picking_captain = players
            .iter()
            .any(|p| p.player_id == actor_player && p.is_captain && p.team == Some(picking));
        if pug.created_by_user_id != actor_user && !is_picking_captain {
            return Err(DomainError::NotAuthorized(format!(
                "It is team {picking}'s pick — only their captain (or the creator) can draft"
            )));
        }

        let moved = self
            .pug_repo
            .set_player_team(pug_id, target, Some(picking), i64::from(pug.team_size))
            .await?;
        if !moved {
            return Err(DomainError::Conflict(format!(
                "Team {picking} filled up before the pick landed"
            )));
        }
        Ok(picking)
    }

    // =========================================================================
    // CODES
    // =========================================================================

    /// Rotate the invite code (creator only). Old links die immediately.
    #[instrument(skip(self))]
    pub async fn rotate_code(
        &self,
        pug_id: PugId,
        actor_user: UserId,
    ) -> Result<String, DomainError> {
        let pug = self.get(pug_id).await?;
        self.require_creator(&pug, actor_user)?;
        if !pug.is_open() {
            return Err(DomainError::InvalidState(
                "The invite code is dead once the lobby locks".to_string(),
            ));
        }
        let code = generate_join_code();
        self.pug_repo.set_join_code(pug_id, &code).await?;
        Ok(code)
    }

    /// Whose pick it is in a captains draft: the team with fewer players
    /// (tie: team 1); None when both teams are full. THE single definition —
    /// the detail endpoint exposes it so the frontend renders exactly the
    /// rule the backend enforces (review nit: the rule was duplicated).
    #[must_use]
    pub fn picking_team(players: &[PugPlayer], team_size: i32) -> Option<i16> {
        let cap = usize::try_from(team_size).unwrap_or(usize::MAX);
        let count = |team: i16| players.iter().filter(|p| p.team == Some(team)).count();
        let (team1, team2) = (count(1), count(2));
        if team1 >= cap && team2 >= cap {
            return None;
        }
        if team1 > team2 && team2 < cap {
            Some(2)
        } else if team1 <= team2 && team1 < cap {
            Some(1)
        } else {
            Some(2)
        }
    }

    // =========================================================================
    // WHEEL NOMINATIONS + DRAW
    // =========================================================================

    /// Upsert the caller's map nomination (wheel mode, gathering only).
    #[instrument(skip(self))]
    pub async fn nominate_map(
        &self,
        pug_id: PugId,
        player: PlayerId,
        map_id: &str,
    ) -> Result<(), DomainError> {
        let pug = self.get(pug_id).await?;
        if pug.map_selection_mode != PugMapSelectionMode::Wheel {
            return Err(DomainError::InvalidState(
                "This PUG uses map veto, not the wheel".to_string(),
            ));
        }
        if !pug.is_open() {
            return Err(DomainError::InvalidState(
                "Nominations close when the lobby locks".to_string(),
            ));
        }
        if !self.pug_repo.is_participant(pug_id, player).await? {
            return Err(DomainError::NotAuthorized(
                "Only lobby players can nominate maps".to_string(),
            ));
        }
        if map_id.is_empty() || map_id.len() > 64 {
            return Err(DomainError::InvalidState("Invalid map id".to_string()));
        }
        self.pug_repo
            .upsert_wheel_entry(pug_id, player, map_id)
            .await
    }

    /// Aggregate nominations into weighted wheel segments, restricted to
    /// `allowed` maps (the veto session's remaining pool at spin time).
    #[must_use]
    pub fn build_segments(entries: &[PugWheelEntry], allowed: &[String]) -> Vec<WheelSegment> {
        let mut by_map: HashMap<&str, WheelSegment> = HashMap::new();
        for entry in entries {
            if !allowed.iter().any(|m| m == &entry.map_id) {
                continue;
            }
            by_map
                .entry(entry.map_id.as_str())
                .and_modify(|s| {
                    s.weight += 1;
                    s.nominated_by.push(entry.player_name.clone());
                })
                .or_insert_with(|| WheelSegment {
                    map_id: entry.map_id.clone(),
                    weight: 1,
                    nominated_by: vec![entry.player_name.clone()],
                });
        }
        // Maps in the pool nobody nominated (pool padding) spin at weight 1.
        for map_id in allowed {
            by_map
                .entry(map_id.as_str())
                .or_insert_with(|| WheelSegment {
                    map_id: map_id.clone(),
                    weight: 1,
                    nominated_by: Vec::new(),
                });
        }
        let mut segments: Vec<WheelSegment> = by_map.into_values().collect();
        // Stable ordering so every client renders identical segments.
        segments.sort_by(|a, b| a.map_id.cmp(&b.map_id));
        segments
    }

    /// Weighted random draw over the segments.
    pub fn draw(segments: Vec<WheelSegment>) -> Result<WheelDraw, DomainError> {
        let total: u32 = segments.iter().map(|s| s.weight).sum();
        if total == 0 {
            return Err(DomainError::InvalidState(
                "No maps available to spin for".to_string(),
            ));
        }
        let mut rng = rand::rng();
        let mut roll = rng.random_range(0..total);
        let mut winner = segments.last().map(|s| s.map_id.clone()).ok_or_else(|| {
            DomainError::InvalidState("No maps available to spin for".to_string())
        })?;
        for segment in &segments {
            if roll < segment.weight {
                winner.clone_from(&segment.map_id);
                break;
            }
            roll -= segment.weight;
        }
        Ok(WheelDraw {
            segments,
            winner_map_id: winner,
            spin_seed: rng.random::<i64>(),
        })
    }

    // =========================================================================
    // LOCK VALIDATION
    // =========================================================================

    /// Validate a lock request and compute everything materialization needs.
    ///
    /// `force` lets the creator start short-handed / uneven (PUG reality);
    /// without it both teams must be exactly `team_size`.
    ///
    /// `frozen`: the caller already won the lock CAS (status is
    /// `map_selection`, no match attached) and is re-snapshotting the roster
    /// after the freeze, so joins/leaves can no longer race the read
    /// (review M3/m5).
    #[instrument(skip(self))]
    pub async fn prepare_lock(
        &self,
        pug_id: PugId,
        actor_user: UserId,
        actor_player: PlayerId,
        force: bool,
        frozen: bool,
    ) -> Result<LockPlan, DomainError> {
        let pug = self.get(pug_id).await?;

        if frozen {
            if pug.status != portal_core::types::PugStatus::MapSelection || pug.is_materialized() {
                return Err(DomainError::Conflict(
                    "This PUG is already locked".to_string(),
                ));
            }
        } else if !pug.is_open() {
            return Err(DomainError::Conflict(
                "This PUG is already locked".to_string(),
            ));
        }

        let players = self.pug_repo.list_players(pug_id).await?;

        // Creator, or any captain, may lock.
        let is_captain = players
            .iter()
            .any(|p| p.player_id == actor_player && p.is_captain);
        if pug.created_by_user_id != actor_user && !is_captain {
            return Err(DomainError::NotAuthorized(
                "Only the creator or a captain can lock the lobby".to_string(),
            ));
        }

        let team1: Vec<PugPlayer> = players
            .iter()
            .filter(|p| p.team == Some(1))
            .cloned()
            .collect();
        let team2: Vec<PugPlayer> = players
            .iter()
            .filter(|p| p.team == Some(2))
            .cloned()
            .collect();

        if team1.is_empty() || team2.is_empty() {
            return Err(DomainError::InvalidState(
                "Both teams need at least one player".to_string(),
            ));
        }
        let full = usize::try_from(pug.team_size).unwrap_or(usize::MAX);
        if !force && (team1.len() != full || team2.len() != full) {
            return Err(DomainError::InvalidState(format!(
                "Teams are not full ({}v{}, need {full}v{full}) — use force to start anyway",
                team1.len(),
                team2.len()
            )));
        }

        // Every rostered player must be able to enter the server.
        let missing: Vec<&str> = team1
            .iter()
            .chain(team2.iter())
            .filter(|p| !p.has_steam_id)
            .map(|p| p.display_name.as_str())
            .collect();
        if !missing.is_empty() {
            return Err(DomainError::InvalidState(format!(
                "These players have no linked Steam account: {}",
                missing.join(", ")
            )));
        }

        let (map_pool_hint, veto_format_id) = match pug.map_selection_mode {
            PugMapSelectionMode::Veto => (
                pug.map_pool.clone().unwrap_or_default(),
                format!("{}_standard", pug.match_format),
            ),
            PugMapSelectionMode::Wheel => {
                let entries = self.pug_repo.list_wheel_entries(pug_id).await?;
                let mut unique: Vec<String> = Vec::new();
                for entry in &entries {
                    if !unique.contains(&entry.map_id) {
                        unique.push(entry.map_id.clone());
                    }
                }
                unique.sort();
                (unique, format!("wheel_bo{}", pug.match_format.game_count()))
            }
        };

        Ok(LockPlan {
            pug,
            team1,
            team2,
            map_pool_hint,
            veto_format_id,
        })
    }

    // =========================================================================
    // HELPERS
    // =========================================================================

    fn require_creator(&self, pug: &Pug, user: UserId) -> Result<(), DomainError> {
        if pug.created_by_user_id != user {
            return Err(DomainError::NotAuthorized(
                "Only the PUG creator can do that".to_string(),
            ));
        }
        Ok(())
    }
}

/// Generate a 10-character join code from an unambiguous alphabet (~49 bits).
#[must_use]
pub fn generate_join_code() -> String {
    crate::util::random_code(CODE_ALPHABET, CODE_LENGTH)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(player: &str, map: &str) -> PugWheelEntry {
        PugWheelEntry {
            pug_id: PugId::new(),
            player_id: PlayerId::new(),
            player_name: player.to_string(),
            map_id: map.to_string(),
            created_at: Utc::now(),
        }
    }

    #[test]
    fn test_join_code_shape() {
        let code = generate_join_code();
        assert_eq!(code.len(), CODE_LENGTH);
        assert!(code.bytes().all(|b| CODE_ALPHABET.contains(&b)));
    }

    #[test]
    fn test_segments_weight_duplicates_and_pad_pool() {
        let entries = vec![
            entry("alice", "de_mirage"),
            entry("bob", "de_mirage"),
            entry("carol", "de_nuke"),
            entry("dave", "de_train"), // no longer in the allowed pool
        ];
        let allowed = vec![
            "de_mirage".to_string(),
            "de_nuke".to_string(),
            "de_ancient".to_string(), // pool padding nobody nominated
        ];
        let segments = PugService::build_segments(&entries, &allowed);

        assert_eq!(segments.len(), 3);
        let mirage = segments.iter().find(|s| s.map_id == "de_mirage").unwrap();
        assert_eq!(mirage.weight, 2, "duplicate nominations weight the wheel");
        assert_eq!(mirage.nominated_by, vec!["alice", "bob"]);
        let ancient = segments.iter().find(|s| s.map_id == "de_ancient").unwrap();
        assert_eq!(ancient.weight, 1, "padded maps spin at weight 1");
        assert!(ancient.nominated_by.is_empty());
        assert!(
            !segments.iter().any(|s| s.map_id == "de_train"),
            "nominations for maps outside the allowed pool are dropped"
        );
    }

    #[test]
    fn test_draw_only_returns_allowed_winner() {
        let entries = vec![entry("alice", "de_mirage")];
        let allowed = vec!["de_mirage".to_string()];
        for _ in 0..20 {
            let segments = PugService::build_segments(&entries, &allowed);
            let draw = PugService::draw(segments).unwrap();
            assert_eq!(draw.winner_map_id, "de_mirage");
        }
    }

    #[test]
    fn test_draw_respects_weights_statistically() {
        // 9:1 weighting should land the heavy map most of the time. Bound is
        // loose (>60%) so the test never flakes.
        let mut entries = vec![entry("solo", "de_nuke")];
        for i in 0..9 {
            entries.push(entry(&format!("fan{i}"), "de_mirage"));
        }
        let allowed = vec!["de_mirage".to_string(), "de_nuke".to_string()];
        let mut mirage_wins = 0;
        for _ in 0..200 {
            let segments = PugService::build_segments(&entries, &allowed);
            let draw = PugService::draw(segments).unwrap();
            if draw.winner_map_id == "de_mirage" {
                mirage_wins += 1;
            }
        }
        assert!(
            mirage_wins > 120,
            "9x-weighted map won only {mirage_wins}/200 draws"
        );
    }
}
