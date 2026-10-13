-- INK (GAME.md, "INK: the needle"): a draft inked for crystals is both of
-- its cards from its level on, in this climb and every one after. The one
-- purchase a mark does not take back.
--
-- `inks` is the drafts inked, in the order they were bought: a JSON list
-- of draft levels the app parses and checks against the drafts it offers.
-- NULL is none. A mark and the ledge leave it standing. The fight loop
-- writes it with the rest of the sheet under the row lock, and the change
-- trigger (migration 202) does not watch it.
ALTER TABLE deadchannel_runners
    ADD COLUMN inks JSONB;

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
        inks = NULL,
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
