-- Balance testing wants every runner back at the top of Static Row. The
-- function puts every sheet back to a fresh one: level 1, bare hands, the
-- starting bits, a full day, no marks, nothing in the locker, no debt, no
-- crystals. The row stays, so nobody has to join again, and the look,
-- the leave stamp, and the guide stamp are untouched. A look above the
-- level-1 rack is snapped onto it the next time the tailor's mirror opens.
--
-- To nuke again, ship a new migration holding only
--     SELECT deadchannel_nuke_runners();
-- A column added to the sheet later is added to this function in the
-- migration that adds it (CREATE OR REPLACE).
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
        unpaid_mark = NULL,
        stash = 0,
        debt = 0,
        crystals = 0,
        drink = NULL,
        updated = current_timestamp;
$$ LANGUAGE sql;

SELECT deadchannel_nuke_runners();
