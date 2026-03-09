//! Veto (map pick/ban) service.
//!
//! Handles the map veto process for tournament matches.
//! Teams alternate performing bans and picks based on a format configuration.

use std::sync::Arc;

use chrono::{Duration, Utc};
use portal_core::{DomainError, TournamentMatchId, TournamentRegistrationId, UserId, VetoSessionId};
use rand::Rng;
use rand::seq::IndexedRandom;
use tracing::{info, instrument, warn};

use crate::entities::veto::{
    MapStatus, MapVetoStatus, VetoAction, VetoActionResult,
    VetoFormat, VetoFormatAction, VetoSession, VetoSessionState, VetoStatus,
};
use portal_core::{SideSelectionMode, VetoActionType, VetoFormatConfig};
use crate::repositories::tournament::{
    CreateVetoAction, CreateVetoSession, TournamentMatchRepository, TournamentRegistrationRepository,
    UpdateVetoSession, VetoActionRepository, VetoSessionRepository,
};

// =============================================================================
// PROVIDER TRAITS
// =============================================================================

/// Provides veto format resolution — implemented by the API layer's plugin adapter.
pub trait VetoFormatProvider: Send + Sync {
    /// Look up a veto format by its ID (e.g., "bo1_standard", "bo3_standard").
    fn get_format(&self, format_id: &str) -> Option<VetoFormatConfig>;
}

/// Provides game-specific side selection behavior.
pub trait SideSelectionProvider: Send + Sync {
    /// Pick a random side for auto-assignment (CoinFlip mode).
    /// Returns None if the game has no sides.
    fn random_side(&self, game_id: &str) -> Option<String>;
}

/// Service for managing map veto sessions.
#[derive(Clone)]
pub struct VetoService<VSR, VAR, TMR, TRR>
where
    VSR: VetoSessionRepository,
    VAR: VetoActionRepository,
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
{
    session_repo: Arc<VSR>,
    action_repo: Arc<VAR>,
    match_repo: Arc<TMR>,
    registration_repo: Arc<TRR>,
    format_provider: Option<Arc<dyn VetoFormatProvider>>,
    side_provider: Option<Arc<dyn SideSelectionProvider>>,
    default_timeout_seconds: u32,
}

