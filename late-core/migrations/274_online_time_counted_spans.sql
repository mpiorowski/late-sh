-- Late Time is counted by every pod that holds a session for the user, and a
-- deploy keeps the old pod's sessions up for hours beside the new pod's. Each
-- pod reports wall-clock spans of presence; the flush credits only the part
-- of a span outside these already-counted spans, under the row lock, so one
-- moment is never counted twice whatever order the pods' batches land in.
-- A list of {"start", "end"} pairs, disjoint and in time order, kept for the
-- last day. Empty for a row nothing has written since: nothing counted yet.
ALTER TABLE user_online_time ADD COLUMN counted_spans JSONB NOT NULL DEFAULT '[]';
