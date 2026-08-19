CREATE TABLE user_online_time (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    total_milliseconds BIGINT NOT NULL CHECK (total_milliseconds >= 0),
    last_flush_id UUID NOT NULL
);

CREATE INDEX idx_user_online_time_total
    ON user_online_time (total_milliseconds DESC);
