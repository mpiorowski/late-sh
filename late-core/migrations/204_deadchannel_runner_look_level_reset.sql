-- The tailor's rack is gated by level now (pieces and tints unlock every
-- three levels, `runner/state.rs`), and the looks rolled before the gate
-- drew from the whole table, so a runner may wear a piece or a tint their
-- level has not opened. Each slot is judged on its own against the tables
-- below (`PIECES` and `Tint::level` as they stood when the gate shipped):
-- a piece or tint at or under the runner's level stays, one above it (or
-- one the table no longer knows) is re-rolled into the level-1 rack,
-- static or amber. The mark is kept. Only rows with something to fix are
-- written, so the change trigger fires for those alone and the directory
-- re-reads them. `random()` is volatile, so each slot rolls its own dice.
WITH piece_levels(code, level) AS (
    VALUES
        ('hood.plain', 1), ('hood.flat', 1), ('hood.cap', 1),
        ('hood.heavy', 4), ('hood.wire', 4), ('hood.hat', 4),
        ('hood.antenna', 7), ('hood.static', 7), ('hood.frame', 7),
        ('hood.ghost', 10), ('hood.tuner', 10), ('hood.spikes', 10),
        ('hood.cross', 13), ('hood.crown', 13), ('hood.halo', 13),
        ('eyes.dot', 1), ('eyes.round', 1), ('eyes.square', 1),
        ('eyes.band', 4), ('eyes.cross', 4), ('eyes.slit', 4),
        ('eyes.visor', 7), ('eyes.glyph', 7), ('eyes.one', 7),
        ('eyes.ghost', 10), ('eyes.noise', 10), ('eyes.test', 10),
        ('eyes.gem', 13), ('eyes.black', 13), ('eyes.signal', 13),
        ('coat.plain', 1), ('coat.thin', 1), ('coat.narrow', 1),
        ('coat.solid', 4), ('coat.heavy', 4), ('coat.faded', 4),
        ('coat.worn', 7), ('coat.bar', 7), ('coat.wire', 7),
        ('coat.ghost', 10), ('coat.cross', 10), ('coat.long', 10),
        ('coat.mantle', 13), ('coat.black', 13), ('coat.drift', 13)
),
tint_levels(code, level) AS (
    VALUES
        ('static', 1), ('amber', 1), ('phosphor', 4), ('cyan', 7),
        ('magenta', 10), ('red', 13), ('white', 15)
),
slots AS (
    SELECT runner.user_id,
           slot.name AS slot,
           runner.look -> slot.name ->> 'piece' AS piece,
           runner.look -> slot.name ->> 'tint' AS tint,
           COALESCE(piece_level.level <= runner.level, false) AS piece_ok,
           COALESCE(tint_level.level <= runner.level, false) AS tint_ok
    FROM deadchannel_runners AS runner
    CROSS JOIN (VALUES ('hood'), ('eyes'), ('coat')) AS slot(name)
    LEFT JOIN piece_levels AS piece_level
        ON piece_level.code = runner.look -> slot.name ->> 'piece'
    LEFT JOIN tint_levels AS tint_level
        ON tint_level.code = runner.look -> slot.name ->> 'tint'
),
dressed AS (
    SELECT user_id,
           slot,
           CASE
               WHEN piece_ok THEN piece
               WHEN slot = 'hood' THEN (ARRAY['hood.plain', 'hood.flat', 'hood.cap'])[1 + floor(random() * 3)::int]
               WHEN slot = 'eyes' THEN (ARRAY['eyes.dot', 'eyes.round', 'eyes.square'])[1 + floor(random() * 3)::int]
               ELSE (ARRAY['coat.plain', 'coat.thin', 'coat.narrow'])[1 + floor(random() * 3)::int]
           END AS piece,
           CASE
               WHEN tint_ok THEN tint
               ELSE (ARRAY['static', 'amber'])[1 + floor(random() * 2)::int]
           END AS tint,
           piece_ok AND tint_ok AS ok
    FROM slots
),
looks AS (
    SELECT user_id,
           jsonb_object_agg(slot, jsonb_build_object('piece', piece, 'tint', tint)) AS worn,
           bool_and(ok) AS ok
    FROM dressed
    GROUP BY user_id
)
UPDATE deadchannel_runners AS runner
SET look = looks.worn || jsonb_build_object('mark', runner.look -> 'mark'),
    updated = current_timestamp
FROM looks
WHERE looks.user_id = runner.user_id
  AND NOT looks.ok;
