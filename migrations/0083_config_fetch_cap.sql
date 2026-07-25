-- Migration: config fetch cap
-- Design §6.2/§9: the match config (SteamIDs + passwords) is served at most
-- 5 times per minted token — review finding M9 (previously unbounded
-- within the 15-minute TTL).

ALTER TABLE server_reservations
    ADD COLUMN config_fetch_count INTEGER NOT NULL DEFAULT 0;
