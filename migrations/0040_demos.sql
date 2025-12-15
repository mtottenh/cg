-- Demo Catalog System
-- Independent demo file catalog with optional match linking
--
-- Demos exist independently of matches and can be:
-- - Browsed/searched by various criteria
-- - Categorized (PUG, League, Scrim, Ignored)
-- - Hidden from public browsing
-- - Linked to tournament matches

-- Main demo catalog table
CREATE TABLE demos (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    game_id UUID NOT NULL REFERENCES games(id),
    file_name VARCHAR(512) NOT NULL,

    -- S3 storage location
    s3_bucket VARCHAR(128) NOT NULL,
    s3_key VARCHAR(512) NOT NULL,
    file_size_bytes BIGINT,

    -- Categorization
    category VARCHAR(32) NOT NULL DEFAULT 'uncategorized',
    is_hidden BOOLEAN NOT NULL DEFAULT false,

    -- Optional organization linkage (for browsing context)
    league_id UUID REFERENCES leagues(id) ON DELETE SET NULL,
    tournament_id UUID REFERENCES tournaments(id) ON DELETE SET NULL,

    -- Parsed metadata from demo stats
    metadata JSONB,

    -- Full stats JSON blob
    stats_json JSONB,

    -- Processing status
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    stats_fetched_at TIMESTAMPTZ,
    stats_fetch_error TEXT,

    -- Admin categorization tracking
    categorized_by_user_id UUID REFERENCES users(id),
    categorized_at TIMESTAMPTZ,
    hidden_by_user_id UUID REFERENCES users(id),
    hidden_at TIMESTAMPTZ,
    admin_notes TEXT,

    -- When the demo file was discovered in S3
    discovered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Standard timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT demos_unique_s3 UNIQUE (s3_bucket, s3_key),
    CONSTRAINT demos_check_category CHECK (category IN (
        'uncategorized', 'pug', 'league', 'scrim', 'ignored'
    )),
    CONSTRAINT demos_check_status CHECK (status IN (
        'pending', 'processing', 'ready', 'failed', 'archived'
    ))
);

-- Indexes for common queries
CREATE INDEX idx_demos_game ON demos(game_id);
CREATE INDEX idx_demos_category ON demos(category) WHERE category != 'ignored';
CREATE INDEX idx_demos_status ON demos(status);
CREATE INDEX idx_demos_league ON demos(league_id) WHERE league_id IS NOT NULL;
CREATE INDEX idx_demos_tournament ON demos(tournament_id) WHERE tournament_id IS NOT NULL;
CREATE INDEX idx_demos_hidden ON demos(is_hidden) WHERE is_hidden = false;
CREATE INDEX idx_demos_pending ON demos(status) WHERE status = 'pending';
CREATE INDEX idx_demos_discovered ON demos(discovered_at DESC);

-- GIN index for metadata JSON queries (map name, team names, etc.)
CREATE INDEX idx_demos_metadata ON demos USING gin(metadata jsonb_path_ops)
    WHERE metadata IS NOT NULL;

CREATE TRIGGER demos_updated_at
    BEFORE UPDATE ON demos
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE demos IS 'Demo file catalog with categorization and stats';
COMMENT ON COLUMN demos.category IS 'Demo type: uncategorized, pug (pickup game), league (official match), scrim (practice), ignored';
COMMENT ON COLUMN demos.status IS 'Processing status: pending, processing, ready, failed, archived';
COMMENT ON COLUMN demos.metadata IS 'Parsed metadata: map_name, team names, scores, duration, match_date';
COMMENT ON COLUMN demos.stats_json IS 'Full stats JSON from demo parser';

-- Demo-Match Link table (many-to-many relationship)
CREATE TABLE demo_match_links (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    demo_id UUID NOT NULL REFERENCES demos(id) ON DELETE CASCADE,
    match_id UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,
    game_number INTEGER,  -- Which game in the series (1, 2, 3 for BO3)

    -- Link type
    link_type VARCHAR(32) NOT NULL DEFAULT 'manual',
    confidence_score REAL,  -- For auto-matched links (0.0 - 1.0)

    -- Validation
    validated BOOLEAN NOT NULL DEFAULT false,
    validated_at TIMESTAMPTZ,
    validation_result JSONB,

    -- Who created the link
    linked_by_user_id UUID REFERENCES users(id),
    linked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- A demo can only be linked once to a specific match (but can be linked to multiple matches)
    CONSTRAINT demo_match_links_unique UNIQUE (demo_id, match_id),
    CONSTRAINT demo_match_links_check_type CHECK (link_type IN (
        'manual', 'auto_matched', 'evidence'
    )),
    CONSTRAINT demo_match_links_check_confidence CHECK (
        confidence_score IS NULL OR (confidence_score >= 0.0 AND confidence_score <= 1.0)
    )
);

CREATE INDEX idx_demo_match_links_demo ON demo_match_links(demo_id);
CREATE INDEX idx_demo_match_links_match ON demo_match_links(match_id);
CREATE INDEX idx_demo_match_links_type ON demo_match_links(link_type);

COMMENT ON TABLE demo_match_links IS 'Links between demos and tournament matches';
COMMENT ON COLUMN demo_match_links.link_type IS 'How the link was created: manual (admin), auto_matched (system), evidence (dispute)';
COMMENT ON COLUMN demo_match_links.confidence_score IS 'Confidence score for auto-matched links (0.0-1.0)';

-- Demo Players table (extracted from demo stats)
CREATE TABLE demo_players (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    demo_id UUID NOT NULL REFERENCES demos(id) ON DELETE CASCADE,

    -- Player identification from demo
    steam_id VARCHAR(64) NOT NULL,
    player_name VARCHAR(128) NOT NULL,
    team_name VARCHAR(128),

    -- Optional link to portal player account
    player_id UUID REFERENCES players(id) ON DELETE SET NULL,

    -- Stats (extracted from demo)
    kills INTEGER NOT NULL DEFAULT 0,
    deaths INTEGER NOT NULL DEFAULT 0,
    assists INTEGER NOT NULL DEFAULT 0,
    damage INTEGER NOT NULL DEFAULT 0,
    adr DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    headshot_kills INTEGER NOT NULL DEFAULT 0,
    hs_percentage DOUBLE PRECISION NOT NULL DEFAULT 0.0,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- A steam_id should only appear once per demo
    CONSTRAINT demo_players_unique_steam UNIQUE (demo_id, steam_id)
);

CREATE INDEX idx_demo_players_demo ON demo_players(demo_id);
CREATE INDEX idx_demo_players_steam_id ON demo_players(steam_id);
CREATE INDEX idx_demo_players_player ON demo_players(player_id) WHERE player_id IS NOT NULL;
CREATE INDEX idx_demo_players_team ON demo_players(team_name) WHERE team_name IS NOT NULL;

COMMENT ON TABLE demo_players IS 'Player appearances and stats extracted from demos';
COMMENT ON COLUMN demo_players.steam_id IS 'Player Steam ID from the demo file';
COMMENT ON COLUMN demo_players.player_id IS 'Link to portal player account (if identified)';
