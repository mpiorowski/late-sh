-- The runner's sheet (GAME.md, Phase 2, "The stat block"): the LoGD numbers
-- under new names. Level, exp, signal (health, 10 per level), the two gear
-- tiers (attack is level + weapon tier, defense level + armor tier), bits
-- on hand, the rations left today, and the UTC day of the last roll.
--
-- The day roll is lazy: the first touch of the row after midnight UTC
-- refills signal and rations, under the row lock the service takes for
-- every action, so no cron and no replica can disagree.
--
-- `fight` is the fight in progress, if one is: the foe and the last few
-- lines of the exchange, as JSON. It lives on the row so a dropped SSH
-- session finds the same fight waiting (the ration was spent when it
-- started), and two devices of one person see one fight.
ALTER TABLE deadchannel_runners
    ADD COLUMN level INTEGER NOT NULL DEFAULT 1,
    ADD COLUMN exp BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN signal INTEGER NOT NULL DEFAULT 10,
    ADD COLUMN weapon_tier INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN armor_tier INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN bits BIGINT NOT NULL DEFAULT 50,
    ADD COLUMN rations_left INTEGER NOT NULL DEFAULT 10,
    ADD COLUMN day DATE NOT NULL DEFAULT current_date,
    ADD COLUMN fight JSONB;

-- The look directory (migration 172) re-reads every look on any change to
-- the row. A fight action changes the sheet ten times a day per runner
-- and the look not at all, so the trigger narrows to the columns the
-- directory serves: the look and the leave stamp.
DROP TRIGGER deadchannel_runners_changed ON deadchannel_runners;
CREATE TRIGGER deadchannel_runners_changed
    AFTER INSERT OR UPDATE OF look, left_at ON deadchannel_runners
    FOR EACH ROW EXECUTE FUNCTION notify_deadchannel_runner_changed();
