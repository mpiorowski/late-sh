-- `last_read_at` updates clear the table's all-visible bits constantly, but
-- the default 20% thresholds never trip on a table this size, so autovacuum
-- never runs and the member-count lateral in Discover reads the heap instead
-- of doing an index-only scan on `chat_room_members_pkey`. Vacuum at about 1%
-- churn instead so the visibility map stays fresh. SCALE.md, DB Hot Queries.
ALTER TABLE chat_room_members SET (
    autovacuum_vacuum_scale_factor = 0.01,
    autovacuum_vacuum_insert_scale_factor = 0.01,
    autovacuum_analyze_scale_factor = 0.02
);
