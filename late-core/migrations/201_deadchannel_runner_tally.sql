-- The runner's tally, for the wire's news (GAME.md, "The wire sees the
-- news, never the play-by-play"): glyphs put down in all, and today's
-- kills and runs. `kills` makes a first kill exact (a death keeps 90% of
-- the exp, so exp alone cannot say "never won"); the two `_today` counts
-- fill the one line the wire gets when the last ration of the day is
-- spent, and the lazy day roll zeroes them with the rest of the sheet.
ALTER TABLE deadchannel_runners
    ADD COLUMN kills INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN kills_today INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN runs_today INTEGER NOT NULL DEFAULT 0;
