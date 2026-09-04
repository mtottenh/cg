//! Integration tests for the Portal API.

mod common;

mod archiving;
mod auth;
mod awards;
mod bans;
mod demo_evidence;
mod demos;
mod dispute;
mod enrichment_idempotency;
mod enrichment_retry;
mod evidence;
mod forfeit;
mod game_server_flow;
mod game_servers;
mod games;
mod league_teams;
mod leagues;
mod lifecycle_automation;
mod lifecycle_races;
mod match_completion_saga;
mod match_participants;
mod my_matches;
mod partial_write_recovery;
mod player_game_profiles;
mod players;
mod poll_backoff;
mod progression;
mod pugs;
mod registration_identity;
mod result_review;
mod results;
mod roles;
mod saga_lifecycle;
mod standings_idempotency;
mod steam_auth;
mod steam_tracking;
mod team_result_authority;
mod tournaments;
mod users;
mod veto;
mod veto_delegates;
mod veto_ws;

mod evidence_s3;
mod scanner_e2e;
