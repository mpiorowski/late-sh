CREATE TABLE notifications (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    actor_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    message_id UUID NOT NULL REFERENCES chat_messages(id) ON DELETE CASCADE,
    room_id UUID NOT NULL REFERENCES chat_rooms(id) ON DELETE CASCADE,
    read_at TIMESTAMPTZ,
    CONSTRAINT notifications_no_self_mention CHECK (user_id <> actor_id)
);

CREATE INDEX idx_notifications_user_unread ON notifications (user_id, created DESC) WHERE read_at IS NULL;
CREATE INDEX idx_notifications_user_created ON notifications (user_id, created DESC);
