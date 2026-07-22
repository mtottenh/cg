-- Add looking_for_team column to players table
ALTER TABLE players ADD COLUMN looking_for_team BOOLEAN NOT NULL DEFAULT false;

-- Partial index for efficient LFT queries
CREATE INDEX idx_players_lft ON players(looking_for_team) WHERE looking_for_team = true;
