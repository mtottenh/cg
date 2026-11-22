-- Migration: Superseded forfeit records
-- Description: A reverted forfeit keeps its record for audit, marked superseded.
--
-- Reverting a forfeit (progression/revert) reopens the match so it can be
-- played or forfeited for the other side. The record of the forfeit that was
-- taken back is kept, stamped `superseded_at`, and no longer counts as the
-- match's forfeit: only records with `superseded_at IS NULL` are active.
--
-- The one-record-per-(match, forfeiting registration) key from migration 0072
-- applies to active records only, so the same side can forfeit a reopened
-- match again without colliding with its superseded record.
ALTER TABLE forfeit_records
    ADD COLUMN superseded_at TIMESTAMPTZ;

COMMENT ON COLUMN forfeit_records.superseded_at IS
    'When the forfeit was reverted (progression/revert); NULL while it stands';

ALTER TABLE forfeit_records
    DROP CONSTRAINT forfeit_records_match_registration_unique;

CREATE UNIQUE INDEX forfeit_records_active_match_registration_unique
    ON forfeit_records (match_id, forfeiting_registration_id)
    WHERE superseded_at IS NULL;
