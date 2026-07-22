-- Fix type mismatch: DECIMAL columns are not compatible with Rust f64.
-- These score columns don't need exact decimal precision, so use DOUBLE PRECISION.
ALTER TABLE tournament_standings ALTER COLUMN buchholz_score TYPE DOUBLE PRECISION;
ALTER TABLE tournament_standings ALTER COLUMN opponent_match_wins TYPE DOUBLE PRECISION;
ALTER TABLE tournament_standings ALTER COLUMN tiebreaker_score TYPE DOUBLE PRECISION;
