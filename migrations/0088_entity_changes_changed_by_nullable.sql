-- P-150: entity_changes.changed_by was NOT NULL REFERENCES players(id)
-- ON DELETE SET NULL — a self-contradictory pair: deleting a player would
-- order the column set to a value it forbids, failing the delete.
--
-- Audit rows must outlive their actor, so the SET NULL semantics win and
-- the NOT NULL constraint goes. A NULL changed_by reads as "actor account
-- since deleted".
ALTER TABLE entity_changes
    ALTER COLUMN changed_by DROP NOT NULL;

COMMENT ON COLUMN entity_changes.changed_by IS 'Player who made the change; NULL when that player account has since been deleted';
