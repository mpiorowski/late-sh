-- Who listened to which radio station on which UTC day (late-ssh
-- `app/audio`, the roll call): one row per account, station and day.
-- Every replica writes the paired clients it holds on the radio source,
-- once every roll call; a listener already written is a no-op, so a day's
-- rows are its distinct listeners per station, not minutes. Read by the
-- paper's YESTERDAY column (the covered day's top stations).
CREATE TABLE radio_listens (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    station TEXT NOT NULL,
    day DATE NOT NULL,
    first_heard TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    PRIMARY KEY (user_id, station, day)
);

-- The paper's read: a day's rows grouped by station.
CREATE INDEX radio_listens_day_idx ON radio_listens (day, station);
