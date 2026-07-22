-- Migration: Match-scoped idempotency key for demo-derived rating history.
-- Description: `process_demo_ratings` (handlers/internal.rs) appends to
-- player_rating_history with a bare INSERT. Re-delivering the same enriched
-- match (a retry / at-least-once delivery) therefore writes a second, identical
-- rating row, polluting data_points / AVG / median in get_rating_stats.
--
-- The intended grain is one rating-history entry per player per enriched match
-- per source. This adds a nullable discovered_match_id column (NULL for rows
-- from other sources, e.g. admin submissions or periodic steam_bot polling,
-- which have no originating discovered match) plus a UNIQUE constraint on
-- (player_id, discovered_match_id, source). NULL discovered_match_id values are
-- treated as distinct by the unique index, so non-demo rows are never
-- constrained; demo rows dedupe on re-delivery via ON CONFLICT DO NOTHING.

ALTER TABLE player_rating_history
    ADD COLUMN discovered_match_id UUID
        REFERENCES discovered_matches(id) ON DELETE SET NULL;

-- NULLS DISTINCT (PostgreSQL 15+ default): rows with NULL discovered_match_id
-- never collide, so non-demo sources are unconstrained. Demo rows carry a
-- concrete id and dedupe on re-delivery.
CREATE UNIQUE INDEX player_rating_history_match_source_unique
    ON player_rating_history (player_id, discovered_match_id, source);

COMMENT ON COLUMN player_rating_history.discovered_match_id
    IS 'Originating discovered match for demo-derived ratings; NULL for other sources. Deduped with (player_id, source) so a re-delivered enrichment does not duplicate the rating entry.';
