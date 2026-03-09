-- Add rank_type_id to player_rating_history so we can filter
-- Premier (11) vs Competitive (6) vs Wingman (7).

ALTER TABLE player_rating_history
    ADD COLUMN rank_type_id INT NOT NULL DEFAULT 11;

CREATE INDEX idx_player_rating_history_rank_type
    ON player_rating_history (player_id, game_id, rank_type_id, recorded_at DESC);
