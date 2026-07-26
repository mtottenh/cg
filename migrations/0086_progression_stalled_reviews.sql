-- P-180: a match-completion saga that permanently fails (retries exhausted,
-- or retired because its standings deltas already applied) used to vanish
-- into logs — the match sat completed with progression possibly half-done
-- and no operator surface said so. Such give-ups now raise a result review
-- flagged with this column, so the stall lands in the admin review queue.
ALTER TABLE result_reviews
    ADD COLUMN progression_stalled BOOLEAN NOT NULL DEFAULT false;
