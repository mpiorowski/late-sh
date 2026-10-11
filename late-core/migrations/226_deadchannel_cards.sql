-- The draft (GAME.md, "The draft"): at four levels on the way up a runner
-- picks one of two new cards, and it takes the place of a strike or a
-- block in the deck.
--
-- `cards` is the picks made, in order: a JSON list of card names the app
-- parses and checks against the drafts it offers. NULL is the starting
-- deck. A mark and the ledge write it back to NULL with the level. The
-- fight loop writes it with the rest of the sheet under the row lock, and
-- the change trigger (migration 202) does not watch it.
ALTER TABLE deadchannel_runners
    ADD COLUMN cards JSONB;

-- A fight stored before this migration has no `muted` flag and the round
-- cannot read it. A runner caught mid-fight gets the day back whole, so
-- the steps on the row and the steps on the road agree. Everybody else is
-- untouched.
UPDATE deadchannel_runners
SET fight = NULL,
    road = NULL,
    rations_left = 10,
    kills_today = 0,
    runs_today = 0,
    updated = current_timestamp
WHERE fight IS NOT NULL;

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
        cards = NULL,
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
