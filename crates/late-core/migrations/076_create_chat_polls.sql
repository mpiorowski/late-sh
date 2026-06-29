CREATE TABLE chat_polls (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    room_id UUID NOT NULL REFERENCES chat_rooms(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    question TEXT NOT NULL CHECK (length(btrim(question)) BETWEEN 1 AND 200),
    starts_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    ends_at TIMESTAMPTZ NOT NULL,
    active BOOLEAN NOT NULL DEFAULT true,
    CHECK (ends_at > starts_at)
);

CREATE INDEX chat_polls_active_room_idx
    ON chat_polls (room_id, ends_at DESC)
    WHERE active = true;

CREATE INDEX chat_polls_room_created_idx
    ON chat_polls (room_id, created DESC);

CREATE TABLE chat_poll_options (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    poll_id UUID NOT NULL REFERENCES chat_polls(id) ON DELETE CASCADE,
    position INT NOT NULL CHECK (position BETWEEN 1 AND 3),
    label TEXT NOT NULL CHECK (length(btrim(label)) BETWEEN 1 AND 80),
    UNIQUE (poll_id, position),
    UNIQUE (poll_id, id)
);

CREATE TABLE chat_poll_votes (
    poll_id UUID NOT NULL REFERENCES chat_polls(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    option_id UUID NOT NULL,
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    PRIMARY KEY (poll_id, user_id),
    FOREIGN KEY (poll_id, option_id)
        REFERENCES chat_poll_options(poll_id, id)
        ON DELETE CASCADE
);

CREATE INDEX chat_poll_votes_option_idx
    ON chat_poll_votes (option_id);
