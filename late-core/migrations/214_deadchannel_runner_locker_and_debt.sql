-- The undercity's two money places (GAME.md, "The stat block"): the
-- lockers and the bits machine.
--
-- `stash` is the bits in the runner's locker: a drop never touches it,
-- every deposit pays the locker's cut on the way in, a withdrawal is free,
-- and an Old Signal mark or a step off the ledge empties it.
--
-- `debt` is what the bits machine is owed: a loan up to the level's cap
-- plus the machine's fee, charged once when it lends, garnished off every
-- glyph's pay until it is gone. Nothing clears it but paying it: not a
-- drop, not a mark, not the ledge.
--
-- Both are whole bits and never negative; the fight loop writes them with
-- the rest of the sheet under the row lock. The change trigger (migration
-- 202) does not watch either: the directory has no use for them.
ALTER TABLE deadchannel_runners
    ADD COLUMN stash BIGINT NOT NULL DEFAULT 0 CHECK (stash >= 0),
    ADD COLUMN debt BIGINT NOT NULL DEFAULT 0 CHECK (debt >= 0);
