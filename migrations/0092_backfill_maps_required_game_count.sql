-- Backfill maps_required to game-count semantics.
--
-- The bracket generators used to stamp `wins_required` into
-- `tournament_matches.maps_required` (2 for a bo3), while the only consumer —
-- the game-server maplist builder — has always treated the column as a map
-- COUNT and does `pool.take(maps_required)`. A non-veto bo3 therefore got a
-- two-map maplist. The generators now stamp `game_count()`, but that only
-- fixes rows written from here on: matches created before the upgrade and
-- not yet played would still hand their server a truncated maplist.
--
-- Only non-terminal rows are touched. Completed/cancelled/forfeit matches are
-- history — their maps_required is a record of what was configured at the
-- time, and rewriting it would silently restate finished results.
--
-- Idempotent: the WHERE clause only matches rows still holding the old value,
-- so a re-run is a no-op. Rows already correct (bo1, where wins_required and
-- game_count are both 1) are untouched by construction.
UPDATE tournament_matches
SET maps_required = CASE match_format
        WHEN 'bo1' THEN 1
        WHEN 'bo3' THEN 3
        WHEN 'bo5' THEN 5
        WHEN 'bo7' THEN 7
    END,
    updated_at = NOW()
WHERE status NOT IN ('completed', 'cancelled', 'forfeit')
  AND maps_required = CASE match_format
        WHEN 'bo1' THEN 1
        WHEN 'bo3' THEN 2
        WHEN 'bo5' THEN 3
        WHEN 'bo7' THEN 4
    END
  AND match_format <> 'bo1';
