-- The runner directory (migration 172, narrowed by 199) serves the level
-- beside the look now: the badge in #deadchannel wears the mark and the
-- level, tinted by band. A level changes at most a few times a day per
-- runner, so the trigger widens to it; the rest of the sheet (signal,
-- rations, bits, the fight) still wakes no directory.
DROP TRIGGER deadchannel_runners_changed ON deadchannel_runners;
CREATE TRIGGER deadchannel_runners_changed
    AFTER INSERT OR UPDATE OF look, left_at, level ON deadchannel_runners
    FOR EACH ROW EXECUTE FUNCTION notify_deadchannel_runner_changed();
