-- The `icecast` audio source is gone: the house `chill` and `classical`
-- mounts are radio stations now. Move its users to `radio`, tuned to the
-- mount they had. `icecast_stream` is left in place and no longer read, so
-- a pod draining on the previous build, and a rollback to it, keep working.
UPDATE users
SET settings = settings || jsonb_build_object(
        'audio_source', 'radio',
        'radio_station', CASE
            WHEN settings->>'icecast_stream' = 'classical' THEN 'classical'
            ELSE 'chill'
        END
    )
WHERE settings->>'audio_source' = 'icecast';

-- Pinned slots default to chillsynth / nightride / datawave. Anyone tuned
-- to a station outside those gets it saved in slot 1, so the station they
-- were on keeps a key.
UPDATE users
SET settings = settings || jsonb_build_object(
        'radio_slots',
        jsonb_build_array(settings->>'radio_station', 'nightride', 'datawave')
    )
WHERE NOT settings ? 'radio_slots'
  AND settings->>'radio_station' IN ('spacesynth', 'rektify', 'chill', 'classical');
