//! Database entity types (row structs).
//!
//! These types map directly to database tables via `sqlx::FromRow`.
//! They are intentionally flat and use nullable types where the schema allows.
//!
//! Domain types are in `portal-domain`; mappings from DB types to domain types
//! are implemented alongside each entity.

mod audit;
mod availability;
mod demo;
mod dispute;
mod evidence;
mod forfeit;
mod game;
mod league;
pub mod league_team;
mod player;
mod rbac;
mod result_review;
pub mod tournament;
mod user;
mod veto_delegate;
mod veto_lobby_message;

pub use audit::{EntityChangeRow, NewEntityChange};
pub use availability::{AvailabilityOverrideRow, AvailabilityWindowRow, SuggestedTimeRow};
pub use demo::{
    DemoMatchLinkRow, DemoPlayerRow, DemoRow, NewDemo, NewDemoMatchLink, NewDemoPlayer,
    UpdateDemoStats,
};
pub use evidence::{
    EvidenceAccessLogRow, EvidenceRow, NewEvidence, NewEvidenceAccessLog, UpdateEvidence,
};
pub use game::{GameRow, NewGame, UpdateGame};
pub use league::{
    LeagueInvitationRow, LeagueMemberRow, LeagueMemberWithUserRow, LeagueRow, NewLeague,
    NewLeagueInvitation, NewLeagueMember, UpdateLeague, UpdateLeagueInvitation,
    UserLeagueMembershipRow,
};
pub use league_team::{
    LeagueSeasonParticipantRow, LeagueSeasonRow, LeagueTeamInvitationRow,
    LeagueTeamInvitationWithTeamRow, LeagueTeamMemberRow, LeagueTeamMemberWithPlayerRow,
    LeagueTeamRow, LeagueTeamSeasonRow, LeagueTeamSummaryRow, NewLeagueSeason,
    NewLeagueSeasonParticipant, NewLeagueTeam, NewLeagueTeamInvitation, NewLeagueTeamMember,
    NewLeagueTeamSeason, PlayerLeagueTeamMembershipRow, UpdateLeagueSeason,
    UpdateLeagueSeasonParticipant, UpdateLeagueTeam, UpdateLeagueTeamInvitation,
    UpdateLeagueTeamMember, UpdateLeagueTeamSeason,
};
pub use player::{
    NewPlayer, NewPlayerGameProfile, PlayerGameProfileRow, PlayerRow, UpdatePlayer,
    UpdatePlayerRating,
};
pub use rbac::{BanRow, NewBan, NewRole, NewUserRole, PermissionRow, RoleRow, UserRoleRow};
pub use tournament::{
    MatchStatusLogRow, NewMatchStatusLog, NewResultClaim, NewTournament, NewTournamentBracket,
    NewTournamentMatch, NewTournamentMatchGame, NewTournamentRegistration, NewTournamentStage,
    NewVetoAction, NewVetoSession, ResultClaimRow, TournamentBracketRow, TournamentMapPoolRow,
    TournamentMatchGameRow, TournamentMatchRow, TournamentRegistrationRow, TournamentRow,
    TournamentStageRow, TournamentStandingRow, UpdateResultClaim, UpdateVetoAction,
    UpdateVetoSession, VetoActionRow, VetoSessionRow,
};
pub use user::{NewUser, UpdateUser, UserRow, UserStatus};
pub use forfeit::{ForfeitRecordRow, NewForfeitRecord};
pub use dispute::{DisputeMessageRow, DisputeRow, NewDispute, NewDisputeMessage};
pub use result_review::{NewResultReview, ResultReviewRow};
pub use veto_delegate::{NewVetoDelegate, VetoDelegateRow};
pub use veto_lobby_message::{NewVetoLobbyMessage, VetoLobbyMessageRow};
