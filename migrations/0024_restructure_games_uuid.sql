-- Migration: Restructure games table to use UUID primary key
-- Description: Change games.id from VARCHAR to UUID, add slug column for the string identifier
-- This makes games consistent with all other entities in the system

-- Step 1: Add new UUID column and slug column to games
ALTER TABLE games ADD COLUMN new_id UUID DEFAULT gen_random_uuid();
ALTER TABLE games ADD COLUMN slug VARCHAR(32);

-- Step 2: Copy current id (varchar) to slug
UPDATE games SET slug = id;

-- Step 3: Add UUID columns to referencing tables
ALTER TABLE player_game_profiles ADD COLUMN new_game_id UUID;
ALTER TABLE teams ADD COLUMN new_game_id UUID;
ALTER TABLE leagues ADD COLUMN new_game_id UUID;

-- Step 4: Update the new UUID columns by looking up the game's new_id via slug
UPDATE player_game_profiles pgp
SET new_game_id = g.new_id
FROM games g
WHERE pgp.game_id = g.id;

UPDATE teams t
SET new_game_id = g.new_id
FROM games g
WHERE t.game_id = g.id;

UPDATE leagues l
SET new_game_id = g.new_id
FROM games g
WHERE l.game_id = g.id;

-- Step 5: Drop old foreign key constraints
ALTER TABLE player_game_profiles DROP CONSTRAINT player_game_profiles_game_id_fkey;
ALTER TABLE teams DROP CONSTRAINT teams_game_id_fkey;
ALTER TABLE leagues DROP CONSTRAINT leagues_game_id_fkey;

-- Step 6: Drop the old id column and rename new_id to id on games
ALTER TABLE games DROP CONSTRAINT games_pkey;
ALTER TABLE games DROP COLUMN id;
ALTER TABLE games RENAME COLUMN new_id TO id;
ALTER TABLE games ADD PRIMARY KEY (id);

-- Step 7: Make slug NOT NULL and UNIQUE
ALTER TABLE games ALTER COLUMN slug SET NOT NULL;
ALTER TABLE games ADD CONSTRAINT games_slug_unique UNIQUE (slug);

-- Step 8: Drop old game_id columns and rename new ones in referencing tables
-- player_game_profiles
ALTER TABLE player_game_profiles DROP COLUMN game_id;
ALTER TABLE player_game_profiles RENAME COLUMN new_game_id TO game_id;
ALTER TABLE player_game_profiles ALTER COLUMN game_id SET NOT NULL;
ALTER TABLE player_game_profiles ADD CONSTRAINT player_game_profiles_game_id_fkey
    FOREIGN KEY (game_id) REFERENCES games(id) ON DELETE CASCADE;

-- teams
ALTER TABLE teams DROP COLUMN game_id;
ALTER TABLE teams RENAME COLUMN new_game_id TO game_id;
ALTER TABLE teams ADD CONSTRAINT teams_game_id_fkey
    FOREIGN KEY (game_id) REFERENCES games(id) ON DELETE SET NULL;

-- leagues
ALTER TABLE leagues DROP COLUMN game_id;
ALTER TABLE leagues RENAME COLUMN new_game_id TO game_id;
ALTER TABLE leagues ALTER COLUMN game_id SET NOT NULL;
ALTER TABLE leagues ADD CONSTRAINT leagues_game_id_fkey
    FOREIGN KEY (game_id) REFERENCES games(id) ON DELETE RESTRICT;

-- Step 9: Recreate any dropped unique constraints that included game_id
DROP INDEX IF EXISTS idx_player_game_profiles_player;
DROP INDEX IF EXISTS idx_player_game_profiles_game;
DROP INDEX IF EXISTS idx_player_game_profiles_rating;
DROP INDEX IF EXISTS idx_player_game_profiles_matches;

-- Drop and recreate the unique constraint (it references the old column type)
ALTER TABLE player_game_profiles DROP CONSTRAINT IF EXISTS player_game_profiles_unique;
ALTER TABLE player_game_profiles ADD CONSTRAINT player_game_profiles_unique UNIQUE (player_id, game_id);

-- Recreate indexes for player_game_profiles
CREATE INDEX idx_player_game_profiles_player ON player_game_profiles(player_id);
CREATE INDEX idx_player_game_profiles_game ON player_game_profiles(game_id);
CREATE INDEX idx_player_game_profiles_rating ON player_game_profiles(game_id, rating DESC);
CREATE INDEX idx_player_game_profiles_matches ON player_game_profiles(game_id, matches_played DESC);

-- Recreate leagues index
DROP INDEX IF EXISTS idx_leagues_game_id;
CREATE INDEX idx_leagues_game_id ON leagues(game_id);

-- Step 10: Add index on games.slug for lookups
CREATE INDEX idx_games_slug ON games(slug);

COMMENT ON COLUMN games.id IS 'UUID primary key (consistent with other entities)';
COMMENT ON COLUMN games.slug IS 'Human-readable identifier like cs2, aoe4 (used in URLs and API)';
