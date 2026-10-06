-- The runner's day is the UTC day of the last roll (migration 199), and
-- the app compares it against the UTC date. `current_date` is the session
-- timezone's date, so on a server ahead of UTC a runner created late in
-- the UTC day was born tomorrow and skipped a refill. The default now
-- names the timezone it means.
ALTER TABLE deadchannel_runners
    ALTER COLUMN day SET DEFAULT (now() AT TIME ZONE 'utc')::date;
