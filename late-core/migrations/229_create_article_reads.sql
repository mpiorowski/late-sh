-- One row per News article a reader opened ahead of their cursor
-- (article_feed_reads): the article modal, reached from the live strip, the
-- Live panel or a Zen Live tile, marks that one article read and no other.
-- An article is unread when it is newer than the cursor and has no row here.
--
-- A visit to the News room moves the cursor to now, which covers every row
-- written before it, so the visit deletes them
-- (late_core::models::article_read::ArticleRead::delete_covered). The rows a
-- reader holds are only the articles opened since their last visit.
CREATE TABLE article_reads (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    article_id UUID NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    PRIMARY KEY (user_id, article_id)
);
