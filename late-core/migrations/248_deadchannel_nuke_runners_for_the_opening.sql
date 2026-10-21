-- The haunting opens to everyone (GAME.md, "First contact"): the runners
-- who climbed during the staff playtest start over with the people the
-- static finds now, so every sheet goes back to a fresh one.
--
-- The nuke clears every sheet's fight, and a crossing still being fought
-- (migration 240) is closed only by the `settle_day` that finds its rival
-- fight on the sheet, so a crossing open at the nuke would stay open for
-- good. The nuke now closes it as dropped first, nothing moved, the same
-- end `settle_day` gives it. A finished crossing is history and stays.
--
-- Otherwise the nuke is migration 238's: it keeps the row, the look, and
-- the leave and guide stamps, clears the day's road, the drafted cards and
-- the inks, bumps `reset_generation` so a mark earned again is paid, and
-- leaves `unpaid_mark` standing.
CREATE OR REPLACE FUNCTION deadchannel_nuke_runners() RETURNS void AS $$
    UPDATE deadchannel_ambushes
    SET ended_at = current_timestamp,
        outcome = 'dropped',
        bits_moved = 0
    WHERE ended_at IS NULL;

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

SELECT deadchannel_nuke_runners();
