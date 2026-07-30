-- Migration: retryable enrichment — exponential backoff, claim leases, and a
-- first-class demo-extraction stage.
--
-- Three defects in the discovered-match pipeline, all of which lose data
-- permanently and all of which share a root cause (retry state that is either
-- absent or not durable):
--
--   1. NO BACKOFF. `find_pending` re-offered a failed match on the very next
--      enricher cycle. With max_retries = 3 and a 30s cycle, a transient GC or
--      Valve outage burned the entire retry budget in ~90 seconds, after which
--      the match was excluded from `find_pending` forever.
--
--   2. DEMO EXTRACTION HAD NO RETRY AT ALL. The enricher downloaded the demo
--      inline; on any failure it logged a warning and submitted the match as
--      `enriched` with no player ratings and no map name. Valve does not
--      publish a demo the instant a match ends, so "not available yet" — the
--      single most likely failure — permanently discarded that match's rank
--      data. This is the observed production failure.
--
--   3. STRANDED CLAIMS. `claim` moves a row to `enriching`, but `find_pending`
--      only ever selected `pending`/`failed` and nothing reset a stale claim.
--      A worker that died between claiming and reporting (GC stream close,
--      portal 5xx on submit, cycle deadline, SIGKILL) left the row in
--      `enriching` forever — invisible to the queue AND uncounted by
--      `count_retry_exhausted`, so the operator view showed nothing wrong.
--
-- The fix gives every retried stage the same three things: a durable attempt
-- counter, a scheduled next-attempt time, and a lease so a dead worker's job
-- returns to the queue exactly once.

-- =============================================================================
-- 1. Enrichment stage: backoff schedule + claim lease
-- =============================================================================

ALTER TABLE discovered_matches
    ADD COLUMN next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ADD COLUMN claimed_at      TIMESTAMPTZ,
    ADD COLUMN last_attempt_at TIMESTAMPTZ;

COMMENT ON COLUMN discovered_matches.next_attempt_at
    IS 'Earliest time the enricher may attempt this match again. Set by mark_failed to an exponentially backed-off, jittered offset; NOW() for a freshly discovered match so the first attempt is immediate.';
COMMENT ON COLUMN discovered_matches.claimed_at
    IS 'When the current enricher claimed this row. A claim older than the lease is reclaimed by find_pending, which costs one retry so a match that reliably kills its worker cannot loop forever.';

-- Three attempts inside 90 seconds is not a retry budget, it is a burst. With
-- backoff the same 6 attempts span roughly an hour, which is the timescale GC
-- and Valve CDN outages actually resolve on.
ALTER TABLE discovered_matches ALTER COLUMN max_retries SET DEFAULT 6;

-- =============================================================================
-- 2. Demo extraction as its own retried stage
-- =============================================================================
--
-- Deliberately separate from `status`: GC enrichment and demo extraction fail
-- independently, on different timescales, against different services. Folding
-- a demo retry back into `status = 'failed'` would re-issue the rate-limited GC
-- call for data we already hold.

CREATE TYPE demo_extraction_status AS ENUM (
    -- Awaiting a first or subsequent attempt.
    'pending',
    -- Parsed, rank updates extracted.
    'succeeded',
    -- Parsed cleanly but carried no rank updates. Casual/deathmatch demos are
    -- legitimately empty; this is a success, not a failure to retry.
    'empty',
    -- Never published, or past Valve's retention window. Terminal.
    'unavailable',
    -- Downloaded but could not be decompressed or parsed. Terminal.
    'failed',
    -- GC returned no demo URL for this match; there is nothing to fetch.
    'not_applicable'
);

ALTER TABLE discovered_matches
    ADD COLUMN demo_status          demo_extraction_status NOT NULL DEFAULT 'pending',
    ADD COLUMN demo_retry_count     INT NOT NULL DEFAULT 0,
    ADD COLUMN demo_max_retries     INT NOT NULL DEFAULT 8,
    ADD COLUMN demo_next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ADD COLUMN demo_last_attempt_at TIMESTAMPTZ,
    ADD COLUMN demo_error           TEXT;

