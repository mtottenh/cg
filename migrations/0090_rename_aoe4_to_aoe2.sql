-- The second seeded game was AoE4; the community actually plays AoE2.
-- No tournament, league, or demo rows reference the aoe4 game yet (the
-- row shipped disabled-by-usage since 0003), so a straight rename is
-- safe. Scoped by slug, so the migration is a no-op once applied.
-- (No plugin is registered under either id — plugin_id is forward-looking.)
UPDATE games
SET slug = 'aoe2',
    display_name = 'Age of Empires II',
    short_name = 'AoE2',
    plugin_id = 'aoe2'
WHERE slug = 'aoe4';
