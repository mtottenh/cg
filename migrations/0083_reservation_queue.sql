-- Migration: reservation queue model
-- Design: docs/matchzy-integration.md §6.6 — a reservation is created at
-- veto completion even when no server is free ("waiting for a free server,
-- position N"); the lifecycle pass allocates one later. That requires
-- `server_id` to be nullable while status = 'pending'.
--
-- The one-live-reservation-per-server invariant is unaffected: partial
-- unique indexes ignore NULLs.

ALTER TABLE server_reservations
    ALTER COLUMN server_id DROP NOT NULL;

ALTER TABLE server_reservations
    ADD CONSTRAINT server_reservations_check_server_when_active CHECK (
        server_id IS NOT NULL OR status IN ('pending', 'failed', 'cancelled')
    );
