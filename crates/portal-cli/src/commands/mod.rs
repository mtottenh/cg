//! CLI command modules.

pub mod api_key;
pub mod audit;
pub mod ban;
pub mod bootstrap;
pub mod db;
pub mod demo;
pub mod game;
pub mod league_team;
pub mod player;
pub mod role;
pub mod seed;
pub mod user;

#[cfg(feature = "scanner")]
pub mod scan;

// TODO: Add these command modules as they are implemented:
// pub mod tournament;
