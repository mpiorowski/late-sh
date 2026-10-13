-- The road and the round (GAME.md, "The road pass"): the day's ten rations
-- are ten steps down a road that is the same for every runner, and a fight
-- is a hand of cards.
--
-- `road` is one runner's day on it: the lanes taken, how each step went
-- (the share card and the map read it), and the static riding the deck
-- between fights. The road itself is a pure function of the UTC date and
-- is stored nowhere. NULL is a day with no step taken; the day roll wipes
-- it. The fight loop writes it with the rest of the sheet under the row
-- lock, and the change trigger (migration 202) does not watch it.
ALTER TABLE deadchannel_runners
    ADD COLUMN road JSONB;

-- A fight stored before this migration is an exchange loop's: it has no
-- deck, no hand, and no turn, and the round cannot read it. Every hanging
-- fight is dropped, and the day's rations are handed back whole so the
-- steps on the row and the steps on the (empty) road agree. The signal,
-- the level, the kit, and the purse are untouched: a runner who was down
-- stays down until the roll.
UPDATE deadchannel_runners
SET fight = NULL,
    rations_left = 10,
    kills_today = 0,
    runs_today = 0,
    updated = current_timestamp;

-- The nuke resets the new column with the rest of the sheet.
CREATE OR REPLACE FUNCTION deadchannel_nuke_runners() RETURNS void AS $$
    UPDATE deadchannel_runners
    SET level = 1,
        peak_level = 1,
        exp = 0,
        signal = 10,
        weapon_tier = 0,
        armor_tier = 0,
        bits = 50,
        rations_left = 10,
        day = (now() AT TIME ZONE 'utc')::date,
        fight = NULL,
        road = NULL,
        kills = 0,
        kills_today = 0,
        runs_today = 0,
        marks = 0,
        stash = 0,
        debt = 0,
        crystals = 0,
        drink = NULL,
        reset_generation = reset_generation + 1,
        updated = current_timestamp;
$$ LANGUAGE sql;
