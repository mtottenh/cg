-- Migration: Player-game-profile match-scoped idempotency ledger
-- Description: One applied-record per (player, match) so a match's contribution
-- to a player's profile counters (matches_played, wins, losses, win_streak) is
-- applied exactly once.
--
-- `PgPlayerGameProfileRepository::update_stats_after_match` is ACCUMULATIVE
-- (`matches_played = matches_played + 1`, `win_streak = win_streak + 1`) and,
-- unlike standings, cannot be trivially recomputed: `win_streak` is
-- order-dependent and a profile aggregates across every tournament/league the
-- player has ever played, not one bracket. So a re-driven completion saga
-- (whose guard is a no-op on elimination brackets, where the standings step
-- records `{"action":"not_applicable"}`) re-applies the same match a second
-- time and corrupts the streak.
--
-- This ledger records that a given match's stats were applied to a given
-- player. The accumulate is wrapped in a transaction that first inserts the
-- ledger row `ON CONFLICT DO NOTHING`; the counter bump only runs when the
-- insert created a new row, so a replay is a no-op.

CREATE TABLE player_match_stats_applied (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    match_id UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT player_match_stats_applied_player_match_unique
        UNIQUE (player_id, match_id)
);

CREATE INDEX idx_player_match_stats_applied_match
    ON player_match_stats_applied (match_id);

COMMENT ON TABLE player_match_stats_applied
    IS 'Idempotency ledger: proves a match''s contribution to a player_game_profile has already been applied, so a saga re-drive does not double-count matches_played/wins/losses/win_streak.';
