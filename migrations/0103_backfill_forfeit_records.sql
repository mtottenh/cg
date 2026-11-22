-- Migration: Backfill forfeit records for captain forfeits
-- Description: Every forfeited match carries an active forfeit record.
--
-- A captain's own forfeit (POST /v1/tournaments/{id}/matches/{id}/forfeit)
-- used to write the match but no forfeit record. Every forfeit path now
-- writes one, and a forfeited match *without* one is taken to be a forfeit
-- interrupted before settlement, which a retry finishes and settles. Left as
-- they were, the old captain forfeits would look interrupted and an admin
-- forfeit for the same side would "finish" - and settle - them again. Give
-- them the record they would have been written with today.
INSERT INTO forfeit_records (
    match_id, forfeiting_registration_id, forfeit_type, reason,
    triggered_by_user_id, triggered_by_system, forfeited_at
)
SELECT m.id, m.loser_registration_id, 'withdrawal',
       'Conceded by the participant (record backfilled by migration 0103)',
       NULL, FALSE, COALESCE(m.completed_at, m.updated_at)
FROM tournament_matches m
WHERE m.status = 'forfeit'
  AND m.loser_registration_id IS NOT NULL
  AND NOT EXISTS (
      SELECT 1 FROM forfeit_records f
      WHERE f.match_id = m.id AND f.superseded_at IS NULL
  );
