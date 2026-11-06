-- Who was on the YouTube source on which UTC day (late-ssh `app/audio`,
-- the roll call): one row per account and day, the booth's twin of
-- `radio_listens`. Every replica writes the online accounts it holds on
-- YouTube, once every roll call; an account already written is a no-op, so
-- a day's rows are its distinct listeners, not minutes. Read by the paper's
-- YESTERDAY column (the booth line).
CREATE TABLE youtube_listens (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    day DATE NOT NULL,
    first_heard TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    PRIMARY KEY (user_id, day)
);

-- The paper's read: a day's rows counted.
CREATE INDEX youtube_listens_day_idx ON youtube_listens (day);
