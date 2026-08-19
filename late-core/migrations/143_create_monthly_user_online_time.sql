CREATE TABLE user_online_time_monthly (
    month_start DATE NOT NULL
        CHECK (month_start = date_trunc('month', month_start)::date),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    total_milliseconds BIGINT NOT NULL CHECK (total_milliseconds >= 0),
    last_flush_id UUID NOT NULL,
    -- user_id leads so the ON DELETE CASCADE lookup is indexed; the board
    -- query is served by idx_user_online_time_monthly_total below.
    PRIMARY KEY (user_id, month_start)
);

CREATE INDEX idx_user_online_time_monthly_total
    ON user_online_time_monthly (month_start, total_milliseconds DESC);
