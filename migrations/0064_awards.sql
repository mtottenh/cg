-- Migration: Tournament / season awards
-- Description:
--   Organizer-authorable, named stat awards ("Swag 7" = most MAG-7 kills,
--   "Blind Monk" = most kills while flashed) scoped to a tournament or
--   league season. An award references a stat_key from the game plugin's
--   stat catalog plus an aggregation/direction/qualifier; standings are
--   computed live from demo_player_stats and snapshotted into
--   award_results on finalization (immutable podium + trophy case).
--
--   Design: docs/design-tournament-awards.md §4

-- ============================================================================
-- 1. TEMPLATES (per game; the organizer's picker)
-- ============================================================================

CREATE TABLE award_templates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    game_id UUID NOT NULL REFERENCES games(id) ON DELETE CASCADE,

    -- Stable slug for seeding/idempotency (e.g. 'swag7').
    key VARCHAR(64) NOT NULL,

    name VARCHAR(64) NOT NULL,
    description TEXT,
    icon VARCHAR(64),                -- mdi icon name
    color VARCHAR(7),                -- #rrggbb

    stat_key VARCHAR(128) NOT NULL,
    aggregation VARCHAR(32) NOT NULL DEFAULT 'sum',
    direction VARCHAR(4) NOT NULL DEFAULT 'desc',
    min_qualifier_type VARCHAR(16),  -- 'matches' | 'rounds' | NULL
    min_qualifier_value INTEGER,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT award_templates_key_unique UNIQUE (game_id, key),
    CONSTRAINT award_templates_check_aggregation
        CHECK (aggregation IN ('sum', 'max_single_demo', 'avg_per_demo')),
    CONSTRAINT award_templates_check_direction CHECK (direction IN ('asc', 'desc')),
    CONSTRAINT award_templates_check_qualifier
        CHECK (min_qualifier_type IS NULL OR min_qualifier_type IN ('matches', 'rounds'))
);

CREATE INDEX idx_award_templates_game ON award_templates(game_id);

-- ============================================================================
-- 2. AWARD INSTANCES
-- ============================================================================

CREATE TABLE awards (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    scope_type VARCHAR(16) NOT NULL,
    scope_id UUID NOT NULL,          -- tournament id or league_season id
    game_id UUID NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    template_id UUID REFERENCES award_templates(id) ON DELETE SET NULL,

    name VARCHAR(64) NOT NULL,
    description TEXT,
    icon VARCHAR(64),
    color VARCHAR(7),

    stat_key VARCHAR(128) NOT NULL,
    aggregation VARCHAR(32) NOT NULL DEFAULT 'sum',
    direction VARCHAR(4) NOT NULL DEFAULT 'desc',
    min_qualifier_type VARCHAR(16),
    min_qualifier_value INTEGER,

    -- Reserved: 'team' awards later; v1 is player-only.
    subject_type VARCHAR(16) NOT NULL DEFAULT 'player',

    status VARCHAR(16) NOT NULL DEFAULT 'active',
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT awards_scope_name_unique UNIQUE (scope_type, scope_id, name),
    CONSTRAINT awards_check_scope CHECK (scope_type IN ('tournament', 'league_season')),
    CONSTRAINT awards_check_aggregation
        CHECK (aggregation IN ('sum', 'max_single_demo', 'avg_per_demo')),
    CONSTRAINT awards_check_direction CHECK (direction IN ('asc', 'desc')),
    CONSTRAINT awards_check_subject CHECK (subject_type IN ('player', 'team')),
    CONSTRAINT awards_check_status CHECK (status IN ('active', 'finalized', 'void')),
    CONSTRAINT awards_check_qualifier
        CHECK (min_qualifier_type IS NULL OR min_qualifier_type IN ('matches', 'rounds'))
);

CREATE INDEX idx_awards_scope ON awards(scope_type, scope_id);
CREATE INDEX idx_awards_status ON awards(status);

CREATE TRIGGER awards_updated_at
    BEFORE UPDATE ON awards
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- ============================================================================
-- 3. FINALIZED RESULTS (immutable podium; the trophy case)
-- ============================================================================

CREATE TABLE award_results (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    award_id UUID NOT NULL REFERENCES awards(id) ON DELETE CASCADE,

    -- Shared rank on ties (two rank-1 rows = shared award).
    rank INTEGER NOT NULL,
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    value DOUBLE PRECISION NOT NULL,
    demos_counted INTEGER NOT NULL DEFAULT 0,

    finalized_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT award_results_unique UNIQUE (award_id, player_id),
    CONSTRAINT award_results_check_rank CHECK (rank >= 1)
);

CREATE INDEX idx_award_results_award ON award_results(award_id);
CREATE INDEX idx_award_results_player ON award_results(player_id);

-- ============================================================================
-- 4. SEED CS2 TEMPLATES
-- ============================================================================
-- stat_keys reference the CS2 plugin's stat catalog:
--   summary scalars ('headshot_kills', 'utility_damage', ...) and
--   weapon-map explosions ('kills.weapon.{name}').

INSERT INTO award_templates
    (game_id, key, name, description, icon, color, stat_key, aggregation, direction)
SELECT g.id, t.key, t.name, t.description, t.icon, t.color, t.stat_key, 'sum', 'desc'
FROM games g
CROSS JOIN (VALUES
    ('headshot_machine', 'Headshot Machine', 'Most headshot kills',
     'mdi-head-flash', '#E53935', 'headshot_kills'),
    ('swag7', 'Swag 7', 'Most MAG-7 kills',
     'mdi-spray', '#8E24AA', 'kills.weapon.mag7'),
    ('blind_monk', 'Blind Monk', 'Most kills while flashed',
     'mdi-eye-off', '#FDD835', 'kills.while_blind'),
    ('knife_fight', 'The Duelist', 'Most knife kills',
     'mdi-knife-military', '#6D4C41', 'kills.weapon.knife'),
    ('the_ninja', 'The Ninja', 'Most bomb defuses',
     'mdi-bomb-off', '#00897B', 'bomb_defuses'),
    ('utility_king', 'Utility King', 'Most utility damage',
     'mdi-fire', '#FB8C00', 'utility_damage'),
    ('wallbanger', 'Wallbanger', 'Most wallbang kills',
     'mdi-wall', '#5E35B1', 'wallbangs'),
    ('flash_god', 'Flash God', 'Most flash assists',
     'mdi-flash', '#FFB300', 'flash_assists')
) AS t(key, name, description, icon, color, stat_key)
WHERE g.slug = 'cs2'
ON CONFLICT (game_id, key) DO NOTHING;

COMMENT ON TABLE awards IS
    'Organizer-named stat awards scoped to a tournament or league season';
COMMENT ON TABLE award_results IS
    'Immutable podium snapshots written at finalization';
