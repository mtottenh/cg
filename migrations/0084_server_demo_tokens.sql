-- Migration: server-scoped demo upload tokens
-- Design: docs/matchzy-integration.md §6.3 — demo-upload cvars are NOT set
-- per match (a series-end cvar restore can race the final map's upload).
-- The agent writes them once into the server's permanent MatchZy config
-- with a server-scoped bearer token, minted at enrollment.

ALTER TABLE game_servers
    ADD COLUMN demo_token_hash VARCHAR(64);
