-- Give the standard CS2 maps thumbnails.
--
-- `available_maps[].image_url` has always existed and both the veto grid and
-- the map-pool picker render it (GameMapCard), but every CS2 map shipped with
-- it NULL — so every map card was a blank tile with a name on it.
--
-- The URLs are site-relative (`/maps/<id>.jpg`), served by the SPA's static
-- assets, so there is no third-party CDN in the render path and no hotlinking
-- of anyone else's artwork. Admins can replace any of them per-map through
-- the game config dialog, which writes the same field.
--
-- Only fills NULLs: a map an admin has already given an image keeps it, so
-- this is safe to re-run and safe on a customised catalog.
UPDATE games
SET available_maps = (
        SELECT jsonb_agg(
            CASE
                WHEN m->>'image_url' IS NULL
                     AND m->>'id' IN (
                         'de_dust2', 'de_mirage', 'de_inferno', 'de_nuke',
                         'de_ancient', 'de_anubis', 'de_vertigo'
                     )
                THEN m || jsonb_build_object('image_url', '/maps/' || (m->>'id') || '.jpg')
                ELSE m
            END
            ORDER BY ord
        )
        FROM jsonb_array_elements(available_maps) WITH ORDINALITY AS t(m, ord)
    )
WHERE slug = 'cs2'
  AND EXISTS (
      SELECT 1
      FROM jsonb_array_elements(available_maps) AS m
      WHERE m->>'image_url' IS NULL
        AND m->>'id' IN (
            'de_dust2', 'de_mirage', 'de_inferno', 'de_nuke',
            'de_ancient', 'de_anubis', 'de_vertigo'
        )
  );
