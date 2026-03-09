-- Fix tournament status CHECK constraint to match Rust enum values.
-- Old constraint had 'check_in' and 'seeding' which don't exist in the enum.
-- Rust enum uses 'scheduled' and 'finalized' instead.

ALTER TABLE tournaments
    DROP CONSTRAINT tournaments_check_status;

ALTER TABLE tournaments
    ADD CONSTRAINT tournaments_check_status CHECK (status IN (
        'draft', 'published', 'registration', 'scheduled',
        'in_progress', 'completed', 'finalized', 'cancelled'
    ));
