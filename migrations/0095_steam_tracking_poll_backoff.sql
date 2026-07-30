-- Migration: replace the poller's error cliff with backoff + recoverable pauses.
--
-- `cs2-poller` skipped any tracking entry with `poll_errors >= 10`. That is not
-- a backoff, it is a one-way door:
--
--   * There was no delay between attempts, so ten consecutive failures took ten
--     minutes of ordinary polling to accumulate. Any Steam-side blip longer
--     than that consumed the whole allowance.
--   * Once skipped, the entry was never polled again. `poll_errors` only resets
--     on a SUCCESSFUL poll, and a skipped entry never gets one — so the counter
--     could not come down by any means the system had.
--   * Nothing could reverse it. `update_auth_code` did not clear `poll_errors`,
--     so the single user action that fixes the most common cause (a revoked
--     match-sharing code) left the entry just as dead. There was no admin
--     endpoint either. Recovery meant a manual UPDATE against production.
--   * Every failure counted the same. A 429 from Steam — a global condition,
--     nothing to do with this player — burned the same budget as a revoked
--     token, so rate limiting alone could permanently kill every entry.
--
-- The replacement separates the two questions the old counter conflated:
-- "should we wait longer before retrying?" and "is retrying pointless?".
--
--   * TRANSIENT failures (network, 5xx, timeouts) back off exponentially and
--     retry FOREVER. There is no attempt ceiling, because there should not be
--     one: a tracking entry is a standing subscription, not a unit of work, and
--     one request every six hours costs nothing. Permanently abandoning it is
--     strictly worse than checking occasionally.
--   * PERMANENT failures (revoked auth code, invalid cursor) pause the entry on
--     the FIRST occurrence rather than the tenth — retrying a 403 nine more
--     times tells nobody anything — and record which human action unsticks it.
--   * RATE LIMITING is neither: it backs the entry off without counting against
--     it at all.

-- =============================================================================
-- 1. Poll scheduling and state
-- =============================================================================

CREATE TYPE steam_tracking_poll_state AS ENUM (
    -- Healthy, polling on the normal cadence.
    'ok',
    -- Failing transiently; retrying on an exponential schedule, indefinitely.
    'backoff',
    -- Steam rejected the match-sharing auth code (403). Only the player can fix
    -- this, by supplying a new code — which clears the pause automatically.
    'auth_expired',
    -- Steam rejected the stored share-code cursor (412). Needs the cursor
    -- reset, by the player re-supplying a recent share code or an admin resume.
    'cursor_invalid'
);

ALTER TABLE steam_tracking
    ADD COLUMN next_poll_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ADD COLUMN poll_state    steam_tracking_poll_state NOT NULL DEFAULT 'ok',
    ADD COLUMN paused_at     TIMESTAMPTZ;

COMMENT ON COLUMN steam_tracking.next_poll_at
    IS 'Earliest time the poller may try this entry again. NOW() when healthy; pushed out by exponential backoff after a transient failure.';
COMMENT ON COLUMN steam_tracking.poll_state
    IS 'Why the poller is or is not working this entry. Paused states (auth_expired, cursor_invalid) each name a specific human action; they are set on the first occurrence because retrying them cannot succeed.';
COMMENT ON COLUMN steam_tracking.poll_errors
    IS 'Consecutive TRANSIENT failures; drives the backoff exponent and resets on any successful poll. No longer a ceiling — it does not stop the entry being polled, and rate limiting does not increment it.';

-- The poller's work list: active, not paused, and due.
CREATE INDEX idx_steam_tracking_due
    ON steam_tracking (game_id, next_poll_at)
    WHERE is_active AND poll_state IN ('ok', 'backoff');

-- =============================================================================
-- 2. Backfill
-- =============================================================================

-- Entries parked by the old cliff. Their errors were never classified, so we
-- cannot know which were transient and which were revoked tokens — and the
-- honest default is to try again, because a wrongly-resumed entry costs one
-- request and re-pauses itself with a real reason within a cycle.
--
-- Staggered over an hour so a large parked backlog does not arrive at Steam as
-- one burst against a 1 req/s budget.
UPDATE steam_tracking
   SET poll_state   = 'backoff',
       next_poll_at = NOW() + (random() * 3600) * INTERVAL '1 second',
       last_error   = COALESCE(
           last_error,
           'parked by the pre-backoff error cliff; resumed by migration 0095'
       )
 WHERE is_active
   AND poll_errors >= 10;

-- Everything else keeps polling immediately; `poll_errors` below the old cliff
-- was never load-bearing.
UPDATE steam_tracking
   SET poll_state = 'ok'
 WHERE is_active
   AND poll_errors < 10;
