-- A reviewed submission is also a pool entry; only approved, active entries
-- can receive a daily assignment. Bytes live in managed object storage.
CREATE TABLE sliding_puzzle_artworks (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    source_message_id UUID UNIQUE REFERENCES chat_messages(id) ON DELETE SET NULL,
    source_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    source_url TEXT,
    image_url TEXT,
    embedded_key TEXT UNIQUE,
    sha256 TEXT CHECK (sha256 IS NULL OR sha256 ~ '^[0-9a-f]{64}$'),
    title TEXT NOT NULL CHECK (char_length(title) BETWEEN 1 AND 120),
    credit TEXT NOT NULL CHECK (char_length(credit) BETWEEN 1 AND 200),
    approved_by UUID REFERENCES users(id) ON DELETE SET NULL,
    approved_at TIMESTAMPTZ,
    available_from DATE,
    active BOOLEAN NOT NULL DEFAULT true,
    CHECK ((embedded_key IS NOT NULL AND image_url IS NULL AND sha256 IS NULL)
        OR (embedded_key IS NULL AND image_url IS NOT NULL AND sha256 IS NOT NULL)),
    CHECK ((approved_at IS NULL) = (available_from IS NULL))
);
CREATE UNIQUE INDEX sliding_puzzle_artworks_hash ON sliding_puzzle_artworks(sha256) WHERE active;
CREATE INDEX sliding_puzzle_artworks_author ON sliding_puzzle_artworks(source_user_id);
CREATE INDEX sliding_puzzle_artworks_reviewer ON sliding_puzzle_artworks(approved_by);
CREATE INDEX sliding_puzzle_artworks_eligible ON sliding_puzzle_artworks(available_from, created, id)
    WHERE active AND approved_at IS NOT NULL;

INSERT INTO sliding_puzzle_artworks (embedded_key, title, credit, approved_at, available_from)
VALUES ('night-terminal', 'Night Terminal', 'late.sh · AI illustration', '2026-09-11 00:00:00+00', '2026-09-11'),
       ('rooftop-garden', 'Rooftop Garden', 'late.sh · AI illustration', '2026-09-11 00:00:00+00', '2026-09-11'),
       ('night-train', 'Night Train', 'late.sh · AI illustration', '2026-09-11 00:00:00+00', '2026-09-11');

CREATE TABLE sliding_puzzle_daily_artworks (
    puzzle_date DATE PRIMARY KEY,
    artwork_id UUID NOT NULL REFERENCES sliding_puzzle_artworks(id),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp
);
CREATE INDEX sliding_puzzle_daily_artworks_artwork ON sliding_puzzle_daily_artworks(artwork_id, puzzle_date DESC);

INSERT INTO chat_rooms (kind, visibility, auto_join, permanent, slug, topic, rules)
VALUES ('topic', 'public', true, true, 'puzzle-art',
    'Share an image for Sliding Puzzle · staff 👍 approves · /rules',
    'Post one direct image URL, optionally followed by a title (120 characters max). You can also paste/upload an image with the existing upload controls, then send its URL here. Square PNG, JPEG or WebP; 256–4096 pixels per side, at most 10 MiB. Submit only artwork you made or have permission to share and allow late.sh to host and use in Sliding Puzzle; your username is credited. Images are copied to late.sh storage before review. Admin/moderator 👍 adds an image to the pool from the next UTC day; other reactions and removing 👍 do nothing to the pool. Replies are for discussion and never enter the pool. Submissions cannot be edited: delete and resubmit to change them. Deleting a submission retires it from future daily selections.')
ON CONFLICT (visibility, slug) WHERE kind = 'topic'
DO UPDATE SET auto_join = true, permanent = true, topic = EXCLUDED.topic, rules = EXCLUDED.rules;
INSERT INTO chat_room_members (room_id, user_id)
SELECT r.id, u.id FROM chat_rooms r CROSS JOIN users u
WHERE r.kind = 'topic' AND r.visibility = 'public' AND r.slug = 'puzzle-art'
ON CONFLICT (room_id, user_id) DO NOTHING;

-- Approval is in the reaction transaction, including IRC and concurrent
-- reviewers. Removal and thumbs-down deliberately never revoke an approval.
CREATE FUNCTION approve_sliding_puzzle_artwork() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF rtrim(NEW.icon, U&'\FE0F\+01F3FB\+01F3FC\+01F3FD\+01F3FE\+01F3FF') = '👍'
       AND EXISTS (SELECT 1 FROM users WHERE id = NEW.user_id AND (is_admin OR is_moderator)) THEN
        UPDATE sliding_puzzle_artworks
        SET approved_by = NEW.user_id, approved_at = current_timestamp,
            available_from = (current_timestamp AT TIME ZONE 'UTC')::date + 1
        WHERE source_message_id = NEW.message_id AND approved_at IS NULL AND active;
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER sliding_puzzle_artwork_approval
AFTER INSERT OR UPDATE OF icon ON chat_message_reactions
FOR EACH ROW EXECUTE FUNCTION approve_sliding_puzzle_artwork();

-- The picture a maintainer reviewed must never change underneath a reaction.
CREATE FUNCTION guard_sliding_puzzle_artwork_message() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        UPDATE sliding_puzzle_artworks SET active = false WHERE source_message_id = OLD.id;
        RETURN OLD;
    END IF;
    IF NEW.body IS DISTINCT FROM OLD.body AND EXISTS (
        SELECT 1 FROM sliding_puzzle_artworks WHERE source_message_id = OLD.id
    ) THEN
        RAISE EXCEPTION 'puzzle-art submissions cannot be edited; delete and resubmit';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER sliding_puzzle_artwork_message_guard
BEFORE UPDATE OF body OR DELETE ON chat_messages
FOR EACH ROW EXECUTE FUNCTION guard_sliding_puzzle_artwork_message();
