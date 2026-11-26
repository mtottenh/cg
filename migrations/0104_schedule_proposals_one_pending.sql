-- One pending schedule proposal per match.
--
-- "Is there already a pending proposal?" was a read followed by an insert,
-- so two captains proposing at the same moment both got a pending row. Only
-- the newest showed as active; once it was answered the older one resurfaced
-- and could still be accepted.

-- Keep the newest pending proposal of each match; withdraw the rest.
UPDATE schedule_proposals sp
SET status = 'cancelled', updated_at = NOW()
WHERE sp.status = 'pending'
  AND EXISTS (
      SELECT 1 FROM schedule_proposals newer
      WHERE newer.match_id = sp.match_id
        AND newer.status = 'pending'
        AND (newer.created_at, newer.id) > (sp.created_at, sp.id)
  );

CREATE UNIQUE INDEX uq_schedule_proposals_one_pending
    ON schedule_proposals (match_id)
    WHERE status = 'pending';
