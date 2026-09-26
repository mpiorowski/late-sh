-- The Old Signal (GAME.md, "Marks: the reset"). Putting it down resets
-- the runner to level 1 with bare hands and starting bits, and leaves a
-- mark. Two columns carry what the reset must not take:
--
-- `peak_level` is the highest level the runner ever reached. The tailor's
-- rack is gated on it, not on `level`, so the reset never takes a piece or
-- a tint back. Seeded from `level`, the only peak anyone has had so far.
--
-- `marks` is the Old Signal kills, ever: the number beside the level in
-- the badge, the capped attack and defense bonus, and the exp scaling.
--
-- Both only move together with `level` (a level gained raises the peak, a
-- kill resets the level), so the change trigger (migration 202 watches
-- `level`) already wakes every look directory when they move.
ALTER TABLE deadchannel_runners
    ADD COLUMN peak_level INTEGER NOT NULL DEFAULT 1,
    ADD COLUMN marks INTEGER NOT NULL DEFAULT 0;

UPDATE deadchannel_runners SET peak_level = level;

-- A fight on the row names its quarry now, a glyph or the Old Signal,
-- instead of a bare index into the glyph table: `{"kind": 3, ...}` becomes
-- `{"quarry": {"glyph": 3}, ...}`. Fights left hanging keep their foe.
UPDATE deadchannel_runners
SET fight = (fight - 'kind') || jsonb_build_object('quarry', jsonb_build_object('glyph', fight -> 'kind'))
WHERE fight IS NOT NULL;