impl<VSR, VAR, TMR, TRR> VetoService<VSR, VAR, TMR, TRR>
where
    VSR: VetoSessionRepository,
    VAR: VetoActionRepository,
    TMR: TournamentMatchRepository,
    TRR: TournamentRegistrationRepository,
{
    /// Create a new veto service.
    pub fn new(
        session_repo: Arc<VSR>,
        action_repo: Arc<VAR>,
        match_repo: Arc<TMR>,
        registration_repo: Arc<TRR>,
    ) -> Self {
        Self {
            session_repo,
            action_repo,
            match_repo,
            registration_repo,
            format_provider: None,
            side_provider: None,
            default_timeout_seconds: 30,
        }
    }

    /// Create a new veto service with custom timeout.
    pub fn with_timeout(mut self, timeout_seconds: u32) -> Self {
        self.default_timeout_seconds = timeout_seconds;
        self
    }

    /// Inject a veto format provider (plugin-backed format resolution).
    pub fn with_format_provider(mut self, provider: Arc<dyn VetoFormatProvider>) -> Self {
        self.format_provider = Some(provider);
        self
    }

    /// Inject a side selection provider (plugin-backed random side picks).
    pub fn with_side_provider(mut self, provider: Arc<dyn SideSelectionProvider>) -> Self {
        self.side_provider = Some(provider);
        self
    }

    /// Create a new veto session for a match.
    #[instrument(skip(self, veto_format))]
    pub async fn create_session(
        &self,
        match_id: TournamentMatchId,
        veto_format: &VetoFormat,
        map_pool: Vec<String>,
        timeout_seconds: Option<u32>,
        side_selection_mode: SideSelectionMode,
    ) -> Result<VetoSession, DomainError> {
        // Verify the match exists
        let match_ = self.match_repo.find_by_id(match_id).await?.ok_or_else(|| {
            DomainError::TournamentMatchNotFound(match_id.to_string())
        })?;

        // Verify match has both participants
        if !match_.has_both_participants() {
            return Err(DomainError::InvalidState(
                "Match must have both participants to start veto".to_string(),
            ));
        }

        // Verify map pool is sufficient
        if map_pool.len() < veto_format.min_map_pool {
            return Err(DomainError::InvalidMatchResult(format!(
                "Map pool must have at least {} maps for {} format",
                veto_format.min_map_pool, veto_format.id
            )));
        }

        // Check if session already exists
        if let Some(_existing) = self.session_repo.find_by_match(match_id).await? {
            return Err(DomainError::Conflict(
                "Veto session already exists for this match".to_string(),
            ));
        }

        let session = self
            .session_repo
            .create(CreateVetoSession {
                match_id,
                veto_format_id: veto_format.id.clone(),
                map_pool: map_pool.clone(),
                timeout_seconds: timeout_seconds.unwrap_or(self.default_timeout_seconds),
                side_selection_mode,
            })
            .await?;

        info!(
            session_id = %session.id,
            match_id = %match_id,
            format = %veto_format.id,
            "Veto session created"
        );

        Ok(session)
    }

    /// Start the veto session (begin coin flip phase).
    #[instrument(skip(self))]
    pub async fn start_session(
        &self,
        session_id: VetoSessionId,
    ) -> Result<VetoSession, DomainError> {
        let session = self.get_session(session_id).await?;

        if !session.status.can_start() {
            return Err(DomainError::InvalidState(format!(
                "Cannot start veto session in {} status",
                session.status
            )));
        }

        let session = self
            .session_repo
            .update_status(session_id, VetoStatus::CoinFlip)
            .await?;

        info!(session_id = %session_id, "Veto session started, awaiting coin flip");

        Ok(session)
    }

    /// Record the coin flip result and set first action team.
    #[instrument(skip(self))]
    pub async fn record_coin_flip(
        &self,
        session_id: VetoSessionId,
        winner: TournamentRegistrationId,
        winner_goes_first: bool,
    ) -> Result<VetoSession, DomainError> {
        let session = self.get_session(session_id).await?;

        if !session.status.can_coin_flip() {
            return Err(DomainError::InvalidState(format!(
                "Cannot record coin flip in {} status",
                session.status
            )));
        }

        // Verify winner is a participant
        let match_ = self.match_repo.find_by_id(session.match_id).await?.ok_or_else(|| {
            DomainError::TournamentMatchNotFound(session.match_id.to_string())
        })?;

        let is_participant = match_.participant1_registration_id == Some(winner)
            || match_.participant2_registration_id == Some(winner);

        if !is_participant {
            return Err(DomainError::NotAuthorized(
                "Coin flip winner must be a match participant".to_string(),
            ));
        }

        // Determine first action team
        let other_team = if match_.participant1_registration_id == Some(winner) {
            match_.participant2_registration_id
        } else {
            match_.participant1_registration_id
        };

        let first_action = if winner_goes_first {
            winner
        } else {
            other_team.ok_or_else(|| {
                DomainError::InvalidState("Other participant not set".to_string())
            })?
        };

        // Set deadline for first action
        let deadline = Utc::now() + Duration::seconds(i64::from(session.timeout_seconds));

        let session = self
            .session_repo
            .update(
                session_id,
                UpdateVetoSession {
                    coin_flip_winner_registration_id: Some(winner),
                    first_action_registration_id: Some(first_action),
                    current_action_number: Some(1),
                    current_team_turn: Some(Some(first_action)),
                    status: Some(VetoStatus::InProgress),
                    action_deadline: Some(Some(deadline)),
                    started_at: Some(Utc::now()),
                    ..Default::default()
                },
            )
            .await?;

        info!(
            session_id = %session_id,
            winner = %winner,
            first_action = %first_action,
            "Coin flip recorded, veto in progress"
        );

        Ok(session)
    }

    /// Perform a veto action (ban or pick).
    ///
    /// The caller is responsible for verifying authorization via `VetoAuthorizationService`
    /// before calling this method. This method only verifies it's the correct team's turn.
    ///
    /// # Arguments
    /// * `session_id` - The veto session ID
    /// * `map_id` - The map being picked/banned
    /// * `acting_for_registration` - The registration the user is acting for (must be current team's turn)
    /// * `performed_by_user` - The user performing the action (for audit)
    #[instrument(skip(self))]
    pub async fn perform_action(
        &self,
        session_id: VetoSessionId,
        map_id: &str,
        acting_for_registration: TournamentRegistrationId,
        performed_by_user: UserId,
    ) -> Result<VetoActionResult, DomainError> {
        let session = self.get_session(session_id).await?;

        if !session.status.can_act() {
            return Err(DomainError::InvalidState(format!(
                "Cannot perform action in {} status",
                session.status
            )));
        }

        // Verify map is available
        if !session.is_map_available(map_id) {
            return Err(DomainError::InvalidMatchResult(format!(
                "Map '{map_id}' is not available for selection"
            )));
        }

        // Get the current team turn
        let current_team = session.current_team_turn.ok_or_else(|| {
            DomainError::InvalidState("No team turn set".to_string())
        })?;

        // Verify it's actually this team's turn
        if current_team != acting_for_registration {
            return Err(DomainError::NotAuthorized(
                "It is not your team's turn".to_string(),
            ));
        }

        // Get the veto format to determine action type
        let format = self.get_format(&session.veto_format_id)?;
        let action_index = (session.current_action_number as usize).saturating_sub(1);
        let format_action = format.get_action(action_index).ok_or_else(|| {
            DomainError::InvalidState("Action index out of bounds".to_string())
        })?;

        // Record the action
        let mut result = self
            .record_action_internal(
                &session,
                &format,
                format_action,
                map_id,
                Some(current_team),
                Some(performed_by_user),
                false,
                None,
            )
            .await?;

        // Auto-chain decider actions (team 0 = automatic, last remaining map)
        while !result.veto_complete {
            if let Some(next_type) = result.next_action_type {
                if !matches!(next_type, VetoActionType::Decider) {
                    break;
                }
            } else {
                break;
            }

            let updated_session = self.get_session(session_id).await?;
            let next_index = (updated_session.current_action_number as usize).saturating_sub(1);
            let next_format_action = format.get_action(next_index).ok_or_else(|| {
                DomainError::InvalidState("Decider action index out of bounds".to_string())
            })?;

            // Pick the last remaining map as the decider
            let decider_map = updated_session
                .remaining_maps
                .first()
                .cloned()
                .ok_or_else(|| DomainError::InvalidState("No maps remaining for decider".to_string()))?;

            result = self
                .record_action_internal(
                    &updated_session,
                    &format,
                    next_format_action,
                    &decider_map,
                    None,
                    None,
                    true,
                    Some("decider_auto"),
                )
                .await?;
        }

        Ok(result)
    }

    /// Process a timeout for the current action.
    ///
    /// Automatically selects a random available map for the team.
    #[instrument(skip(self))]
    pub async fn process_timeout(
        &self,
        session_id: VetoSessionId,
    ) -> Result<VetoActionResult, DomainError> {
        let session = self.get_session(session_id).await?;

        if !session.is_timed_out() {
            return Err(DomainError::InvalidState(
                "Session is not timed out".to_string(),
            ));
        }

        // Select random map from remaining
        let map_id = session
            .remaining_maps
            .choose(&mut rand::rng())
            .ok_or_else(|| DomainError::InvalidState("No maps remaining".to_string()))?
            .clone();

        let current_team = session.current_team_turn;
        let format = self.get_format(&session.veto_format_id)?;
        let action_index = (session.current_action_number as usize).saturating_sub(1);
        let format_action = format.get_action(action_index).ok_or_else(|| {
            DomainError::InvalidState("Action index out of bounds".to_string())
        })?;

        let result = self
            .record_action_internal(
                &session,
                &format,
                format_action,
                &map_id,
                current_team,
                None,
                true,
                Some("timeout"),
            )
            .await?;

        warn!(
            session_id = %session_id,
            map = %map_id,
            "Timeout auto-action performed"
        );

        Ok(result)
    }

    /// Select a side for a picked map (e.g., CT vs T for CS2).
    ///
    /// The caller is responsible for verifying authorization via `VetoAuthorizationService`
    /// before calling this method. This method verifies it's the correct team selecting the side.
    ///
    /// # Arguments
    /// * `session_id` - The veto session ID
    /// * `action_number` - The action number to select side for
    /// * `side` - The side being selected (e.g., "ct", "t")
    /// * `acting_for_registration` - The registration the user is acting for (must be opponent of picker)
    /// * `selected_by_user` - The user performing the selection (for audit)
    #[instrument(skip(self))]
    pub async fn select_side(
        &self,
        session_id: VetoSessionId,
        action_number: u32,
        side: &str,
        acting_for_registration: TournamentRegistrationId,
        selected_by_user: UserId,
    ) -> Result<VetoAction, DomainError> {
        let session = self.get_session(session_id).await?;

        // Get the action
        let action = self
            .action_repo
            .find_by_session_and_number(session_id, action_number)
            .await?
            .ok_or_else(|| DomainError::InvalidMatchResult("Action not found".to_string()))?;

        // Verify action is a pick (only picks have side selection)
        if !action.is_pick() {
            return Err(DomainError::InvalidState(
                "Side selection only applies to picks".to_string(),
            ));
        }

        // Verify side hasn't been selected
        if action.has_side_selection() {
            return Err(DomainError::InvalidState(
                "Side already selected for this action".to_string(),
            ));
        }

        // Validate side selection is allowed for this mode
        match session.side_selection_mode {
            SideSelectionMode::Knife => {
                return Err(DomainError::InvalidState(
                    "Side selection is not available in knife mode".to_string(),
                ));
            }
            SideSelectionMode::CoinFlip => {
                return Err(DomainError::InvalidState(
                    "Side selection is automatic in coin flip mode".to_string(),
                ));
            }
            SideSelectionMode::PickerChoice => {
                // Picker chooses side — reject for decider maps
                if action.is_decider() {
                    return Err(DomainError::InvalidState(
                        "Side selection is not available for decider maps".to_string(),
                    ));
                }
            }
        }

        let picker = action.performed_by_registration_id.ok_or_else(|| {
            DomainError::InvalidState("Action has no performer".to_string())
        })?;

        // In picker_choice mode, the picker selects the side
        if acting_for_registration != picker {
            return Err(DomainError::NotAuthorized(
                "Only the picker can select the side".to_string(),
            ));
        }

        // Record side selection
        let action = self
            .action_repo
            .update_side_selection(action.id, side.to_string(), picker)
            .await?;

        info!(
            session_id = %session_id,
            action_number = action_number,
            side = side,
            "Side selected"
        );

        Ok(action)
    }

    /// Get the current state of a veto session.
    #[instrument(skip(self))]
    pub async fn get_session_state(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<VetoSessionState, DomainError> {
        let session = self
            .session_repo
            .find_by_match(match_id)
            .await?
            .ok_or_else(|| DomainError::Internal("Veto session not found".to_string()))?;

        let actions = self.action_repo.list_by_session(session.id).await?;
        let format = self.get_format(&session.veto_format_id)?;

        // Build current action info
        let current_action = if session.is_in_progress() {
            let action_index = (session.current_action_number as usize).saturating_sub(1);
            format.get_action(action_index).cloned()
        } else {
            None
        };

        // Build map status list
        let maps_with_status = self.build_map_status(&session, &actions);

        Ok(VetoSessionState {
            session,
            actions,
            format,
            current_action,
            maps_with_status,
        })
    }

    /// Find all sessions with expired action deadlines.
    pub async fn find_timed_out_sessions(&self) -> Result<Vec<VetoSession>, DomainError> {
        self.session_repo.find_timed_out().await
    }

    // =========================================================================
    // INTERNAL HELPERS
    // =========================================================================

    async fn get_session(&self, id: VetoSessionId) -> Result<VetoSession, DomainError> {
        self.session_repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| DomainError::Internal(format!("Veto session {id} not found")))
    }

    fn get_format(&self, format_id: &str) -> Result<VetoFormat, DomainError> {
        // Try injected provider first (plugin-backed)
        if let Some(provider) = &self.format_provider {
            if let Some(fmt) = provider.get_format(format_id) {
                return Ok(fmt);
            }
        }

        // Fall back to built-in formats
        match format_id {
            "bo1_veto" | "bo1_standard" => Ok(VetoFormat::bo1()),
            "bo3_veto" | "bo3_standard" => Ok(VetoFormat::bo3()),
            "bo5_veto" | "bo5_standard" => Ok(VetoFormat::bo5()),
            _ => Err(DomainError::InvalidMatchResult(format!(
                "Unknown veto format: {format_id}"
            ))),
        }
    }

    async fn record_action_internal(
        &self,
        session: &VetoSession,
        format: &VetoFormat,
        format_action: &VetoFormatAction,
        map_id: &str,
        performed_by: Option<TournamentRegistrationId>,
        performed_by_user: Option<UserId>,
        was_auto: bool,
        auto_reason: Option<&str>,
    ) -> Result<VetoActionResult, DomainError> {
        // Create the action
        let mut action = self
            .action_repo
            .create(CreateVetoAction {
                session_id: session.id,
                action_number: session.current_action_number,
                action_type: format_action.action_type,
                map_id: map_id.to_string(),
                performed_by_registration_id: performed_by,
                performed_by_user_id: performed_by_user,
                was_auto_action: was_auto,
                auto_action_reason: auto_reason.map(String::from),
            })
            .await?;

        // Auto-assign random side in CoinFlip mode for pick actions
        if session.side_selection_mode == SideSelectionMode::CoinFlip
            && matches!(format_action.action_type, VetoActionType::Pick)
        {
            // Use injected side provider for game-agnostic random side,
            // falling back to coin-flip between "ct" and "t".
            let side = self.side_provider.as_ref()
                .and_then(|sp| sp.random_side(&session.veto_format_id))
                .unwrap_or_else(|| {
                    if rand::rng().random_bool(0.5) { "ct".to_string() } else { "t".to_string() }
                });
            let selector = performed_by.unwrap_or_else(|| {
                session.first_action_registration_id.unwrap_or_default()
            });
            action = self
                .action_repo
                .update_side_selection(action.id, side, selector)
                .await?;
        }

        // Update session state
        let mut remaining = session.remaining_maps.clone();
        remaining.retain(|m| m != map_id);

        let mut selected = session.selected_maps.clone();
        if matches!(format_action.action_type, VetoActionType::Pick | VetoActionType::Decider) {
            selected.push(map_id.to_string());
        }

        let next_action_number = session.current_action_number + 1;
        let veto_complete = format.is_complete_at(next_action_number as usize);

        // Determine next team turn
        let (next_team, next_status, deadline) = if veto_complete {
            (None, VetoStatus::Completed, None)
        } else {
            let next_format_action = format.get_action(next_action_number as usize - 1);
            let next_team = self.determine_next_team(session, next_format_action).await?;
            let deadline = Utc::now() + Duration::seconds(i64::from(session.timeout_seconds));
            (Some(next_team), VetoStatus::InProgress, Some(deadline))
        };

        let update = UpdateVetoSession {
            current_action_number: Some(next_action_number),
            current_team_turn: Some(next_team),
            remaining_maps: Some(remaining),
            selected_maps: Some(selected.clone()),
            status: Some(next_status),
            action_deadline: Some(deadline),
            completed_at: if veto_complete { Some(Utc::now()) } else { None },
            ..Default::default()
        };

        let updated_session = self.session_repo.update(session.id, update).await?;

        info!(
            session_id = %session.id,
            action_type = %format_action.action_type,
            map = %map_id,
            complete = veto_complete,
            "Veto action recorded"
        );

        Ok(VetoActionResult {
            session: updated_session,
            action,
            veto_complete,
            next_team,
            next_action_type: if veto_complete {
                None
            } else {
                format.get_action(next_action_number as usize - 1).map(|a| a.action_type)
            },
        })
    }

    async fn determine_next_team(
        &self,
        session: &VetoSession,
        next_action: Option<&VetoFormatAction>,
    ) -> Result<TournamentRegistrationId, DomainError> {
        let match_ = self.match_repo.find_by_id(session.match_id).await?.ok_or_else(|| {
            DomainError::TournamentMatchNotFound(session.match_id.to_string())
        })?;

        let team1 = match_.participant1_registration_id.ok_or_else(|| {
            DomainError::InvalidState("Participant 1 not set".to_string())
        })?;
        let team2 = match_.participant2_registration_id.ok_or_else(|| {
            DomainError::InvalidState("Participant 2 not set".to_string())
        })?;

        let first_action = session.first_action_registration_id.ok_or_else(|| {
            DomainError::InvalidState("First action team not set".to_string())
        })?;

        let second_action = if first_action == team1 { team2 } else { team1 };

        match next_action {
            Some(action) => {
                match action.team {
                    0 => {
                        // Decider - automatic, no team turn
                        Ok(first_action) // Default to first action team
                    }
                    1 => Ok(first_action),
                    2 => Ok(second_action),
                    _ => Err(DomainError::InvalidMatchResult("Invalid team number in format".to_string())),
                }
            }
            None => Err(DomainError::InvalidState("No next action".to_string())),
        }
    }

    fn build_map_status(&self, session: &VetoSession, actions: &[VetoAction]) -> Vec<MapStatus> {
        session
            .map_pool
            .iter()
            .map(|map_id| {
                // Find action for this map
                let action = actions.iter().find(|a| a.map_id == *map_id);

                let (status, banned_by, picked_by, game_number) = match action {
                    Some(a) if a.is_ban() => {
                        (MapVetoStatus::Banned, a.performed_by_registration_id, None, None)
                    }
                    Some(a) if a.is_pick() => {
                        let game_num = session.selected_maps.iter().position(|m| m == map_id).map(|i| i as u32 + 1);
                        (MapVetoStatus::Picked, None, a.performed_by_registration_id, game_num)
                    }
                    Some(a) if a.is_decider() => {
                        let game_num = session.selected_maps.iter().position(|m| m == map_id).map(|i| i as u32 + 1);
                        (MapVetoStatus::Decider, None, None, game_num)
                    }
                    _ if session.remaining_maps.contains(map_id) => {
                        (MapVetoStatus::Available, None, None, None)
                    }
                    _ => (MapVetoStatus::Banned, None, None, None), // Default to banned if not remaining
                };

                MapStatus {
                    map_id: map_id.clone(),
                    map_name: map_id.clone(), // Would get from plugin in reality
                    image_url: None,
                    status,
                    banned_by,
                    picked_by,
                    game_number,
                }
            })
            .collect()
    }
}
