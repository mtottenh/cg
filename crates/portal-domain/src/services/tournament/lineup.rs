//! Lineup service — provisional declaration write path (Phase B) and locking.
//!
//! A captain declares a provisional lineup for their registration. The
//! declaration is optional (§0b Q2): the system is correct even if nobody
//! declares, because the authoritative lineup is derived from the demo (Phase C).
//!
//! Authorization (who may declare) is enforced at the API layer via
//! `require_registration_actor` — the same captain/owner/delegate model as
//! check-in (P-24). This service assumes the caller is already authorized and
//! enforces only domain invariants.

use portal_core::types::{ParticipationStatus, TournamentMatchStatus};
use portal_core::{DomainError, PlayerId, TournamentMatchId, TournamentRegistrationId, UserId};
use std::sync::Arc;
use tracing::info;

use crate::entities::match_lineup::{
    DeclareLineupCommand, LineupPlayerInput, MatchLineup, MatchLineupWithPlayers,
};
use crate::repositories::league_team::LeagueTeamMemberRepository;
use crate::repositories::match_lineup::{MatchLineupRepository, MaterializeDemoLineup};
use crate::repositories::tournament::{
    TournamentMatchRepository, TournamentRegistrationRepository,
};

/// Service for declaring and reading match lineups.
#[derive(Clone)]
pub struct LineupService<LR, MR, RR, MMR> {
    lineup_repo: Arc<LR>,
    match_repo: Arc<MR>,
    registration_repo: Arc<RR>,
    member_repo: Arc<MMR>,
}

