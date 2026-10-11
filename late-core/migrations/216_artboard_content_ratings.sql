-- Content ratings are independent of applause and never close at month end.
ALTER TABLE artboard_pieces ADD COLUMN owner_marked_nsfw BOOLEAN NOT NULL DEFAULT false;

CREATE TABLE artboard_piece_content_votes (
    piece_id UUID NOT NULL REFERENCES artboard_pieces(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    author_user_id UUID NOT NULL,
    nsfw BOOLEAN NOT NULL,
    PRIMARY KEY (piece_id, user_id),
    CHECK (user_id <> author_user_id)
);

CREATE TABLE artboard_piece_staff_marks (
    piece_id UUID NOT NULL REFERENCES artboard_pieces(id) ON DELETE CASCADE,
    -- Like moderation_audit_log, actor identity survives account deletion.
    actor_user_id UUID NOT NULL,
    authority TEXT NOT NULL CHECK (authority IN ('moderator', 'admin')),
    nsfw BOOLEAN NOT NULL,
    reason TEXT NOT NULL DEFAULT '',
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    PRIMARY KEY (piece_id, actor_user_id)
);

-- One aggregate shape serves listings and the lightweight login check.
CREATE VIEW artboard_piece_content_ratings AS
SELECT p.id AS piece_id, p.owner_marked_nsfw,
       coalesce(v.sfw_votes, 0)::bigint AS sfw_votes,
       coalesce(v.nsfw_votes, 0)::bigint AS nsfw_votes,
       coalesce(m.mod_sfw, 0)::bigint AS mod_sfw,
       coalesce(m.mod_nsfw, 0)::bigint AS mod_nsfw,
       coalesce(m.admin_sfw, 0)::bigint AS admin_sfw,
       coalesce(m.admin_nsfw, 0)::bigint AS admin_nsfw
FROM artboard_pieces p
LEFT JOIN (
    SELECT piece_id, count(*) FILTER (WHERE NOT nsfw) AS sfw_votes,
           count(*) FILTER (WHERE nsfw) AS nsfw_votes
    FROM artboard_piece_content_votes GROUP BY piece_id
) v ON v.piece_id = p.id
LEFT JOIN (
    SELECT piece_id,
           count(*) FILTER (WHERE authority = 'moderator' AND NOT nsfw) AS mod_sfw,
           count(*) FILTER (WHERE authority = 'moderator' AND nsfw) AS mod_nsfw,
           count(*) FILTER (WHERE authority = 'admin' AND NOT nsfw) AS admin_sfw,
           count(*) FILTER (WHERE authority = 'admin' AND nsfw) AS admin_nsfw
    FROM artboard_piece_staff_marks GROUP BY piece_id
) m ON m.piece_id = p.id;
