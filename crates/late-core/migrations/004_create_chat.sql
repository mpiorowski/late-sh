CREATE TABLE chat_rooms (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    kind TEXT NOT NULL CHECK (kind IN ('general', 'language', 'dm')),
    visibility TEXT NOT NULL DEFAULT 'public' CHECK (visibility IN ('public', 'private', 'dm')),
    auto_join BOOLEAN NOT NULL DEFAULT true,
    slug TEXT,
    language_code TEXT,
    dm_user_a UUID REFERENCES users(id) ON DELETE CASCADE,
    dm_user_b UUID REFERENCES users(id) ON DELETE CASCADE,
    CONSTRAINT chat_rooms_general_slug_chk
        CHECK ((kind <> 'general') OR (slug = 'general')),
    CONSTRAINT chat_rooms_language_code_chk
        CHECK ((kind <> 'language') OR (language_code IS NOT NULL)),
    CONSTRAINT chat_rooms_dm_users_chk
        CHECK (
            (kind <> 'dm')
            OR (dm_user_a IS NOT NULL AND dm_user_b IS NOT NULL AND dm_user_a <> dm_user_b)
        ),
    CONSTRAINT chat_rooms_dm_order_chk
        CHECK ((kind <> 'dm') OR (dm_user_a::text < dm_user_b::text)),
    CONSTRAINT chat_rooms_visibility_kind_chk
        CHECK (
            (kind = 'dm' AND visibility = 'dm')
            OR (kind <> 'dm' AND visibility IN ('public', 'private'))
        ),
    CONSTRAINT chat_rooms_auto_join_public_chk
        CHECK ((auto_join = false) OR (visibility = 'public'))
);

CREATE UNIQUE INDEX uq_chat_rooms_general_slug
ON chat_rooms (slug)
WHERE kind = 'general';

CREATE UNIQUE INDEX uq_chat_rooms_language_code
ON chat_rooms (language_code)
WHERE kind = 'language';

CREATE UNIQUE INDEX uq_chat_rooms_dm_pair
ON chat_rooms (dm_user_a, dm_user_b)
WHERE kind = 'dm';

CREATE TABLE chat_room_members (
    room_id UUID NOT NULL REFERENCES chat_rooms(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    joined_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    last_read_at TIMESTAMPTZ,
    PRIMARY KEY (room_id, user_id)
);

CREATE INDEX idx_chat_room_members_user
ON chat_room_members (user_id, joined_at DESC);

CREATE TABLE chat_messages (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    room_id UUID NOT NULL REFERENCES chat_rooms(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    body TEXT NOT NULL CHECK (length(trim(body)) > 0 AND length(body) <= 2000),
    edited_at TIMESTAMPTZ,
    deleted_at TIMESTAMPTZ
);

CREATE INDEX idx_chat_messages_room_created
ON chat_messages (room_id, created DESC, id DESC);

CREATE INDEX idx_chat_messages_user_created
ON chat_messages (user_id, created DESC);
