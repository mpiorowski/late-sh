-- The undercity's rare currency and the bar's glass (GAME.md, "The
-- crystal pass").
--
-- `crystals` is what a runner holds of the one thing bits cannot buy: a
-- glyph leaves one behind now and then, the bright one always does. A
-- dropped signal never takes them; an Old Signal mark and a step off the
-- ledge do. Dead Air pours for them and the blade cart sells over the
-- armorer's wall for them.
--
-- `drink` is the glass a runner has in them today, a code from the bar's
-- closed menu, cleared by the day roll: one glass a day, and it wears off
-- with the day.
--
-- The fight loop writes both with the rest of the sheet under the row
-- lock. The change trigger (migration 202) watches neither: the directory
-- has no use for them.
ALTER TABLE deadchannel_runners
    ADD COLUMN crystals INT NOT NULL DEFAULT 0 CHECK (crystals >= 0),
    ADD COLUMN drink TEXT;
