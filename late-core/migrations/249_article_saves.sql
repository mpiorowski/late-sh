-- One row per News story a reader keeps for later (`s` in the News room or
-- the reader). The saved stories sit on their own shelf above the feed, at
-- most NEWS_SAVED_LIMIT per reader (late_core::models::article_save), the
-- cap enforced in the save transaction, never here. A delete of the story
-- takes the save with it, so a shelf never points at a story that is gone.
CREATE TABLE article_saves (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    article_id UUID NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    PRIMARY KEY (user_id, article_id)
);

-- The body sweep asks whether any reader holds a story before retiring its
-- text; the cascade on delete walks the same index.
CREATE INDEX article_saves_article_id_idx ON article_saves (article_id);
