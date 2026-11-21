-- Bookkeeping for the lifecycle's finished-stage sweep.
--
-- The sweep re-runs settlement for group / round-robin stages whose matches
-- are all settled but which are still open. A stage that can never advance
-- (too few eligible finishers for the playoffs, say) would otherwise be
-- retried on every pass forever. These columns bound that: the sweep counts
-- its attempts, stops after a few, and starts again only when one of the
-- stage's matches changes.
ALTER TABLE tournament_stages
    ADD COLUMN settlement_attempts INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN settlement_attempted_at TIMESTAMPTZ;

COMMENT ON COLUMN tournament_stages.settlement_attempts IS
    'Lifecycle sweep attempts to settle this finished stage since its matches last changed';
COMMENT ON COLUMN tournament_stages.settlement_attempted_at IS
    'When the lifecycle sweep last tried to settle this stage';
