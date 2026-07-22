-- =============================================================================
-- 0071: Persist Swiss byes so standings can be DERIVED, not accumulated.
-- =============================================================================
--
-- `tournament_standings` used to be maintained with accumulative deltas
-- (`points = points + $8`). Every path that could fire twice for the same
-- match therefore double-counted, and revert drove the columns negative.
--
-- Standings are now recomputed from their source of truth — the completed
-- rows of `tournament_matches` — which is idempotent by construction.
--
-- A Swiss bye, however, has never been represented as a match row: the
-- bracket generator returns a `bye_participant` and the service awarded
-- +3 points directly. A recompute driven only by match rows would silently
-- erase those points, so byes get their own (idempotent, uniquely keyed)
-- table and are folded into the recompute.

CREATE TABLE tournament_bracket_byes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    bracket_id UUID NOT NULL REFERENCES tournament_brackets(id) ON DELETE CASCADE,
    registration_id UUID NOT NULL REFERENCES tournament_registrations(id) ON DELETE CASCADE,

    -- The Swiss round the bye was awarded in. Part of the uniqueness key so
    -- re-running round generation cannot award the same bye twice.
    round INTEGER NOT NULL,

    -- Points awarded for the bye. Kept as a column so the historical value
    -- survives a future change to the win-points constant.
    points INTEGER NOT NULL DEFAULT 3,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT tournament_bracket_byes_unique UNIQUE (bracket_id, registration_id, round)
);

CREATE INDEX idx_tournament_bracket_byes_bracket ON tournament_bracket_byes(bracket_id);

COMMENT ON TABLE tournament_bracket_byes IS
    'Swiss byes awarded to a participant; folded into the derived standings recompute';

-- -----------------------------------------------------------------------------
-- Backfill: reconstruct historical byes.
--
-- Under the old accumulative model a bye incremented `matches_played` without
-- producing a match row, so for every standing the number of byes it was
-- awarded is exactly:
--
--     matches_played - (completed match rows this registration appears in)
--
-- Without this backfill the first recompute of a live Swiss bracket would drop
-- those points. `generate_series` gives each reconstructed bye a distinct
-- synthetic round so the uniqueness key holds.
-- -----------------------------------------------------------------------------
INSERT INTO tournament_bracket_byes (bracket_id, registration_id, round)
SELECT p.bracket_id, p.registration_id, g
FROM (
    SELECT
        s.bracket_id,
        s.registration_id,
        s.matches_played,
        (
            SELECT COUNT(*)
            FROM tournament_matches m
            WHERE m.bracket_id = s.bracket_id
              AND m.status = 'completed'
              AND m.winner_registration_id IS NOT NULL
              AND m.loser_registration_id IS NOT NULL
              AND (
                  m.winner_registration_id = s.registration_id
                  OR m.loser_registration_id = s.registration_id
              )
        ) AS real_matches
    FROM tournament_standings s
) p
CROSS JOIN LATERAL generate_series(1, p.matches_played - p.real_matches) AS g
WHERE p.matches_played > p.real_matches
ON CONFLICT DO NOTHING;
