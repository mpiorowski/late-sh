CREATE TABLE user_online_time_monthly (
    month_start DATE NOT NULL
        CHECK (month_start = date_trunc('month', month_start)::date),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    total_milliseconds BIGINT NOT NULL CHECK (total_milliseconds >= 0),
    last_flush_id UUID NOT NULL,
    PRIMARY KEY (month_start, user_id)
);

CREATE INDEX idx_user_online_time_monthly_total
    ON user_online_time_monthly (month_start, total_milliseconds DESC);