impl<LR, MR, RR, MMR> LineupService<LR, MR, RR, MMR>
where
    LR: MatchLineupRepository,
    MR: TournamentMatchRepository,
    RR: TournamentRegistrationRepository,
    MMR: LeagueTeamMemberRepository,
{
    /// Create a new lineup service.
    pub fn new(
        lineup_repo: Arc<LR>,
        match_repo: Arc<MR>,
        registration_repo: Arc<RR>,
        member_repo: Arc<MMR>,
    ) -> Self {
        Self {
            lineup_repo,
            match_repo,
            registration_repo,
            member_repo,
        }
    }

    /// Declare (create or replace) the provisional lineup for a registration.
    ///
    /// Validates that the registration is a participant in the match and that
    /// the match has not started (declaration closes at the `PickBan`/
    /// `InProgress` lock). `was_rostered` is snapshotted per player.
    pub async fn declare_lineup(
        &self,
        match_id: TournamentMatchId,
        registration_id: TournamentRegistrationId,
        player_ids: Vec<PlayerId>,
        declared_by: UserId,
        submit: bool,
        notes: Option<String>,
    ) -> Result<MatchLineupWithPlayers, DomainError> {
        let match_ = self
            .match_repo
            .find_by_id(match_id)
            .await?
            .ok_or(DomainError::TournamentMatchNotFound(match_id))?;

        // The registration must be one of the two participants.
        let is_participant = match_.participant1_registration_id == Some(registration_id)
            || match_.participant2_registration_id == Some(registration_id);
        if !is_participant {
            return Err(DomainError::NotAuthorized(
                "Registration is not a participant in this match".to_string(),
            ));
        }

        // Declaration window closes once the match starts. The lock is stamped
        // on the PickBan/InProgress transition; before that, editing is allowed.
        if !declaration_window_open(match_.status) {
            return Err(DomainError::InvalidState(format!(
                "Lineup cannot be declared while match is {}",
                match_.status
            )));
        }
        // Defence in depth: if a lineup already exists and is locked, refuse.
        if let Some(existing) = self
            .lineup_repo
            .find_by_match_registration(match_id, registration_id)
            .await?
            && !existing.is_editable()
        {
            return Err(DomainError::InvalidState(
                "Lineup is locked and can no longer be edited".to_string(),
            ));
        }

        // Resolve the roster snapshot for each declared player.
        let registration = self
            .registration_repo
            .find_by_id(registration_id)
            .await?
            .ok_or(DomainError::TournamentRegistrationNotFound(registration_id))?;

        let mut players = Vec::with_capacity(player_ids.len());
        for player_id in player_ids {
            let was_rostered = if let Some(team_season_id) = registration.team_season_id {
                self.member_repo
                    .is_member(team_season_id, player_id)
                    .await?
            } else {
                // Individual/adhoc registration: the "roster" is the registered player.
                registration.player_id == Some(player_id)
            };
            players.push(LineupPlayerInput {
                player_id,
                was_rostered,
                game_number: None,
                participation_status: ParticipationStatus::Confirmed,
            });
        }

        // Short-handed is advisory here; the min-size rule lives on the season
        // and is enforced at check-in gating (not built in this phase). We flag
        // an empty lineup as an obvious signal.
        let short_handed = players.is_empty();

        let cmd = DeclareLineupCommand {
            match_id,
            registration_id,
            players,
            declared_by,
            submit,
            short_handed,
            notes,
        };
        let lineup = self.lineup_repo.declare(cmd).await?;

        info!(
            match_id = %match_id,
            registration_id = %registration_id,
            declared_by = %declared_by,
            submit,
            "Provisional lineup declared"
        );

        self.lineup_repo
            .get_with_players(lineup.id)
            .await?
            .ok_or_else(|| DomainError::Internal("Lineup vanished after declare".to_string()))
    }

    /// Materialize the authoritative, demo-derived lineup for one registration
    /// and map (Phase C, the P-25 fix).
    ///
    /// `player_ids` are the demo's players that resolved to registered accounts
    /// AND played on this registration's side (the caller assigns the side from
    /// the demo's team structure). For each, `was_rostered` is snapshotted from
    /// the registration's roster; the repository auto-sets `is_substitute` when
    /// the player is not rostered (a registered non-rostered player is the
    /// ordinary casual-league substitute, §0b). Unregistered demo players are
    /// NOT materialized here — they are the ringer case raised through the
    /// result-review flow (Phase D).
    ///
    /// The demo is authoritative, so this runs regardless of whether a
    /// provisional lineup was declared — the system is correct even if nobody
    /// declared (§0b).
    pub async fn materialize_demo_lineup(
        &self,
        match_id: TournamentMatchId,
        registration_id: TournamentRegistrationId,
        game_number: Option<i32>,
        player_ids: Vec<PlayerId>,
    ) -> Result<MatchLineup, DomainError> {
        let registration = self
            .registration_repo
            .find_by_id(registration_id)
            .await?
            .ok_or(DomainError::TournamentRegistrationNotFound(registration_id))?;

        let mut players = Vec::with_capacity(player_ids.len());
        for player_id in player_ids {
            let was_rostered = if let Some(team_season_id) = registration.team_season_id {
                self.member_repo
                    .is_member(team_season_id, player_id)
                    .await?
            } else {
                registration.player_id == Some(player_id)
            };
            players.push(LineupPlayerInput {
                player_id,
                was_rostered,
                game_number,
                participation_status: ParticipationStatus::Confirmed,
            });
        }

        let lineup = self
            .lineup_repo
            .materialize_demo(MaterializeDemoLineup {
                match_id,
                registration_id,
                game_number,
                players,
            })
            .await?;

        info!(
            match_id = %match_id,
            registration_id = %registration_id,
            game_number = ?game_number,
            "Materialized demo-derived lineup"
        );
        Ok(lineup)
    }

    /// List all lineups for a match with their player rows.
    pub async fn list_for_match(
        &self,
        match_id: TournamentMatchId,
    ) -> Result<Vec<MatchLineupWithPlayers>, DomainError> {
        self.lineup_repo.list_for_match(match_id).await
    }

    /// Fetch a single lineup (with players) for one registration.
    pub async fn get_for_registration(
        &self,
        match_id: TournamentMatchId,
        registration_id: TournamentRegistrationId,
    ) -> Result<Option<MatchLineupWithPlayers>, DomainError> {
        let Some(lineup) = self
            .lineup_repo
            .find_by_match_registration(match_id, registration_id)
            .await?
        else {
            return Ok(None);
        };
        self.lineup_repo.get_with_players(lineup.id).await
    }

    /// Lock every lineup for a match. Called when the match transitions to
    /// `PickBan`/`InProgress` (§0 Q2). Idempotent.
    pub async fn lock_lineups(&self, match_id: TournamentMatchId) -> Result<u64, DomainError> {
        let locked = self.lineup_repo.lock_for_match(match_id).await?;
        if locked > 0 {
            info!(match_id = %match_id, locked, "Locked match lineups");
        }
        Ok(locked)
    }
}

