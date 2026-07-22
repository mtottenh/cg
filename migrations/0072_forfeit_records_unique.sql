-- Migration: Forfeit record idempotency
-- Description: One forfeit record per (match, forfeiting registration).
--
-- `process_forfeit` / `process_double_forfeit` write the forfeit record and
-- then update the match in separate statements. Without a unique key the only
-- protection against a retry after a partial write was an application-level
-- `exists_for_match` read, which refused the retry and stranded the match.
-- With this constraint the insert can be made idempotent (ON CONFLICT DO
-- NOTHING) so the retry can proceed to the match update.

-- Collapse any pre-existing duplicates first (keep the earliest record for
-- each pair) so the constraint can be added against a database with data.
DELETE FROM forfeit_records a
USING forfeit_records b
WHERE a.match_id = b.match_id
  AND a.forfeiting_registration_id = b.forfeiting_registration_id
  AND (a.forfeited_at, a.id) > (b.forfeited_at, b.id);

ALTER TABLE forfeit_records
    ADD CONSTRAINT forfeit_records_match_registration_unique
    UNIQUE (match_id, forfeiting_registration_id);

COMMENT ON CONSTRAINT forfeit_records_match_registration_unique ON forfeit_records
    IS 'Makes forfeit record creation idempotent so a retry after a partial write can recover the match';
