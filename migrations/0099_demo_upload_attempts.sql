-- Migration: record every MatchZy demo upload attempt
-- (docs/demo-ingestion-admin-design.md §4.1).
--
-- MatchZy never retries a demo upload and reports nothing on its side, so a
-- refused or failed upload used to exist only as a log line. One row per
-- request to POST /v1/gameserver/demos — success and every refusal — gives
-- the Pipeline page's "Server uploads" stage something to count, and lets an
-- operator tell "late" from "failed" from "never sent".
--
-- Not `server_events`: that is MatchZy's webhook vocabulary with a dedupe
-- index keyed on reservation, and an upload can arrive with no reservation
-- or no known server at all (a bad token). The request body is never stored.
-- The lifecycle pass prunes rows older than 90 days.

CREATE TABLE demo_upload_attempts (
    id              UUID PRIMARY KEY,
    received_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- NULL when the token was missing or unknown.
    server_id       UUID REFERENCES game_servers(id) ON DELETE SET NULL,
    outcome         VARCHAR(24) NOT NULL CHECK (outcome IN (
                        'stored', 'rejected_auth', 'rejected_request',
                        'store_failed', 'catalog_failed')),
    http_status     SMALLINT NOT NULL,
    -- Sanitised; NULL when the MatchZy-FileName header was missing.
    file_name       VARCHAR(255),
    matchzy_id      BIGINT,
    map_number      INTEGER,
    bytes           BIGINT NOT NULL,
    duration_ms     INTEGER NOT NULL,
    demo_id         UUID REFERENCES demos(id) ON DELETE SET NULL,
    linked_match_id UUID REFERENCES tournament_matches(id) ON DELETE SET NULL,
    -- linked | no_match_id | no_reservation | other_server | link_failed
    -- (stored outcomes only).
    link_note       VARCHAR(64),
    error           TEXT
);

CREATE INDEX idx_demo_upload_attempts_received ON demo_upload_attempts(received_at DESC);
CREATE INDEX idx_demo_upload_attempts_server ON demo_upload_attempts(server_id, received_at DESC);
CREATE INDEX idx_demo_upload_attempts_demo ON demo_upload_attempts(demo_id);

COMMENT ON TABLE demo_upload_attempts IS 'One row per MatchZy demo upload request, whatever its outcome. Pruned after 90 days.';