#[async_trait::async_trait]
impl<LR, MR, RR, MMR> crate::services::demo::DemoLineupMaterializer
    for LineupService<LR, MR, RR, MMR>
where
    LR: MatchLineupRepository,
    MR: TournamentMatchRepository,
    RR: TournamentRegistrationRepository,
    MMR: LeagueTeamMemberRepository,
{
    async fn materialize_from_demo(
        &self,
        match_id: TournamentMatchId,
        game_number: Option<i32>,
        demo_team1_name: Option<String>,
        demo_team2_name: Option<String>,
        players: Vec<crate::services::demo::DemoResolvedPlayer>,
    ) -> Result<(), DomainError> {
        // Opt-in gate (§9): do nothing unless the season requires lineups. This
        // keeps the pre-cutover path (and the global attribution) unchanged.
        if !self
            .lineup_repo
            .is_lineup_required_for_match(match_id)
            .await?
        {
            return Ok(());
        }

        let match_ = self
            .match_repo
            .find_by_id(match_id)
            .await?
            .ok_or(DomainError::TournamentMatchNotFound(match_id))?;
        let Some(reg1) = match_.participant1_registration_id else {
            return Ok(());
        };
        let Some(reg2) = match_.participant2_registration_id else {
            return Ok(());
        };

        // Resolve each registration's team-season (None for individual/adhoc).
        let ts1 = self
            .registration_repo
            .find_by_id(reg1)
            .await?
            .and_then(|r| r.team_season_id);
        let ts2 = self
            .registration_repo
            .find_by_id(reg2)
            .await?
            .and_then(|r| r.team_season_id);

        // Assign each resolved player to a side. Roster membership is
        // authoritative; fall back to the demo's team-name for subs. A player we
        // cannot place is dropped (attribution then de-scopes it — P-25).
        let mut side1 = Vec::new();
        let mut side2 = Vec::new();
        for p in players {
            let on_ts1 = match ts1 {
                Some(ts) => self.member_repo.is_member(ts, p.player_id).await?,
                None => false,
            };
            let on_ts2 = match ts2 {
                Some(ts) => self.member_repo.is_member(ts, p.player_id).await?,
                None => false,
            };

            let side = if on_ts1 && !on_ts2 {
                Some(1)
            } else if on_ts2 && !on_ts1 {
                Some(2)
            } else {
                // Not rostered on exactly one side: infer from the demo team name.
                infer_side(&p.demo_team_name, &demo_team1_name, &demo_team2_name)
            };

            match side {
                Some(1) => side1.push(p.player_id),
                Some(2) => side2.push(p.player_id),
                _ => {}
            }
        }

        if !side1.is_empty() {
            self.materialize_demo_lineup(match_id, reg1, game_number, side1)
                .await?;
        }
        if !side2.is_empty() {
            self.materialize_demo_lineup(match_id, reg2, game_number, side2)
                .await?;
        }
        Ok(())
    }
}

/// Infer a registration side (1 or 2) from the demo's team name for a player.
///
/// Returns `None` when neither team name matches — the caller drops the player
/// rather than guessing a side (attribution then de-scopes them).
fn infer_side(
    player_team: &Option<String>,
    demo_team1: &Option<String>,
    demo_team2: &Option<String>,
) -> Option<i32> {
    let name = player_team.as_deref()?;
    if demo_team1.as_deref() == Some(name) {
        Some(1)
    } else if demo_team2.as_deref() == Some(name) {
        Some(2)
    } else {
        None
    }
}

/// Whether a provisional lineup may be declared while the match is in `status`.
///
/// Open before the match starts; closed once veto/play begins (the lock point).
const fn declaration_window_open(status: TournamentMatchStatus) -> bool {
    matches!(
        status,
        TournamentMatchStatus::Pending
            | TournamentMatchStatus::Ready
            | TournamentMatchStatus::Scheduled
            | TournamentMatchStatus::CheckingIn
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declaration_window_open_before_start() {
        assert!(declaration_window_open(TournamentMatchStatus::CheckingIn));
        assert!(declaration_window_open(TournamentMatchStatus::Scheduled));
        assert!(!declaration_window_open(TournamentMatchStatus::PickBan));
        assert!(!declaration_window_open(TournamentMatchStatus::InProgress));
        assert!(!declaration_window_open(TournamentMatchStatus::Completed));
    }
}
