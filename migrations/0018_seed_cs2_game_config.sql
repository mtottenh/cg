-- Migration: Seed CS2 game configuration from plugin defaults
-- Description: Populates games table with CS2 maps, rank tiers, and updates rating constraint

-- First, fix the rating constraint to support CS2 Premier scale (0-35,000+)
-- and other games with different rating scales
ALTER TABLE player_game_profiles
    DROP CONSTRAINT player_game_profiles_check_rating;

ALTER TABLE player_game_profiles
    ADD CONSTRAINT player_game_profiles_check_rating
    CHECK (rating >= 0 AND rating <= 100000);

-- Update CS2 game with plugin defaults
UPDATE games SET
    description = 'Valve''s tactical FPS, featuring 5v5 bomb defusal and competitive matchmaking.',
    available_maps = '[
        {"id": "de_dust2", "display_name": "Dust II", "image_url": null, "game_modes": ["competitive", "casual"]},
        {"id": "de_mirage", "display_name": "Mirage", "image_url": null, "game_modes": ["competitive", "casual"]},
        {"id": "de_inferno", "display_name": "Inferno", "image_url": null, "game_modes": ["competitive", "casual"]},
        {"id": "de_nuke", "display_name": "Nuke", "image_url": null, "game_modes": ["competitive", "casual"]},
        {"id": "de_ancient", "display_name": "Ancient", "image_url": null, "game_modes": ["competitive", "casual"]},
        {"id": "de_anubis", "display_name": "Anubis", "image_url": null, "game_modes": ["competitive", "casual"]},
        {"id": "de_vertigo", "display_name": "Vertigo", "image_url": null, "game_modes": ["competitive", "casual"]}
    ]'::jsonb,
    default_map_pool = '["de_dust2", "de_mirage", "de_inferno", "de_nuke", "de_ancient", "de_anubis", "de_vertigo"]'::jsonb,
    rank_tiers = '[
        {"id": "grey", "display_name": "Grey", "min_rating": 0, "max_rating": 4999, "icon_url": null, "color": "#808080", "order": 1},
        {"id": "light_blue", "display_name": "Light Blue", "min_rating": 5000, "max_rating": 9999, "icon_url": null, "color": "#87CEEB", "order": 2},
        {"id": "blue", "display_name": "Blue", "min_rating": 10000, "max_rating": 14999, "icon_url": null, "color": "#4169E1", "order": 3},
        {"id": "purple", "display_name": "Purple", "min_rating": 15000, "max_rating": 19999, "icon_url": null, "color": "#9932CC", "order": 4},
        {"id": "pink", "display_name": "Pink", "min_rating": 20000, "max_rating": 24999, "icon_url": null, "color": "#FF69B4", "order": 5},
        {"id": "red", "display_name": "Red", "min_rating": 25000, "max_rating": 29999, "icon_url": null, "color": "#DC143C", "order": 6},
        {"id": "gold", "display_name": "Gold", "min_rating": 30000, "max_rating": null, "icon_url": null, "color": "#FFD700", "order": 7}
    ]'::jsonb,
    config = '{
        "stats_schema": {
            "kills": {"type": "integer", "default": 0},
            "deaths": {"type": "integer", "default": 0},
            "assists": {"type": "integer", "default": 0},
            "headshots": {"type": "integer", "default": 0},
            "mvps": {"type": "integer", "default": 0},
            "total_damage": {"type": "integer", "default": 0},
            "rounds_played": {"type": "integer", "default": 0},
            "clutches_won": {"type": "integer", "default": 0},
            "clutches_attempted": {"type": "integer", "default": 0},
            "opening_kills": {"type": "integer", "default": 0},
            "opening_deaths": {"type": "integer", "default": 0},
            "flash_assists": {"type": "integer", "default": 0},
            "utility_damage": {"type": "integer", "default": 0}
        },
        "matchmaking": {
            "max_rating_difference": 3000,
            "max_team_rating_difference": 1000,
            "max_queue_time_seconds": 300,
            "rating_relaxation_per_minute": 500,
            "min_games_for_strict_matching": 10,
            "allow_wide_party_spread": false,
            "max_party_rating_spread": 5000
        },
        "supported_match_formats": ["bo1", "bo3", "bo5"],
        "default_match_format": "bo3",
        "supported_tournament_formats": ["single_elimination", "double_elimination", "swiss", "group_stage"],
        "map_pick_ban_formats": [
            {"id": "random", "display_name": "Random", "description": "Random map selected from pool"},
            {"id": "bo1_veto", "display_name": "Best of 1 Veto", "description": "Teams alternate banning maps until one remains"},
            {"id": "bo3_veto", "display_name": "Best of 3 Veto", "description": "Ban-Ban-Pick-Pick-Ban-Ban-Decider"},
            {"id": "bo5_veto", "display_name": "Best of 5 Veto", "description": "Ban-Ban-Pick-Pick-Pick-Pick-Decider"}
        ],
        "rating_system": {
            "type": "cs2_premier",
            "min_rating": 0,
            "max_rating": null,
            "initial_rating": 0,
            "k_factors": {
                "grey_light_blue": 200,
                "blue_purple": 150,
                "pink_red_gold": 100
            },
            "elo_divisor": 2000
        }
    }'::jsonb,
    updated_at = NOW()
WHERE id = 'cs2';

-- Also set initial rating to 0 (not 1500) for CS2 Premier since it starts at 0
-- Note: This affects the default in player_game_profiles
-- We might want to make this game-specific later via a config column
