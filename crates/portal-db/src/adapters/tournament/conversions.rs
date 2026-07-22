//! Type conversions from database rows to domain entities.

use crate::entities::tournament::{
    TournamentBracketRow, TournamentMapPoolRow, TournamentMatchGameRow, TournamentMatchRow,
    TournamentRegistrationRow, TournamentRow, TournamentStageRow, TournamentStandingRow,
};
use portal_core::{
    GameId, LeagueId, LeagueSeasonId, LeagueTeamSeasonId, PlayerId, TournamentBracketId,
    TournamentId, TournamentMapPoolId, TournamentMatchGameId, TournamentMatchId,
    TournamentRegistrationId, TournamentStageId, UserId,
};
use portal_domain::entities::tournament::{
    GameStatus, Tournament, TournamentBracket, TournamentMapPool, TournamentMatch,
    TournamentMatchGame, TournamentRegistration, TournamentStage, TournamentStanding,
};

impl From<TournamentRow> for Tournament {
    fn from(row: TournamentRow) -> Self {
        Self {
            id: TournamentId::from_uuid(row.id),
            game_id: GameId::from_uuid(row.game_id),
            league_id: row.league_id.map(LeagueId::from_uuid),
            season_id: row.season_id.map(LeagueSeasonId::from_uuid),
            name: row.name,
            slug: row.slug,
            description: row.description,
            logo_url: row.logo_url,
            banner_url: row.banner_url,
            format: row.format.parse().unwrap_or_default(),
            format_settings: row.format_settings,
            participant_type: row.participant_type.parse().unwrap_or_default(),
            team_size: row.team_size,
            min_participants: row.min_participants,
            max_participants: row.max_participants,
            registration_type: row.registration_type.parse().unwrap_or_default(),
            registration_start: row.registration_start,
            registration_end: row.registration_end,
            check_in_start: row.check_in_start,
            check_in_end: row.check_in_end,
            check_in_required: row.check_in_required,
            scheduling_mode: row.scheduling_mode.parse().unwrap_or_default(),
            starts_at: row.starts_at,
            ends_at: row.ends_at,
            timezone_hint: row.timezone_hint,
            default_match_format: row.default_match_format.parse().unwrap_or_default(),
            default_map_veto_format: row.default_map_veto_format,
            prize_pool: row.prize_pool,
            rules_url: row.rules_url,
            settings: row.settings,
            withdrawal_policy: row.withdrawal_policy.parse().unwrap_or_default(),
            status: row.status.parse().unwrap_or_default(),
            created_by: UserId::from_uuid(row.created_by),
            organization_id: row.organization_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
            published_at: row.published_at,
            started_at: row.started_at,
            completed_at: row.completed_at,
        }
    }
}

