-- Topic rooms are now unique per visibility bucket (public/private), not
-- globally by slug, and default to opt-in unless explicitly marked auto-join.

ALTER TABLE chat_rooms
    ALTER COLUMN auto_join SET DEFAULT false;

DROP INDEX IF EXISTS uq_chat_rooms_topic_slug;

CREATE UNIQUE INDEX uq_chat_rooms_topic_visibility_slug
ON chat_rooms (visibility, slug)
WHERE kind = 'topic';
