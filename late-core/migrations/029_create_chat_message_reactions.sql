CREATE TABLE chat_message_reactions (
    message_id UUID NOT NULL REFERENCES chat_messages(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind SMALLINT NOT NULL CHECK (kind BETWEEN 1 AND 5),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    PRIMARY KEY (message_id, user_id)
);

CREATE INDEX idx_chat_message_reactions_message_kind
ON chat_message_reactions (message_id, kind);
