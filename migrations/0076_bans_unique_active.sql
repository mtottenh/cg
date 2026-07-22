-- Migration: Partial unique indexes to prevent duplicate active bans.
--
-- A user must never accumulate more than one active (not-yet-lifted) ban of the
-- same type + scope. The service already read-guards this, but the read+insert
-- is a TOCTOU: concurrent identical ban creations both pass the guard and both
-- INSERT. These partial unique indexes make the second (and later) racing INSERT
-- fail at the database, which the adapter surfaces as a Conflict.
--
-- Grain matches `ban_repo.get_active_for_user`'s notion of "active" as far as a
-- partial-index predicate can express it (immutable predicate: NOW()-based
-- expiry cannot be referenced, so uniqueness keys on `lifted_at IS NULL`).
-- Two variants because scoped bans key on (scope_type, scope_id) while
-- platform/global bans have both NULL:
--   * unscoped: one active ban per (user_id, ban_type)
--   * scoped:   one active ban per (user_id, ban_type, scope_type, scope_id)
-- Different ban_types for the same user, and same-type bans across different
-- users or different scopes, remain allowed (existing tests rely on this).

-- Dedupe any pre-existing duplicates so the unique indexes can be created
-- against a database that already holds data. Keep the earliest active ban in
-- each group and lift the rest (preserves history rather than deleting rows).
WITH ranked_unscoped AS (
    SELECT id,
           ROW_NUMBER() OVER (
               PARTITION BY user_id, ban_type
               ORDER BY starts_at, created_at, id
           ) AS rn
    FROM bans
    WHERE lifted_at IS NULL
      AND scope_type IS NULL
)
UPDATE bans
SET lifted_at = NOW(),
    lift_reason = COALESCE(lift_reason, 'Superseded duplicate active ban (migration 0076)'),
    updated_at = NOW()
FROM ranked_unscoped
WHERE bans.id = ranked_unscoped.id
  AND ranked_unscoped.rn > 1;

WITH ranked_scoped AS (
    SELECT id,
           ROW_NUMBER() OVER (
               PARTITION BY user_id, ban_type, scope_type, scope_id
               ORDER BY starts_at, created_at, id
           ) AS rn
    FROM bans
    WHERE lifted_at IS NULL
      AND scope_type IS NOT NULL
)
UPDATE bans
SET lifted_at = NOW(),
    lift_reason = COALESCE(lift_reason, 'Superseded duplicate active ban (migration 0076)'),
    updated_at = NOW()
FROM ranked_scoped
WHERE bans.id = ranked_scoped.id
  AND ranked_scoped.rn > 1;

CREATE UNIQUE INDEX bans_unique_active_unscoped
    ON bans (user_id, ban_type)
    WHERE lifted_at IS NULL AND scope_type IS NULL;

CREATE UNIQUE INDEX bans_unique_active_scoped
    ON bans (user_id, ban_type, scope_type, scope_id)
    WHERE lifted_at IS NULL AND scope_type IS NOT NULL;
