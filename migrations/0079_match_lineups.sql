-- Migration: Match Lineups (two-phase: provisional declared, authoritative from demo)
-- Design: docs/lineup-design.md §0a (authoritative schema), §0b (two-phase model)
--
-- Splits the two questions league_team_members conflates:
--   "who is eligible?"  (the roster — unchanged)
--   "who actually played?" (a lineup — new, this migration)
--
-- A lineup is declared per-match per-registration. Its players carry a `source`:
--   'declared' — the PROVISIONAL lineup a captain entered at pick/ban (a promise,
--                match-level, game_number NULL).
--   'demo'     — the AUTHORITATIVE lineup derived from the map demo (per-map,
--                game_number set). This is what counts for stats, awards, and
--                eligibility enforcement.
--   'evidence' — from a submitted screenshot/artefact, entered by an admin (ladder).
--   'admin'    — manual entry, last resort (ladder).

CREATE TABLE match_lineups (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    match_id          UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,
    registration_id   UUID NOT NULL REFERENCES tournament_registrations(id) ON DELETE CASCADE,

    -- draft | submitted | locked (Q2). locked_at stamped on PickBan/InProgress.
    status            VARCHAR(16) NOT NULL DEFAULT 'draft',

    declared_by       UUID REFERENCES users(id),
    declared_at       TIMESTAMPTZ,
    locked_at         TIMESTAMPTZ,
    short_handed      BOOLEAN NOT NULL DEFAULT false,
    notes             TEXT,

    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE (match_id, registration_id),

    CONSTRAINT match_lineups_check_status CHECK (
        status IN ('draft', 'submitted', 'locked')
    )
);

CREATE INDEX idx_match_lineups_match ON match_lineups(match_id);
CREATE INDEX idx_match_lineups_registration ON match_lineups(registration_id);

CREATE TABLE match_lineup_players (
    id                    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    lineup_id             UUID NOT NULL REFERENCES match_lineups(id) ON DELETE CASCADE,
    player_id             UUID NOT NULL REFERENCES players(id),

    -- Provenance (§0a). See table comment above.
    source                VARCHAR(16) NOT NULL,

    -- Per-map. NULL for 'declared' rows; set for 'demo' rows from
    -- demo_match_links.game_number. Aligns with per-map demo attribution (P-25).
    game_number           INTEGER,

    -- For 'demo' rows: auto-set true when the player is not on the team roster.
    is_substitute         BOOLEAN NOT NULL DEFAULT false,

    -- Snapshot at ingestion: roster membership at time T is not reconstructable.
    was_rostered          BOOLEAN NOT NULL,

    -- confirmed | no_show | left_early | substituted | removed
    participation_status  VARCHAR(32) NOT NULL DEFAULT 'confirmed',

    created_at            TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE (lineup_id, player_id, game_number, source),

    CONSTRAINT match_lineup_players_check_source CHECK (
        source IN ('declared', 'demo', 'evidence', 'admin')
    ),
    CONSTRAINT match_lineup_players_check_participation CHECK (
        participation_status IN ('confirmed', 'no_show', 'left_early', 'substituted', 'removed')
    )
);

CREATE INDEX idx_match_lineup_players_lineup ON match_lineup_players(lineup_id);
CREATE INDEX idx_match_lineup_players_player ON match_lineup_players(player_id);
-- Supports the substitute-appearance / eligibility counting queries (§6).
CREATE INDEX idx_match_lineup_players_sub ON match_lineup_players(player_id)
    WHERE is_substitute = true;

COMMENT ON TABLE match_lineups IS 'Who played a match, per registration. Provisional (declared) + authoritative (demo).';
COMMENT ON COLUMN match_lineup_players.source IS 'declared (provisional, a promise) | demo (authoritative) | evidence | admin';
COMMENT ON COLUMN match_lineup_players.game_number IS 'Per-map for demo rows; NULL for match-level declared rows';
COMMENT ON COLUMN match_lineup_players.is_substitute IS 'Auto-set for demo rows when the player is not on the roster';
COMMENT ON COLUMN match_lineup_players.was_rostered IS 'Snapshot of roster membership at ingestion — not reconstructable later';

-- §9 step 1: additive, opt-in, default OFF so nothing changes until a season enables it.
-- When false, everything behaves exactly as today and eligibility falls back to the roster.
ALTER TABLE league_seasons
    ADD COLUMN lineup_required BOOLEAN NOT NULL DEFAULT false;

COMMENT ON COLUMN league_seasons.lineup_required IS 'Opt-in per season (§9). When false, lineup falls back to the roster and nothing changes.';
