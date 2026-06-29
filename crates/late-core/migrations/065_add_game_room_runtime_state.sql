ALTER TABLE game_rooms
ADD COLUMN runtime_state JSONB NOT NULL DEFAULT '{}'::jsonb;
