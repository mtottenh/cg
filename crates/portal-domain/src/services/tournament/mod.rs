//! Tournament services.
//!
//! This module contains the business logic for tournament operations:
//!
//! - `TournamentService`: Core tournament management
//! - `BracketGenerator`: Bracket generation for various formats

mod bracket_generator;
mod service;

pub use bracket_generator::{BracketGenerator, GeneratedBracket};
pub use service::TournamentService;
