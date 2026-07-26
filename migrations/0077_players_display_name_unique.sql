-- Migration: Enforce unique player display names (case-insensitive)
-- Description:
--   Until now nothing stopped two players from registering the same
--   display name — `UserService::register_user` did no check and the
--   schema had none — yet `PlayerService::update_player` DID reject
--   duplicates. Net effect: you could sign up with a taken name and then
--   never save your profile again (409 on every PATCH /v1/players/me).
--   See COVERAGE-PLAN §9b P-5. Decision: display names are unique.
--
--   Case-insensitivity: the constraint is on the existing generated column
--   `display_name_normalized` (= lower(display_name)), which is also what
--   `PlayerRepository::find_by_display_name` matches on. A case-SENSITIVE
--   constraint would let "Bob" and "bob" coexist while the lookup treated
--   them as the same person — the two layers must agree, and impersonation
--   by case is exactly what this is meant to prevent.
--
-- EFFECT ON EXISTING DATA:
--   No rows are deleted. Any pre-existing case-insensitive collision is
--   resolved by RENAMING the newer rows: the oldest row (by created_at,
--   then id) keeps the name, every later row gets `_2`, `_3`, … appended,
--   truncated on the left so the result still fits VARCHAR(32), and
--   re-checked so the new name is itself free. Each rename is emitted as a
--   NOTICE so it can be recovered from the migration log.

DO $$
DECLARE
    dup       RECORD;
    base      TEXT;
    candidate TEXT;
    suffix    INT;
BEGIN
    FOR dup IN
        SELECT id, display_name
        FROM (
            SELECT id,
                   display_name,
                   ROW_NUMBER() OVER (
                       PARTITION BY display_name_normalized
                       ORDER BY created_at, id
                   ) AS rn
            FROM players
        ) ranked
        WHERE rn > 1
        ORDER BY id
    LOOP
        suffix := 1;
        LOOP
            suffix := suffix + 1;
            base := dup.display_name;
            IF length(base) + 1 + length(suffix::text) > 32 THEN
                base := left(base, 32 - 1 - length(suffix::text));
            END IF;
            candidate := base || '_' || suffix::text;
            EXIT WHEN NOT EXISTS (
                SELECT 1 FROM players WHERE display_name_normalized = lower(candidate)
            );
        END LOOP;

        UPDATE players SET display_name = candidate, updated_at = NOW() WHERE id = dup.id;

        RAISE NOTICE 'players: duplicate display_name % renamed to % (player %)',
            dup.display_name, candidate, dup.id;
    END LOOP;
END $$;

-- The plain lookup index is subsumed by the unique index on the same
-- expression column, so replace rather than stack them.
DROP INDEX IF EXISTS idx_players_display_name;

CREATE UNIQUE INDEX idx_players_display_name_unique
    ON players (display_name_normalized);
