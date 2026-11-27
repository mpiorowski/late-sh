-- The daily streak pays no chips any more: any daily board win keeps the
-- day (`late-core/src/models/daily_streak.rs`), so the bonus level it once
-- climbed has no reader. The old `daily_quest_streak_reward` ledger rows
-- stay so history reads.
--
-- Held back one release in migrations/deferred/: the binary before the
-- any-win streak read `bonus_level` off every row it loaded for the Arcade
-- strip, and drained pods outlive a deploy by hours.
ALTER TABLE user_daily_quest_streaks DROP COLUMN bonus_level;
