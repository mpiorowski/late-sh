-- News reads are rows only. A reader has read an article when they hold an
-- `article_reads` row for it, and nothing else: a visit to the News room
-- inserts a row for every article in the snapshot (the newest 20,
-- late_core::models::article::NEWS_FEED_LIMIT), the article modal inserts
-- one, a new user is seeded the same way a visit reads. The per-user cursor
-- (`article_feed_reads`, migration 229 read it beside these rows) goes.
--
-- The cursor's reads carry over: every article at or before a reader's
-- cursor becomes a row, among the newest 100. The badge only ever looks at
-- the newest 20, and a delete backfills an older article into them one at a
-- time, so anything past 100 cannot reach the snapshot before it is read
-- again by a visit.
CREATE INDEX article_reads_article_id_idx ON article_reads (article_id);

INSERT INTO article_reads (user_id, article_id, created)
SELECT r.user_id, a.id, r.last_read_at
FROM article_feed_reads r
JOIN (SELECT id, created FROM articles ORDER BY created DESC LIMIT 100) a
  ON a.created <= r.last_read_at
WHERE r.last_read_at IS NOT NULL
ON CONFLICT (user_id, article_id) DO NOTHING;

DROP TABLE article_feed_reads;
