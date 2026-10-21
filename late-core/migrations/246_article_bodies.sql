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
--
-- The checks hold the contract every reader relies on: a status no build
-- writes cannot land (one such row would fail the whole feed read on every
-- replica), a ready body has words, and an unavailable one has neither
-- words nor lines.
CREATE TABLE article_bodies (
    article_id UUID PRIMARY KEY REFERENCES articles(id) ON DELETE CASCADE,
    status TEXT NOT NULL CHECK (status IN ('ready', 'unavailable')),
    word_count INTEGER NOT NULL CHECK (word_count >= 0),
    lines JSONB NOT NULL,
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    CHECK ((status = 'ready') = (word_count > 0)),
    CHECK (status = 'ready' OR lines = '[]'::jsonb)
);

-- The cards read the body status off the snapshot (a LEFT JOIN), so a body
-- landing re-reads every replica's snapshot the way an article write does.
CREATE TRIGGER article_bodies_changed
    AFTER INSERT OR UPDATE OR DELETE ON article_bodies
    FOR EACH STATEMENT EXECUTE FUNCTION notify_articles_changed();
