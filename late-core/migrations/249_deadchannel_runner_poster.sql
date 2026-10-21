-- The poster (GAME.md, "The look"): a 32 by 12 picture a runner paints at
-- PRINTS, hung on their profile in the terminal and on the web.
--
-- `poster` is the serialized `dartboard_core::Canvas`, JSON the app decodes
-- where it draws it. NULL is no poster (the profile shows the test card).
-- Only PRINTS writes it (the runner saving or burning their own) and a
-- moderator's `deadchannel remove-poster` clears it.
--
-- The change trigger (migration 202, `UPDATE OF look, left_at, level`) does
-- not watch it: the runner directory carries no posters, and a profile reads
-- the row when it opens. The nuke (migration 248) keeps it, as it keeps the
-- look.
ALTER TABLE deadchannel_runners
    ADD COLUMN poster JSONB;