COMMENT ON COLUMN discovered_matches.demo_retry_count
    IS 'Attempts made, incremented when the job is LEASED rather than when it fails. A worker that dies mid-parse has still spent an attempt, so a demo that reliably crashes the parser cannot be retried forever — and the count survives a worker restart, unlike an in-process counter.';
COMMENT ON COLUMN discovered_matches.demo_next_attempt_at
    IS 'Doubles as the lease expiry while a job is in flight: leasing pushes this out by the lease duration, so a crashed worker''s job becomes eligible again only after the lease, and never concurrently.';

-- =============================================================================
-- 3. Work-queue indexes
-- =============================================================================

CREATE INDEX idx_discovered_matches_enrich_queue
    ON discovered_matches (game_id, next_attempt_at)
    WHERE status IN ('pending', 'failed');

CREATE INDEX idx_discovered_matches_demo_queue
    ON discovered_matches (game_id, demo_next_attempt_at)
    WHERE demo_status = 'pending';

CREATE INDEX idx_discovered_matches_claim_lease
    ON discovered_matches (claimed_at)
    WHERE status = 'enriching';

-- =============================================================================
-- 4. Backfill
-- =============================================================================

-- 4a. Rows stranded in 'enriching' by defect 3. Return them to the queue with
-- their retry budget intact — they were never fairly attempted.
UPDATE discovered_matches
   SET status          = 'failed',
       error           = COALESCE(error, 'stranded in enriching before claim leases existed'),
       claimed_at      = NULL,
       next_attempt_at = NOW() + (random() * 1800) * INTERVAL '1 second'
 WHERE status = 'enriching';

-- 4b. Rows parked by defect 1: budget spent in a burst, never retried since.
-- Raise them to the new ceiling and stagger re-entry over 30 minutes so the
-- backlog does not arrive as one thundering herd against the GC.
UPDATE discovered_matches
   SET max_retries     = 6,
       next_attempt_at = NOW() + (random() * 1800) * INTERVAL '1 second'
 WHERE status = 'failed'
   AND retry_count >= max_retries;

-- 4c. Demo stage for rows that predate it. No demo URL means nothing to fetch.
UPDATE discovered_matches
   SET demo_status = 'not_applicable'
 WHERE status = 'enriched'
   AND demo_url IS NULL;

-- 4d. A demo-derived rating row is proof the demo was fetched and parsed.
UPDATE discovered_matches m
   SET demo_status = 'succeeded'
 WHERE m.status = 'enriched'
   AND m.demo_url IS NOT NULL
   AND EXISTS (
       SELECT 1 FROM player_rating_history h
        WHERE h.discovered_match_id = m.id
          AND h.source = 'demo_rank_update'
   );

-- 4e. Valve retains matchmaking demos for about two weeks and the CDN URL dies
-- with them. Anything older than that is unrecoverable, so mark it terminal
-- rather than spending eight attempts proving the 404.
UPDATE discovered_matches
   SET demo_status = 'unavailable',
       demo_error  = 'demo retention window elapsed before the retry stage existed'
 WHERE status = 'enriched'
   AND demo_url IS NOT NULL
   AND demo_status = 'pending'
   AND COALESCE(enriched_at, discovered_at) < NOW() - INTERVAL '14 days';

-- 4f. What remains is genuinely recoverable: matches enriched inside the
-- retention window whose demo was dropped by defect 2. These are the ones the
-- fix exists to recover. Stagger them too.
UPDATE discovered_matches
   SET demo_next_attempt_at = NOW() + (random() * 1800) * INTERVAL '1 second'
 WHERE status = 'enriched'
   AND demo_url IS NOT NULL
   AND demo_status = 'pending';
