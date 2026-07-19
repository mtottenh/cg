-- Deduplicate live saga executions per match.
--
-- Two concurrent claim confirms could both start a match_completion saga
-- for the same match; each run applies non-idempotent standings deltas
-- (points = points + N), double-counting round-robin/swiss standings.
-- The claim-confirm UPDATE now carries a status guard as the primary
-- defense; this index is the database-level backstop: at most one live
-- saga of a given type may exist per match.
--
-- Live means 'pending' (the status rows are created with) or 'running'.
-- 'paused' is deliberately NOT included: a saga paused for result review
-- stays 'paused' while continue_after_review starts a fresh execution for
-- the same match, which must not be blocked. Terminal and compensation
-- states never need to block a new run.
CREATE UNIQUE INDEX uq_saga_executions_live_per_match
ON saga_executions (match_id, saga_type)
WHERE match_id IS NOT NULL AND status IN ('pending', 'running');
