-- The nuke (migration 222) puts `marks` back to 0 and keeps the row, so a
-- mark earned again would ask for its chips under the key the first one
-- claimed (`<runner row id>:<mark>`), and unique claims never expire: the
-- house would never pay it. `reset_generation` counts the nukes a row has
-- been through, and the payout's key carries it
-- (`<runner row id>:<generation>:<mark>`), so every climb after a nuke
-- claims under keys of its own.
--
-- The nuke also stops clearing `unpaid_mark`: that column is the retry
-- record for a grant that errored (migration 210), chips the house still
-- owes, and a balance reset is no reason to drop them.
ALTER TABLE deadchannel_runners
    ADD COLUMN reset_generation INT NOT NULL DEFAULT 0;

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
