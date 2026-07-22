-- Migration: 0070_backfill_tournament_map_pools.sql
-- Description: Every tournament must own an explicit map pool.
--
-- Tournament creation now requires a map pool, and result-submission map
-- validation fails closed (a tournament with no resolvable pool rejects
-- submitted maps). Historical and in-flight tournaments predate that rule and
-- may have no `tournament_map_pools` row, so backfill them with their
-- effective pool: the game's `default_map_pool`, falling back to the IDs in
-- the game's `available_maps` catalog.

INSERT INTO tournament_map_pools (id, tournament_id, stage_id, maps, veto_format_id)
SELECT
    gen_random_uuid(),
    t.id,
    NULL,
    COALESCE(
        -- Preferred: the game's configured default pool.
        (
            SELECT array_agg(value #>> '{}')
            FROM jsonb_array_elements(g.default_map_pool) AS value
            WHERE jsonb_typeof(g.default_map_pool) = 'array'
        ),
        -- Fallback: every map ID in the game's catalog.
        (
            SELECT array_agg(entry ->> 'id')
            FROM jsonb_array_elements(g.available_maps) AS entry
            WHERE jsonb_typeof(g.available_maps) = 'array'
              AND entry ->> 'id' IS NOT NULL
        )
    ),
    t.default_map_veto_format
FROM tournaments t
JOIN games g ON g.id = t.game_id
WHERE NOT EXISTS (
    SELECT 1
    FROM tournament_map_pools p
    WHERE p.tournament_id = t.id
      AND p.stage_id IS NULL
)
-- Skip games with no usable map data; there is nothing to backfill from.
AND (
    jsonb_typeof(g.default_map_pool) = 'array' AND jsonb_array_length(g.default_map_pool) > 0
    OR jsonb_typeof(g.available_maps) = 'array' AND jsonb_array_length(g.available_maps) > 0
);
