CREATE TABLE character_sheets (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    room_id UUID NOT NULL REFERENCES chat_rooms(id) ON DELETE CASCADE,
    name TEXT NOT NULL DEFAULT '' CHECK (length(name) <= 48),
    body TEXT NOT NULL DEFAULT '' CHECK (length(body) <= 4000),
    UNIQUE (user_id, room_id)
);
