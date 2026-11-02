-- Headcount samples for Home's pulse tile: one row per replica per
-- five-minute bucket, the people online on that replica when it sampled.
-- The pulse reads the last day and sums the replicas per bucket; the
-- writer prunes rows past two days.
CREATE TABLE presence_samples (
    bucket TIMESTAMPTZ NOT NULL,
    replica_id UUID NOT NULL,
    online BIGINT NOT NULL CHECK (online >= 0),
    PRIMARY KEY (bucket, replica_id)
);
