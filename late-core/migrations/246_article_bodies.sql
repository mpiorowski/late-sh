-- The News reader's article text, fetched once after a share lands and read
-- by id when someone opens the story. It never rides the shared news
-- snapshot: that is re-read on every notify on every replica, and a body
-- per article would multiply the read.
--
-- No row means the fetch is still pending. `status` is 'ready' (the text is
-- in `lines`) or 'unavailable' (a video, an X post, a paywall, a page the
-- fetch could not read; `lines` is empty). An hourly sweep settles a story
-- past the share budget with no row as 'unavailable' (its fetch was cut
-- short) and strips the text of ready rows older than 30 days.
CREATE TABLE article_bodies (
    article_id UUID PRIMARY KEY REFERENCES articles(id) ON DELETE CASCADE,
    status TEXT NOT NULL,
    domain TEXT NOT NULL,
    word_count INTEGER NOT NULL,
    lines JSONB NOT NULL,
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp
);

-- The cards read the body status off the snapshot (a LEFT JOIN), so a body
-- landing re-reads every replica's snapshot the way an article write does.
CREATE TRIGGER article_bodies_changed
    AFTER INSERT OR UPDATE OR DELETE ON article_bodies
    FOR EACH STATEMENT EXECUTE FUNCTION notify_articles_changed();
