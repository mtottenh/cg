//! Test builders for creating entities.
//!
//! Builders provide a fluent API for creating test data with sensible defaults.
//!
//! ## New Builders (using repositories)
//!
//! These builders use repository methods from `portal-db` for consistency:
//!
//! - `GameBuilder` - Create test games
//! - `TournamentStageBuilder` - Create tournament stages
//! - `TournamentBracketBuilder` - Create tournament brackets
//! - `TournamentRegistrationBuilder` - Create tournament registrations
//! - `TournamentMatchBuilder` - Create tournament matches
//! - `VetoSessionBuilder` - Create veto sessions
//!
//! ## Example
//!
//! ```ignore
//! use portal_test::builders::*;
//!
//! // Create a complete tournament setup
//! let tournament = TournamentBuilder::new()
//!     .name("Test Tournament")
//!     .build_persisted(&pool).await;
//!
//! let stage = TournamentStageBuilder::new()
//!     .tournament_id_from_uuid(tournament.id)
//!     .single_elimination()
//!     .build_persisted(&pool).await;
//!
//! let bracket = TournamentBracketBuilder::new()
//!     .tournament_id_from_uuid(tournament.id)
//!     .stage_id(stage.id)
//!     .build_persisted(&pool).await;
//! ```

mod demo;
mod game;
mod league;
mod league_season;
mod league_season_participant;
mod league_team;
mod league_team_invitation;
mod league_team_member;
mod league_team_season;
mod player;
mod tournament;
mod tournament_bracket;
mod tournament_match;
mod tournament_registration;
mod tournament_stage;
mod user;
mod veto_delegate;
mod veto_session;

pub use demo::{DemoBuilder, DemoMatchLinkBuilder};
pub use game::GameBuilder;
pub use league::LeagueBuilder;
pub use league_season::LeagueSeasonBuilder;
pub use league_season_participant::LeagueSeasonParticipantBuilder;
pub use league_team::LeagueTeamBuilder;
pub use league_team_invitation::LeagueTeamInvitationBuilder;
pub use league_team_member::LeagueTeamMemberBuilder;
pub use league_team_season::LeagueTeamSeasonBuilder;
pub use player::PlayerBuilder;
pub use tournament::TournamentBuilder;
pub use tournament_bracket::TournamentBracketBuilder;
pub use tournament_match::TournamentMatchBuilder;
pub use tournament_registration::TournamentRegistrationBuilder;
pub use tournament_stage::TournamentStageBuilder;
pub use user::UserBuilder;
pub use veto_delegate::VetoDelegateBuilder;
pub use veto_session::{VetoSessionBuilder, DEFAULT_CS2_MAP_POOL};
