-- The Artboard gallery (late-ssh `app/artboard/gallery`): pieces hung off
-- the shared board, and the applause they gather.
--
-- A piece is an immutable crop of the live board at the moment it was hung:
-- the vandal and the monthly wipe cannot touch it afterwards. The hanger
-- must own most of the glyphs inside the frame (per-cell provenance decides,
-- `own_share_percent` records the share at hanging time), and the credits
-- for the rest are kept in the cropped `provenance`.
--
-- `period_month` is the UTC month the piece was hung in, which is the
-- competition window the monthly `artboard` profile award ranks (best
-- single piece by applause, minimum applause enforced in the award query).
-- The content hash refuses the same cells twice in one month, which is the
-- one cheap defence against copy-and-rehang; the rest is a mod's
-- `/mod artboard remove`.
CREATE TABLE artboard_pieces (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title TEXT NOT NULL CHECK (char_length(title) BETWEEN 1 AND 40),
    width INTEGER NOT NULL CHECK (width > 0),
    height INTEGER NOT NULL CHECK (height > 0),
    canvas JSONB NOT NULL,
    provenance JSONB NOT NULL,
    glyph_count INTEGER NOT NULL CHECK (glyph_count > 0),
    own_share_percent INTEGER NOT NULL CHECK (own_share_percent BETWEEN 0 AND 100),
    content_hash TEXT NOT NULL,
    period_month DATE NOT NULL,
    UNIQUE (content_hash, period_month)
);

CREATE INDEX artboard_pieces_user_created_idx ON artboard_pieces (user_id, created DESC);
CREATE INDEX artboard_pieces_period_month_idx ON artboard_pieces (period_month);

-- Applause: one per person per piece, free, revocable. `author_user_id` is
-- denormalized off the piece so the no-self-applause rule can be a CHECK,
-- the way gilds do it.
CREATE TABLE artboard_piece_votes (
    piece_id UUID NOT NULL REFERENCES artboard_pieces(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    author_user_id UUID NOT NULL,
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    PRIMARY KEY (piece_id, user_id),
    CONSTRAINT artboard_piece_votes_no_self_applause CHECK (user_id <> author_user_id)
);

-- The gallery's kill switch: while off nothing can be hung or applauded and
-- the rail hides the gallery rows. Starts on.
INSERT INTO app_flags (key, enabled) VALUES
    ('artboard_gallery_enabled', true);
