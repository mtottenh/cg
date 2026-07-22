//! Integration tests for the Portal API.

mod common;

mod auth;
mod awards;
mod bans;
mod demo_evidence;
mod demos;
mod dispute;
mod enrichment_idempotency;
mod evidence;
mod forfeit;
mod games;
mod league_teams;
mod leagues;
mod lifecycle_automation;
mod lifecycle_races;
mod match_completion_saga;
mod partial_write_recovery;
mod player_game_profiles;
mod players;
mod progression;
mod result_review;
mod results;
mod roles;
mod saga_lifecycle;
mod standings_idempotency;
mod steam_auth;
mod steam_tracking;
mod tournaments;
mod users;
mod veto;
mod veto_delegates;
mod veto_ws;

mod evidence_s3;
mod scanner_e2e;
