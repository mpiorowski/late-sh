-- Late Time is counted by every pod that holds a session for the user, and a
-- deploy keeps the old pod's sessions up for hours beside the new pod's. Each
-- pod reports wall-clock spans; the flush credits only the part of a span past
-- this mark, under the row lock, so one moment is never counted twice. NULL
-- for a row nothing has written since: nothing counted yet.
ALTER TABLE user_online_time ADD COLUMN counted_until TIMESTAMPTZ;
