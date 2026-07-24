-- Room "about" info for user-created rooms: a friendly display name, a short
-- description of what the room is about, and the general rules. All nullable so
-- every existing room (and the lounge/language/dm/game rooms) is untouched.
-- `created_by` records who opened a user-created room so only they (or a mod)
-- can edit its info later. It is nullable and unset for system rooms.
ALTER TABLE chat_rooms
    ADD COLUMN title      TEXT,
    ADD COLUMN about      TEXT,
    ADD COLUMN rules      TEXT,
    ADD COLUMN created_by UUID REFERENCES users(id) ON DELETE SET NULL;
