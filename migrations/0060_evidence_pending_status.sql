-- Add 'pending' to the evidence status CHECK constraint.
-- Evidence is now created as 'pending' during upload initiation,
-- then transitioned to 'active' when the upload is confirmed.

ALTER TABLE match_evidence
    DROP CONSTRAINT match_evidence_check_status;

ALTER TABLE match_evidence
    ADD CONSTRAINT match_evidence_check_status CHECK (status IN (
        'pending', 'active', 'expired', 'deleted', 'quarantined'
    ));