impl From<TournamentStageRow> for TournamentStage {
    fn from(row: TournamentStageRow) -> Self {
        Self {
            id: TournamentStageId::from_uuid(row.id),
            tournament_id: TournamentId::from_uuid(row.tournament_id),
            name: row.name,
            stage_order: row.stage_order,
            format: row.format.parse().unwrap_or_default(),
            format_settings: row.format_settings,
            advancement_count: row.advancement_count,
            advancement_rule: row.advancement_rule.parse().unwrap_or_default(),
            match_format: row.match_format.and_then(|s| s.parse().ok()),
            map_veto_format: row.map_veto_format,
            status: row.status.parse().unwrap_or_default(),
            starts_at: row.starts_at,
            ends_at: row.ends_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

impl From<TournamentBracketRow> for TournamentBracket {
    fn from(row: TournamentBracketRow) -> Self {
        Self {
            id: TournamentBracketId::from_uuid(row.id),
            stage_id: TournamentStageId::from_uuid(row.stage_id),
            tournament_id: TournamentId::from_uuid(row.tournament_id),
            name: row.name,
            bracket_type: row.bracket_type.parse().unwrap_or_default(),
            total_rounds: row.total_rounds,
            current_round: row.current_round,
            group_number: row.group_number,
            status: row.status.parse().unwrap_or_default(),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

impl From<TournamentRegistrationRow> for TournamentRegistration {
    fn from(row: TournamentRegistrationRow) -> Self {
        Self {
            id: TournamentRegistrationId::from_uuid(row.id),
            tournament_id: TournamentId::from_uuid(row.tournament_id),
            team_season_id: row.team_season_id.map(LeagueTeamSeasonId::from_uuid),
            player_id: row.player_id.map(PlayerId::from_uuid),
            adhoc_team_id: row.adhoc_team_id,
            participant_name: row.participant_name,
            participant_logo_url: row.participant_logo_url,
            registered_by: UserId::from_uuid(row.registered_by),
            registered_at: row.registered_at,
            checked_in: row.checked_in,
            checked_in_at: row.checked_in_at,
            checked_in_by: row.checked_in_by.map(UserId::from_uuid),
            seed: row.seed,
            seed_rating: row.seed_rating,
            status: row.status.parse().unwrap_or_default(),
            admin_notes: row.admin_notes,
            created_at: row.created_at,
            updated_at: row.updated_at,
            withdrawn_at: row.withdrawn_at,
        }
    }
}

impl From<TournamentMatchRow> for TournamentMatch {
    fn from(row: TournamentMatchRow) -> Self {
        Self {
            id: TournamentMatchId::from_uuid(row.id),
            bracket_id: TournamentBracketId::from_uuid(row.bracket_id),
            stage_id: TournamentStageId::from_uuid(row.stage_id),
            tournament_id: TournamentId::from_uuid(row.tournament_id),
            round: row.round,
            match_number: row.match_number,
            bracket_position: row.bracket_position,
            participant1_registration_id: row
                .participant1_registration_id
                .map(TournamentRegistrationId::from_uuid),
            participant2_registration_id: row
                .participant2_registration_id
                .map(TournamentRegistrationId::from_uuid),
            participant1_name: row.participant1_name,
            participant1_logo_url: row.participant1_logo_url,
            participant1_seed: row.participant1_seed,
            participant2_name: row.participant2_name,
            participant2_logo_url: row.participant2_logo_url,
            participant2_seed: row.participant2_seed,
            participant1_source: row
                .participant1_source
                .and_then(|v| serde_json::from_value(v).ok()),
            participant2_source: row
                .participant2_source
                .and_then(|v| serde_json::from_value(v).ok()),
            match_format: row.match_format.parse().unwrap_or_default(),
            maps_required: row.maps_required,
            scheduled_at: row.scheduled_at,
            schedule_deadline: row.schedule_deadline,
            started_at: row.started_at,
            completed_at: row.completed_at,
            participant1_score: row.participant1_score,
            participant2_score: row.participant2_score,
            winner_registration_id: row
                .winner_registration_id
                .map(TournamentRegistrationId::from_uuid),
            loser_registration_id: row
                .loser_registration_id
                .map(TournamentRegistrationId::from_uuid),
            winner_progresses_to: row.winner_progresses_to.map(TournamentMatchId::from_uuid),
            loser_progresses_to: row.loser_progresses_to.map(TournamentMatchId::from_uuid),
            status: row.status.parse().unwrap_or_default(),
            disputed: row.disputed,
            dispute_reason: row.dispute_reason,
            dispute_resolved_by: row.dispute_resolved_by.map(UserId::from_uuid),
            dispute_resolution: row.dispute_resolution,
            dispute_resolved_at: row.dispute_resolved_at,
            stream_url: row.stream_url,
            vod_url: row.vod_url,
            check_in_opens_at: row.check_in_opens_at,
            check_in_deadline: row.check_in_deadline,
            participant1_checked_in_at: row.participant1_checked_in_at,
            participant2_checked_in_at: row.participant2_checked_in_at,
            participant1_checked_in_by: row.participant1_checked_in_by.map(UserId::from_uuid),
            participant2_checked_in_by: row.participant2_checked_in_by.map(UserId::from_uuid),
            veto_required: row.veto_required,
            check_in_required: row.check_in_required,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

impl From<TournamentMatchGameRow> for TournamentMatchGame {
    fn from(row: TournamentMatchGameRow) -> Self {
        Self {
            id: TournamentMatchGameId::from_uuid(row.id),
            match_id: TournamentMatchId::from_uuid(row.match_id),
            game_number: row.game_number,
            map_id: row.map_id,
            map_picked_by: row.map_picked_by.map(TournamentRegistrationId::from_uuid),
            side_selection_by: row
                .side_selection_by
                .map(TournamentRegistrationId::from_uuid),
            participant1_score: row.participant1_score,
            participant2_score: row.participant2_score,
            winner_registration_id: row
                .winner_registration_id
                .map(TournamentRegistrationId::from_uuid),
            started_at: row.started_at,
            completed_at: row.completed_at,
            duration_seconds: row.duration_seconds,
            status: row.status.parse().unwrap_or(GameStatus::Pending),
            game_data: row.game_data,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

impl From<TournamentMapPoolRow> for TournamentMapPool {
    fn from(row: TournamentMapPoolRow) -> Self {
        Self {
            id: TournamentMapPoolId::from_uuid(row.id),
            tournament_id: TournamentId::from_uuid(row.tournament_id),
            stage_id: row.stage_id.map(TournamentStageId::from_uuid),
            maps: row.maps,
            veto_format_id: row.veto_format_id,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

impl From<TournamentStandingRow> for TournamentStanding {
    fn from(row: TournamentStandingRow) -> Self {
        Self {
            id: row.id,
            bracket_id: TournamentBracketId::from_uuid(row.bracket_id),
            registration_id: TournamentRegistrationId::from_uuid(row.registration_id),
            position: row.position,
            matches_played: row.matches_played,
            matches_won: row.matches_won,
            matches_lost: row.matches_lost,
            matches_drawn: row.matches_drawn,
            game_wins: row.game_wins,
            game_losses: row.game_losses,
            game_differential: row.game_differential,
            buchholz_score: row.buchholz_score,
            opponent_match_wins: row.opponent_match_wins,
            head_to_head: serde_json::from_value(row.head_to_head).unwrap_or_default(),
            tiebreaker_score: row.tiebreaker_score,
            is_tied: row.is_tied,
            participant_name: row.participant_name,
            points: row.points,
            updated_at: row.updated_at,
        }
    }
}
